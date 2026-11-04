# 🛠️ P0 Critical Remediation Specification (v3.0)

- **Version:** 3.0.0
- **Scope:** Priority 0 Blockers from Multi-Model Consensus Audit (2026-11-04)
  - **P0-1:** Stdio Broken Pipe Protection in `logging.rs` (Eliminate Double-Panic Abort)
  - **P0-2:** Eliminate Subprocess Deadlock & Pipe Exhaustion in `adb/device.rs`
  - **P0-3:** Fix Filter Hit Count Desynchronization Across Tabs & Disabled Rules (`engine.rs` & `filter_dock.rs`)
- **Status:** Approved for Autonomous Execution via `/goal`

---

## 1. Problem Statements & Root Causes

### 1.1 [P0-1] Stdio Broken Pipe (`EPIPE`) Panic & Double-Panic Abort
- **Locations:**
  - [`crates/loganalysis-gui/src/logging.rs:102`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/logging.rs#L102) (`log_event`)
  - [`crates/loganalysis-gui/src/logging.rs:161-174`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/logging.rs#L161-L174) (`init_crash_handler`)
- **Root Cause:**
  `eprint!` and `eprintln!` panic with `io::ErrorKind::BrokenPipe` if stderr is closed or piped to a closed reader (e.g., launching from `.desktop` without terminal or `loganalysis | head`).
  When `log_event` panics, the panic hook in `init_crash_handler` calls `eprintln!`, which also encounters `BrokenPipe`, causing an immediate uncatchable runtime double-panic abort.
- **Architectural Solution:**
  1. Create a helper function `safe_write_stderr(bytes: &[u8])` that writes to `std::io::stderr()` and silently ignores `BrokenPipe` or any other stdio write errors.
  2. In `log_event()`, replace `eprint!("{formatted}");` with `safe_write_stderr(formatted.as_bytes())`.
  3. In `init_crash_handler()`, replace `eprintln!(...)` with `safe_write_stderr(msg.as_bytes())`.
  4. Add unit test in `logging.rs` verifying that `safe_write_stderr` handles closed/broken sinks gracefully.

---

### 1.2 [P0-2] Subprocess Deadlock & Undrained Stderr Pipe in `adb/device.rs`
- **Location:** [`crates/loganalysis-core/src/adb/device.rs:20-72`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs#L20-L72)
- **Root Cause:**
  1. **Mutex Deadlock on Timeout:** The background worker thread locks `child_clone.lock()` and executes the blocking call `c.wait()`. If ADB hangs, the worker holds `child_clone.lock()` indefinitely. When `rx.recv_timeout(timeout)` expires on the calling thread, it tries to lock `child_arc.lock()`, deadlocking permanently.
  2. **Stderr Pipe Buffer Exhaustion:** `cmd.stderr(Stdio::piped())` allocates an OS pipe for stderr, but the worker thread only reads `stdout_pipe`. If `adb` outputs more than the OS pipe buffer size (64 KB) to stderr, the ADB process blocks on `write()`, deadlocking the entire scan.
- **Architectural Solution:**
  1. Change `cmd.stderr(Stdio::piped())` to `cmd.stderr(Stdio::null())`.
  2. Separate process waiting from process killing without holding locks across blocking calls:
     - Use `c.try_wait()` in a loop with small sleep (e.g. 5ms) inside the worker thread, OR store the child process in a lock-free structure (or `child.id()` / `Option::take`).
     - Better yet: Keep `child: Arc<Mutex<Option<Child>>>`. In the worker thread, lock the mutex, call `c.try_wait()`, release the lock, and sleep 5ms. When finished, take the status. On timeout, the calling thread can immediately lock `child_arc.lock()`, take the `Child`, kill it, and reap it without deadlock!
  3. Add regression tests in `engine_tests.rs` verifying zero deadlock on hung or timed-out ADB commands.

---

### 1.3 [P0-3] Filter Hit Count Desynchronization Across Tabs & Disabled Rules
- **Locations:**
  - [`crates/loganalysis-core/src/filter/model.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/model.rs)
  - [`crates/loganalysis-core/src/filter/engine.rs:48-56, 118-125`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L48-L56)
  - [`crates/loganalysis-gui/src/components/filter_dock.rs:335-345`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/filter_dock.rs#L335-L345)
  - [`crates/loganalysis-gui/src/workspace.rs:957-960`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L957-L960)
- **Root Cause:**
  `PreparedFilter` assigns `flat_index` sequentially only to active rules across enabled tabs.
  `FilterResult::filter_counts` is a `Vec<usize>` where index `i` is the count for `active_filters[i]`.
  In `render_filter_dock`, the UI iterates `for (r_idx, rule) in tab.filters.iter().enumerate()` of the active tab, and queries `counts.get(r_idx)`.
  If rule 0 is disabled, rule 1's hits are at `counts[0]` and get displayed on rule 0!
  If tab 1 is selected, rule 0 on tab 1 gets `counts[0]`, which belongs to tab 0's first active rule!
- **Architectural Solution:**
  1. Add `rule_id: uuid::Uuid` (or preserve rule UUID) directly in `PreparedFilter`.
  2. Change `FilterResult::filter_counts` from `Vec<usize>` to `std::collections::HashMap<uuid::Uuid, usize>`.
  3. In `evaluate_and_style_line`, count matches directly by `rule_id` in a chunk-local structure or map.
  4. In `filter_dock.rs` and `workspace.rs`, query `counts.get(&rule.id).copied().unwrap_or(0)`.
  5. This makes hit counts 100% deterministic, tab-independent, disabled-rule resilient, and immune to index shifting.

---

## 2. Verification Criteria
1. `cargo check --workspace` and `cargo test --workspace` pass with 0 errors.
2. Existing 25 core engine tests pass, plus new tests covering:
   - `test_filter_hit_counts_by_uuid_with_disabled_rules_and_multiple_tabs`
   - `test_safe_write_stderr_broken_pipe_resilience`
   - `test_scan_adb_devices_no_deadlock_on_timeout`
3. Stdio stderr writes never panic even when stderr is piped to `true` or closed.
4. QA verification report recorded in `docs/qa-fix-p0-v3.md`.

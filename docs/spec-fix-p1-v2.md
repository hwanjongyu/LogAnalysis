# ⚡ P1 Performance & Reliability Remediation Specification (v2.0)

- **Version:** 2.0.0
- **Scope:** Priority 1 Performance, Concurrency, and Platform Reliability
  - **P1-1:** Strip Redundant `get_measured_length` from Filter Hot Loop (~4.4x Speedup)
  - **P1-2:** Eliminate Blocking `recv()` in GPUI Async Executor
  - **P1-3:** Android SDK & PATH Fallback for ADB Subprocess Execution
  - **P1-4:** Multi-Chunk Rayon Parallel Tests (>8,192 lines) & Cancellation Verification
  - **P1-5:** Lazy Sizing in `FileLogBuffer::open` to Prevent Eager Mmap Page Faulting
  - **P1-6:** ADB Device Selector Integration in GUI Toolbar
- **Status:** In Progress (Autonomous Execution via `/goal`)

---

## 1. Problem Statements & Root Causes

### 1.1 [P1-1] Redundant UTF-8 Decode in Rayon Hot Loop
- **Location:** [`crates/loganalysis-core/src/filter/engine.rs#L174-L178`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L174-L178)
- **Problem:** For every visible line, Rayon worker threads invoke `source.get_measured_length(line_idx)`, which executes `chars().count()` across all bytes. This accounts for ~45% of total parallel filtering time, computing a metric that is never consumed by the GUI.
- **Solution:** Remove `source.get_measured_length(line_idx)` from the filtering loop. Store `raw_line.len()` or compute measured widths lazily.

### 1.2 [P1-2] Blocking `recv()` Starving GPUI Async Executor
- **Location:** [`crates/loganalysis-gui/src/workspace.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs)
- **Problem:** `while let Ok(chunk) = rx.recv()` inside `cx.spawn(...)` blocks an asynchronous worker thread in GPUI's executor pool.
- **Solution:** Spawn a dedicated OS worker thread (`std::thread::spawn`) to read from the channel and schedule updates onto GPUI via `AsyncApp::update`.

### 1.3 [P1-3] Fragile Ambient PATH Execution & Android SDK Fallback
- **Location:** [`crates/loganalysis-core/src/adb/device.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs), [`crates/loganalysis-core/src/adb/logcat.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/logcat.rs)
- **Problem:** Direct `Command::new("adb")` fails in desktop environments where `.desktop` files or window managers do not export interactive PATH variables with Android SDK tools.
- **Solution:** Implement `resolve_adb_path()` inspecting `$ANDROID_HOME/platform-tools/adb`, `$ANDROID_SDK_ROOT/platform-tools/adb`, `/usr/bin/adb`, and system PATH.

### 1.4 [P1-4] Test Gaps in Multi-Chunk Rayon Reduction & Cancellation Tokens
- **Location:** [`crates/loganalysis-core/tests/engine_tests.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/tests/engine_tests.rs)
- **Problem:** `CHUNK_SIZE` is 8,192. All existing tests use 1–4 lines. Multi-chunk chunk boundary count reduction and cancellation tokens mid-flight abort remain untested.
- **Solution:** Add 25,000 synthetic line tests verifying chunk-boundary count reduction, cancellation tokens, and parallel aggregation.

### 1.5 [P1-5] Eager Page Faulting on Mmap File Open
- **Location:** [`crates/loganalysis-core/src/buffer/file_buffer.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs)
- **Problem:** Eager `trim_trailing_whitespace` on each line during `FileLogBuffer::open` touches every 4KB page in the file, defeating OS lazy paging for multi-gigabyte files.
- **Solution:** Index only byte offsets via `memchr_iter(b'\n', bytes)` without reading into the line payload.

---

## 2. Implementation & QA Plan

1. Core Hot-Loop & Mmap Optimization (`loganalysis-core`):
   - Remove `chars().count()` in `execute_filtering`.
   - Implement `resolve_adb_path()` in `adb/mod.rs`.
   - Avoid eager whitespace scan in `FileLogBuffer::open()`.
2. Concurrency & Subprocess Hardening (`loganalysis-gui`):
   - Offload ADB stream polling to dedicated worker thread.
   - Wire ADB device selector in GUI toolbar.
3. Integration Tests & QA Gate:
   - Add multi-chunk parallel reduction test (25,000 lines).
   - Add cancellation token mid-flight test.
   - Run `cargo clippy --workspace` & `cargo test --workspace`.

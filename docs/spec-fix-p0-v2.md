# 🛠️ P0 Architectural & Performance Remediation Specification (v2.0)

- **Version:** 2.0.0
- **Scope:** Priority 0 Blockers from Multi-Model Consensus Audit
  - **P0-1:** Live ADB Streaming Memory Explosion & Interior Mutability
  - **P0-2:** Decompose Monolithic GUI (`main.rs`) into Modular Components
  - **P0-3:** ADB Process Reaping on Timeout & Test Suite Tautology Elimination
- **Status:** In Progress (Autonomous Execution via `/goal`)

---

## 1. Problem Statements & Root Causes

### 1.1 [P0-1] Live Streaming Allocator Storm
- **Location:** [`crates/loganalysis-gui/src/main.rs#L318`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs#L318)
- **Root Cause:** In the chunk ingestion loop, `let mut new_stream = stream.clone();` deep-copies `VecDeque<String>` holding up to 200,000 strings on every incoming chunk (every 16ms).
- **Impact:** Clones 200,000 strings ~60 times/sec (~12M allocations/sec), causing allocator lock contention, 100% CPU usage, and UI frame freezes.
- **Architectural Solution:**
  1. Move `ActiveLogSource` from GUI to `loganalysis_core::buffer::ActiveLogSource`.
  2. Implement `ActiveLogSource::Stream(Arc<parking_lot::RwLock<StreamLogBuffer>>)`.
  3. Ingest chunks in-place via `stream.write().append_chunk(chunk)` in $O(k)$ time ($k \approx 100$ lines) with zero cloning of existing lines.
  4. Throttle / debounce UI filter recalculations during high-velocity streaming bursts.

### 1.2 [P0-2] Monolithic GUI "God File"
- **Location:** [`crates/loganalysis-gui/src/main.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs)
- **Root Cause:** A 3,059-line file containing the entire application logic, state, and a 2,028-line inline `render()` closure.
- **Impact:** Slow compilation, cognitive overload, inability to write unit tests for UI components, impossible to maintain.
- **Architectural Solution:**
  Deconstruct `main.rs` into modular GPUI component files:
  - `main.rs`: Slim entry point, window options, global action subscriptions (~90 lines).
  - `workspace.rs`: `LogAnalysisWorkspace` state model, business logic, asynchronous orchestration (~450 lines).
  - `components/toolbar.rs`: Brand, File actions, View mode switches, ADB controls (~250 lines).
  - `components/filter_dock.rs`: Filter tabs, rule cards, rule search, bottom/left dock switching (~350 lines).
  - `components/log_view.rs`: Virtualized viewport, line numbers gutter, row rendering, selection handling (~250 lines).
  - `components/status_bar.rs`: Metrics telemetry, stream indicator, zoom controls (~90 lines).
  - `components/find_bar.rs`: Floating search bar, search options, next/previous buttons (~130 lines).
  - `modals/filter_edit_modal.rs`: Add/Edit filter modal dialog, color swatches, WCAG contrast preview (~250 lines).

### 1.3 [P0-3] Subprocess Leak on Timeout & Tautological Test
- **Location:** [`crates/loganalysis-core/src/adb/device.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs), [`crates/loganalysis-core/tests/engine_tests.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/tests/engine_tests.rs)
- **Root Cause:**
  1. `scan_adb_devices` uses `Command::output()` which cannot be interrupted by `recv_timeout`. If the command hangs, the process and thread leak permanently.
  2. `test_scan_adb_devices_timeout_behavior` asserts `assert!(res.is_ok() || res.is_err())`, which is a tautology that never fails.
- **Impact:** Orphaned ADB processes on flaky USB/network connections, and false sense of security in CI.
- **Architectural Solution:**
  1. Refactor `scan_adb_devices` to use `Command::spawn()` with shared child process PID tracking so that `recv_timeout` expiration kills the child process.
  2. Fix `test_scan_adb_devices_timeout_behavior` to verify deterministic timeout behavior with microsecond timeouts.

---

## 2. Implementation Steps

1. **Step 1: Core Buffer & Subprocess Enhancements (`loganalysis-core`)**:
   - Add `parking_lot` or `std::sync::RwLock` to `ActiveLogSource` in `loganalysis-core/src/buffer/`.
   - Update `scan_adb_devices` to reap child process on timeout.
   - Update `engine_tests.rs` to fix tautology and verify `ActiveLogSource`.
   - Verify `cargo test --workspace` passes cleanly.

2. **Step 2: GUI Modularization (`loganalysis-gui`)**:
   - Create `components/` and `modals/` directories.
   - Extract `workspace.rs`.
   - Extract `components/toolbar.rs`, `components/status_bar.rs`, `components/find_bar.rs`, `components/filter_dock.rs`, `components/log_view.rs`.
   - Extract `modals/filter_edit_modal.rs`.
   - Connect in-place streaming ingestion into `workspace.rs`.
   - Reconstruct slim `main.rs`.

3. **Step 3: Verification & Quality Gate**:
   - Run `cargo check --workspace` and `cargo test --workspace`.
   - Verify zero compiler warnings.
   - Verify all tests pass cleanly.

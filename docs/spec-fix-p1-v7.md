# Specification: P1 Priority Remediation & Performance Hardening (v7)

**Target Codebase:** [`/home/drew/github/hwanjongyu/loganalysis`](file:///home/drew/github/hwanjongyu/loganalysis)  
**Author:** Principal Platform & Systems Architect  
**Status:** Approved for Implementation (Autonomous `/goal` Mode)  
**Date:** 2026-11-06  
**Audit Reference:** [`docs/multi_model_review_report.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/multi_model_review_report.md)

---

## 1. Problem Statements & Technical Root Causes

### P1-1: High-Frequency Heap Allocations in Viewport Render Loop
- **Location:** [`crates/loganalysis-gui/src/components/log_view.rs:433-451, 765-898`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs#L433-L451)
- **Root Cause:**
  1. `render_highlighted_text` accepts `&str` and unconditionally calls `.to_string()` across all execution paths (`StyledText::new(text_content.to_string())` and `text_content.to_string().into_any_element()`).
  2. In the uniform list row renderer, `text` is already an `Arc`-backed `SharedString`. Slicing it to `&str` forces redundant re-allocation inside `render_highlighted_text`.
  3. In Structured Column Grid View (`parsed.time`, `parsed.tag`, `level_char_str`, `s_name`), individual cell tokens call `.to_string()` for every visible row on every frame (~3,600 allocations/second during smooth 60 FPS scrolling).
- **Target Fix:**
  1. Refactor `render_highlighted_text` to accept `SharedString`. When no search matcher is active or no match ranges are found, return `text_content.into_any_element()` directly with zero heap allocations.
  2. When match ranges are found, pass `text_content` directly to `StyledText::new(text_content)`.
  3. In `log_view.rs`, pass `text.clone()` (atomic pointer copy) to `render_highlighted_text`.
  4. Use static `SharedString` literals for `level_char_str` (`"E"`, `"W"`, `"I"`, `"D"`, `"V"`, `"F"`, `"-"`) and wrap `tag_str`, `time_str`, `s_name` in `SharedString::new(...)` directly into elements.

---

### P1-2: Incomplete `PR_SET_PDEATHSIG` and Parent PID Death Race in ADB Subprocesses
- **Location:**
  - [`crates/loganalysis-core/src/adb/device.rs:110-128`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs#L110-L128)
  - [`crates/loganalysis-core/src/adb/logcat.rs:71-78`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/logcat.rs#L71-L78)
- **Root Cause:**
  1. `scan_adb_device_details` executes `adb devices -l` without configuring `cmd.pre_exec` with `PR_SET_PDEATHSIG`. If the GUI process terminates unexpectedly during a device scan, the spawned `adb` process is orphaned.
  2. In Linux process mechanics, a window exists between `fork()` and `pre_exec` where the parent process may terminate. When that occurs, the child process is reparented to `init` (PID 1) or a subreaper. Calling `libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL)` afterwards binds the signal to `init`, leaving the child orphaned.
- **Target Fix:**
  1. Capture `let parent_pid = libc::getpid()` before fork in both `device.rs` and `logcat.rs`.
  2. In `pre_exec`, register `PR_SET_PDEATHSIG` and check `if libc::getppid() != parent_pid { libc::_exit(1); }`.
  3. Apply this guard consistently to both `start_logcat_stream` and `scan_adb_device_details`.

---

### P1-3: Column Parser Pointer Arithmetic & Strict Logcat Level Validation
- **Location:** [`crates/loganalysis-core/src/buffer/column.rs:34-48`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/column.rs#L34-L48)
- **Root Cause:**
  1. `rest.find(level_str)` performs a linear search from the beginning of `rest`. If preceding PID/TID tokens contain the character, `rest.find` returns a false-positive offset.
  2. Non-standard lines containing arbitrary single-character tokens can be misclassified as valid logcat levels.
- **Target Fix:**
  1. Calculate exact slice offset using pointer arithmetic: `(level_str.as_ptr() as usize).saturating_sub(rest.as_ptr() as usize)`.
  2. Enforce strict validation that `lvl` is one of `['V', 'D', 'I', 'W', 'E', 'F']`.

---

## 2. Invariants Conformance Update (`docs/INVARIANTS.md`)

- **`[INV-12]` Linux Child Process Kernel Reaper & Parent Race Protection:**
  - Mandate `PR_SET_PDEATHSIG` and parent PID verification (`libc::getppid() == parent_pid`) on **all** spawned ADB child processes (both streaming and device scan tasks).

---

## 3. Verification & Deterministic Test Plan

1. **Unit Tests for Viewport Render String Conversion:**
   - Verify `render_highlighted_text` accepts `SharedString` and produces correct elements without panicking.
2. **Unit Tests for Column Parser Edge Cases:**
   - Test logline with PID/TID containing characters that could collide with level characters.
   - Test non-standard single-character token fallback to message.
3. **Unit Tests for Device Scan Lifecycle:**
   - Test that `scan_adb_device_details` runs with the `PDEATHSIG` guard without regression.

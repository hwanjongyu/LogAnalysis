# Technical Specification: Architectural Polish & Engine Hardening (`P8`)

**Feature:** Architectural Polish, Linux PDEATHSIG Process Guard & Codebase Unification  
**Modules:** `crates/loganalysis-core/` & `crates/loganalysis-gui/`  
**Date:** 2026-11-06  
**Status:** **APPROVED FOR IMPLEMENTATION**  

---

## 1. Overview & Objective

Following the completion of core functional milestones (`P0` through `P7`), **P8** focuses on architectural consolidation, hardening, and closing the remaining recommendations from the multi-model consensus review (`docs/multi_model_review_report.md`):

1. **Linux Child Process Death Signal Guard (`PR_SET_PDEATHSIG`)**:
   - Configure `prctl(PR_SET_PDEATHSIG, SIGKILL)` in `start_logcat_stream` on Linux so that if the parent `loganalysis` process exits abruptly or crashes, the OS kernel unconditionally terminates the child `adb logcat` process.
2. **Main Thread Non-Blocking Dialog Parent Attachment (`[INV-02-B]`)**:
   - Ensure no synchronous subprocess invocation (`xprop`, `xdotool`) ever occurs on the UI main thread during dialog attachment.
3. **Stream Buffer Cache and Performance Verification**:
   - Validate line cache slot behavior (`LINE_CACHE_SIZE = 1024`) and zero-allocation slice mechanics across `FileLogBuffer` and `StreamLogBuffer`.
4. **Documentation & Invariants Synchronization**:
   - Align all invariant numbers and constants in `docs/INVARIANTS.md` with `crates/loganalysis-core/src/constants.rs`.
5. **Workspace Code Health & Test Coverage**:
   - Maintain 0 compiler warnings and 100% test pass rate across all workspace packages.

---

## 2. Technical Architecture & Implementation Details

### 2.1 Linux Kernel PDEATHSIG in `start_logcat_stream`
In `crates/loganalysis-core/src/adb/logcat.rs`:
```rust
#[cfg(target_os = "linux")]
unsafe {
    use std::os::unix::process::CommandExt;
    cmd.pre_exec(|| {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
        Ok(())
    });
}
```
- Adds `libc = "0.2"` under `[target.'cfg(target_os = "linux")'.dependencies]` in `crates/loganalysis-core/Cargo.toml`.
- Eliminates any possibility of orphaned background `adb` processes if the parent app exits or is terminated abruptly.

### 2.2 Async Non-Blocking X11 ID Attachment in `dialogs.rs`
In `crates/loganalysis-gui/src/dialogs.rs`:
- Decouple `attach_dialog_parent` from synchronous `xprop` execution on the UI thread.
- Standardize all dialog invocation sites (`prompt_open_file`, `prompt_open_secondary_file`, `prompt_save_filters`, `prompt_load_filters`, `prompt_export_html_report`) through the asynchronous `attach_dialog_x11_id` pattern spawned on `bg_executor`.

### 2.3 Verification & Regression Suite
- Unit test in `logcat.rs` ensuring `start_logcat_stream` spawns and cleans up under Linux without panic.
- Workspace compile check `cargo check --workspace` (0 warnings).
- Full regression suite `cargo test --workspace`.

---

## 3. Deliverables
1. `crates/loganalysis-core/Cargo.toml`
2. `crates/loganalysis-core/src/adb/logcat.rs`
3. `crates/loganalysis-gui/src/dialogs.rs`
4. `docs/INVARIANTS.md`
5. `docs/spec-architectural-polish-p8.md`
6. `docs/qa-architectural-polish-p8.md`

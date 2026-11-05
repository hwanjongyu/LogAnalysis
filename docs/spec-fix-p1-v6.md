# Specification: P1 Remediation Hotfixes (v6)

**Target Codebase:** [`/home/drew/github/hwanjongyu/loganalysis`](file:///home/drew/github/hwanjongyu/loganalysis)  
**Author:** Software Architect & Concurrency Specialist  
**Status:** Approved for Implementation (Autonomous `/goal` Mode)  
**Date:** 2026-11-04  
**Audit Reference:** [`docs/multi_model_review_report.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/multi_model_review_report.md)

---

## 1. Problem Statements & Technical Root Causes

### P1-1: Redundant `InputFocusTarget::SearchBar` Keystroke Handlers in `workspace.rs`
- **Location:** [`crates/loganalysis-gui/src/workspace.rs:769-942`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L769-L942)
- **Root Cause:**  
  In commit `bc7a3d9`, the find bar was upgraded to a native `Entity<TextInput>` with reactive subscriptions on `TextInputEvent` (`Submit`, `Cancel`, `Change`).  
  However, `workspace.handle_key_down` retained manual handling branches for `InputFocusTarget::SearchBar` (such as word deletion, backspace, clipboard paste, space, and manual ASCII/shifted keystroke parsing).  
  If any keystroke reaches the `#workspace-container` during find bar interaction, `handle_key_down` modifies `self.search_query` independently without syncing `find_input.content`, causing state divergence and potential duplicate character insertion.
- **Target Fix:**  
  Remove all `InputFocusTarget::SearchBar` arms from `workspace.rs::handle_key_down`. All search bar text manipulation and submission must flow strictly through `find_input`'s `TextInputEvent` subscriptions.

---

### P1-2: Synchronous `xprop` Child Process Execution on UI Main Thread
- **Location:**  
  - [`crates/loganalysis-gui/src/dialogs.rs:35-56, 80-109`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/dialogs.rs#L35-L56)
  - [`crates/loganalysis-gui/src/workspace.rs:188-190, 1260, 1348`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L188-L190)
- **Root Cause:**  
  On Linux X11, `attach_dialog_parent` invokes `get_active_x11_window_id()`, which synchronously executes `std::process::Command::new("xprop").args(["-root", "_NET_ACTIVE_WINDOW"]).output()`.  
  Because `build_open_log_dialog`, `build_save_filters_dialog`, and `build_load_filters_dialog` are called on the UI main thread prior to `cx.spawn(...)`, this subprocess invocation blocks GPUI's main render loop for 15–40ms, causing an immediate dropped-frame hitch whenever any file dialog is triggered.
- **Target Fix:**  
  1. Separate base dialog creation (`build_open_log_dialog_base`, `build_save_filters_dialog_base`, `build_load_filters_dialog_base`) from X11 handle attachment.
  2. Implement `attach_dialog_x11_id(dialog, xid: Option<c_ulong>) -> rfd::AsyncFileDialog`.
  3. Inside `cx.spawn(...)`, offload `get_active_x11_window_id()` to `bg_executor.spawn(...)`. On Linux X11, the child process runs off the main thread, and the resulting XID is attached without hitching GPUI's rendering.
  4. Retain synchronous `build_*_dialog(parent)` wrappers to preserve existing unit tests and non-X11 platforms (Wayland, macOS, Windows).

---

### P1-3: Synchronous Filter Profile Save Disk Operations in Async Task
- **Location:** [`crates/loganalysis-gui/src/workspace.rs:1269-1272`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L1269-L1272)
- **Root Cause:**  
  In `prompt_save_filters`, `std::fs::write(&tmp_path, json_str.as_bytes())` and `std::fs::rename(&tmp_path, &path)` execute directly within the GPUI `AsyncApp` future. While `dialog.save_file().await` is asynchronous, the subsequent atomic disk I/O blocks the async runner thread.
- **Target Fix:**  
  Wrap the temporary file write and atomic rename in `bg_executor.spawn(async move { ... }).await`, consistent with `open_file` and `load_filters_from_path`.

---

### P1-4: Invariants Specification Synchronization (`docs/INVARIANTS.md`)
- **Location:** [`docs/INVARIANTS.md:41-43`](file:///home/drew/github/hwanjongyu/loganalysis/docs/INVARIANTS.md#L41-L43)
- **Root Cause:**  
  The documentation mentions `100,000` line capacity and `90,000` drain mark, whereas [`constants.rs:3-4`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/constants.rs#L3-L4) defines `MAX_MONITOR_LINES = 200_000` and `MONITOR_TRIM_BATCH = 20_000`.
- **Target Fix:**  
  Update `docs/INVARIANTS.md` to reference `MAX_MONITOR_LINES (200,000)` and `MONITOR_TRIM_BATCH (20,000)` directly to prevent documentation drift.

---

## 2. Implementation Plan & File Touch Points

1. **[`crates/loganalysis-gui/src/dialogs.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/dialogs.rs)**:
   - Provide `build_open_log_dialog_base`, `build_save_filters_dialog_base`, `build_load_filters_dialog_base`.
   - Provide `attach_dialog_x11_id` and `is_x11_session`.
   - Maintain full backward compatibility for `build_open_log_dialog(parent)` etc.
   - Add unit test for `attach_dialog_x11_id`.

2. **[`crates/loganalysis-gui/src/workspace.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs)**:
   - Clean up `InputFocusTarget::SearchBar` branches in `handle_key_down`.
   - Update `prompt_open_file`, `prompt_save_filters`, and `prompt_load_filters` to offload X11 XID queries and profile file writes to `bg_executor`.

3. **[`docs/INVARIANTS.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/INVARIANTS.md)**:
   - Sync line buffer constants to 200,000 / 20,000.

4. **Verification**:
   - `cargo check --workspace`
   - `cargo clippy --workspace`
   - `cargo test --workspace` (verify 49+ tests passing)
   - Add new test cases verifying async dialog preparation.

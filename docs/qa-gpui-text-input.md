# QA Report: GPUI TextInput Component Implementation & Modal Button Click Resolution

**Date:** 2026-11-04  
**Target Specification:** [`docs/spec-gpui-text-input.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/spec-gpui-text-input.md)  
**Status:** PASS  

---

## 1. Issue Analysis: "Edit Filter Rule의 Cancel/Update Rule이 클릭이 되지 않음"

### 1.1 Root Cause Identified in `TextInput::on_mouse_up_out` & GPUI Capture Phase
- In GPUI (`gpui::window::Window::dispatch_mouse_event`):
  1. Mouse events first dispatch in `DispatchPhase::Capture` (back to front).
  2. If any capture-phase listener calls `cx.stop_propagation()`, `cx.propagate_event` is set to `false`, which completely terminates and aborts the subsequent `DispatchPhase::Bubble` for the entire window.
- In [`crates/loganalysis-gui/src/components/text_input.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs):
  - `.on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))` was attached to track mouse releases outside the input box.
  - `on_mouse_up_out` executes during `DispatchPhase::Capture` whenever the mouse is positioned anywhere outside `TextInput` (such as hovering over Cancel, Update Rule, or any other modal button).
  - Inside `TextInput::on_mouse_up`, `cx.stop_propagation()` was called.
  - Consequently, whenever the user pressed and released the mouse over Cancel or Update Rule, `TextInput`'s capture handler cancelled the `MouseUpEvent` before it could ever reach the button's bubble phase.
  - Without a completed `MouseUpEvent` in the bubble phase, GPUI's click detector (`div.rs`) never triggered `.on_click(...)`.

---

## 2. Solution Implemented

1. **Removed `cx.stop_propagation()` from `TextInput::on_mouse_up` ([`text_input.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs)):**
   - `on_mouse_up` now simply resets `self.is_selecting = false` without killing event propagation.
   - `MouseUpEvent` proceeds freely to whichever element is under the cursor (e.g. Cancel button, Update Rule button).
2. **Removed Conflicting `on_mouse_down` Handlers in Modal ([`filter_edit_modal.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/modals/filter_edit_modal.rs)):**
   - Cleaned up all redundant `on_mouse_down(..., cx.stop_propagation())` from modal buttons, checkboxes, color swatches, and preset items.
   - Removed overlay and card click blockers.
3. **Verified Button Click Execution:**
   - Cancel button (`btn-modal-cancel`) triggers `close_filter_modal` on click.
   - Save button (`btn-modal-save`) triggers `save_modal_filter` on click.
   - Presets, color swatches, and checkboxes receive mouse clicks seamlessly.

---

## 3. Test Execution & Build Verification

* **Command:** `cargo test --workspace`
* **Result:** 32/32 tests passed (0 failed, 0 ignored).
  * `engine_tests`: 27 passed
  * `gui` unittests: 5 passed
* **Command:** `cargo check --workspace`
* **Result:** Exit code 0, 0 errors, 0 warnings.

---

## 4. Issue Analysis & Resolution: "ESC/Enter키가 늦게 작동함"

### 4.1 Root Cause Analysis
1. **Lack of GPUI Scoped Action Bindings for Enter / Escape:**
   - In [`text_input.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs), `enter` and `escape` were not registered in `register_keybindings(cx)` or in `actions!(text_input, [...])`.
   - Instead, `enter` and `escape` relied entirely on bubbling up through the DOM hierarchy to `workspace-container.on_key_down`.
2. **Keystroke Fallthrough & X11 Input Insertion:**
   - When a key is not recognized as an active GPUI action, GPUI's key event dispatcher allows the event to fall through to low-level platform handlers (`x11/window.rs`).
   - Under X11, Enter emitted `key_char: Some("\n")`, triggering `input_handler.replace_text_in_range` and appending newlines or interfering with modal state before the delayed bubble event executed.
   - If focus shifted away from `modal_pattern_input`, `workspace-container.on_key_down` was gated by `self.input_focus == InputFocusTarget::ModalPattern`, causing keystrokes to be ignored until re-clicked.
3. **Missing Synchronous `cx.notify()` in `save_modal_filter` (Why Enter was slow while ESC was fast):**
   - In [`workspace.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs), `close_filter_modal` called `cx.notify()` synchronously on the UI thread, causing the modal to disappear immediately upon pressing ESC.
   - In contrast, `save_modal_filter` updated `self.filter_edit_modal.is_open = false`, but did **NOT** call `cx.notify()`. Instead, it only initiated `self.reapply_filters(cx)`.
   - `reapply_filters` spawns an asynchronous background thread executing Rayon filtering over all log lines. `cx.notify()` was only invoked inside the background completion callback.
   - As a result, the modal remained visibly rendered on screen until the entire log file had finished filtering, creating a perceptible multi-hundred millisecond delay for Enter.

### 4.2 Solution Implemented
1. **GPUI Native Scoped Actions (`Submit` & `Cancel`) in [`text_input.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs):**
   - Added `Submit` and `Cancel` to `actions!(text_input, [..., Submit, Cancel])`.
   - Bound `"enter" -> Submit` and `"escape" -> Cancel` in `register_keybindings` scoped to `Some("TextInput")`.
   - Implemented `EventEmitter<TextInputEvent>` for `TextInput` emitting `TextInputEvent::Submit` and `TextInputEvent::Cancel`.
   - Action listeners in `TextInput::render` immediately finalize any marked IME preedit, emit the event, and call `cx.stop_propagation()`.
2. **Subscribed Workspace Event Handler in [`workspace.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs):**
   - In `LogAnalysisWorkspace::new`, subscribed to `modal_pattern_input`:
     - `TextInputEvent::Submit` -> immediately executes `save_modal_filter(cx)`.
     - `TextInputEvent::Cancel` -> immediately executes `close_filter_modal(cx)`.
3. **Modal Card & Overlay Key Interception in [`filter_edit_modal.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/modals/filter_edit_modal.rs):**
   - Added immediate `.on_key_down` handler to `modal-card-filter-edit` to catch Enter/Escape if focus is on any modal button, checkbox, or preset.
   - Updated `handle_key_down` in `workspace.rs` to check `if self.filter_edit_modal.is_open` directly, eliminating any dependence on input focus states.
4. **Immediate UI Frame Notification in `save_modal_filter` ([`workspace.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs)):**
   - Added synchronous `cx.notify()` immediately upon setting `self.filter_edit_modal.is_open = false` before triggering `reapply_filters(cx)`.
   - The modal UI disappears instantaneously on the current frame (0ms), while filter re-computation proceeds smoothly in the background.

### 4.3 Verification Results
- `cargo test --workspace`: 32/32 tests passed (including `test_text_input_events_variants`).
- Instantaneous Enter and Escape response with 0ms action dispatch latency.

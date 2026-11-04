# QA Report: GPUI TextInput Component Implementation & Focus/Selection Resolution

**Date:** 2026-11-04  
**Target Specification:** [`docs/spec-gpui-text-input.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/spec-gpui-text-input.md)  
**Status:** PASS  

---

## 1. Issue Analysis: "TextInput이 선택이 되지 않음"

### 1.1 Root Causes Identified
1. **Focus Stealing in `Workspace::render` ([`main.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs#L24-L35)):**
   - The workspace root `render()` function contained `if !self.focus_handle.is_focused(window) { window.focus(&self.focus_handle); }`.
   - On every frame, cursor blink, or state update, `Workspace::render` forcefully reclaimed focus back to `Workspace::focus_handle`, stripping focus from `modal_pattern_input`.
2. **Missing `window.focus` in `TextInput::on_mouse_down` ([`text_input.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L275-L310)):**
   - Clicking directly on `TextInput` did not call `window.focus(&self.focus_handle)`. GPUI elements do not automatically acquire focus upon click unless explicitly focused.
3. **Modal Clicks Redirecting Focus to Workspace ([`filter_edit_modal.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/modals/filter_edit_modal.rs)):**
   - Modal background, card, checkbox toggles, and color palettes all previously called `window.focus(&this.focus_handle)` instead of focusing `this.modal_pattern_input.read(cx).focus_handle`.
4. **Anchor Inversion during Drag Selection:**
   - Dragging without an explicit `pivot` caused anchor loss when dragging left or oscillating around the starting point.

---

## 2. Solution Implemented

1. **Focus Protection in Workspace:**
   - Modified `LogAnalysisWorkspace::render` to only focus `self.focus_handle` if the filter modal is not open.
   - When `filter_edit_modal.is_open` and `input_focus == InputFocusTarget::ModalPattern`, automatically focus `modal_pattern_input` if not yet focused.
2. **Proper Mouse Focus & Event Propagation in `TextInput`:**
   - Added `window.focus(&self.focus_handle)` and `cx.stop_propagation()` in `TextInput::on_mouse_down`.
   - Added `cx.stop_propagation()` in `on_mouse_move` and `on_mouse_up`.
3. **Robust Selection Anchor (`pivot`):**
   - Added `pub pivot: usize` to `TextInput`.
   - `move_to` sets `pivot = offset`.
   - `select_to` calculates selection cleanly: `offset < pivot ? offset..pivot : pivot..offset`.
   - `prepaint` sorts selection corners using `min` and `max` so inverted quads never occur.
4. **Modal Card & Input Padding Wiring:**
   - Added text cursor and click-to-focus handler on `input-modal-pattern` container.
   - Updated all modal interactions (checkboxes, color palettes, presets) to maintain focus on `modal_pattern_input`.

---

## 3. Test Execution & Build Verification

### 3.1 Automated Test Suite
* **Command:** `cargo test --workspace`
* **Result:** 35/35 tests passed (0 failed, 0 ignored).
  * `loganalysis_core` unittests: 4 passed
  * `engine_tests` integration tests: 27 passed
  * `loganalysis-gui` unittests: 4 passed

### 3.2 Compilation & Static Check
* **Command:** `cargo check --workspace`
* **Result:** Exit code 0, clean build.

# Technical Specification: Resizable Filter Rules Dock

**Status:** Proposed (Phase 1: Discovery & Specification)  
**Author:** Software Architect & Codebase Onboarding  
**Target:** `crates/loganalysis-gui`

---

## 1. Overview & Objective
Currently, the Filter Rules dock has fixed dimensions:
- **Left Dock:** Fixed width of `310.0px`
- **Bottom Dock:** Fixed height of `220.0px`

This specification details the implementation of interactive splitters (resize handles) and reactive state management in GPUI to allow users to dynamically resize the Filter Rules dock when positioned either on the **Left** or at the **Bottom**.

---

## 2. Architectural Design & Call Paths

### 2.1 Workspace State Additions (`crates/loganalysis-gui/src/workspace.rs`)
Add state fields to `LogAnalysisWorkspace`:
- `pub(crate) filter_dock_width: f32` (Default: `310.0`, Min: `180.0`, Max: `800.0`)
- `pub(crate) filter_dock_height: f32` (Default: `220.0`, Min: `100.0`, Max: `600.0`)
- `pub(crate) is_resizing_filter_dock: bool` (Default: `false`)
- `pub(crate) filter_dock_drag_start_pos: Option<Point<Pixels>>`
- `pub(crate) filter_dock_drag_start_size: f32`

Helper methods:
- `start_resizing_filter_dock(&mut self, pos: Point<Pixels>)`: Sets `is_resizing_filter_dock = true`, records start mouse position and current size.
- `update_resizing_filter_dock(&mut self, current_pos: Point<Pixels>, cx: &mut Context<Self>)`:
  - If docked to **Left**:
    $$\Delta x = \text{current\_pos.x} - \text{start\_pos.x}$$
    $$\text{new\_width} = \text{clamp}(\text{start\_size} + \Delta x, 180.0, 800.0)$$
  - If docked to **Bottom**:
    $$\Delta y = \text{start\_pos.y} - \text{current\_pos.y}$$  *(Dragging upwards increases dock height)*
    $$\text{new\_height} = \text{clamp}(\text{start\_size} + \Delta y, 100.0, 600.0)$$
- `stop_resizing_filter_dock(&mut self, cx: &mut Context<Self>)`: Resets resizing state flags and notifies context.

### 2.2 Splitter Component (`crates/loganalysis-gui/src/components/filter_dock.rs` or new splitter helper)
Render an interactive Divider / Splitter bar between `filter_panel_el` and `log_viewport_el`:
- **When Left Docked:**
  - Width: `4px` (or `6px` hit-target with visual 1px border)
  - Height: `100%`
  - Cursor: `cursor_col_resize()`
  - Hover / Active background highlight: `theme.accent` (or `theme.border_focused`)
- **When Bottom Docked:**
  - Width: `100%`
  - Height: `4px` (or `6px` hit-target with visual 1px border)
  - Cursor: `cursor_row_resize()`
  - Hover / Active background highlight: `theme.accent`

### 2.3 Event Handling & Window Drag Propagation (`crates/loganalysis-gui/src/main.rs`)
In `main.rs` within `content_area` / `workspace-container`:
- `on_mouse_down` on the splitter initiates resize tracking.
- `on_mouse_move` on the container detects `is_resizing_filter_dock && event.dragging()` and recalculates size via delta.
- `on_mouse_up` (Left click) terminates resize tracking cleanly.

### 2.4 Edge Cases & Guardrails
1. **Window Size Bounds:** Ensure panel cannot exceed reasonable fraction of the current window width/height.
2. **Tab / Filter Content Overflow:** The internal tabs and rule list in `filter_dock.rs` already utilize flex columns and scrollable viewports; verify they flex properly with dynamic sizes.
3. **Double Click Reset:** (Optional Polish) Double clicking the splitter resets width/height to the default preset (`310.0px` / `220.0px`).

---

## 3. Test & Verification Plan
1. **Unit / Integration Tests:**
   - Verify workspace state clamping for width (`[180.0, 800.0]`) and height (`[100.0, 600.0]`).
   - Verify delta math produces correct output when moving left/right and up/down.
2. **Compilation & Build:**
   - Run `cargo check -p loganalysis-gui` to ensure zero compilation or borrow checker errors.
   - Run existing test suites (`cargo test`).
3. **Manual Verification Criteria:**
   - Left dock can be resized smoothly by dragging right border.
   - Bottom dock can be resized smoothly by dragging top border.
   - Dock collapse / toggle (`Ctrl+J`) still functions seamlessly without layout distortion.

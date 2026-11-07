# Technical Specification: Bookmark Annotation Drawer & Inline Notes (`P5`)

**Feature:** Bookmark Annotation Drawer & Inline Notes  
**Module:** `crates/loganalysis-core/src/export/` & `crates/loganalysis-gui/`  
**Date:** 2026-11-06  
**Status:** **APPROVED FOR IMPLEMENTATION**  

---

## 1. Overview & Problem Statement

In **P4**, we introduced basic bookmarking ($F2$) and offline HTML incident report generation. However:
1. When debugging incidents across thousands of lines, engineers need to attach **triage notes** (e.g., `"Root cause null pointer"`, `"ANR input dispatch timeout"`, `"Suspicious retry storm"`) directly to bookmarked lines.
2. Navigating through multiple bookmarks via cyclic $F2$/$\text{Shift}+F2$ requires cycling sequentially; engineers need a **unified overview drawer/modal** to inspect all bookmarks at a glance and jump directly to any bookmark.
3. In the exported HTML report, notes are already styled (`<div class="bookmark-note"><span class="note-icon">📝</span> ...</div>`), so allowing note editing in the GUI immediately makes exported incident reports complete and actionable.

---

## 2. Core Engine Enhancements (`crates/loganalysis-core/src/export/bookmark.rs`)

### 2.1 API Additions
```rust
impl BookmarkManager {
    /// Retrieves a reference to the bookmark at `source_idx`.
    pub fn get(&self, source_idx: usize) -> Option<&Bookmark>;

    /// Updates the note of an existing bookmark at `source_idx`.
    /// Automatically trims leading/trailing whitespace and normalizes empty strings to None.
    /// Returns true if updated, false if source_idx was not bookmarked.
    pub fn set_note(&mut self, source_idx: usize, note: Option<String>) -> bool;
}
```

---

## 3. UI/UX Architecture (`crates/loganalysis-gui/`)

### 3.1 Bookmark Drawer Modal (`crates/loganalysis-gui/src/modals/bookmark_drawer.rs`)
- **Keybinding & Palette Trigger:**
  - `Ctrl+B` / `Cmd+B` toggles the drawer.
  - Palette command: `Bookmark: Toggle Bookmarks Drawer`.
  - Status bar click: Clicking the `🔖 {N} bm` badge toggles the drawer.
- **Drawer Components:**
  - **Header:** `🔖 Bookmarks ({N})` with count, `Export HTML...` button, `Clear All` button, and `✕` close button.
  - **Live Search Filter:** Optional query to filter across bookmarked lines and annotations.
  - **Bookmark Card / Row:**
    - Line number (`#124`).
    - Severity badge (`ERROR`, `WARN`, `MATCH`, `INFO`).
    - Log line text snippet.
    - Note display (`📝 {note}`) or `[+ Add Note]` button.
    - Quick actions: `Jump (Enter)`, `Edit Note`, `Delete (Del)`.
- **Keyboard Navigation:**
  - `↑` / `↓`: Select previous / next bookmark.
  - `Enter`: Jump to line in log view & close drawer.
  - `Delete` / `Backspace`: Remove selected bookmark.
  - `Esc`: Close drawer.

### 3.2 Inline Note Editor Modal (`crates/loganalysis-gui/src/modals/note_edit_modal.rs`)
- **Trigger:**
  - `Alt+Enter` on any bookmarked row in log view.
  - Clicking `[+ Add Note]` / `Edit Note` in the Bookmark Drawer.
  - Command Palette: `Bookmark: Edit Note on Selected Line`.
- **Layout:**
  - Clean floating modal card with title `📝 Bookmark Note (Line {N})`.
  - Monospace snippet of log statement.
  - `TextInput` focused with existing note text.
  - Buttons: `Save Note (Enter)`, `Clear Note`, `Cancel (Esc)`.

---

## 4. Verification Plan

1. **Unit Tests (`loganalysis-core`):**
   - Verify `get()` returns correct bookmark.
   - Verify `set_note()` updates note, handles `None`, and trims whitespace.
   - Verify HTML report output includes updated notes.
2. **GUI Integration Tests (`loganalysis-gui`):**
   - Verify `Ctrl+B` keybinding and action `ToggleBookmarkDrawer`.
   - Verify `EditBookmarkNote` action.
   - Verify drawer commands registered in Command Palette.
   - Verify 100% test pass rate across workspace.

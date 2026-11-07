# Technical Specification: Dual-Pane Split View & Synced Scrolling (`P7`)

**Feature:** Dual-Pane Split View & Timestamp-Synchronized Lockstep Scrolling  
**Module:** `crates/loganalysis-core/src/buffer/correlation.rs` & `crates/loganalysis-gui/`  
**Date:** 2026-11-06  
**Status:** **APPROVED FOR IMPLEMENTATION**  

---

## 1. Overview & Objective

When debugging distributed failures, client-server handshake drops, microservice anomalies, or multi-device Bluetooth/IPC logs, developers must inspect two log streams concurrently.

**LogAnalysis P7** implements:
1. **Dual-Pane Split View:**
   - Single View $\leftrightarrow$ Split View toggle (`Ctrl+\` / `Cmd+\`).
   - Split Orientation toggle (Vertical Side-by-Side vs Horizontal Stacked via `Ctrl+Alt+\`).
   - Secondary pane independent log file loading (`Open Log in Secondary Pane...`).
2. **Lockstep Synchronized Timestamp Scrolling:**
   - Synchronizes scrolling and selection across Pane A and Pane B based on microsecond timestamps.
   - When scrolling/clicking a timestamped line in Pane A, Pane B automatically finds the nearest chronologically corresponding line in $O(\log N)$ time and centers it, and vice versa.
   - Synced indicator badge (`🔗 Synced` vs `Unlinked`) in the splitter header bar with one-click toggle (`Ctrl+Alt+S`).
3. **Independent Gutter, Selection & Metrics:**
   - Each pane maintains independent selection, line numbering, and hit styles.

---

## 2. Core Correlation Engine (`crates/loganalysis-core/src/buffer/correlation.rs`)

### 2.1 API Specification
```rust
/// Finds the visible row index in `source` whose timestamp is closest to `target_micros`.
/// Uses logarithmic binary search over visible lines with graceful fallback.
pub fn find_nearest_timestamp_row(
    source: &dyn LogSource,
    visible_indices: &[usize],
    target_micros: i64,
) -> Option<usize>;
```

### 2.2 Complexity & Invariants
- Time Complexity: $O(\log N)$ timestamp extractions over visible rows.
- Resilience: Gracefully handles lines without timestamps by local linear probe or skipping.

---

## 3. UI/UX Architecture (`crates/loganalysis-gui`)

### 3.1 Workspace State Additions
```rust
pub(crate) is_split_view: bool,
pub(crate) is_split_vertical: bool, // true = Left | Right, false = Top / Bottom
pub(crate) is_scroll_synced: bool,  // true = Lockstep timestamp synchronization
pub(crate) secondary_log_source: Option<Arc<ActiveLogSource>>,
pub(crate) secondary_file_name: Option<SharedString>,
pub(crate) secondary_filter_result: Option<Arc<FilterResult>>,
pub(crate) secondary_scroll_handle: UniformListScrollHandle,
pub(crate) secondary_selection: SelectionModel,
```

### 3.2 Actions & Keybindings
- `ToggleSplitView`: `Ctrl+\` / `Cmd+\`
- `ToggleSplitOrientation`: `Ctrl+Alt+\`
- `ToggleSyncedScroll`: `Ctrl+Alt+S`
- `OpenSecondaryFile`: Command Palette & Splitter button

---

## 4. Verification Plan

1. **Core Unit Tests (`loganalysis-core`):**
   - Exact timestamp match correlation.
   - Nearest neighbor interpolation for timestamps falling between lines.
   - Non-timestamped log fallback safety.
2. **GUI Tests (`loganalysis-gui`):**
   - Split view toggle and orientation state checks.
   - Sync scroll toggle and command palette lookup.
   - 100% test pass rate across workspace.

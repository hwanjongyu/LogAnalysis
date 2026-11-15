# Phase 3 Architectural & Remediation Specification

**Status:** Approved Scope Blueprint (Phase 3 Remediation + Bonus Synergies)  
**Target Branch:** `master`  
**Author:** Software Architect  
**Scope Sources:** `docs/review-report.md` (Phase 3 Plan & Remediation Matrix), `docs/INVARIANTS.md`  
**Preservation Guarantee:** All Phase 2 deliverables (`docs/spec-phase2-remediation.md`, `docs/qa-phase2-remediation.md`, and Phase 2 source modifications) MUST be preserved intact.

---

## 1. Architectural Constraints, Dependency Direction & Protected Invariants

Phase 3 eliminates remaining architectural schisms, hot-loop CPU/allocation bottlenecks, test-in-binary coupling, and CI/tooling blind spots while preserving zero-panic determinism and strict unidirectional layering.

### 1.1 Dependency Direction & Layer Boundaries
1. **`loganalysis-core` (`crates/loganalysis-core/`)**:
   - Pure domain, filtering, indexing, stream buffering, search, analytics, and export library.
   - MUST maintain **zero** dependency on `gpui`, `rfd`, OS windowing, or UI state.
   - New dependency permitted: `parking_lot = "0.12"` for `SharedStreamBuffer`.
2. **`loganalysis-gui` Library (`crates/loganalysis-gui/src/lib.rs`)**:
   - Owns all application modules (`actions`, `cli`, `components`, `controllers`, `dialogs`, `logging`, `modals`, `selection`, `theme`, `types`, `workspace`).
   - `controllers/` (`adb`, `bookmarks`, `search`, `split_view`) MUST remain **pure headless Rust** with zero `gpui::Entity<T>` or `gpui::ScrollHandle` ownership so all controller unit tests execute without instantiating a GPUI runtime (`[INV]` headless testability). `controllers/split_view.rs` MAY continue using `gpui::SharedString` only if already present in `types.rs` snapshots (`SplitViewState`), without GPUI async/window contexts.
   - GPUI handles (`find_input: Entity<TextInput>`, `note_input: Entity<TextInput>`, `secondary_scroll_handle: ScrollHandle`, `secondary_selection: SelectionModel`, `secondary_filter_cancel_gen: Arc<AtomicU64>`) stay directly on `LogAnalysisWorkspace`.
3. **`loganalysis-gui` Binary (`crates/loganalysis-gui/src/main.rs`)**:
   - Thin executable entrypoint containing **only** `use loganalysis_gui::*;` and `fn main()` (desktop integration setup + GPUI window creation).
   - **Critical GPUI Rule:** `src/main.rs` MUST NEVER declare `mod actions;` (or any other `mod ...;` owned by `lib.rs`). Duplicate module compilation across bin and lib crates assigns distinct `std::any::TypeId` values to `gpui::actions!` types, causing silent shortcut failures or runtime dispatch panics.
4. **`crates/loganalysis-gui/tests/gui_integration_tests.rs`**:
   - External integration tests depend solely on the public API exported from `loganalysis_gui` (`use loganalysis_gui::*;`). Zero `include_str!` source-text inspections are permitted anywhere under `crates/`.

### 1.2 Protected Domain & Behavioral Invariants
- **`[INV-01-B]` Escape Dismissal Priority:** `handle_escape` MUST enforce exact precedence: Device Selector -> Note Modal -> Command Palette -> Bookmark Drawer -> Saved Search -> Active Search Bar -> Zen Mode -> `None`.
- **`[INV-05]` Sorted Visible Indices:** `FilterResult.visible_indices` is strictly ascending (`0..source.line_count()`), making `slice::binary_search` unconditionally valid and equivalent to linear search (`MIN-07`).
- **`[INV-09]` Filter Rule Count Alignment:** Active flat `FilterRule` ordering in `CompiledStreamFilters` MUST map 1:1 to `FilterResult.filter_counts` and sync `FilterResult.rule_counts` identically across initial filtering, streaming append, and ring-buffer trimming.
- **`MAJ-02` Controller Test Contract Preservation:**
  - `SplitViewController::new()` MUST retain `is_split_vertical: false` (verified by `test_split_view_controller_toggles`). `LogAnalysisWorkspace::new()` explicitly sets `split_view.is_split_vertical = true` to preserve the existing vertical-split GUI startup default.
  - `SearchController::update_matcher(&mut self)` MUST retain its unconditional non-empty-query compilation behavior (verified by `test_search_controller_lifecycle`). Workspace search caching MUST call a separate visibility-gated method `SearchController::update_matcher_when_visible(&mut self)`.
- **`MAJ-07` Zero-Lock Borrow & Snapshot Isolation:** `ActiveLogSource::Stream(StreamLogBuffer)` MUST retain $O(1)$ zero-lock `get_raw_line(&self, idx: usize) -> Option<&[u8]>` borrowing `&self`, while `StreamLogBuffer::clone()` MUST provide full Copy-on-Write (CoW) snapshot isolation (`test_stream_log_buffer_in_place_arc_get_mut_and_fallback`) in $O(\text{pages})$ rather than $O(\text{lines})$.

---

## 2. Detailed Remediation Specifications

---

### 2.1 `MAJ-02`: Resolve Architectural Schism & Decompose `LogAnalysisWorkspace`

#### Problem & Trade-offs
Phase 2 created well-tested headless controllers (`AdbController`, `BookmarkController`, `SearchController`, `SplitViewController`) and snapshot structs (`types.rs`), yet `LogAnalysisWorkspace` retained 29+ flat fields duplicating that state, forcing four `#[allow(dead_code)]` exemptions in `types.rs` and leaving controller logic disconnected from production execution.  
**Trade-off:** Embed all 4 controllers directly on `LogAnalysisWorkspace` and keep only GPUI handles (`Entity<TextInput>`, `ScrollHandle`, `SelectionModel`, `secondary_filter_cancel_gen`) on the workspace struct. Adding `Deref<Target = BookmarkManager>` + `DerefMut` on `BookmarkController` avoids rewriting `BookmarkManager` method call sites while routing all bookmark drawer/note UI state cleanly through `self.bookmarks`.

#### Exact File Changes & Signatures

1. **`crates/loganalysis-gui/src/types.rs`**
   - **Remove all 4 `#[allow(dead_code)]` attributes** immediately above:
     - `pub struct SplitViewState`
     - `pub struct AdbStreamState`
     - `pub struct SearchState`
     - `pub struct BookmarkDrawerState`
   - Retain all public fields on those 4 structs verbatim so external callers and snapshot assertions preserve compatibility.

2. **`crates/loganalysis-gui/src/controllers/adb.rs`**
   - Add snapshot accessor on `AdbController`:
     ```rust
     impl AdbController {
         pub fn state(&self) -> crate::types::AdbStreamState {
             crate::types::AdbStreamState {
                 selected_device: self.selected_device.clone(),
                 is_device_selector_open: self.is_device_selector_open,
                 is_scanning_adb: self.is_scanning,
                 is_streaming: self.is_streaming,
                 is_stream_paused: self.is_paused,
                 auto_scroll: self.auto_scroll,
                 paused_stream_line_count: self.paused_line_count,
             }
         }
     }
     ```
   - Ensure all fields currently accessed by workspace (`devices`, `device_details`, `selected_device`, `is_device_selector_open`, `is_scanning`, `stream_handle`, `is_streaming`, `is_paused`, `auto_scroll`, `paused_buffer`, `paused_line_count`) are `pub` within crate (`pub` already present; keep `pub` so `workspace` and integration tests can access cleanly).

3. **`crates/loganalysis-gui/src/controllers/bookmarks.rs`**
   - Implement `Deref` and `DerefMut` to `BookmarkManager`:
     ```rust
     impl std::ops::Deref for BookmarkController {
         type Target = loganalysis_core::BookmarkManager;

         fn deref(&self) -> &Self::Target {
             &self.manager
         }
     }

     impl std::ops::DerefMut for BookmarkController {
         fn deref_mut(&mut self) -> &mut Self::Target {
             &mut self.manager
         }
     }
     ```
   - Add snapshot accessor:
     ```rust
     impl BookmarkController {
         pub fn state(&self) -> crate::types::BookmarkDrawerState {
             crate::types::BookmarkDrawerState {
                 is_bookmark_drawer_open: self.is_drawer_open,
                 bookmark_drawer_selected_index: self.drawer_selected_index,
                 is_note_modal_open: self.is_note_modal_open,
                 note_editing_source_idx: self.note_editing_source_idx,
             }
         }
     }
     ```
   - **Inherent Method Shadowing Edge Case:** `BookmarkController::clear(&mut self)` clears `self.manager.clear()` AND resets `self.drawer_selected_index = 0`. `reset_derived_log_state` and `clear_logs` calling `self.bookmarks.clear()` inherit this reset automatically, and MUST additionally close `self.bookmarks.is_drawer_open = false`, `self.bookmarks.is_note_modal_open = false`, `self.bookmarks.note_editing_source_idx = None` where appropriate (e.g., `clear_logs`). Meanwhile `BookmarkController::toggle_bookmark` does not shadow `BookmarkManager::toggle`.

4. **`crates/loganalysis-gui/src/controllers/search.rs`**
   - **Preserve `pub fn update_matcher(&mut self)` unchanged** (`!self.query.is_empty()` trigger) so `test_search_controller_lifecycle` continues passing.
   - Add visibility-gated matcher compiler for workspace use:
     ```rust
     impl SearchController {
         /// Compiles `cached_matcher` only when search UI is visible and trimmed query is non-empty.
         /// Clears `cached_matcher` when hidden, empty, whitespace-only, or invalid regex.
         pub fn update_matcher_when_visible(&mut self) {
             if !self.is_visible || self.query.trim().is_empty() {
                 self.cached_matcher = None;
                 return;
             }
             self.cached_matcher = loganalysis_core::CompiledMatcher::compile(
                 &self.query,
                 self.is_regex,
                 self.is_case_sensitive,
             )
             .ok()
             .map(std::sync::Arc::new);
         }

         pub fn state(&self) -> crate::types::SearchState {
             crate::types::SearchState {
                 search_query: self.query.clone(),
                 is_search_visible: self.is_visible,
                 is_search_regex: self.is_regex,
                 is_search_case_sensitive: self.is_case_sensitive,
                 search_match_status: self.match_status.clone(),
             }
         }
     }
     ```

5. **`crates/loganalysis-gui/src/controllers/split_view.rs`**
   - **Preserve `SplitViewController::new()` setting `is_split_vertical: false`** so `test_split_view_controller_toggles` passes untouched.
   - Add snapshot accessor:
     ```rust
     impl SplitViewController {
         pub fn state(&self) -> crate::types::SplitViewState {
             crate::types::SplitViewState {
                 is_split_view: self.is_split_view,
                 is_split_vertical: self.is_split_vertical,
                 is_scroll_synced: self.is_scroll_synced,
                 secondary_file_name: self
                     .secondary_file_name
                     .as_ref()
                     .map(|name| gpui::SharedString::from(name.clone())),
             }
         }
     }
     ```

6. **`crates/loganalysis-gui/src/workspace/mod.rs` (`LogAnalysisWorkspace`)**
   - Replace the 29 loose fields with 4 controller fields (keeping GPUI handle fields directly on `LogAnalysisWorkspace`):
     - `pub(crate) adb: crate::controllers::AdbController`
     - `pub(crate) bookmarks: crate::controllers::BookmarkController`
     - `pub(crate) search_ctrl: crate::controllers::SearchController`
     - `pub(crate) split_view: crate::controllers::SplitViewController`
     - Retained GPUI/async workspace fields: `pub(crate) find_input: Entity<TextInput>`, `pub(crate) note_input: Entity<TextInput>`, `pub(crate) secondary_filter_cancel_gen: Arc<AtomicU64>`, `pub(crate) secondary_scroll_handle: ScrollHandle`, `pub(crate) secondary_selection: SelectionModel`.
   - In `LogAnalysisWorkspace::new(...)`:
     ```rust
     let mut split_view = crate::controllers::SplitViewController::new();
     split_view.is_split_vertical = true; // Preserve GUI default vertical split
     ```
     Initialize `adb: AdbController::new()`, `bookmarks: BookmarkController::new()`, `search_ctrl: SearchController::new()`, `split_view`.
   - Expose workspace snapshot methods:
     ```rust
     impl LogAnalysisWorkspace {
         pub fn adb_state(&self) -> crate::types::AdbStreamState {
             self.adb.state()
         }

         pub fn bookmark_drawer_state(&self) -> crate::types::BookmarkDrawerState {
             self.bookmarks.state()
         }

         pub fn search_state(&self) -> crate::types::SearchState {
             self.search_ctrl.state()
         }

         pub fn split_view_state(&self) -> crate::types::SplitViewState {
             self.split_view.state()
         }
     }
     ```
   - Import/reference `AdbController, BookmarkController, SearchController, SplitViewController` directly inside `crates/loganalysis-gui/src/workspace/` so `grep -rnE "AdbController|BookmarkController|SearchController|SplitViewController" crates/loganalysis-gui/src/workspace/` returns all 4 matches.

7. **Complete Field Migration Mapping Across `crates/loganalysis-gui/src/`**

   | Legacy `LogAnalysisWorkspace` Field | New Controller Field Path |
   |---|---|
   | `adb_devices` | `adb.devices` |
   | `adb_device_details` | `adb.device_details` |
   | `selected_device` | `adb.selected_device` |
   | `is_device_selector_open` | `adb.is_device_selector_open` |
   | `is_scanning_adb` | `adb.is_scanning` |
   | `adb_stream_handle` | `adb.stream_handle` |
   | `is_streaming` | `adb.is_streaming` |
   | `is_stream_paused` | `adb.is_paused` |
   | `auto_scroll` | `adb.auto_scroll` |
   | `paused_stream_buffer` | `adb.paused_buffer` |
   | `paused_stream_line_count` | `adb.paused_line_count` |
   | `bookmarks` (`BookmarkManager`) | `bookmarks` (`BookmarkController` via `Deref`/`DerefMut`) |
   | `is_bookmark_drawer_open` | `bookmarks.is_drawer_open` |
   | `bookmark_drawer_selected_index` | `bookmarks.drawer_selected_index` |
   | `is_note_modal_open` | `bookmarks.is_note_modal_open` |
   | `note_editing_source_idx` | `bookmarks.note_editing_source_idx` |
   | `search_query` | `search_ctrl.query` |
   | `is_search_visible` | `search_ctrl.is_visible` |
   | `is_search_regex` | `search_ctrl.is_regex` |
   | `is_search_case_sensitive` | `search_ctrl.is_case_sensitive` |
   | `cached_search_matcher` | `search_ctrl.cached_matcher` |
   | `search_cancel_gen` | `search_ctrl.cancel_gen` |
   | `search_match_status` | `search_ctrl.match_status` |
   | `is_split_view` | `split_view.is_split_view` |
   | `is_split_vertical` | `split_view.is_split_vertical` |
   | `is_scroll_synced` | `split_view.is_scroll_synced` |
   | `secondary_log_source` | `split_view.secondary_log_source` |
   | `secondary_file_name` | `split_view.secondary_file_name` |
   | `secondary_filter_result` | `split_view.secondary_filter_result` |
   | `secondary_is_filtering` | `split_view.secondary_is_filtering` |

   **Affected Files to Update:**
   - `crates/loganalysis-gui/src/workspace/{mod.rs, adb.rs, bookmarks.rs, search.rs, selection.rs, split_view.rs, merged.rs, command_palette.rs}`
   - `crates/loganalysis-gui/src/lib.rs` (migrated `impl Render` & root handlers from `main.rs`)
   - `crates/loganalysis-gui/src/components/{activity_bar.rs, find_bar.rs, log_view.rs, status_bar.rs, toolbar.rs}`
   - `crates/loganalysis-gui/src/modals/{bookmark_drawer.rs, note_edit_modal.rs}`

---

### 2.2 `MAJ-06`: Eliminate Hot-Loop Automaton Recompilation

#### Problem & Trade-offs
Every live ADB batch (10–20Hz) and snapshot reconciliation calls `append_streaming_batch`, rebuilding `Vec<PreparedFilter>` (including regex & `memmem` compilation) and `MultiFilterMatcher` (Aho-Corasick DFA construction) from scratch on every tick.  
**Trade-off:** Introduce `CompiledStreamFilters` owning the prepared rules + prebuilt `MultiFilterMatcher`, cache `Option<Arc<CompiledStreamFilters>>` on `LogAnalysisWorkspace`, and keep the legacy `append_streaming_batch` free function as a 1-call wrapper for backward API compatibility.

#### Exact File Changes & Signatures

1. **`crates/loganalysis-core/src/filter/engine.rs`**
   - Add matcher-reusing streaming batch function:
     ```rust
     pub fn append_streaming_batch_with_matcher(
         result: &mut FilterResult,
         source: &(dyn LogSource + Sync),
         new_lines_range: std::ops::Range<usize>,
         trimmed_count: usize,
         prepared: &[PreparedFilter],
         matcher: &MultiFilterMatcher,
         show_only_filtered: bool,
     )
     ```
     **Exact Behavior & Edge Cases:**
     1. If `result.filter_counts.len() != prepared.len()`, resize `result.filter_counts` to `prepared.len()` (filling `0` on grow, truncating on shrink).
     2. If `trimmed_count > 0`:
        - Partition `result.visible_indices` at the first index `>= trimmed_count`; decrement counts in `result.filter_counts` for every trimmed visible row via `result.row_matched_filters[0..split]`, drain `0..split` from `visible_indices`, `row_styles`, and `row_matched_filters`, then shift surviving `visible_indices` down by `trimmed_count` (`*idx -= trimmed_count`).
        - Decrement `result.total_lines = source.line_count()` (or match existing post-trim calculation cleanly).
     3. If `new_lines_range.is_empty()`: update `result.total_lines = source.line_count()` and return early.
     4. For each `line_idx` in `new_lines_range`: fetch `source.get_raw_line(line_idx)` (skip if `None`), call `evaluate_and_style_line_with_matcher_full(raw, prepared, matcher, show_only_filtered, Some(&mut result.filter_counts))`, and when `visible` push `line_idx`, `style`, and `matched_indices` into `result`.
     5. Set `result.total_lines = source.line_count()`.

   - Add `CompiledStreamFilters` struct:
     ```rust
     pub struct CompiledStreamFilters {
         rules: Vec<FilterRule>,
         prepared: Vec<PreparedFilter>,
         matcher: MultiFilterMatcher,
     }

     impl std::fmt::Debug for CompiledStreamFilters {
         fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
             f.debug_struct("CompiledStreamFilters")
                 .field("rules_len", &self.rules.len())
                 .field("prepared_len", &self.prepared.len())
                 .finish()
         }
     }

     impl CompiledStreamFilters {
         pub fn new(filters: &[FilterRule]) -> Self {
             let rules = filters.to_vec();
             let prepared: Vec<PreparedFilter> = rules
                 .iter()
                 .filter_map(PreparedFilter::new)
                 .collect();
             let matcher = MultiFilterMatcher::new(&prepared);
             Self { rules, prepared, matcher }
         }

         pub fn prepared(&self) -> &[PreparedFilter] {
             &self.prepared
         }

         pub fn matcher(&self) -> &MultiFilterMatcher {
             &self.matcher
         }

         pub fn rules(&self) -> &[FilterRule] {
             &self.rules
         }

         pub fn append_streaming_batch(
             &self,
             result: &mut FilterResult,
             source: &(dyn LogSource + Sync),
             new_lines_range: std::ops::Range<usize>,
             trimmed_count: usize,
             show_only_filtered: bool,
         ) {
             append_streaming_batch_with_matcher(
                 result,
                 source,
                 new_lines_range,
                 trimmed_count,
                 &self.prepared,
                 &self.matcher,
                 show_only_filtered,
             );
             // Maintain [INV-09] rule_counts parity on both trim and append paths
             result.rule_counts = self
                 .rules
                 .iter()
                 .zip(result.filter_counts.iter())
                 .map(|(r, &c)| (r.id, c))
                 .collect();
         }
     }
     ```
     *Note on `[INV-09]` Index Alignment:* Callers MUST construct `CompiledStreamFilters::new(&active_filters)` using the exact active `FilterRule` slice passed to `execute_filtering` so `filter_counts` indices and `self.rules` align identically even if an active rule has an uncompilable pattern. (Preserve exact `filter_map(PreparedFilter::new)` behavior of existing `append_streaming_batch` so invalid regex rules do not cause regressions.)

   - Refactor existing `append_streaming_batch` free function into a thin backward-compatible wrapper:
     ```rust
     pub fn append_streaming_batch(
         result: &mut FilterResult,
         source: &(dyn LogSource + Sync),
         new_lines_range: std::ops::Range<usize>,
         trimmed_count: usize,
         filters: &[FilterRule],
         show_only_filtered: bool,
     ) {
         CompiledStreamFilters::new(filters).append_streaming_batch(
             result,
             source,
             new_lines_range,
             trimmed_count,
             show_only_filtered,
         );
     }
     ```

2. **`crates/loganalysis-core/src/filter/mod.rs` & `crates/loganalysis-core/src/lib.rs`**
   - Re-export `CompiledStreamFilters`, `append_streaming_batch_with_matcher`, and `evaluate_and_style_line_with_matcher_full` from `filter/mod.rs` and at crate root `loganalysis_core::*`.

3. **`crates/loganalysis-gui/src/workspace/{mod.rs, adb.rs}`**
   - Add field `pub(crate) cached_stream_filters: Option<Arc<loganalysis_core::CompiledStreamFilters>>` to `LogAnalysisWorkspace` (initialized to `None` in `new()`).
   - Add helper on `LogAnalysisWorkspace`:
     ```rust
     pub(crate) fn rebuild_cached_stream_filters(&mut self) -> Arc<loganalysis_core::CompiledStreamFilters> {
         let active_filters = self.collect_active_flat_filters();
         let compiled = Arc::new(loganalysis_core::CompiledStreamFilters::new(&active_filters));
         self.cached_stream_filters = Some(compiled.clone());
         compiled
     }

     pub(crate) fn ensure_cached_stream_filters(&mut self) -> Arc<loganalysis_core::CompiledStreamFilters> {
         if let Some(cached) = &self.cached_stream_filters {
             return cached.clone();
         }
         self.rebuild_cached_stream_filters()
     }
     ```
     *(Where `collect_active_flat_filters` extracts the existing active-filter flattening currently repeated across `reapply_filters` and `adb.rs`.)*
   - In `LogAnalysisWorkspace::reapply_filters`: call `let compiled = self.rebuild_cached_stream_filters();` before spawning the background filter task; pass/use `compiled` in `reconcile_snapshot_filter_result` (passing `compiled: &CompiledStreamFilters` instead of `&[FilterRule]` or resolving via `self.ensure_cached_stream_filters()`).
   - In `workspace/adb.rs` live batch ingestion: call `let compiled = self.ensure_cached_stream_filters();` once per batch (outside any inner line iteration) and invoke `compiled.append_streaming_batch(...)`. Never recompile `MultiFilterMatcher` on steady-state ADB ticks unless filter rules changed (`reapply_filters` / invalidated cache).

---

### 2.3 `MAJ-07`: Prevent Stream Buffer Clone Thrashing

#### Problem & Trade-offs
At 200,000 stream lines, cloning `StreamLogBuffer` for background analytics (`compute_analytics_snapshot`) or fallback when `Arc::get_mut` yields `None` executes `VecDeque<Arc<str>>::clone()`, causing 200,000 atomic refcount increments plus a `~3.2 MB` heap copy on the UI thread.  
**Trade-off:** Partition `StreamLogBuffer` into fixed-size CoW chunked pages (`STREAM_PAGE_SIZE = 1024`) stored in `VecDeque<Arc<Vec<Arc<str>>>>`, AND expose `SharedStreamBuffer(Arc<parking_lot::RwLock<StreamBufferInner>>)`. Keeping `ActiveLogSource::Stream(StreamLogBuffer)` holding `StreamLogBuffer` directly preserves zero-lock $O(1)$ `get_raw_line(&self, idx: usize) -> Option<&[u8]>` borrows while reducing `clone()` cost at 200,000 lines from 200,000 atomic increments to **~196 page `Arc` increments**.

#### Exact File Changes & Signatures

1. **`crates/loganalysis-core/Cargo.toml`**
   - Under `[dependencies]`, add:
     ```toml
     parking_lot = "0.12"
     ```

2. **`crates/loganalysis-core/src/buffer/stream_buffer.rs`**
   - Constants & Types:
     ```rust
     pub const STREAM_PAGE_SIZE: usize = 1024;

     #[derive(Clone)]
     pub struct StreamLogBuffer {
         pages: VecDeque<Arc<Vec<Arc<str>>>>,
         head_offset: usize,
         len: usize,
         max_capacity: usize,
         trim_batch: usize,
         max_line_len: usize,
         longest_line_text: String,
         total_trimmed: usize,
     }

     /// Alias for the underlying chunked stream buffer managed by [`SharedStreamBuffer`].
     pub type StreamBufferInner = StreamLogBuffer;

     /// Thread-safe shared wrapper supporting concurrent reads/appends and O(pages) CoW snapshots.
     #[derive(Clone)]
     pub struct SharedStreamBuffer(Arc<parking_lot::RwLock<StreamBufferInner>>);
     ```

   - **Core Page Indexing Invariants & Edge Cases (`StreamLogBuffer`)**:
     - **Page Occupancy Invariant:** Whenever `!pages.is_empty()`, every non-tail page `pages[0..pages.len() - 1]` MUST have `page.len() == STREAM_PAGE_SIZE`, and `head_offset < pages[0].len()`. When `pages.is_empty()`, `head_offset == 0` and `len == 0`.
     - **$O(1)$ Direct Indexing (`get_line_arc` / `get_raw_line`)**:
       ```rust
       fn get_line_arc(&self, idx: usize) -> Option<&Arc<str>> {
           if idx >= self.len {
               return None;
           }
           let physical = self.head_offset + idx;
           let page_idx = physical / STREAM_PAGE_SIZE;
           let in_page = physical % STREAM_PAGE_SIZE;
           self.pages.get(page_idx).and_then(|page| page.get(in_page))
       }

       pub fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
           self.get_line_arc(idx).map(|s| s.as_bytes())
       }
       ```
     - **`append_chunk(&mut self, lines: Vec<String>) -> (std::ops::Range<usize>, usize)`**:
       - Capture `start_idx_before_trim = self.len`. (Match existing return contract: check whether existing `append_chunk` returns `(new_lines_range_after_trim, trimmed)` — preserve exact existing semantics of `(start_idx..end_idx, trimmed)`!).
       - For each incoming line (converted via `Arc::<str>::from(...)`), strip trailing `\r`/`\n` identically to existing implementation and track `max_line_len` / `longest_line_text`.
       - Push into tail page:
         - If `self.pages.is_empty()` or `self.pages.back().is_some_and(|p| p.len() == STREAM_PAGE_SIZE)`: push `Arc::new(Vec::with_capacity(STREAM_PAGE_SIZE))`.
         - Call `Arc::make_mut(self.pages.back_mut().expect("tail page exists"))` and push the new `Arc<str>`.
         - **Isolation Guarantee:** If a snapshot `StreamLogBuffer` shares the current tail page, `Arc::make_mut` clones at most `<= 1023` `Arc<str>` elements of that single tail page once; all prior `pages[0..N-1]` remain immutable and untouched across all snapshots, guaranteeing `test_stream_log_buffer_in_place_arc_get_mut_and_fallback` passes with full snapshot isolation.
       - Increment `self.len += 1`.
       - Execute `let trimmed = self.trim_if_needed();` and compute adjusted range identically to existing `append_chunk`.
     - **`trim_if_needed(&mut self) -> usize`**:
       - Only trim when `self.len > self.max_capacity` and `let excess = self.len - self.max_capacity; excess >= self.trim_batch` (preserving batch hysteresis!).
       - Before mutating pages, scan only the trimmed prefix `0..excess` (via `get_line_arc` or front page slices) to detect `longest_was_trimmed`: whether any trimmed line has `line.len() == self.max_line_len` (preserve exact longest-line tracking semantics).
       - Pop/advance `remaining = excess`:
         ```rust
         let mut remaining = excess;
         while remaining > 0 {
             let front_len = self.pages.front().map_or(0, |p| p.len());
             let avail = front_len.saturating_sub(self.head_offset);
             if avail == 0 {
                 break;
             }
             if remaining >= avail {
                 self.pages.pop_front();
                 self.head_offset = 0;
                 remaining -= avail;
             } else {
                 self.head_offset += remaining;
                 remaining = 0;
             }
         }
         self.len -= excess;
         self.total_trimmed += excess;
         if longest_was_trimmed {
             self.recalculate_max_line();
         }
         ```
       - **Critical Edge Case (`recalculate_max_line`)**: Reset `max_line_len = 0` and `longest_line_text.clear()`, then iterate active lines starting strictly at `self.head_offset` on `pages[0]` and `0..` on `pages[1..]` so logically trimmed lines in `pages[0][0..head_offset]` are NEVER counted toward `max_line_len`!
     - **`clear(&mut self)`**:
       - `self.pages.clear(); self.head_offset = 0; self.len = 0; self.max_line_len = 0; self.longest_line_text.clear(); self.total_trimmed = 0;`
     - **Preserve all existing public getters & `LogSource` impl:** `len()`, `is_empty()`, `total_trimmed()`, `max_capacity()`, `trim_batch()`, `max_line_len()`, `longest_line_text()`, `get_line(idx)`, `iter_lines()` / `LogSource::line_count`, `LogSource::get_raw_line`, `LogSource::max_line_len`, `LogSource::longest_line_text`.

   - **`SharedStreamBuffer` Methods:**
     ```rust
     impl SharedStreamBuffer {
         pub fn new(inner: StreamBufferInner) -> Self {
             Self(Arc::new(parking_lot::RwLock::new(inner)))
         }

         pub fn with_capacity(max_capacity: usize, trim_batch: usize) -> Self {
             Self::new(StreamLogBuffer::new(max_capacity, trim_batch))
         }

         pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, StreamBufferInner> {
             self.0.read()
         }

         pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, StreamBufferInner> {
             self.0.write()
         }

         /// Returns an isolated CoW snapshot by cloning only page Arcs (O(len / 1024)).
         pub fn snapshot(&self) -> StreamLogBuffer {
             self.0.read().clone()
         }

         pub fn append_chunk(&self, lines: Vec<String>) -> (std::ops::Range<usize>, usize) {
             self.0.write().append_chunk(lines)
         }

         pub fn line_count(&self) -> usize {
             self.0.read().len()
         }
     }

     impl Default for SharedStreamBuffer {
         fn default() -> Self {
             Self::new(StreamLogBuffer::default())
         }
     }
     ```

3. **`crates/loganalysis-core/src/buffer/mod.rs` & `crates/loganalysis-core/src/lib.rs`**
   - Re-export `STREAM_PAGE_SIZE`, `SharedStreamBuffer`, `StreamBufferInner`, and `StreamLogBuffer`.

---

### 2.4 `MAJ-11`: Purge Test Bloat from `main.rs` & Eliminate `include_str!` Source Checks

#### Problem & Trade-offs
`crates/loganalysis-gui/src/main.rs` exceeds 1,750 lines because `#[cfg(test)] mod tests` (lines 617–1759, 18 tests) lives directly inside the binary root, and 8 tests across `main.rs` (7) and `components/text_input.rs` (1) use brittle `include_str!` regex/substring assertions against source files rather than testing compiled types and runtime behavior.  
**Trade-off:** Add a `[lib]` target (`loganalysis_gui`) exporting contract constants and pure layout/visibility predicates, slim `main.rs` strictly to `use loganalysis_gui::*;` + `fn main()`, move all 18 tests to `tests/gui_integration_tests.rs`, and replace every `include_str!` check with real compiled behavioral/contract tests.

#### Exact File Changes & Signatures

1. **`crates/loganalysis-gui/Cargo.toml`**
   ```toml
   [lib]
   name = "loganalysis_gui"
   path = "src/lib.rs"

   [[bin]]
   name = "loganalysis"
   path = "src/main.rs"
   ```

2. **`crates/loganalysis-gui/src/lib.rs`**
   - Export modules cleanly:
     ```rust
     pub mod actions;
     pub mod cli;
     pub mod components;
     pub mod controllers;
     pub mod dialogs;
     pub mod logging;
     pub mod modals;
     pub mod selection;
     pub mod theme;
     pub mod types;
     pub mod workspace;

     pub use workspace::LogAnalysisWorkspace;
     ```
   - Move `impl Render for LogAnalysisWorkspace`, `ensure_desktop_integration()`, `pub fn detect_initial_window_decorations(...)`, `pub fn run_app()` (or window boot function called by `main()`), and extract pure, directly testable UI/keybinding contract helpers:
     ```rust
     pub fn workspace_shows_activity_bar(is_zen_mode: bool) -> bool {
         !is_zen_mode
     }

     pub fn workspace_shows_status_bar(is_zen_mode: bool) -> bool {
         !is_zen_mode
     }

     pub fn workspace_shows_filter_dock(is_zen_mode: bool, is_filter_dock_visible: bool) -> bool {
         !is_zen_mode && is_filter_dock_visible
     }

     /// Returns keystroke -> action name contract entries used when binding GPUI global shortcuts.
     pub fn default_keybinding_specs() -> &'static [(&'static str, &'static str)] { ... }
     ```
     Use `workspace_shows_activity_bar`, `workspace_shows_status_bar`, `workspace_shows_filter_dock`, and `default_keybinding_specs()` (building actual `KeyBinding`s from a central binding registration helper) inside `lib.rs` so production layout/bindings and integration tests share a single source of truth.

3. **`crates/loganalysis-gui/src/main.rs`**
   - Strip all `mod ...;` declarations, `impl Render`, tests (`lines 617–1759`), and `include_str!` references.
   - Retain **only** `use loganalysis_gui::*;` and `fn main() { loganalysis_gui::run_app(); }` (or equivalent `main()` calling `ensure_desktop_integration()` and GPUI `Application::new()` boot from `loganalysis_gui`).
   - **Verify:** `main.rs` MUST contain **0** `mod ` declarations and **0** `#[test]` functions.

4. **Replace All 8 `include_str!` Checks (`grep -rn "include_str!" crates/` == 0)**

   | # | Legacy `include_str!` Location | Replacement Contract / Export | Verification Test |
   |---|---|---|---|
   | 1 | `components/text_input.rs:1240` (`test_text_element_paint_has_no_unwrap_panics`) | Add `impl PrepaintState { pub fn into_paint_parts(self) -> Option<(ShapedLine, Option<PaintQuad>, Option<PaintQuad>, Pixels)> }` returning `None` when `self.line.is_none()`; `TextElement::paint` calls `let Some((line, cursor, selection, _)) = prepaint.into_paint_parts() else { return; };` | `test_prepaint_state_missing_line_returns_none_without_panic` verifies `PrepaintState { line: None, .. }.into_paint_parts().is_none()` cleanly in `text_input.rs` |
   | 2 | `main.rs:1457` (`toolbar.rs` DOM IDs) | Export `pub const HEADER_CONTRACT_DOM_IDS: [&str; 16]` in `components/toolbar.rs` and reference those constants in `render_unified_header` `.id(...)` calls | `test_unified_compact_header_spec_and_dom_ids` checks array len == 16, non-empty, unique, plus `UNIFIED_HEADER_HEIGHT == 38.0` and `LOGO_PILL_SIZE == 18.0` |
   | 3 | `main.rs:1458` (`titlebar.rs` DOM IDs) | Export `pub const WINDOW_CONTROL_DOM_IDS: [&str; 4]` in `components/titlebar.rs` and reference those constants in `render_window_controls` `.id(...)` calls | `test_unified_compact_header_spec_and_dom_ids` checks array len == 4 (total 20 unique IDs) and `WINDOW_CLOSE_HOVER_COLOR == 0xe81123` |
   | 4 | `main.rs:1621` (`actions.rs` 45 actions & `ToggleZenMode`) | In `actions.rs`, export `pub const REGISTERED_ACTION_COUNT: usize = 45;` and `pub fn all_action_names() -> Vec<&'static str>` built from actual `gpui::Action::name(&...)` calls (including `ToggleZenMode`) | Test asserts `REGISTERED_ACTION_COUNT == 45`, `all_action_names().len() == 45`, uniqueness, and `all_action_names().contains(&gpui::Action::name(&actions::ToggleZenMode))` |
   | 5 | `main.rs:1651` (`workspace/mod.rs` defaults) | In `workspace/mod.rs`, export `pub struct WorkspaceDefaults { pub is_timeline_visible: bool, pub is_zen_mode: bool }` and `pub const WORKSPACE_DEFAULTS: WorkspaceDefaults = WorkspaceDefaults { is_timeline_visible: false, is_zen_mode: false };` consumed by `LogAnalysisWorkspace::new()` | Test asserts `!WORKSPACE_DEFAULTS.is_timeline_visible && !WORKSPACE_DEFAULTS.is_zen_mode` |
   | 6 | `main.rs:1690` (`workspace/command_palette.rs` `ToggleZenMode`) | Extract pure `pub fn palette_command_action_name(id: PaletteCommandId) -> &'static str` (used by command execution/telemetry) and inspect `get_palette_commands()` | Test verifies `PaletteCommandId::ToggleZenMode` is registered in `get_palette_commands()` and maps via `palette_command_action_name` to `gpui::Action::name(&actions::ToggleZenMode)` |
   | 7 | `main.rs:1697` (`components/timeline_sparkline.rs`) | Extract `pub fn should_render_timeline_ribbon(is_timeline_visible: bool, is_zen_mode: bool) -> bool { is_timeline_visible && !is_zen_mode }` consumed by `render_timeline_sparkline` | Test verifies all 4 boolean pairs `(false,false)->false`, `(true,false)->true`, `(true,true)->false`, `(false,true)->false` |
   | 8 | `main.rs:1709` (`lib.rs` zen-mode suppression, `ctrl-shift-z`, Escape) | Use exported `workspace_shows_activity_bar`, `workspace_shows_status_bar`, `workspace_shows_filter_dock`, `default_keybinding_specs()`, and `workspace::determine_escape_action(...)` | Test verifies zen mode hides bars/dock, `"ctrl-shift-z"` binds to `ToggleZenMode`, and Escape returns `EscapeAction::ExitZenMode` when zen mode is active with no higher-priority overlay |

5. **`crates/loganalysis-gui/tests/gui_integration_tests.rs`**
   - Migrate all 18 tests formerly at `main.rs:617-1759` into `tests/gui_integration_tests.rs`, updating items #2–#8 to execute pure behavioral/contract assertions with zero `include_str!`.

---

### 2.5 `MIN-07` & `MIN-08` — Bookmark Binary Search & Selection Bounds

1. **`MIN-07` (`crates/loganalysis-gui/src/workspace/bookmarks.rs`)**
   - Extract $O(\log N)$ lookup helper:
     ```rust
     /// Finds the visible row index corresponding to `target_source_idx`.
     /// Precondition `[INV-05]`: `visible_indices` is strictly sorted ascending.
     pub fn find_visible_row_for_source_idx(
         visible_indices: &[usize],
         target_source_idx: usize,
     ) -> Option<usize> {
         visible_indices.binary_search(&target_source_idx).ok()
     }
     ```
   - Replace all 3 `.iter().position(|&idx| idx == target_source_idx)` scans in `jump_next_bookmark`, `jump_prev_bookmark`, and `jump_to_bookmark_source_idx` with `find_visible_row_for_source_idx(&self.filter_result.visible_indices, target_source_idx)`.
   - Edge cases: empty slice -> `None`, hidden/filtered-out bookmark -> `None`, first element / last element -> exact `Some(row)`.

2. **`MIN-08` (`crates/loganalysis-gui/src/selection.rs`, `workspace/selection.rs`, `workspace/split_view.rs`, `components/log_view.rs`)**
   - Place shared pure bounds helper in `crates/loganalysis-gui/src/selection.rs` (respecting downward dependency from `workspace/` and `components/`):
     ```rust
     /// Safely resolves a visible row selection to its underlying `source_idx`.
     /// Returns `None` if `selected_row` is `None` or out of bounds of `visible_indices`.
     pub fn resolve_selected_source_idx(
         visible_indices: &[usize],
         selected_row: Option<usize>,
     ) -> Option<usize> {
         selected_row.and_then(|row| visible_indices.get(row).copied())
     }
     ```
   - Remove every `.visible_indices.get(...).copied().unwrap_or(0)` fallback:
     - `crates/loganalysis-gui/src/workspace/selection.rs` (`copy_selection`, `add_quick_filter_from_selection`): guard empty/out-of-bounds via `resolve_selected_source_idx` and `get(row).copied()` loops — return cleanly without reading or copying line `0` when invalid.
     - `crates/loganalysis-gui/src/components/log_view.rs:828` (in `render_log_list`): use `if let Some(source_idx) = crate::selection::resolve_selected_source_idx(&res.visible_indices, Some(idx))` to read the line, else default to `(0, SharedString::default())` without ever reading line `0` from `log_source` for an out-of-bounds row.
     - `crates/loganalysis-gui/src/workspace/split_view.rs:244, 270`: guard with `let Some(source_idx) = crate::selection::resolve_selected_source_idx(&self.split_view.secondary_filter_result.visible_indices, Some(row)) else { return; };`.

---

### 2.6 `MIN-05`, `MIN-06`, `MIN-04` — DRY Consolidations

1. **`MIN-05` — Deduplicate Escape Handling (`workspace/mod.rs` & `lib.rs`)**
   - Add method on `LogAnalysisWorkspace` in `crates/loganalysis-gui/src/workspace/mod.rs`:
     ```rust
     /// Handles `Escape` according to `[INV-01-B]` dismissal priority.
     /// Returns `true` if any overlay, modal, search bar, or zen mode was dismissed.
     pub fn handle_escape(&mut self, cx: &mut Context<Self>) -> bool
     ```
   - Both the root `.on_key_down` listener in `crates/loganalysis-gui/src/lib.rs` and `LogAnalysisWorkspace::handle_key_down` in `workspace/mod.rs` delegate `Key::Escape` directly to `self.handle_escape(cx)`.

2. **`MIN-06` — Unify Stream Analytics Async Dispatch (`workspace/adb.rs`)**
   - Extract consolidated helper in `crates/loganalysis-gui/src/workspace/adb.rs`:
     ```rust
     impl LogAnalysisWorkspace {
         /// Spawns background heatmap/facet/EPS analytics calculation on a CoW snapshot
         /// when either analytics view is visible, dropping stale completions via `analytics_cancel_gen`.
         pub(crate) fn spawn_and_apply_stream_analytics(&mut self, cx: &mut Context<Self>)
     }
     ```
   - Replace the 3 duplicated analytics snapshot-and-spawn blocks in `workspace/adb.rs` (periodic live stream flush, `stop_adb_stream`, and `toggle_stream_pause` resume) with calls to `self.spawn_and_apply_stream_analytics(cx)`. Also inspect `update_analytics_data_async` in `workspace/mod.rs`; share the snapshot-and-spawn pattern if compatible without altering sync vs async caller semantics.
   - Invariant: `self.analytics_cancel_gen` generation MUST be bumped before spawning and verified before mutating `heatmap_data`, `facets_data`, or `eps_data` inside `cx.spawn`.

3. **`MIN-04` — Fix `$WINDOWID` Fragility & Deduplicate Native Dialog Handling (`dialogs.rs`)**
   - **`get_active_x11_window_id()` & Parsing (`crates/loganalysis-gui/src/dialogs.rs`)**:
     - Stop reading `$WINDOWID` (which points to the user's parent terminal window rather than the GPUI application window, causing native dialogs to attach to or hide behind the terminal).
     - Read explicit `LOGANALYSIS_X11_WINDOW_ID` via a pure helper:
       ```rust
       pub fn parse_x11_window_id(raw: Option<&str>) -> Option<u64> {
           raw.map(str::trim)
               .filter(|s| !s.is_empty())
               .and_then(|s| s.parse::<u64>().ok())
               .filter(|&id| id != 0)
       }

       pub fn get_active_x11_window_id() -> Option<u64> {
           parse_x11_window_id(std::env::var("LOGANALYSIS_X11_WINDOW_ID").ok().as_deref())
       }
       ```
   - **Shared Dialog Helpers (`crates/loganalysis-gui/src/dialogs.rs`)**:
     ```rust
     /// Attaches active OS parent window handle (X11/Win32/macOS where applicable) to an `rfd::AsyncFileDialog`.
     pub fn prepare_dialog_with_parent(dialog: rfd::AsyncFileDialog) -> rfd::AsyncFileDialog

     /// Awaits an async file dialog operation cleanly on the active executor context.
     pub async fn resolve_dialog_for_display<T, Fut>(dialog_fut: Fut) -> Option<T>
     where
         Fut: std::future::Future<Output = Option<T>>,
     ```
   - Migrate **all 6** dialog call sites to use `prepare_dialog_with_parent` + `resolve_dialog_for_display`:
     1. `workspace/mod.rs` (open primary file)
     2. `workspace/merged.rs` (save merged log)
     3. `workspace/filters.rs` (import presets)
     4. `workspace/filters.rs` (export presets)
     5. `workspace/split_view.rs` (open secondary file)
     6. `workspace/bookmarks.rs` (export bookmarks)

---

### 2.7 `INF-04` & `INF-06` — Doc Hygiene & CI Security/Compliance Gates

1. **`INF-04` — Fix Unclosed HTML Tag in Rustdoc (`crates/loganalysis-core/src/buffer/mod.rs:142`)**
   - Change bare `Arc<str>` in doc comment on line 142 to backticked `` `Arc<str>` ``.
   - Gate: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` MUST exit with status `0`.

2. **`INF-06` — CI Doc, Vulnerability & License Policy Gates (`.github/workflows/ci.yml`, `deny.toml`)**
   - Update `.github/workflows/ci.yml` after `Cargo Test`:
     1. **Cargo Doc step:**
        ```yaml
        - name: Cargo Doc
          run: RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
        ```
     2. **Install & Run Security/License Checks** (note: `cargo-audit` / `cargo-deny` use prebuilt release binaries via `taiki-e/install-action@v2` across Linux/macOS/Windows CI runners — or `cargo-audit` + `cargo-deny` Linux job if scoping audit to Linux to avoid redundant crates.io downloads across 3 OSes; keeping on at least `ubuntu-22.04` or all matrix legs):
        ```yaml
        - name: Install Audit & Deny Tools
          if: runner.os == 'Linux'
          uses: taiki-e/install-action@v2
          with:
            tool: cargo-audit,cargo-deny

        - name: Cargo Audit
          if: runner.os == 'Linux'
          run: cargo audit

        - name: Cargo Deny
          if: runner.os == 'Linux'
          run: cargo deny check
        ```
        *(Running audit/deny once on `ubuntu-22.04` validates the shared `Cargo.lock` and workspace manifests without tripling network fetch latency on macOS/Windows.)*
   - Create root **`/home/drew/github/hwanjongyu/loganalysis/deny.toml`** (modern `cargo-deny` config tailored to verified `cargo metadata` dependency licenses):
     ```toml
     [advisories]
     ignore = []

     [licenses]
     # Workspace crates currently omit `license` in Cargo.toml; ignore private/unpublished workspace packages cleanly
     private = { ignore = true }
     allow = [
         "MIT",
         "Apache-2.0",
         "Apache-2.0 WITH LLVM-exception",
         "BSD-2-Clause",
         "BSD-3-Clause",
         "CC0-1.0",
         "ISC",
         "MPL-2.0",
         "Unicode-3.0",
         "NCSA",
         "Zlib",
         "0BSD",
         "Unlicense",
     ]

     [bans]
     multiple-versions = "allow"
     wildcards = "allow"
     highlight = "all"

     [sources]
     unknown-registry = "deny"
     unknown-git = "warn"
     allow-registry = ["https://github.com/rust-lang/crates.io-index"]
     ```
     *Note:* All transitive dependency licenses verified via `cargo metadata` (`MPL-2.0`, `Unicode-3.0`, `NCSA`, permissive BSD/MIT/Apache/ISC/Zlib/CC0/0BSD/Unlicense; dual-licensed `Apache-2.0 OR GPL-2.0-only` and `MIT OR Apache-2.0 OR LGPL-2.1-or-later` are satisfied via `Apache-2.0`/`MIT`). Add `allow-git` entries for any `Cargo.lock` git repositories before tightening `unknown-git = "deny"`. When `cargo-audit` / `cargo-deny` binaries are absent locally, validate CI YAML/TOML syntax locally and enforce full execution on the CI runner.

---

### 2.8 Bonus Synergies (`MIN-02`, `MIN-09`, `MIN-11`, `INF-02`)

1. **`MIN-02` — Unify `ASYNC_SEARCH_THRESHOLD`**
   - **`crates/loganalysis-core/src/constants.rs`:** Update `pub const ASYNC_SEARCH_THRESHOLD: usize = 50_000;` (replacing `5_000`).
   - **`crates/loganalysis-gui/src/workspace/search.rs:9`:** Remove duplicate private `const ASYNC_SEARCH_THRESHOLD: usize = 50_000;` and use `loganalysis_core::ASYNC_SEARCH_THRESHOLD`.

2. **`MIN-09` — Replace `.when(opt.is_some(), ...unwrap())` with `.when_some(...)`**
   - Refactor all 3 GPUI element builder sites cleanly using `gpui::prelude::FluentBuilder::when_some`:
     - `crates/loganalysis-gui/src/components/status_bar.rs:267` (`.when_some(status_opt, |el, status| ...)`)
     - `crates/loganalysis-gui/src/modals/bookmark_drawer.rs:181` (`.when_some(b.note.clone(), |el, note| ...)`)
     - `crates/loganalysis-gui/src/components/activity_bar.rs:85` (`.when_some(badge, |el, badge_text| ...)`)

3. **`MIN-11` — Bound Single-Character Severity Keyword Scans in `heatmap.rs`**
   - In **`crates/loganalysis-core/src/filter/heatmap.rs`**:
     - Inside `has_error_keyword(bytes: &[u8])`: restrict single-char token checks (`b" E "`, `b" F "`, `b"E/"`, `b"F/"`) strictly to `let prefix = &bytes[..bytes.len().min(35)];`. Keep full-line scans for multi-character tokens (`b"ERROR"`, `b"FATAL"`, `b"Exception"`, `b"CRITICAL"`).
     - Inside `has_warn_keyword(bytes: &[u8])`: restrict single-char token checks (`b" W "`, `b"W/"`) strictly to `let prefix = &bytes[..bytes.len().min(35)];`. Keep full-line scans for multi-character tokens (`b"WARN"`, `b"WARNING"`).
     - **Rationale:** Log payloads/stack URLs past byte 35 frequently contain `" W "` or `" E "` substrings (e.g., identifiers, coordinates), causing false-positive amber/red spikes in the heatmap.

4. **`INF-02` — Canonical `format_epoch_utc` Shared by Core & GUI**
   - Move the existing pure-Rust calendar implementation `pub fn format_epoch_utc(secs: u64) -> String` from `crates/loganalysis-gui/src/logging.rs` into **`crates/loganalysis-core/src/buffer/timestamp.rs`** (and re-export as `pub use buffer::format_epoch_utc;` in `crates/loganalysis-core/src/buffer/mod.rs` and `crates/loganalysis-core/src/lib.rs`).
   - Update **`crates/loganalysis-core/src/export/html_report.rs`** (`chrono_timestamp_approx` / export timestamp calls) to call `crate::format_epoch_utc(secs)` instead of producing `"Epoch {secs}"`.
   - Update **`crates/loganalysis-gui/src/logging.rs`** to re-export/delegate `pub use loganalysis_core::format_epoch_utc;` so existing GUI log filename/line formatting and unit tests (`test_format_epoch_utc_epoch_zero`, `test_format_epoch_utc_known_date`) remain 100% preserved with zero duplicate math.

---

## 3. Deterministic Test Verification Matrix

| Item ID | Target File(s) | Deterministic Verification / Unit Test Case | Expected Behavior / Assertion |
|---|---|---|---|
| `MAJ-02` | `controllers/{adb,bookmarks,search,split_view}.rs`, `types.rs`, `workspace/*.rs` | `test_workspace_controller_snapshots_and_defaults` + existing 4 controller unit tests (`test_adb_controller_defaults_and_selection`, `test_bookmark_controller_lifecycle`, `test_search_controller_lifecycle`, `test_split_view_controller_toggles`) | `SplitViewController::new().is_split_vertical == false`; `BookmarkController` `Deref`/`DerefMut` forwards `.len()`, `.shift_on_trim()`, `.set_note()`; `update_matcher_when_visible()` clears matcher when `!is_visible` or whitespace-only; `adb_state()`, `search_state()`, `split_view_state()`, `bookmark_drawer_state()` match live controller mutations; zero `#[allow(dead_code)]` in `types.rs` |
| `MAJ-06` | `loganalysis-core/src/filter/engine.rs`, `workspace/{mod.rs,adb.rs}` | `test_compiled_stream_filters_parity_with_append_streaming_batch` & `test_compiled_stream_filters_trim_syncs_rule_counts` | `CompiledStreamFilters::append_streaming_batch` produces identical `visible_indices`, `row_styles`, `row_matched_filters`, `filter_counts`, `rule_counts`, and `total_lines` as legacy `append_streaming_batch` across append-only, trim-only, and append+trim batches |
| `MAJ-07` | `loganalysis-core/src/buffer/stream_buffer.rs` | `test_stream_log_buffer_cow_page_boundaries_and_trim` + `test_shared_stream_buffer_concurrent_snapshot_isolation` + existing `test_stream_log_buffer_in_place_arc_get_mut_and_fallback` | Pushes across multiple `STREAM_PAGE_SIZE (1024)` pages (e.g., 3,500 lines), clones snapshot, appends + trims partial & full pages (`head_offset` crossing page boundaries), asserts snapshot `len`/`get_raw_line`/`max_line_len` remain frozen while active buffer advances and `recalculate_max_line` ignores `[0..head_offset]` |
| `MAJ-11` | `loganalysis-gui/{Cargo.toml,src/lib.rs,src/main.rs}`, `tests/gui_integration_tests.rs`, `components/text_input.rs` | `cargo test -p loganalysis-gui --test gui_integration_tests` + `test_prepaint_state_missing_line_returns_none_without_panic` + CLI `grep -rn "include_str!" crates/` | All 18 migrated integration tests compile and pass outside `main.rs`; `main.rs` has zero `#[test]` / `mod actions;`; `HEADER_CONTRACT_DOM_IDS` + `WINDOW_CONTROL_DOM_IDS` total 20 unique IDs; `REGISTERED_ACTION_COUNT == 45`; `should_render_timeline_ribbon` tested on all 4 bool combinations; `grep -rn "include_str!" crates/` returns **0** matches |
| `MIN-07` | `workspace/bookmarks.rs` | `test_find_visible_row_for_source_idx_binary_search` | Tests empty slice (`None`), hidden source index (`None`), first/middle/last matches (`Some(row)`), and full parity against `.iter().position(...)` over sparse sorted indices |
| `MIN-08` | `selection.rs`, `workspace/{selection,split_view}.rs`, `components/log_view.rs` | `test_resolve_selected_source_idx_bounds` | Returns `None` for `None` selection, empty `visible_indices`, and out-of-bounds row indices (never falls back to `Some(0)`); returns exact `Some(source_idx)` when valid |
| `MIN-05` | `workspace/mod.rs`, `lib.rs` | Existing `test_determine_escape_action_priority` + `test_escape_priority_with_controllers` | Covers full `[INV-01-B]` chain cleanly with controller flags (`adb.is_device_selector_open`, `bookmarks.is_note_modal_open`, `bookmarks.is_drawer_open`, `search_ctrl.is_visible`, etc.) |
| `MIN-06` | `workspace/adb.rs` | `spawn_and_apply_stream_analytics` compilation + stale `analytics_cancel_gen` check | Single consolidated analytics dispatcher used by all 3 stream paths (`live batch`, `stop`, `resume`) without duplicate blocks |
| `MIN-04` | `dialogs.rs` + 6 dialog call sites | `test_parse_x11_window_id_ignores_terminal_windowid` | `LOGANALYSIS_X11_WINDOW_ID` unset/`""`/`"0"`/`"abc"` -> `None`; `" 4194305 "` -> `Some(4194305)`; verifies `$WINDOWID` has zero influence; `grep -rn '"WINDOWID"' crates/` == 0 |
| `INF-04` | `loganalysis-core/src/buffer/mod.rs` | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | Exits `0` with zero rustdoc HTML tag warnings |
| `INF-06` | `.github/workflows/ci.yml`, `deny.toml` | YAML/TOML structural check + `cargo doc` step validation | Valid TOML against workspace dependency licenses; CI workflow includes `Cargo Doc`, `Cargo Audit`, `Cargo Deny` |
| `MIN-02` | `loganalysis-core/src/constants.rs`, `workspace/search.rs` | `test_async_search_threshold_unified` | `loganalysis_core::ASYNC_SEARCH_THRESHOLD == 50_000`; zero duplicate `const ASYNC_SEARCH_THRESHOLD` inside `loganalysis-gui` |
| `MIN-09` | `status_bar.rs`, `bookmark_drawer.rs`, `activity_bar.rs` | `cargo clippy --workspace --all-targets -- -D warnings` | Zero `.when(...is_some(), ...unwrap())` patterns across `components/` and `modals/` |
| `MIN-11` | `loganalysis-core/src/filter/heatmap.rs` | `test_heatmap_single_char_tokens_restricted_to_prefix_35` | `"01-01 12:00:00.000  100  200 E Tag: ok"` -> Error; `"01-01 12:00:00.000  100  200 I Tag: payload beyond 35 bytes E fake"` -> Normal; `"payload beyond 35 bytes ERROR"` -> Error; equivalent boundary assertions for `W/` / ` W ` vs `WARN` |
| `INF-02` | `loganalysis-core/src/buffer/timestamp.rs`, `html_report.rs`, `logging.rs` | `test_format_epoch_utc_epoch_zero`, `test_format_epoch_utc_known_date`, `test_html_report_contains_utc_timestamp` | `format_epoch_utc(0)` and known-date assertions pass in `loganalysis-core` & `logging.rs`; HTML export renders formatted UTC timestamps rather than raw `"Epoch ..."` placeholders |

### Global Verification Gate Commands
Downstream `test-automation-engineer` and `reality-checker` MUST execute and report passing results for **every** gate below:

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Plus structural/hygiene assertions:
```bash
# Must return 0 matches (MAJ-11 & MIN-10)
grep -rn "include_str!" crates/

# Must match all 4 controllers in workspace/ (MAJ-02)
grep -rnE "AdbController|BookmarkController|SearchController|SplitViewController" crates/loganalysis-gui/src/workspace/

# Must return 0 matches for raw terminal WINDOWID env var (MIN-04)
grep -rn '"WINDOWID"' crates/

# Must return at most 1 definition of ASYNC_SEARCH_THRESHOLD in core constants.rs (MIN-02)
grep -rn "const ASYNC_SEARCH_THRESHOLD" crates/
```

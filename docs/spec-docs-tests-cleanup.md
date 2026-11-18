# Specification: Documentation Alignment, Test Strengthening & Code Deduplication (`docs-tests-cleanup`)

- **Target Workspace**: `/home/drew/github/hwanjongyu/loganalysis` (`v0.2.4`, `loganalysis-core` + `loganalysis-gui`)
- **Current Baseline**: `334/334` workspace tests passing (`234` in `loganalysis-core` across lib + 9 integration suites; `100` in `loganalysis-gui` across lib + 1 integration suite); zero `cargo bench` / Criterion targets in workspace.
- **Architectural Strategy & Trade-offs**:
  - **Pure Logic Extraction over Heavy UI Mocking**: Rather than enabling `gpui`'s `test-support` feature (which in `gpui 0.2.2` unconditionally pulls `wayland` and `x11` dependencies and breaks cross-platform CI on `macos-15` / `windows-latest`), extract deterministic pure functions and pure state helpers from GPUI view/workspace code so integration and unit tests invoke real production symbols directly. Trade-off: GPUI element layout/painting itself remains verified by compilation + `render_*` smoke coverage while all state transitions, clamping, highlighting range computation, facet mutations, telemetry formatting, and atomic file persistence are 100% exercised.
  - **Dependency-Safe Sequencing**: Execute **Pillar 3** (helper/constant extraction & dead-code cleanup) before or alongside **Pillar 2** (rewiring "Test Theater" tests to those extracted production APIs), then run QA verification and synchronize exact passing test counts into **Pillar 1** (`README.md`, `docs/INVARIANTS.md`).
  - **Preserved System Invariants**: All invariants in `docs/INVARIANTS.md` (`[INV-01]`–`[INV-15]`) remain binding and are updated only where documentation drifted from verified implementation.

---

## Pillar 1 — Documentation & Config Alignment (`문서 현행화`)

### `[P1-01]` Create Root `LICENSE`
- **Target**: `/home/drew/github/hwanjongyu/loganalysis/LICENSE` (new file)
- **Context**: `README.md:5, 130` links to `LICENSE`, and both `crates/loganalysis-core/Cargo.toml:5-6` and `crates/loganalysis-gui/Cargo.toml:5-6` declare `license = "MIT"` and `authors = ["Drew Yu"]`, yet no root `LICENSE` file exists.
- **Contract**:
  - Create `/home/drew/github/hwanjongyu/loganalysis/LICENSE` containing the standard MIT License text with `Copyright (c) 2026 Drew Yu`.

### `[P1-02]` Update `README.md`
- **Target**: `README.md`
- **Contracts**:
  1. **Test Badge (`README.md:6`)**:
     - Replace stale `[![Tests: Passing](https://img.shields.io/badge/Tests-45%2F45%20Passing-brightgreen.svg)](docs/qa-fix-p1-p2.md)` with a synchronized badge linking to `docs/qa-docs-tests-cleanup.md` (`Tests-<N>%2F<N>%20Passing-brightgreen.svg`).
     - **Mandatory Post-QA Sync Gate**: After Pillar 2 & Pillar 3 pass `cargo test --workspace`, update `<N>/<N>` in `README.md:6` (and `docs/INVARIANTS.md`) to match the exact observed pass count.
  2. **Engine Highlights & Performance Table (`README.md:27-47`)**:
     - Ground Key Highlights in actual implemented mechanisms:
       - `FileLogBuffer`: `memmap2` memory mapping with `memchr` line-offset indexing, `1024`-entry `RwLock` `Arc<str>` line cache, and `open_heap` fallback (`crates/loganalysis-core/src/buffer/file_buffer.rs:12-40, 121-157`).
       - `StreamLogBuffer`: `1024`-line Copy-on-Write (`Arc<Vec<Arc<str>>>`) pages (`STREAM_PAGE_SIZE = 1024`) capped at `MAX_MONITOR_LINES = 200_000` with `MONITOR_TRIM_BATCH = 20_000` hysteresis (`constants.rs:3-4`, `stream_buffer.rs:15, 27-37`).
       - Filtering & Matching Engine: `Aho-Corasick` multi-literal matching (`MultiFilterMatcher`), linear DFA `regex::bytes::Regex` (`REGEX_DFA_SIZE_LIMIT = 2MB`), AST-validated `fancy_regex` (`has_nested_repetition`, `FANCY_REGEX_BACKTRACK_LIMIT = 250`, SIMD required-literal pre-filter), and `rayon` parallel evaluation (`filter/matcher.rs:8-11, 160-199`, `filter/engine.rs`).
     - **Zero Unverified Benchmark Claims**: Zero Criterion / `cargo bench` benchmarks exist in the repository. Replace unverified numeric latency/RAM/binary-size benchmark claims (`< 100ms`, `~30MB – 60MB`, `~40ms – 80ms`, `< 50ms`, `120 FPS+`, `~15MB – 25MB`) with qualitative architectural design characteristics (or mark the comparison table explicitly as an **Architectural Design Comparison (Qualitative)** without fabricated benchmark numbers).
  3. **Architecture Tree (`README.md:54-69`)**:
     - Update `crates/loganalysis-core/src/` to reflect:
       - `lib.rs`, `constants.rs`, `error.rs`
       - `adb/` (`mod.rs`, `device.rs`, `logcat.rs`)
       - `buffer/` (`mod.rs`, `file_buffer.rs`, `stream_buffer.rs`, `merged_buffer.rs`, `column.rs`, `timestamp.rs`, `correlation.rs`)
       - `export/` (`mod.rs`, `bookmark.rs`, `html_report.rs`)
       - `filter/` (`mod.rs`, `model.rs`, `matcher.rs`, `engine.rs`, `contrast.rs`, `heatmap.rs`, `timeline.rs`)
       - `search/` (`mod.rs`, `searcher.rs`, `fuzzy.rs`)
     - Update `crates/loganalysis-gui/src/` to reflect:
       - `main.rs`, `lib.rs`, `actions.rs`, `cli.rs`, `dialogs.rs`, `logging.rs`, `selection.rs`, `theme.rs`, `types.rs`
       - `components/`, `controllers/`, `modals/`, `workspace/`
     - Reference `docs/INVARIANTS.md` and `docs/qa-docs-tests-cleanup.md` instead of only historical v1 docs.
  4. **Linux Ubuntu/Debian Prerequisites (`README.md:77-81`)**:
     - Align with `.github/workflows/ci.yml:41-49` plus Wayland/XCB dev libraries:
       ```bash
       sudo apt-get update
       sudo apt-get install -y build-essential pkg-config libvulkan-dev \
         libx11-xcb-dev libxkbcommon-x11-dev libxkbcommon-dev libasound2-dev \
         libssl-dev libfontconfig1-dev libxcb1-dev libwayland-dev
       ```
  5. **CLI Usage & Running Tests (`README.md:89-106`)**:
     - Document CLI flags & positional paths (`cli.rs:11-53`, `lib.rs:548-626`):
       - `loganalysis [OPTIONS] [--] [FILE...]`
       - `-h` / `--help` (prints help, exit `0`)
       - `-v` / `-V` / `--version` (prints `LogAnalysis <version>`, exit `0`)
       - Unknown flags exit with status `2`
       - `--` treats remaining arguments as file paths
       - Single path opens directly (`open_file`); 2+ paths open via chronological multi-file merge (`open_merged_files`).
     - Update `Running Tests` description (`README.md:102`) from "10 core architectural edge cases" to accurately describe the full `loganalysis-core` + `loganalysis-gui` workspace unit and integration test suites.
  6. **Keyboard Shortcuts (`README.md:110-125`)**:
     - Fix wrong bindings (`Ctrl+L` -> `Ctrl+Shift+O` for Load Filter Profile, `Space` -> `Ctrl+Space` for Pause/Resume ADB Stream, `Ctrl+S` saves current profile or prompts Save As if unsaved, `Ctrl+Shift+S` is Save Filters As).
     - List all bound shortcuts from `crates/loganalysis-gui/src/lib.rs:68-123`:
       - File & Merge: `Ctrl+O` (`OpenFile`), `Ctrl+Shift+M` / `Cmd+Shift+M` (`OpenMergedLogs`)
       - Search & Copy: `Ctrl+F` (`Find`), `F3` (`FindNext`), `Shift+F3` (`FindPrev`), `Ctrl+C` (`CopySelection`)
       - Filter Rules & Profiles: `Ctrl+H` (`ToggleFilteredOnly`), `Ctrl+Shift+F` (`AddFilter`), `Ctrl+S` (`SaveFilters`), `Ctrl+Shift+S` (`SaveFiltersAs`), `Ctrl+Shift+O` (`LoadFilters`), `Ctrl+T` (`AddFilterTab`), `Ctrl+D` (`DeleteFilterTab`), `Ctrl+R` (`RenameFilterTab`)
       - ADB & Logs: `Ctrl+Shift+R` / `Cmd+Shift+R` (`StartAdb`), `Ctrl+Space` (`TogglePauseStream`), `Ctrl+K` (`ClearLogs`)
       - Bookmarks: `F2` / `Ctrl+F2` (`ToggleBookmark`), `Shift+F2` (`PrevBookmark`), `Alt+F2` (`NextBookmark`), `Ctrl+B` / `Cmd+B` (`ToggleBookmarkDrawer`), `Alt+Enter` (`EditBookmarkNote`)
       - Views & Layout: `Ctrl+P` / `Cmd+P` / `Ctrl+Shift+P` / `Cmd+Shift+P` (`ToggleCommandPalette`), `Ctrl+J` / `Cmd+J` (`ToggleFilterDock`), `Ctrl+Shift+L` (`ToggleFilterPanelPosition`), `Alt+F` (`ToggleFacetsBar`), `Ctrl+Shift+T` / `Cmd+Shift+T` (`ToggleEpsTimeline`), `F11` / `Ctrl+Shift+Z` / `Cmd+Shift+Z` (`ToggleZenMode`), `Ctrl+\` / `Cmd+\` (`ToggleSplitView`), `Ctrl+Alt+\` (`ToggleSplitOrientation`), `Ctrl+Alt+S` (`ToggleSyncedScroll`), `Ctrl+G` / `Cmd+G` (`ToggleColumnView`), `Alt+Z` (`ToggleFullLineDisplay`), `Ctrl+U` / `Cmd+U` / `F10` (`ToggleTheme`)
       - Zoom: `Ctrl+=` / `Ctrl++` (`ZoomIn`), `Ctrl+-` (`ZoomOut`), `Ctrl+0` / `Cmd+0` (`ResetZoom`)

### `[P1-03]` Update `PROJECT.md`
- **Target**: `PROJECT.md`
- **Contracts**:
  1. **F11 Note (`PROJECT.md:24`)**: Clarify that `modals/command_palette.rs:413-415, 520-523` (`on_click` and `capture_key_down` Enter) pass `Some(window)` to `execute_palette_command`/`submit_command_palette`, whereas the `palette_input` `TextInputEvent::Submit` subscription fallback in `workspace/mod.rs:260-265` passes `None`.
  2. **Interface Contract 1 (`PROJECT.md:45-52`)**:
     - Rename `SingleLogBuffer` to `FileLogBuffer` (noting `pub type SingleLogBuffer = FileLogBuffer;` alias in `buffer/mod.rs:18`).
     - Document `TimestampKind` (`MonthDayTime`, `Iso8601`, `TimeOnly`), `ParsedTimestamp`, `extract_timestamp_with_kind`, and `normalize_month_day_rollover` across `FileLogBuffer` (`file_buffer.rs:246-255`), `StreamLogBuffer` (`stream_buffer.rs:275-284`), and `MergedLogBuffer` (`merged_buffer.rs:322-324`).
  3. **Interface Contract 2 (`PROJECT.md:53-56`)**:
     - Update `SelectionModel::shift_on_trim(&mut self, trimmed: usize)` to exact signature `SelectionModel::shift_on_trim(&mut self, dropped_visible_rows: usize, raw_trimmed_count: usize)` (`selection.rs:25`).
  4. **Interface Contract 3 (`PROJECT.md:57-58`)**:
     - Replace `analytics_seq` with `analytics_cancel_gen`, `search_ctrl.cancel_gen`, `primary_source_gen`, `SplitViewController::secondary_source_gen`, and synchronous pre-spawn `reset_derived_log_state()` (`workspace/mod.rs:446-455, 1038-1052`, `workspace/split_view.rs:94-103`).
  5. **Code Layout (`PROJECT.md:60-84`)**:
     - Update to list all current modules in `crates/loganalysis-core/src/` (`lib.rs`, `constants.rs`, `error.rs`, `adb/`, `buffer/`, `export/`, `filter/`, `search/`) and `crates/loganalysis-gui/src/` (`main.rs`, `lib.rs`, `actions.rs`, `cli.rs`, `dialogs.rs`, `logging.rs`, `selection.rs`, `theme.rs`, `types.rs`, `components/`, `controllers/`, `modals/`, `workspace/`).

### `[P1-04]` Update `CHANGELOG.md`
- **Target**: `CHANGELOG.md`
- **Contracts**:
  1. Add `## [Unreleased]` at top summarizing `fix-all-bugs` remediation, documentation alignment (`README.md`, `PROJECT.md`, `CHANGELOG.md`, `INVARIANTS.md`, `LICENSE`, `deny.toml`, `scripts/status_dashboard.sh`), test quality strengthening (replacing Test Theater with real production assertions, `contrast.rs` & `searcher.rs` coverage, `tempfile` hygiene, deterministic zero-sleep async/stream tests), and Pillar 3 deduplication/dead-code cleanup.
  2. Correct verified factual inaccuracies in historical entries:
     - `[0.2.4]` (`CHANGELOG.md:20`): `appimagetool` pinned to `1.9.1` (`scripts/build-appimage.sh:67`), not `13`.
     - `[0.2.3]` (`CHANGELOG.md:43`): `analytics_cancel_gen` (`workspace/mod.rs:53`), not `analytics_seq`.
     - `[0.2.3]` (`CHANGELOG.md:44`): `workspace.rs` split into `src/workspace/` and `src/controllers/` modules — remove false `(each < 1,000 lines)` claim (`workspace/mod.rs` is > 1,000 lines).
     - `[0.2.1]` (`CHANGELOG.md:89`): heatmap coalescing in `render_log_list` (`components/log_view.rs:1216-1290`), not `render_scrollbar_track`.
     - `[0.2.1]` (`CHANGELOG.md:90`): clarify `EpsTimelineData::compute` is gated behind `is_timeline_visible` during live ADB streaming (`workspace/adb.rs:36, 58, 518`) and `toggle_eps_timeline`, whereas `reapply_filters` (`workspace/mod.rs:552-557`) computes it eagerly.
     - `[0.2.0]` (`CHANGELOG.md:103`): `MergedLogBuffer` interleaves via stable `entries.sort_by_key(...)` (`merged_buffer.rs:229`), not a priority heap. (Also align onboarding card text in `components/log_view.rs:1751` from `"⚡ k-way Priority Heap"` to `"⚡ Chronological Stable Sort"`.)
     - `[0.2.0]` (`CHANGELOG.md:104`): Structured Column Grid View uses fixed-width columns (`components/log_view.rs:685-774`), not sortable columns.
     - `[0.2.0]` (`CHANGELOG.md:106`): Bookmark shortcuts are `F2` / `Ctrl+F2` toggle, `Shift+F2` previous, `Alt+F2` next (`lib.rs:94-97`), not `F2 / Shift+F2` cyclic navigation.
     - `[0.2.0]` (`CHANGELOG.md:110`): Built-in quick filter presets live in `modals/filter_edit_modal.rs:560-570` (`ERROR`, `WARN`, `INFO`, `DEBUG`, `TRACE`, `FATAL`, `SUCCESS`, `ActivityTaskManager`, `System`); no `examples/` automotive profile files exist in the repository.
     - `[0.1.1]` (`CHANGELOG.md:123`): CLI flags include `--version`, `-V`, and `-v` (`cli.rs:46`); version badges appear in the status bar (`status_bar.rs:364`) and onboarding view (`log_view.rs:1575`), not the CSD titlebar.

### `[P1-05]` Update `docs/INVARIANTS.md`, `deny.toml`, and `scripts/status_dashboard.sh`
- **Target 1**: `docs/INVARIANTS.md`
  - `[INV-02-A]` (`:28`): Update `search_cancel_gen` -> `search_ctrl.cancel_gen` (`controllers/search.rs:12`) and note independent `analytics_cancel_gen` (`workspace/mod.rs:53`) and `secondary_filter_cancel_gen` (`workspace/mod.rs:103`).
  - `[INV-05]` (`:55-57`): Document `--help`/`-h`, `--version`/`-V`/`-v`, `UnknownFlag` exit code `2` (`cli.rs:44-49`, `lib.rs:548-562`), multi-file CLI path args merging via `open_merged_files` (`lib.rs:618-626`), and version badges in status bar (`status_bar.rs:364`) and onboarding view (`log_view.rs:1575`) (removing "CSD titlebar" reference).
  - `[INV-06]` (`:63-66`): Include `open_merged_files` alongside `open_file`, drag-and-drop, and `clear_logs` via `begin_primary_source_transition` (`workspace/mod.rs:446-455`), plus monotonic generation checks `primary_source_gen` / `SplitViewController::secondary_source_gen`.
  - `[INV-07]` (`:69`): Update timeout from 3 seconds to `2` seconds (`workspace/adb.rs:114`: `Duration::from_secs(2)`).
  - `[INV-10]` (`:100`): Document `atomic_replace_file` (`MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` on Windows; `std::fs::rename` on Unix in `workspace/filters.rs:29-68`), shared by both filter profiles and recent files (`workspace/recent_files.rs:128-145`).
  - `[INV-11]` (`:105`): Fix theme switching shortcut from `<kbd>Ctrl+T</kbd>` to `<kbd>Ctrl+U</kbd>` / `<kbd>F10</kbd>` (`lib.rs:119-121`).
  - `[INV-13]` (`:112`): Correct `probe_parent_timestamp` (`workspace/split_view.rs:9-21`) to **10-line backward** probe (not forward probe).
  - Section 11 Verification Checklist (`:122-128`): Align with CI & packaging gates (`cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` with post-QA synchronized `<N>/<N>` count, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, `cargo audit`, `cargo deny check`, `./scripts/build-appimage.sh`, `./scripts/build-deb.sh`).
- **Target 2**: `deny.toml:19`
  - Replace stale comment `# Workspace crates currently omit license in Cargo.toml; ignore private/unpublished workspace packages cleanly` with `# Both workspace crates declare license = "MIT"; private = { ignore = true } skips unpublished workspace packages cleanly`.
- **Target 3**: `scripts/status_dashboard.sh`
  - Remove stale `t10` / `hc get t10` / `spec-fix-p0-v4.md` / `workspace.rs:382` / `32/32 PASSED` references; display live `git status -s`, recent commits (`git log -5 --oneline`), current workspace crate inventory, and clean live verification commands (`cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`).

---

## Pillar 2 — Test Quality Strengthening (`테스트 품질 보강`)

### `[P2-01]` Replace "Test Theater" Tests with Real Production Calls & Assertions
1. **`crates/loganalysis-gui/tests/gui_integration_tests.rs`**:
   - `test_ui_ux_log_level_color_presets_validity` (`:7-35`):
     - Delete hardcoded local `presets` array; import `loganalysis_gui::modals::filter_edit_modal::QUICK_FILTER_PRESETS`.
     - Assert non-empty `QUICK_FILTER_PRESETS`, non-empty labels/patterns, valid `parse_color_spec(bg, true)`, and valid `parse_color_spec(fg, false)` for every preset entry.
   - `test_render_highlighted_text_zero_allocation` (`:469-487`):
     - Call `loganalysis_gui::components::log_view::compute_highlight_ranges` and assert exact byte ranges on `"2026-11-06 12:00:00 Normal log line"`:
       - `&None` -> `Vec::<Range<usize>>::new()`
       - `"Normal"` matcher -> `vec![20..26]`
       - `"NonExistentPattern"` matcher -> `Vec::<Range<usize>>::new()`
     - Keep calling `render_highlighted_text` across all 3 cases to verify compilation & return path.
   - `test_recent_files_lru_and_persistence` (`:506-546`):
     - Delete local `add_rec` closure and inline serde check; import `push_recent_path`, `save_recent_files_to_path`, and `load_recent_files_from_path` from `loganalysis_gui::workspace`.
     - Use `tempfile::TempDir` with real files on disk (`log1.txt`..`log6.txt` plus a nonexistent path), verifying:
       - LRU ordering, duplicate promotion to index `0`, and cap at `5` entries.
       - Atomic persistence via `save_recent_files_to_path` followed by `load_recent_files_from_path` (returning existing paths, filtering deleted/nonexistent paths, and safely returning `Vec::new()` on malformed JSON).
   - `test_phase3_workstation_facets_and_telemetry` (`:549-608`):
     - Delete local tag-counting and `format_bytes` reimplementations; call production `compute_facet_tags`, `apply_facet_filter_to_tab`, and `format_source_memory_display` / `format_byte_size` from `loganalysis_gui::workspace`.
     - Build a `tempfile::NamedTempFile` log, load `ActiveLogSource::File(FileLogBuffer::open(...))`, verify `compute_facet_tags` ranks `"ActivityManager"` `#1` with count `3` out of `3` distinct tags.
     - Test `apply_facet_filter_to_tab(&mut tab, "ActivityManager", false)`:
       - Initial add inserts active include rule and sets `tab.modified == true`.
       - Reset `tab.modified = false`, call again (`is_exclude = false`) -> toggles `active` to `false` AND sets `tab.modified == true`.
       - Reset `tab.modified = false`, call with `is_exclude = true` -> switches rule to `exclude == true`, `bg_color == "None"`, `text_color == "None"`, `active == true`, AND sets `tab.modified == true`.
     - Verify `format_source_memory_display` across `None` (`("0 MB", "Idle")`), `ActiveLogSource::File` (`"MMap"`), `ActiveLogSource::Stream` (`"Heap"`), and `ActiveLogSource::Merged` (`"Merged"`), plus `format_byte_size` KB/MB/GB boundaries.
     - Keep command palette shortcut assertions (`ToggleFilterDock` -> `"Ctrl+J"`, `ToggleFacetsBar` -> `"Alt+F"`).
   - `test_resizable_filter_dock_clamping_and_reset` (`:611-631`):
     - Delete local `let default_width = ...` / `let clamp_width = ...`; import `DEFAULT_FILTER_DOCK_WIDTH`, `DEFAULT_FILTER_DOCK_HEIGHT`, `clamp_filter_dock_width`, `clamp_filter_dock_height`, and `default_filter_dock_size` from `loganalysis_gui::workspace` and assert clamping bounds `[180.0, 800.0]` / `[100.0, 700.0]` and defaults `(310.0, 220.0)`.
2. **`crates/loganalysis-gui/src/components/text_input.rs`**:
   - `test_ime_composition_marked_range_semantics` (`:981-1000`):
     - Delete local `let mut marked_range = Some(0..6);` simulation.
     - Exercise real `TextInputState::default()`:
       - `replace_text_state(None, "prefix ")`
       - `replace_and_mark_state(None, "가나", Some(0..2))` -> assert `content == "prefix 가나"`, `marked_range == Some(7..13)`, `range_to_utf16` consistency.
       - `cancel_ime_preedit()` -> assert returns `true`, `marked_range.is_none()`, `content == "prefix "`, `selected_range == 7..7`, `pivot == 7`; second call returns `false`.
       - Re-enter preedit via `replace_and_mark_state(None, "한", None)` then commit via `replace_text_state(None, "한글")` -> assert `marked_range.is_none()`, `content == "prefix 한글"`, `selected_range == 13..13`.

### `[P2-02]` Close Core Coverage Gaps (`contrast.rs` & `searcher.rs`)
1. **`crates/loganalysis-core/src/filter/contrast.rs`**:
   - Add `#[cfg(test)] mod tests` testing:
     - `srgb_to_linear` low linear branch (`c_byte = 0` -> `0.0`, `c_byte = 10` -> `(10.0 / 255.0) / 12.92`) vs gamma high branch (`c_byte = 11` -> `((11.0 / 255.0 + 0.055) / 1.055)^2.4`, `c_byte = 255` -> `1.0`).
     - `relative_luminance`: `0x000000` -> `0.0`, `0xFFFFFF` -> `1.0`, `relative_luminance(0x00FF00) > relative_luminance(0xFF0000) > relative_luminance(0x0000FF)`.
     - `contrast_ratio`: identical color -> `1.0`, black/white -> `21.0`, commutativity `contrast_ratio(a, b) == contrast_ratio(b, a)`.
     - `evaluate_contrast`: `ContrastLevel::Pass` (`>= 4.5`, e.g. `0x000000`/`0xFFFFFF`), `ContrastLevel::Warning` (`[3.0, 4.5)`, e.g. `0x000000`/`0x767676`? verify exact hex ratios in test), `ContrastLevel::Critical` (`< 3.0`, e.g. `0x808080`/`0x999999`), and exact boundary colors flanking `3.0` and `4.5`.
2. **`crates/loganalysis-core/src/search/searcher.rs`**:
   - Add `#[cfg(test)] mod tests` testing:
     - Empty `visible_indices` or empty `query.pattern` -> `Ok(None)`.
     - Invalid regex pattern -> `Err(SearchError::InvalidRegex(_))`.
     - Full-scan miss -> `Ok(None)`.
     - Forward wrap-around (`start_row = Some(total - 1)` wraps to `0`; `start_row = None` starts scanning from row `0`), backward wrap-around (`start_row = Some(0)` wraps to `total - 1`; `start_row = None` starts scanning from row `total - 1`), and out-of-bounds `start_row = Some(total + 5)` wrapping to check row `1` (or `0` when `forward = false`).
     - `progress_callback` invocation cadence: with `25_000` lines and no match, verify callback receives `[(10_000, 25_000), (20_000, 25_000)]`.
     - Deterministic mid-scan cancellation at the `1,024`-line stride (zero `thread::sleep`):
       - Pre-stale `cancel_token` (`visited_count == 0` check) returns `Err(SearchError::Cancelled)` with `0` lines read.
       - Custom `LogSource` counting `get_raw_line` calls that flips `cancel_token` at call #`500` (inside `visited_count` `0..1024`), verifying `search_in_log` returns `Err(SearchError::Cancelled)` at exact stride boundary `visited_count == 1024` (reading `1024` lines out of e.g. `5000`, proving both cancellation and stride behavior deterministically).
       - Also verify cancellation triggered inside `progress_callback` at `visited_count == 10_000` cancels at the next `1024` stride (`10 * 1024 = 10_240`).

### `[P2-03]` Temp-File Hygiene (`tempfile`) & Zero `std::thread::sleep` in Tests
1. **Add `tempfile` dev-dependency in GUI**:
   - Add `tempfile = "3.13"` under `[dev-dependencies]` in `crates/loganalysis-gui/Cargo.toml` (`tempfile 3.27.0` is already locked in `Cargo.lock:5399`).
2. **Replace all manual `std::env::temp_dir()` + `remove_file` / `TestTempDir` setups with `tempfile`**:
   - `crates/loganalysis-gui/tests/gui_integration_tests.rs:287-343, 392-465`
   - `crates/loganalysis-gui/src/workspace/filters.rs:652-686`
   - `crates/loganalysis-gui/src/logging.rs:307-331, 437, 503, 577` (delete `TestTempDir` & `TEST_DIR_COUNTER`, use `tempfile::TempDir::new()`)
   - `crates/loganalysis-core/src/buffer/correlation.rs:162-186`
   - `crates/loganalysis-core/src/buffer/merged_buffer.rs:341-694`
   - `crates/loganalysis-core/tests/year_rollover_tests.rs:7-245`
   - `crates/loganalysis-core/tests/year_rollover_adversarial_stress.rs:8-411`
3. **Eliminate all `std::thread::sleep` in tests**:
   - Replace existing `thread::sleep` calls in `crates/loganalysis-core/tests/stream_robustness_stress.rs` (`:212, 285, 351, 383, 385`), `crates/loganalysis-core/tests/adb_logcat_adversarial_tests.rs` (`:86, 146, 193, 267, 313`), and `crates/loganalysis-core/src/adb/logcat.rs` (`:388`) with deterministic synchronization (`rx.is_full()` + `thread::yield_now()` under a bounded timeout for mock-ADB/channel saturation, and a signaling `Read` wrapper / `AtomicBool` + `Condvar` or `Barrier` for `stream_log_lines` saturated-channel tests; for `cycle % 3` in `test_stress_rapid_racing_stop_and_drop_cycles`, vary channel capacity/batch sizes across cycles instead of sleeping).

---

## Pillar 3 — Code Duplication, Dead Code Cleanup & Polish (`코드 중복/Dead code 정리`)

### `[P3-01]` Facet `modified` Flag Fix & Tab Helper Extraction (`workspace/dock.rs`)
- **Files**: `crates/loganalysis-gui/src/workspace/dock.rs:86-122, 156-209`, `crates/loganalysis-gui/src/workspace/mod.rs:354-355`
- **Contracts**:
  1. Extract dock geometry constants and pure helpers in `workspace/dock.rs`:
     - `pub const DEFAULT_FILTER_DOCK_WIDTH: f32 = 310.0;`
     - `pub const DEFAULT_FILTER_DOCK_HEIGHT: f32 = 220.0;`
     - `pub const MIN_FILTER_DOCK_WIDTH: f32 = 180.0;`, `pub const MAX_FILTER_DOCK_WIDTH: f32 = 800.0;`
     - `pub const MIN_FILTER_DOCK_HEIGHT: f32 = 100.0;`, `pub const MAX_FILTER_DOCK_HEIGHT: f32 = 700.0;`
     - `pub fn clamp_filter_dock_width(start_width: f32, delta: f32) -> f32`
     - `pub fn clamp_filter_dock_height(start_height: f32, delta: f32) -> f32`
     - `pub fn default_filter_dock_size() -> (f32, f32)`
     - Use these in `LogAnalysisWorkspace::new` (`workspace/mod.rs:354-355`), `update_resizing_filter_dock`, and `reset_filter_dock_size`.
  2. Extract `pub fn apply_facet_filter_to_tab(tab: &mut FilterTabData, tag: &str, is_exclude: bool) -> String`:
     - Handles all 3 cases (toggle existing rule active, switch include/exclude with palette/None colors, or push new `FilterRule`), ALWAYS setting `tab.modified = true`.
     - `LogAnalysisWorkspace::add_facet_filter` delegates to `apply_facet_filter_to_tab`, calls `self.mark_filter_tabs_modified()` (`[INV]` dirty-flag parity across tabs), updates `self.status_text`, calls `self.reapply_filters(cx)`, and calls `cx.notify()`.
  3. Re-export `compute_facet_tags`, `apply_facet_filter_to_tab`, `DEFAULT_FILTER_DOCK_WIDTH`, `DEFAULT_FILTER_DOCK_HEIGHT`, `clamp_filter_dock_width`, `clamp_filter_dock_height`, and `default_filter_dock_size` from `workspace/mod.rs`.

### `[P3-02]` Delete Dead Code, Remove Stale `#[allow(...)]`, and Fix Stale Doc Comments
1. **Delete dead functions**:
   - `render_titlebar` (`crates/loganalysis-gui/src/components/titlebar.rs:150-159`)
   - `render_toolbar` (`crates/loganalysis-gui/src/components/toolbar.rs:632-644`)
   - `LogAnalysisWorkspace::update_analytics_data` (`crates/loganalysis-gui/src/workspace/mod.rs:1054-1073`)
   - `build_open_log_dialog`, `build_save_filters_dialog`, `build_load_filters_dialog` (`crates/loganalysis-gui/src/dialogs.rs:249-265`), migrating `dialogs::tests::test_build_dialogs_without_panic` (`dialogs.rs:284-298`) to test `prepare_dialog_with_parent(build_*_dialog_base(), None)` and `attach_dialog_parent(build_*_dialog_base(), None)`.
2. **Remove unnecessary `#[allow(...)]` attributes**:
   - `crates/loganalysis-gui/src/components/toolbar.rs:11, 27` (`#[allow(dead_code)]` on `render_kbd_badge` and `render_kbd_badge_on_accent`)
   - `crates/loganalysis-gui/src/theme.rs:3` (`#[allow(dead_code)]` on `Theme`)
   - `crates/loganalysis-gui/src/selection.rs:7` (`#[allow(dead_code)]` on `impl SelectionModel`)
   - `crates/loganalysis-gui/src/workspace/mod.rs:12` (`#[allow(unused_imports)]` on `pub use split_view::probe_parent_timestamp;`)
   - Keep `#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]` on `SafeDialogParent` (`dialogs.rs:44`) for non-Windows non-test targets.
3. **Fix stale doc comments**:
   - `crates/loganalysis-gui/src/workspace/mod.rs:935`: Change `/// Adds a new filter tab (Ctrl+T)` above `toggle_theme` to `/// Toggles between Light and Dark theme (Ctrl+U / F10)`.
   - `crates/loganalysis-gui/src/workspace/mod.rs:1003`: Remove stale `/// Toggles filter dock visibility between shown and hidden` above `get_buffer_memory_display`.

### `[P3-03]` Deduplicate GUI Rendering, Scroll, Split-View, Persistence & ADB Helpers
1. **Row Height Constant & Helper (`components/log_view.rs` & `workspace/mod.rs`)**:
   - Define `pub const LOG_ROW_HEIGHT_MULTIPLIER: f32 = 1.55;` and `#[inline] pub fn log_row_height_px(font_size: f32) -> f32 { font_size * LOG_ROW_HEIGHT_MULTIPLIER }` in `components/log_view.rs`.
   - Replace all 4 hardcoded `* 1.55` literals (`log_view.rs:785, 1201`; `workspace/mod.rs:892, 926`).
2. **Unified Viewport & Highlight / Heatmap Helpers (`components/log_view.rs`)**:
   - `render_log_viewport` (`:9-119`): Unify single-pane and split-pane branches around a single outer container builder (`.flex_1().h_full().relative().flex().flex_col().bg(theme.bg_dark).overflow_hidden().on_mouse_move(...).on_mouse_up(...).children(render_facets_bar(...)).child(main_content).children(render_find_bar(...)).children(render_selection_action_bar(...))`).
   - Extract `pub fn compute_highlight_ranges(text: &str, search_matcher: &Option<Arc<CompiledMatcher>>) -> Vec<std::ops::Range<usize>>` and refactor `render_highlighted_text` (`:645-663`) to use it while keeping the zero-allocation `text_content.into_any_element()` fast path when `ranges.is_empty()`.
   - Unify unstructured stack-trace indentation & `render_highlighted_text` wrapping between column-view fallback (`:1137-1154`) and raw monospace view (`:1157-1172`) via a helper `render_unstructured_line_content` (preserving exact `24px`/`12px` + `overflow_hidden` vs `22px`/`10px` metrics).
   - Extract `fn build_heatmap_ticks(heatmap: Option<&HeatmapData>, track_height: f32) -> Vec<Div>` and inner `push_heatmap_tick` (`:1216-1290`), removing duplicated tick construction inside and after the loop.
   - Align onboarding card label at `log_view.rs:1751` (`"⚡ k-way Priority Heap"` -> `"⚡ Chronological Stable Sort"`).
3. **Scrollbar Mouse-Ratio Drag Helper (`workspace/mod.rs`)**:
   - Extract `fn scroll_handle_to_mouse_y(handle: &UniformListScrollHandle, row_count: usize, font_size: Pixels, mouse_y: Pixels) -> bool` unifying `scroll_to_ratio_from_mouse_y` (`:869-899`) and `scroll_secondary_to_ratio_from_mouse_y` (`:902-933`).
4. **Memory Telemetry Formatter (`workspace/mod.rs`)**:
   - Extract `pub fn format_byte_size(bytes: usize) -> String` (uniformly covering KB / MB / GB) and `pub fn format_source_memory_display(source: Option<&ActiveLogSource>) -> (String, &'static str)` (`None -> ("0 MB", "Idle")`, `File -> "MMap"`, `Stream -> "Heap"`, `Merged -> "Merged"`).
   - Delegate `LogAnalysisWorkspace::get_buffer_memory_display` to `format_source_memory_display(self.log_source.as_deref())`, and re-export both helpers from `workspace/mod.rs`.
5. **Split-View Deduplication (`workspace/split_view.rs`)**:
   - Replace manual filter-rule collection loop in `reapply_secondary_filters` (`:159-168`) with `self.collect_active_flat_filters()`.
   - Extract `fn sync_pane_to_timestamp(...) -> bool` unifying `sync_scroll_to_secondary` (`:213-240`) and `sync_scroll_to_primary` (`:243-270`).
6. **Filter Dock Search Box Deduplication (`components/filter_dock.rs`)**:
   - Extract `fn render_filter_rule_search_box(workspace: &LogAnalysisWorkspace, input_id: &'static str, clear_id: &'static str, height_px: f32, width_px: Option<f32>, cx: &mut Context<LogAnalysisWorkspace>) -> Div` unifying bottom inline (`:192-224`) and left sidebar (`:260-292`) search boxes while preserving exact element IDs.
7. **Quick Filter Presets Constant (`modals/filter_edit_modal.rs`)**:
   - Extract `pub const QUICK_FILTER_PRESETS: &[(&str, &str, &str, &str)] = &[ ... ];` at module level and reuse it in `render_filter_edit_modal` (`:560-570`).
8. **Shared Atomic Persistence & Recent Files Path API (`workspace/filters.rs` & `workspace/recent_files.rs`)**:
   - Replace `write_filters_atomically` (`filters.rs:129-137`) with `pub(crate) fn write_file_atomically(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()>` (ensuring parent dir exists and cleaning up temp file on error, delegating replacement to `atomic_replace_file`).
   - Refactor `workspace/recent_files.rs` to expose `pub fn push_recent_path`, `pub fn load_recent_files_from_path(path: &Path) -> Vec<PathBuf>`, and `pub fn save_recent_files_to_path(path: &Path, files: &[PathBuf]) -> std::io::Result<()>` (using `write_file_atomically`), with `load_recent_files` / `save_recent_files` delegating to them; re-export from `workspace/mod.rs`.
9. **Linux `PR_SET_PDEATHSIG` Consolidation (`crates/loganalysis-core/src/adb/mod.rs`)**:
   - Extract `#[cfg(target_os = "linux")] pub(crate) fn configure_linux_pdeathsig(cmd: &mut std::process::Command)` in `adb/mod.rs` and invoke it from both `adb/device.rs:129-140` and `adb/logcat.rs:113-124`, preserving `[INV-12]` (`prctl(PR_SET_PDEATHSIG, SIGKILL)` + `getppid() != parent_pid` guard).
10. **Multi-Arch Linux Build Script (`crates/loganalysis-gui/build.rs`)**:
    - Expand `libxkbcommon-x11.so` / `.so.0` discovery across `/usr/lib/x86_64-linux-gnu`, `/usr/lib/aarch64-linux-gnu`, `/usr/lib64`, and `/usr/lib` (plus `CARGO_CFG_TARGET_ARCH`-derived `/usr/lib/<arch>-linux-gnu`), and emit `cargo:rerun-if-changed=build.rs`.
11. **Non-Blocking Conditional Desktop Integration (`crates/loganalysis-gui/src/lib.rs:458-509`)**:
    - Extract testable helpers (`build_desktop_entry_content`, `write_if_changed`) preserving quote/control-char sanitization (`replace(['\n', '\r', '"', '\\'], "")`).
    - Write `loganalysis.png` and `loganalysis.desktop` only when missing or content actually changed.
    - Spawn `update-desktop-database` and `gtk-update-icon-cache` in a detached background thread (`"desktop-integration-refresh"`) **only** when icon or desktop entry changed, eliminating synchronous startup subprocess blocking.

---

## Global Quality Gates (Mandatory Phase 2 & Phase 3 Exit Criteria)

All of the following gates MUST pass cleanly before Phase 3 marks the task `PASS`:

1. **Workspace Compilation (All Targets)**:
   `cargo check --workspace --all-targets` -> exit `0`, 0 warnings.
2. **Strict Clippy Lints**:
   `cargo clippy --workspace --all-targets -- -D warnings` -> exit `0`, 0 warnings.
3. **Full Automated Test Suite + Badge Synchronization**:
   `cargo test --workspace` -> exit `0`, 0 failures, 0 ignored regressions; exact passing count `<N>/<N>` verified and synchronized in `README.md` and `docs/INVARIANTS.md`.
4. **Rustdoc Warning-Free Check**:
   `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` -> exit `0`, 0 doc warnings.
5. **Zero `std::thread::sleep` in Tests**:
   `grep -rn "thread::sleep" crates/` returns **zero** matches inside any `crates/*/tests/` file or `#[cfg(test)]` block (only production backoff in `crates/loganalysis-core/src/adb/device.rs` is permitted).
6. **Zero `allow(dead_code)` / `allow(unused_imports)` on Live Items**:
   `grep -rn "allow(" crates/` contains zero `allow(dead_code)` or `allow(unused_imports)` on live code (`SafeDialogParent` at `dialogs.rs` retains `#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]` for platform-gated usage).
7. **Zero `std::env::temp_dir()` Leaks in Tests**:
   `grep -rn "temp_dir" crates/` returns zero matches in test files / test modules; all temporary test files/dirs use `tempfile::TempDir` or `tempfile::NamedTempFile`.
8. **Supply Chain & Dependency Checks (if tools available locally)**:
   `cargo deny check` / `cargo audit` validated cleanly (or recorded in `docs/qa-docs-tests-cleanup.md` alongside CI config parity).

---

## Per-Item Verification Matrix

| ID | Category | Target File(s) | Exact Verification Method |
|----|----------|----------------|---------------------------|
| `[P1-01]` | Root License | `LICENSE` | File exists with MIT License text & `Copyright (c) 2026 Drew Yu`; `README.md` link valid |
| `[P1-02]` | README Sync | `README.md` | Manual + link/command check; badge `<N>/<N>` matches `cargo test --workspace`; zero unverified benchmark claims; `apt-get` matches `.github/workflows/ci.yml`; shortcuts match `lib.rs:68-123` |
| `[P1-03]` | PROJECT Sync | `PROJECT.md` | Symbols `FileLogBuffer`, `SingleLogBuffer`, `TimestampKind`, `ParsedTimestamp`, `extract_timestamp_with_kind`, `SelectionModel::shift_on_trim`, `analytics_cancel_gen`, `search_ctrl.cancel_gen`, `primary_source_gen`, `secondary_source_gen`, and Code Layout match repo |
| `[P1-04]` | CHANGELOG Sync | `CHANGELOG.md`, `components/log_view.rs:1751` | `[Unreleased]` added; `[0.2.4]`/`[0.2.3]`/`[0.2.1]`/`[0.2.0]`/`[0.1.1]` facts verified against source (`1.9.1`, `analytics_cancel_gen`, `render_log_list`, `sort_by_key`, fixed-width columns, `F2`/`Shift+F2`/`Alt+F2`, `-v`, etc.) |
| `[P1-05]` | Invariants & Config | `docs/INVARIANTS.md`, `deny.toml`, `scripts/status_dashboard.sh` | `[INV-02-A]`, `[INV-05]`, `[INV-06]`, `[INV-07]` (`2s`), `[INV-10]`, `[INV-11]` (`Ctrl+U`/`F10`), `[INV-13]` (10-line backward), Section 11, `deny.toml:19`, and `bash -n scripts/status_dashboard.sh` verified |
| `[P2-01]` | GUI Test Quality | `crates/loganalysis-gui/tests/gui_integration_tests.rs`, `components/text_input.rs` | All 6 former Test Theater tests invoke real production symbols (`QUICK_FILTER_PRESETS`, `compute_highlight_ranges`, `push_recent_path`/`save_recent_files_to_path`/`load_recent_files_from_path`, `compute_facet_tags`/`apply_facet_filter_to_tab`/`format_source_memory_display`, `clamp_filter_dock_*`, `TextInputState`) and pass |
| `[P2-02]` | Core Coverage | `crates/loganalysis-core/src/filter/contrast.rs`, `search/searcher.rs` | Inline unit tests cover `ContrastLevel::{Pass,Warning,Critical}`, `srgb_to_linear` low/high branches, `contrast_ratio` symmetry, `search_in_log` wrap-around, `10_000`-line `progress_callback`, and deterministic `1,024`-stride cancellation |
| `[P2-03]` | Test Hygiene | `crates/loganalysis-gui/Cargo.toml`, `gui_integration_tests.rs`, `filters.rs`, `logging.rs`, `correlation.rs`, `merged_buffer.rs`, `year_rollover_*.rs`, `stream_robustness_stress.rs`, `adb_logcat_adversarial_tests.rs`, `adb/logcat.rs` | Zero `temp_dir` in `crates/`; zero `thread::sleep` in tests; `cargo test --workspace` passes deterministically |
| `[P3-01]` | Facet Dirty Flag & Dock | `workspace/dock.rs`, `workspace/mod.rs` | `apply_facet_filter_to_tab` sets `modified = true` across toggle/switch/add; `add_facet_filter` calls `mark_filter_tabs_modified`; dock clamp/default helpers verified via `gui_integration_tests` |
| `[P3-02]` | Dead Code & Allows | `titlebar.rs`, `toolbar.rs`, `workspace/mod.rs`, `dialogs.rs`, `theme.rs`, `selection.rs` | Dead functions deleted; stale `#[allow(...)]` & doc comments removed; `clippy --all-targets -- -D warnings` + `RUSTDOCFLAGS="-D warnings" cargo doc` pass |
| `[P3-03]` | Deduplication & Polish | `log_view.rs`, `workspace/{mod,split_view,filters,recent_files}.rs`, `filter_dock.rs`, `filter_edit_modal.rs`, `adb/{mod,device,logcat}.rs`, `build.rs`, `lib.rs` | Single `LOG_ROW_HEIGHT_MULTIPLIER` (`1.55`), unified viewport/highlight/heatmap/scroll/sync/search-box/atomic-write/pdeathsig helpers, multi-arch `build.rs`, and conditional background `ensure_desktop_integration` unit-tested & verified via full workspace checks |

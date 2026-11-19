# Changelog

All notable changes to the **LogAnalysis** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.2.5] - 2026-11-19

### Added
- **Extended Filter Color Palette & Custom `#RRGGBB` Hex Inputs**: Expanded the New/Edit Filter Rule modal to a 24+24 Light/Dark WCAG AA-balanced background and text swatch grid (adding 11 dark-surface colors to `COLOR_MAP` and 13 light/dark-optimized colors to `TEXT_COLOR_MAP`), added dynamic BG checkmark contrast, adaptive FG swatch backing plates, hover tooltips (`Name (#RRGGBB)`), and inline custom `#RRGGBB` hex inputs with live preview, `Tab`/`Shift+Tab` focus cycling, and validation.
- **Filter Pattern History & Autocomplete Suggestions**: The New/Edit Filter Rule modal now remembers saved patterns (MRU, deduplicated by pattern+Regex+Case, capped at 200, persisted atomically to `~/.config/loganalysis/pattern_history.json`) and shows a dropdown of up to 8 matches (prefix before substring) while typing. Selecting restores the Regex/Case toggles (colors are not stored). Keyboard: Up/Down, Enter/Tab select, Esc closes only the dropdown, Shift+Delete or the row ✕ removes an entry. New Command Palette command `filter:clear_pattern_history`.
- **Filter List UX Redesign & Line Detail Inspector**: Redesigned the filter rules list with double-click editing, toggle checkboxes, full-span color strips, and drag-and-drop rule reordering; added the Line Detail inspector panel and Windows application icon embedding.
- **Root `LICENSE` & `cliff.toml`**: Added standard MIT License file (`Copyright (c) 2026 Drew Yu`) and `git-cliff` release configuration.
- **Extracted Production Helpers & Coverage**: Extracted reusable workspace and GUI helpers (`apply_facet_filter_to_tab`, `clamp_filter_dock_width`, `clamp_filter_dock_height`, `default_filter_dock_size`, `compute_highlight_ranges`, `LOG_ROW_HEIGHT_MULTIPLIER`, `log_row_height_px`, `format_byte_size`, `format_source_memory_display`, `QUICK_FILTER_PRESETS`, `push_recent_path`, `load_recent_files_from_path`, `save_recent_files_to_path`, `build_desktop_entry_content`, `write_if_changed`) along with comprehensive `contrast.rs` and `searcher.rs` unit test suites.

### Changed
- **CI & Documentation Alignment**: Upgraded GitHub Actions to Node.js 24 versions, configured YAML `env` for `RUSTDOCFLAGS`, and synchronized `README.md`, `PROJECT.md`, `CHANGELOG.md`, `docs/INVARIANTS.md`, `deny.toml`, and `scripts/status_dashboard.sh` with verified codebase behavior.
- **Deduplication & Build/Startup Polish**: Unified viewport builders, unstructured line wrappers, heatmap tick coalescing (`build_heatmap_ticks`), scrollbar ratio dragging (`scroll_handle_to_mouse_y`), split-view timestamp correlation (`sync_pane_to_timestamp`), filter dock search boxes (`render_filter_rule_search_box`), atomic file writes (`write_file_atomically`), Linux `PR_SET_PDEATHSIG` configuration (`configure_linux_pdeathsig`), multi-arch `libxkbcommon-x11` discovery in `build.rs`, and conditional non-blocking desktop integration in `ensure_desktop_integration`.
- **Test Quality Strengthening**: Replaced local simulation tests with calls to real production symbols, migrated temporary file tests to `tempfile`, and replaced `std::thread::sleep` synchronization in tests with deterministic signaling/yield loops.

### Fixed
- **Windows ADB Console Popup & Full-Line Horizontal Scroll**: Hidden the console window (`CREATE_NO_WINDOW`) when spawning `adb` subprocesses on Windows and fixed horizontal scrolling in Full Line view mode.
- **Async Source Races, ADB Timeout/Pause, & Timestamp Rollover (`fix-all-bugs`)**: Remediated async source generation races, ADB timeout/pause bugs, year-end timestamp rollover ordering, and UI/filter defects.
- **Facet Dirty Flag Parity**: Fixed `add_facet_filter` / `apply_facet_filter_to_tab` so toggling or switching an existing facet rule always marks filter tabs as modified.
- **Dead Code & Lint Cleanup**: Removed obsolete stubs (`render_titlebar`, `render_toolbar`, `update_analytics_data`, `build_*_dialog`), stale `#[allow(...)]` attributes, and stale doc comments.

---

## [0.2.4] - 2026-11-14

### Added
- **Extensible Log Format Parser SPI (`INF-01`)**: Added zero-copy `LogFormatParser` (`Send + Sync`), `ThreadtimeColumnParser`, `BriefColumnParser`, `DEFAULT_COLUMN_PARSERS`, `parse_column_line_with_parsers`, and `parse_column_line_auto` in `loganalysis-core`, wired into GUI structured column rendering and forensic facet extraction.
- **Pre-Compiled Streaming Filters (`MAJ-06`)**: Added `CompiledStreamFilters` and `append_streaming_batch_with_matcher` in `filter::engine`, cached in `LogAnalysisWorkspace` to avoid per-batch regex and Aho-Corasick recompilation.
- **Cross-Platform Subprocess & Atomic Persistence (`MAJ-03`, `MAJ-04`)**: Added `atomic_replace_file` (Win32 `MoveFileExW` on Windows) and cross-platform `run_command_with_timeout` in `adb::device` using bounded `crossbeam_channel` readers.

### Changed
- **Streaming CoW Buffer & GUI Modularization (`MAJ-07`, `MAJ-11`, `MAJ-02`)**: Refactored `StreamLogBuffer` to `1024`-line Copy-on-Write (`Arc<Vec<Arc<str>>>`) pages with `SharedStreamBuffer`, wired headless controllers (`AdbController`, `BookmarkController`, `SearchController`, `SplitViewController`), and split `loganalysis-gui` into `[lib]` and thin `[[bin]]` targets.
- **Buffered Logging & OS User Dir Precedence (`MIN-03`)**: Switched `loganalysis-gui` logging to cache `BufWriter<File>` (`LogWriterState`), prefer the OS user log directory exclusively (`cwd` fallback only when env lookup returns `None`), flush immediately on `ERROR`/`CRITICAL`/`CRITICAL_PANIC`/`FATAL` and clean exit, flush buffered logs ahead of crash reports, and isolate tests from repo checkout files.
- **Supply Chain & CI Hardening (`MAJ-01`, `MAJ-12`, `INF-06`)**: Enforced `cargo clippy -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc`, `cargo audit`, and `cargo deny check` in CI; pinned `appimagetool` to tag `1.9.1` with SHA256 verification; pinned `crates.io-index` allowlist in `deny.toml`.

### Fixed
- **Streaming Filter Reconciliation & Selection Sync (`CRIT-01`, `CRIT-02`)**: Tracked cumulative ring-buffer trims via `reconcile_snapshot_filter_result` and decoupled `SelectionModel::shift_on_trim(dropped_visible_rows, raw_trimmed_count)`.
- **Excluded-Line Trim Counter Parity (`[INV-09]`)**: Tracked `excluded_filter_matches` across parallel `execute_filtering` and streaming append, ensuring excluded filter counters and line indices shift accurately during trims even when `visible_indices` is empty.
- **Non-Blocking Child Process Reaping**: Released `self.child` mutex before calling `reap_child_nonblocking` in `LogcatStreamHandle::stop` with a `try_wait()` fast path and detached reaper thread fallback.
- **Gregorian Leap-Year & Mmap Safety (`MAJ-10`, `MAJ-05`, `INF-03`)**: Fixed ISO-8601 leap-year and Unix epoch day math (`div_euclid`), added heap fallback (`open_heap`) and truncation detection in `FileLogBuffer`, rewound existing `File` handles on mmap fallback, unified `Regex Error` feedback (`MAJ-08`) with modal filter validation (`MAJ-09`), and replaced X11/Win32 dialog/menu unwrap edge cases (`MIN-04`..`MIN-10`).

---

## [0.2.3] - 2026-11-08

### Added
- **Unified 38px Header Bar**: Merged the CSD titlebar (34px) and toolbar (44px) into a single compact header with branding, smart ADB combo button, active document pill, view toggles, find trigger, overflow menu, and integrated window controls.
- **Zen / Focus Mode (`F11` / `Ctrl+Shift+Z`)**: Hides the activity bar, filter dock, timeline ribbon, and status bar for distraction-free log reading.
- **Contextual Selection Mini-Bar**: Floating Copy / Filter Selection / Clear actions with dynamic collision offset relative to the Find widget and Forensic Facets bar.
- **Overflow Menu (`⋯`)**: Low-frequency actions (load/save filter profiles, clear logs, dock layout, H-scroll, theme switch) relocated into a popover.
- **Collapsible Timeline (`Ctrl+Shift+T`)**: Timeline sparkline can be collapsed to reclaim vertical viewport space.
- **Resizable Filter Rules Dock**: Drag splitter with `col-resize`/`row-resize` cursors, clamped to 180–800px width / 100–700px height, and double-click reset to defaults (310px / 220px).
- **Stack Trace Timestamp Probing**: `probe_parent_timestamp` scans up to 10 lines backward so dual-pane lockstep correlation works on multi-line stack trace rows.
- **Windows Log Paths**: `%LOCALAPPDATA%` / `%APPDATA%` fallbacks in log path resolution.

### Changed
- **Background Streaming Analytics**: `HeatmapData` and `EpsTimelineData` computation moved off the GPUI main thread, with an `analytics_cancel_gen` monotonic guard against out-of-order batch results.
- **Workspace Modularization**: Split the ~3,200-line `workspace.rs` into focused `src/workspace/` and `src/controllers/` modules.
- **Decluttered Toolbar**: Inline shortcut badges removed from action buttons; shortcuts are surfaced via tooltips and the Command Palette.
- **CI**: macOS runners migrated from `macos-13` to `macos-15`; fixed release workflow input tag fallback.

### Fixed
- **ADB Stream Shutdown Deadlock (CRIT-01)**: Replaced blocking channel sends with a `send_timeout(20ms)` loop that checks the stop flag, and removed synchronous UI-thread joins in `LogcatStreamHandle::drop`.
- **TextInput UTF-8 Boundary Panics (CRIT-02)**: All slice and cursor operations clamp to char boundaries, fixing crashes when editing Korean, Japanese, Chinese, and emoji input.
- **Year-End Rollover Ordering (CRIT-04)**: Backward timestamp jumps across Dec 31 → Jan 01 are normalized with a 1-year offset in `MergedLogBuffer` and the EPS timeline.
- **Merged Buffer Source Overflow**: `MergedLogBuffer` now rejects more than 255 sources, preventing `u8` source index overflow.

### Security
- **Regex ReDoS Mitigation (CRIT-03)**: Linear DFA regexes are preferred. The `fancy_regex` fallback rejects nested unbounded quantifiers at compile time and enforces a runtime `backtrack_limit(250)`.

---

## [0.2.2] - 2026-11-07

### Added
- **Workstation Navigation Activity Bar (Phase 3)**: Left-docked quick navigation bar with dedicated modes for Explorer, Filters, Bookmarks, Forensic Facets, and Telemetry.
- **Forensic Facets Bar (Phase 3)**: Interactive horizontal tag & process frequency distribution bar with instant single-click include/exclude filter creation and toggle deduplication.
- **Live Buffer Telemetry Ribbon (Phase 3)**: Status telemetry displaying active memory footprint, total parsed lines, visible filtered rows, and throughput counters.
- **Welcome Onboarding Grid (Phase 2)**: 3-pillar quick ingestion hero grid (Open File, Stream ADB Logcat, Merge Chronological Logs).
- **Recent Files Shelf (Phase 2)**: Quick-launch shelf tracking up to 5 recently opened files with canonical path deduplication and LRU ordering.
- **Contextual Breadcrumbs Bar (Phase 2)**: Monospace path breadcrumbs showing active file origin, ingestion mode, and row status.
- **Micro-Hover Tooltip System (Phase 1)**: Instant zero-delay tooltips with keyboard shortcut badges for all action buttons.
- **Exception Stack Trace Indentation (Phase 1)**: Visual hierarchy and left border indentation for multiline exception traces and causal chains.

### Changed
- **Dual-Pane Split Scrolling & Dragging**: Replaced discontinuous 0.98 ratio jump with smooth continuous top-index mapping, implemented full mouse drag lifecycle for Pane B scrollbar, and added strict center alignment to timestamp lockstep synchronization.
- **Active Split Pane Focus & Navigation**: Added active pane state tracking with visual header indicators and Up/Down arrow key routing to the currently focused pane.

### Fixed
- **Sub-Pixel Border Alignment**: Standardized 1px borders and container dividers to eliminate 2px doubling artifacts across nested panels.

---

## [0.2.1] - 2026-11-06

### Added
- **Exact Stream Trim Counter Decrement (`[INV-09]`)**: Track `visible_filter_matches` in `FilterResult` to decrement `filter_counts` and `rule_counts` when rolling buffer lines are trimmed, achieving 100% pass across all 15 system invariants.
- **Structured Controller State Architecture**: Modularized window controller state into domain-specific structs (`SplitViewState`, `AdbStreamState`, `SearchState`, `BookmarkDrawerState`) in `crates/loganalysis-gui/src/types.rs`.
- **English Documentation**: Fully translated all architectural specifications, system invariants (`docs/INVARIANTS.md`), and QA verification reports to technical English.

### Changed
- **Zero-Allocation Viewport Rendering**: Refactored `render_highlighted_text` to accept `SharedString` and directly return `into_any_element()` when unhighlighted, eliminating ~3,600 heap allocations per second during smooth scrolling.
- **Scrollbar Heatmap Quad Coalescing**: Culled `LineSeverity::Normal` buckets and coalesced contiguous severity buckets in `render_log_list`, reducing GPU heatmap quad allocations per render tick.
- **EPS Timeline Profiling Throttling**: Guarded `EpsTimelineData::compute` behind `is_timeline_visible` during live ADB streaming (`workspace/adb.rs`) and `toggle_eps_timeline`, whereas `reapply_filters` (`workspace/mod.rs`) computes timeline bins eagerly.
- **Column Parser Pointer Slicing**: Replaced fragile substring matching with subslice pointer arithmetic in `parse_logcat_column_line`, strictly enforcing standard logcat severities (`V`, `D`, `I`, `W`, `E`, `F`).

### Security
- **ADB Process Lifecycle Isolation (`[INV-12]`)**: Hardened ADB subprocess spawning with Linux `PR_SET_PDEATHSIG` and added post-fork parent PID verification (`getppid() == parent_pid`) in both device scanner and logcat streamer to eliminate daemon reparenting races.
- **Desktop Entry Sanitization**: Sanitized executable and working directory paths in `ensure_desktop_integration` to prevent quote and control character injection in `~/.local/share/applications/loganalysis.desktop`.

---

## [0.2.0] - 2026-11-06

### Added
- **Dual-Pane Split View (`Ctrl+\`)**: Side-by-side or stacked split view with timestamp lockstep synchronization (`[INV-11]`) for cross-log causal comparison.
- **Chronological Multi-File Merge View (`Ctrl+Shift+M`)**: Interleaves multiple asynchronous log files into a unified timeline using stable chronological sorting (`entries.sort_by_key(...)`).
- **Structured Column Grid View (`Ctrl+G`)**: High-performance grid mode parsing logcat timestamps, PID/TID, tags, and severity levels into dedicated fixed-width columns.
- **Interactive Command Palette (`Ctrl+P` / `Ctrl+Shift+P`)**: Fuzzy-searchable catalog of all application actions, filter presets, and viewing modes.
- **Interactive Bookmarks & Notes (`F2`, `Ctrl+B`)**: Slide-over bookmark drawer with inline annotations (`Alt+Enter`), `F2` / `Ctrl+F2` toggle, and `Shift+F2` / `Alt+F2` previous/next cyclic navigation.
- **Interactive Offline HTML Incident Reports**: Export self-contained HTML reports with embedded CSS styling, filter tags, and bookmarks.
- **Real-Time Throughput Sparkline & EPS Timeline**: Live events-per-second visual histogram with statistical 3σ anomaly spike detection.
- **Delta-Time Inspector (Δt)**: Real-time visual latency badges measuring microsecond intervals between consecutive log lines.
- **Built-In Quick Filter Presets**: Modal quick-import presets (`ERROR`, `WARN`, `INFO`, `DEBUG`, `TRACE`, `FATAL`, `SUCCESS`, `ActivityTaskManager`, `System`) defined in `modals/filter_edit_modal.rs`.
- **GPU Scrollbar Error Heatmap**: Overview rail displaying distribution of errors and warnings across the entire log buffer.

### Changed
- **Professional UI Design System**: Overhauled theme with 4-tier elevation surfaces, sub-pixel borders, and deterministic tag color hashing.
- **Floating Find Widget**: Replaced embedded search bar with non-intrusive floating find panel to avoid layout shifts.

---

## [0.1.1] - 2026-11-05

### Added
- **Interactive Window Resize Borders**: 8-direction resize handles for Wayland Client-Side Decorations (CSD).
- **CLI Flags**: Added `--version` / `-V` / `-v` support with status bar and onboarding view version badges.
- **Auto-Detection for Wayland CSD**: Automatically detects window manager capabilities and adapts decorations.
- **Custom X11 Window Handle**: Non-blocking asynchronous window attachment resolving native file dialog lockups.

### Fixed
- **Filter Rule Search**: Fixed in-tab search input focus stealing and filter list selection reset.
- **CJK Display Width Calculation**: Corrected character count measurement for Chinese, Japanese, and Korean glyphs.

---

## [0.1.0] - 2026-11-04

### Added
- **Pure Rust + GPUI Desktop Engine**: Native 120 FPS+ GPU-accelerated rendering pipeline replacing legacy PyQt5 architecture.
- **Zero-Copy Memory-Mapped Buffer (`memmap2`)**: Instant sub-100ms loading of multi-gigabyte log files.
- **Parallel SIMD Filtering (`rayon`)**: Multi-core regex and literal matching with independent foreground/background color inheritance.
- **Android ADB Streaming**: Real-time `adb logcat` ingestion with 200,000-line rolling buffer and hysteresis trimming.
- **Multi-Tab Filter Management**: 100% backward-compatible loading of legacy 3-tier JSON filter formats.
- **WCAG 2.1 Contrast Ratio Verification**: Built-in luminance calculator warning against unreadable color rules.

# Changelog

All notable changes to the **LogAnalysis** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- **Scrollbar Heatmap Quad Coalescing**: Culled `LineSeverity::Normal` buckets and coalesced contiguous severity buckets in `render_scrollbar_track`, reducing GPU heatmap quad allocations by ~90% per render tick.
- **EPS Timeline Profiling Throttling**: Guarded `EpsTimelineData::compute` behind `is_timeline_visible` during live streaming ingestion, computing timeline bins lazily only when toggled on.
- **Column Parser Pointer Slicing**: Replaced fragile substring matching with subslice pointer arithmetic in `parse_logcat_column_line`, strictly enforcing standard logcat severities (`V`, `D`, `I`, `W`, `E`, `F`).

### Security
- **ADB Process Lifecycle Isolation (`[INV-12]`)**: Hardened ADB subprocess spawning with Linux `PR_SET_PDEATHSIG` and added post-fork parent PID verification (`getppid() == parent_pid`) in both device scanner and logcat streamer to eliminate daemon reparenting races.
- **Desktop Entry Sanitization**: Sanitized executable and working directory paths in `ensure_desktop_integration` to prevent quote and control character injection in `~/.local/share/applications/loganalysis.desktop`.

---

## [0.2.0] - 2026-11-06

### Added
- **Dual-Pane Split View (`Ctrl+\`)**: Side-by-side or stacked split view with timestamp lockstep synchronization (`[INV-11]`) for cross-log causal comparison.
- **Chronological Multi-File Merge View (`Ctrl+Shift+M`)**: Interleaves multiple asynchronous log files into a unified timeline using priority heap k-way merge.
- **Structured Column Grid View (`Ctrl+G`)**: High-performance grid mode parsing logcat timestamps, PID/TID, tags, and severity levels into dedicated sortable columns.
- **Interactive Command Palette (`Ctrl+P` / `Ctrl+Shift+P`)**: Fuzzy-searchable catalog of all application actions, filter presets, and viewing modes.
- **Interactive Bookmarks & Notes (`F2`, `Ctrl+B`)**: Slide-over bookmark drawer with inline annotations (`Alt+Enter`) and cyclic navigation (`F2` / `Shift+F2`).
- **Interactive Offline HTML Incident Reports**: Export self-contained HTML reports with embedded CSS styling, filter tags, and bookmarks.
- **Real-Time Throughput Sparkline & EPS Timeline**: Live events-per-second visual histogram with statistical 3σ anomaly spike detection.
- **Delta-Time Inspector (Δt)**: Real-time visual latency badges measuring microsecond intervals between consecutive log lines.
- **Automotive Preset Profiles**: Quick-import presets for Android Automotive OS (VHAL, CarService, Audio/Camera HALs).
- **GPU Scrollbar Error Heatmap**: Overview rail displaying distribution of errors and warnings across the entire log buffer.

### Changed
- **Professional UI Design System**: Overhauled theme with 4-tier elevation surfaces, sub-pixel borders, and deterministic tag color hashing.
- **Floating Find Widget**: Replaced embedded search bar with non-intrusive floating find panel to avoid layout shifts.

---

## [0.1.1] - 2026-11-05

### Added
- **Interactive Window Resize Borders**: 8-direction resize handles for Wayland Client-Side Decorations (CSD).
- **CLI Flags**: Added `--version` / `-V` support with titlebar and statusbar version badges.
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

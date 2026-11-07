# Specification: UI Professional Evolution (Phase 3 - Activity Bar, Forensic Facets, & Workstation Telemetry)

**Status:** IN PROGRESS  
**Target Version:** `v0.2.1` -> `v0.2.2`  
**Workspace:** `/home/drew/github/hwanjongyu/loganalysis`  

---

## 1. Motivation & Architectural Objectives

Phases 1 and 2 established sub-pixel border elevation, tooltip infrastructure, 3-pillar welcome onboarding, and contextual breadcrumb trails. However, as LogAnalysis scales to advanced forensic workflows, two key ergonomic challenges remain:
1. **Screen Real Estate & Clutter**: Controls are packed into horizontal strips, and the filter dock permanently consumes viewport space even when engineers want uninterrupted full-height log reading.
2. **Forensic Discovery Velocity**: When diagnosing crash dumps or ADB streams, engineers spend unnecessary time identifying which Android subsystems (tags) are active before manually configuring filter rules.

Phase 3 introduces the **Linear / Zed / Fleet workstation layout architecture**:
- **Slim Activity Bar (Left Navigation Rail)**: Unifies mode switching (Files, Filters, Facets, Bookmarks, Timeline, ADB, Split View, Theme, Command Palette) into a compact 42px rail with active indicator stripes and badges.
- **Forensic Faceted Quick Filter Bar**: Automatically samples active logs to identify top Android tags (e.g., `ActivityManager`, `SurfaceFlinger`, `AudioPolicy`) with occurrence counters, allowing 1-click inclusion or exclusion.
- **Toggleable Filter Dock**: Enables full-viewport immersive reading by collapsing/expanding the filter dock via the Activity Bar or keyboard shortcut.
- **Workstation Telemetry in Status Bar**: Surfaces live buffer memory footprint (`💾 24.5 MB`) and engine acceleration modes (`⚡ Zero-Copy MMap`, `⚡ SIMD Parallel`).

---

## 2. Detailed Technical Design

### 2.1 Slim Activity Bar (`crates/loganalysis-gui/src/components/activity_bar.rs`)
- **Width**: `42.0px`, fixed width, non-shrinkable.
- **Location**: Leftmost column in the main workspace body.
- **Border**: Right 1px subtle divider (`theme.border_subtle`).
- **Background**: `theme.bg_header` (matches window header elevation).
- **Items & Actions**:
  | Icon | Label | Shortcut | Behavior | Indicator / Badge |
  |---|---|---|---|---|
  | 📁 | Files / Recent | `Ctrl+O` | Prompts file picker or recent shelf | Active when log is open |
  | 🔍 | Filter Rules | `Ctrl+J` | Toggles filter dock visibility | Badge with active rules count |
  | 🏷️ | Forensic Facets | `Alt+F` | Toggles quick tag facets bar | Active when facets bar open |
  | 🔖 | Bookmarks | `Ctrl+B` | Toggles slide-over bookmark drawer | Badge with bookmark count |
  | 📈 | Timeline Profiler | `Ctrl+Shift+T` | Toggles anomaly sparkline ribbon | Active when ribbon visible |
  | 📱 | ADB Streaming | `Ctrl+Shift+R` | Toggles device selector / streaming | Green/yellow dot indicator |
  | 🪟 | Dual Pane Split | `Ctrl+\` | Toggles side-by-side log comparison | Active when split mode on |
  | 📋 | Column View | `Ctrl+G` | Toggles structured tabular columns | Active when column mode on |
  | ⚙️ | Command Palette | `Ctrl+P` | Opens command palette & shortcuts | Bottom anchor |
  | 🌓 | Toggle Theme | `Ctrl+U` | Toggles Dark / Light theme | Bottom anchor |

- **Visual Cues**:
  - Active state: Left border 2px accent stripe (`theme.accent`), subtle hover/active surface highlight (`theme.btn_hover`).
  - Rich tooltips with keyboard shortcut badges via `SimpleTooltip`.

---

### 2.2 Forensic Faceted Quick Filter Bar
- **Data Model**:
  - `workspace.facet_tags: Vec<(String, usize)>` holding the top 7 most frequent logcat tags extracted from the active buffer.
  - `workspace.is_facets_visible: bool` (default: `true` when log source is present).
- **Fast Sampling Strategy**:
  - Extracts tags from the first 25,000 lines (or entire buffer if smaller) using `loganalysis_core::parse_column_view`.
  - Zero lock-up: computed synchronously or during buffer load/reapply.
- **Interactive Facet Chip**:
  - Displays `🏷️ <TagName> <Count>`.
  - **Left Click**: Instantly creates an `Include` filter rule matching `<TagName>`, assigns an auto-generated pastel color, and reapplies filters.
  - **Click Exclude (✕)**: Instantly creates an `Exclude` filter rule matching `<TagName>`.

---

### 2.3 Toggleable Filter Dock
- Add `is_filter_dock_visible: bool` (default: `true`) to `LogAnalysisWorkspace`.
- When `false`, the filter dock is omitted from GPUI layout, granting 100% width and height to the log viewport.
- Action: `ToggleFilterDock` (`Shift+F` / Activity Bar click).

---

### 2.4 Workstation Status Bar Telemetry
- **Buffer Memory Footprint**:
  - Reads `log_source.line_count()`, file size if available, or approximate memory (`lines * 96 bytes`).
  - Formatted cleanly: `💾 18.4 MB (MMap)` or `💾 32.1 MB (Heap)`.
- **Engine Processing Badge**:
  - Displays `⚡ Zero-Copy` or `⚡ SIMD Parallel`.
  - Tooltip explaining non-blocking threadpool & Rayon chunking.

---

## 3. Invariants & Guardrails
- **[INV-01] Zero-Copy MMap**: Preserved for all file inspection.
- **[INV-04] Zero-Allocation Search**: Preserved for all text highlighting.
- **[INV-06] Non-blocking Tasks**: Preserved; facet extraction runs bounded sampling.
- **[INV-11] Theme Tokens**: All new UI components must strictly consume `theme.rs` tokens.
- **[INV-14] Zero Warning**: Must compile cleanly with 0 clippy warnings and pass all 104+ tests.

---

## 4. Work Breakdown & Plan

1. **Workspace State Extensions**:
   - Add `is_filter_dock_visible: bool`.
   - Add `is_facets_visible: bool`.
   - Add `facet_tags: Vec<(String, usize)>` and `refresh_facet_tags()`.
   - Add `add_facet_filter(tag: &str, is_exclude: bool)`.
2. **Activity Bar Component**:
   - Create `crates/loganalysis-gui/src/components/activity_bar.rs`.
   - Integrate into root layout in `main.rs`.
3. **Forensic Facets Bar Component**:
   - Implement facet chip rendering above log view.
4. **Telemetry Metric in Status Bar**:
   - Add memory footprint and engine mode badges.
5. **Actions & Keyboard Bindings**:
   - Register `ToggleFilterDock` and `ToggleFacetsBar`.
6. **Testing & QA**:
   - Add unit tests for facet calculation and dock toggle.
   - Run `cargo clippy --workspace --all-targets` and `cargo test --workspace`.
   - Generate `docs/qa-ui-professional-phase3.md`.

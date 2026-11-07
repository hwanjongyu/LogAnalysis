# Comprehensive Architecture, Quality, and Resilience Review: LogAnalysis

**Project:** `loganalysis` (High-Performance GPU-Accelerated Log Analysis Workstation)  
**Version:** `0.2.2`  
**Target Repository:** [`/home/drew/github/hwanjongyu/loganalysis`](file:///home/drew/github/hwanjongyu/loganalysis)  
**Date of Audit:** November 7, 2026  
**Auditor:** Autonomous Systems & Architecture Review Specialist  
**Workspace Crates:**
- [`crates/loganalysis-core`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core) (Headless systems engine)
- [`crates/loganalysis-gui`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui) (GPUI 0.2.2 GPU-accelerated desktop workstation)

---

## 1. Executive Summary

A comprehensive architectural, code quality, test coverage, and resilience audit was conducted across the entire **LogAnalysis** workspace.

LogAnalysis is a specialized high-throughput log analysis tool built in Rust, engineered to handle multi-gigabyte log files and high-volume live ADB streams without memory exhaustion or UI thread lockup. The system is split cleanly into a pure headless engine (`loganalysis-core`) and an immediate/virtualized desktop application (`loganalysis-gui`) utilizing Zed's `gpui` 0.2.2 framework.

### Key Verification Metrics
- **Automated Test Suite**: **105 passed, 0 failed, 0 ignored** across 3 test binaries (`35` core unit tests, `38` integration engine tests, `32` GUI unit tests). Execution time: **0.22s**.
- **Clippy Static Analysis**: **0 warnings** (`cargo clippy --workspace --all-targets`).
- **Codebase Size**: **17,632 lines of code** across 37 Rust source files and test suites.
- **Structural Invariants Adherence**: **15 / 15 invariants** defined in [`docs/INVARIANTS.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/INVARIANTS.md) audited and confirmed.

### Scorecard Summary

| Dimension | Rating | Status | Summary |
| :--- | :---: | :---: | :--- |
| **System Architecture & Crate Isolation** | **9.5 / 10** | Excellent | Strict boundary separation; core has zero GUI dependencies; clean dependency flow. |
| **Memory Safety & Zero-Copy Efficiency** | **9.4 / 10** | Excellent | Mmap paging, direct-mapped line cache, zero-copy `Arc<str>` and `SharedString` rendering. |
| **Concurrency & Threading Resilience** | **9.2 / 10** | Strong | Decoupled cancellation generations, thread-isolated Rayon chunk counters, non-blocking I/O. |
| **Subprocess Safety & Crash Defense** | **9.3 / 10** | Strong | Linux kernel `PR_SET_PDEATHSIG` with parent PID race checks; strict serial input sanitization. |
| **IME & Unicode Alignment** | **9.6 / 10** | Outstanding | Custom `TextInput` with UTF-16 code unit to UTF-8 boundary mapping; CJK 2-column width calculations. |
| **Test Coverage & Regression Defense** | **8.8 / 10** | Strong | Thorough unit and integration test coverage for core and algorithms; GUI component rendering relies on logic tests. |
| **Maintainability & Code Hygiene** | **8.4 / 10** | Good (Refactor Advised) | `workspace.rs` has grown into a 2,990-line God Struct; sub-controllers recommended. |

---

## 2. Architecture & Crate Decomposition

The workspace follows a strict two-tier architecture:

```
┌─────────────────────────────────────────────────────────────┐
│                     loganalysis-gui                         │
│  (GPUI Desktop Workstation, Virtualized uniform_list,       │
│   Modals, Activity Bar, Facets Bar, Timeline, Find Widget)  │
└──────────────────────────────┬──────────────────────────────┘
                               │ depends on
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                    loganalysis-core                         │
│  (Mmap File Buffer, Ring Buffer, Multi-File Chrono-Merge,   │
│   Aho-Corasick SIMD Matcher, Rayon Engine, ADB Process,     │
│   Microsecond Timestamp Parser, HTML Dossier Export)        │
└─────────────────────────────────────────────────────────────┘
```

### 2.1. [`loganalysis-core`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core)

The core crate has zero dependencies on GPUI, rendering, or window systems. It is organized into 5 primary functional domains:

1. **Buffer Subsystem** ([`crates/loganalysis-core/src/buffer/`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/)):
   - [`file_buffer.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs): File-backed log buffer combining `memmap2` with line-offset indexing (`Vec<usize>`). Includes a 1,024-entry direct-mapped `RwLock` line cache returning `Arc<str>` for zero-copy access.
   - [`stream_buffer.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/stream_buffer.rs): Real-time rolling ring buffer backed by `VecDeque<Arc<str>>`. Configured with `MAX_MONITOR_LINES = 200,000` and `MONITOR_TRIM_BATCH = 20,000` chunked draining to prevent memory copying and reallocation thrash.
   - [`merged_buffer.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/merged_buffer.rs): Interleaves 2 or more log files into a unified chronological stream based on microsecond timestamps, while ensuring unparsed multi-line exception stack traces inherit their parent event's timestamp and source attribution.
   - [`timestamp.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/timestamp.rs): Sub-microsecond timestamp parser supporting Android threadtime (`MM-DD HH:MM:SS.mmm`), ISO-8601, bracketed timestamps, year-end wrap-around, and negative clock skew calculation.
   - [`correlation.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/correlation.rs): $O(\log N)$ binary search nearest-neighbor timestamp correlation for split-pane lockstep scrolling, augmented with a 4-line linear forward probe for stack traces.
   - [`column.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/column.rs): High-speed zero-copy sub-pass parser for structured threadtime lines (`time`, `pid`, `tid`, `level`, `tag`, `message`) using pointer-offset arithmetic.

2. **Filter Subsystem** ([`crates/loganalysis-core/src/filter/`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/)):
   - [`matcher.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/matcher.rs): Two-tier matcher architecture: `MultiFilterMatcher` builds Aho-Corasick SIMD automatons for literal rules (case-sensitive and case-insensitive), delegating regex rules to `regex::bytes::Regex` or `fancy_regex::Regex`.
   - [`engine.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs): Rayon chunked multi-core parallel filtering (`CHUNK_SIZE = 8,192`). Chunk workers maintain stack-local match hit counters to eliminate cache-line ping-pong (false sharing). Independent reverse style inheritance preserves background and foreground colors accurately.
   - [`heatmap.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/heatmap.rs): Scrollbar error and warning density binning using parallel reduction for large log sets.
   - [`timeline.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/timeline.rs): Real-time events-per-second (EPS) timeline aggregator with $\mu + 3\sigma$ statistical anomaly detection.
   - [`contrast.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/contrast.rs): WCAG AA relative luminance contrast calculator ($\ge 4.5:1$).

3. **ADB Subsystem** ([`crates/loganalysis-core/src/adb/`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/)):
   - [`device.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs): SDK/PATH auto-discovery, safe serial validation (`^[a-zA-Z0-9._:-]+$`), and 3-second bounded timeout device enumeration.
   - [`logcat.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/logcat.rs): Subprocess lifecycle with RAII `LogcatStreamHandle`, Linux kernel-level `PR_SET_PDEATHSIG` cleanup with parent PID validation, and bounded 64 KB line reading.

4. **Export Subsystem** ([`crates/loganalysis-core/src/export/`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/export/)):
   - [`bookmark.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/export/bookmark.rs): `BTreeSet`-backed bookmark manager with $O(\log N)$ toggling, line annotations, and forward/backward cyclic navigation.
   - [`html_report.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/export/html_report.rs): Zero-dependency self-contained HTML incident report generator with complete HTML entity escaping (`escape_html`).

5. **Search Subsystem** ([`crates/loganalysis-core/src/search/`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/search/)):
   - [`searcher.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/search/searcher.rs): Cyclic search forward and backward across visible indices with generation-based cancellation checks every 1,024 lines.
   - [`fuzzy.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/search/fuzzy.rs): Subsequence fuzzy matching with acronym, prefix, word boundary, and camelCase bonuses.

### 2.2. [`loganalysis-gui`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui)

The GUI crate implements a desktop log workstation using GPUI:
- [`workspace.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs): Central state machine managing data models, background executor tasks, and streaming batches.
- [`log_view.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs): Virtualized rendering via `uniform_list`. Renders only rows intersecting the viewport. Converts `Arc<str>` to `SharedString` without heap allocation.
- [`text_input.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs): Full-featured text editor supporting multi-byte IME composition (Korean, Japanese, Chinese), UTF-16 to UTF-8 boundary conversions, selection, clipboard, and keybindings.
- [`dialogs.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/dialogs.rs): Native file dialog bindings with extension filtering and Linux X11 panic defenses (`CustomX11WindowHandle` to bypass GPUI's `unimplemented!()` window handle on X11).
- [`modals/`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/modals/): Command palette (`command_palette.rs`), filter rule editor (`filter_edit_modal.rs`), bookmark drawer (`bookmark_drawer.rs`), and bookmark note editor (`note_edit_modal.rs`).

---

## 3. Code Quality & Rust Idioms

### 3.1. Strengths & Best Practices

1. **Zero-Warning Clean Build & Clippy**:
   The workspace passes both `cargo check` and `cargo clippy --workspace --all-targets` with 0 warnings.
2. **Defensive Subprocess Management (`[INV-12]`)**:
   In both `logcat.rs` and `device.rs`, Linux subprocesses are spawned using `libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL)` within `pre_exec`. Furthermore, the potential parent death race between fork and `pre_exec` is neutralized by capturing `parent_pid = libc::getpid()` before fork and verifying `if libc::getppid() != parent_pid { libc::_exit(1); }`.
3. **Decoupled Cancellation Tokens (`[INV-02-A]`)**:
   `filter_cancel_gen` and `search_cancel_gen` are separate atomic integers (`Arc<AtomicU64>`). Typing in the search bar or dismissing the search widget does not abort in-flight filtering passes.
4. **SIMD & Multi-Pattern Automaton (`MultiFilterMatcher`)**:
   Combining active literal filter rules into a unified Aho-Corasick automaton reduces scanning complexity from $O(N \cdot K)$ to $O(N)$, drastically accelerating line evaluation on large datasets.
5. **IME Boundary Alignment & Character Integrity (`[INV-08-A]`)**:
   [`utf16_range_to_utf8_in_str`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L66) safely iterates `chars()`, accumulating UTF-16 code units while tracking UTF-8 byte lengths. Slicing never lands within a multi-byte UTF-8 code point, eliminating runtime panics on CJK preedit.
6. **Atomic File Persistence (`[INV-10]`)**:
   Filter profile JSON and recent files JSON are written to a `.tmp` sibling file before atomic replacement via `std::fs::rename`, preventing 0-byte truncation on system crashes.
7. **Lossy UTF-8 & Bounded Memory Defense (`[INV-03-A]`)**:
   ADB streaming uses `take(MAX_STREAM_LINE_BYTES)` (64 KB) and `String::from_utf8_lossy`, preventing malicious or malformed infinite streams from causing out-of-memory panics.

---

## 4. Test Coverage Analysis

### 4.1. Test Distribution & Verification

The project includes **105 automated unit and integration tests** distributed across 3 test suites:

| Suite | File / Location | Test Count | Key Areas Covered |
| :--- | :--- | :---: | :--- |
| **Core Unit Tests** | `crates/loganalysis-core/src/lib.rs` | **35** | ADB parsing/serial validation, column slicing, timestamp parsing, clock skew, wrap-around, merge ordering, bookmarks, HTML XSS escaping, heatmap binning, timeline anomaly detection, fuzzy matching. |
| **Engine Integration** | `crates/loganalysis-core/tests/engine_tests.rs` | **38** | Rayon multi-chunk filtering, cancellation abortion, ring buffer trim hysteresis, hit count consistency, SIMD matcher, zero-width regex safety, WCAG contrast, ADB timeout reaper, automotive presets. |
| **GUI Unit Tests** | `crates/loganalysis-gui/src/main.rs` | **32** | CLI `--version` early exit, IME UTF-16/UTF-8 mapping, word boundaries, X11 handle safety, dialog filter specs, stderr resilience, delta-time calculation, recent files LRU persistence, command palette catalog. |

All tests execute in **~0.22 seconds**, maintaining deterministic performance.

### 4.2. Coverage Gaps & Blind Spots

While algorithmic and core business logic is thoroughly covered, the following areas have coverage gaps:

1. **GPUI Visual Component Rendering**:
   GUI layout elements (`activity_bar.rs`, `facets_bar.rs`, `toolbar.rs`, `status_bar.rs`, `timeline_sparkline.rs`, `filter_dock.rs`) lack visual snapshot or component-level automated tests. Tests verify data models and helper functions, but actual GPUI element tree rendering is not tested in automated CI.
2. **File I/O Failure Paths in `workspace.rs`**:
   Tests for `FileLogBuffer::open` test empty files and normal logs, but `workspace.rs` error paths (e.g. attempting to open an unreadable file, permission-denied file, or invalid UTF-16 file) rely on runtime status string logging rather than automated integration assertions.
3. **Dual-Pane Timestamp Lockstep on Stack Trace Rows**:
   Tests verify correlation when both panes contain valid timestamps, but correlation when the selected source row lacks a timestamp (e.g., an exception trace line) is untested.

---

## 5. Potential Issues & Technical Debt

### 5.1. Performance & Concurrency Considerations

#### Issue 1: Synchronous `HeatmapData::compute` on the UI Thread During High-Rate Streaming (P1 / Major)
- **Location**: [`workspace.rs#L753-L758`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L753-L758)
- **Detail**: When receiving streaming ADB logcat batches, `HeatmapData::compute` is called synchronously inside `this.update` (on the GPUI main thread) on every incoming batch:
  ```rust
  workspace.heatmap_data = Some(Arc::new(HeatmapData::compute(
      &**src_ref,
      &res_mut.visible_indices,
      &res_mut.visible_styles,
      200,
  )));
  ```
  Inside [`HeatmapData::compute`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/heatmap.rs#L85), when `total_lines >= 40_000`, it invokes Rayon:
  ```rust
  visible_indices.par_iter().enumerate().fold(...)
  ```
  Spawning a Rayon parallel reduction on the main UI thread blocks the GPUI render loop until all worker threads complete. Under log bursts (20,000–50,000 lines/sec), running this reduction repeatedly every 16ms can cause UI frame drops.
- **Recommendation**: Compute `HeatmapData` asynchronously on the `background_executor` or throttle its recomputation to at most once every 150–250ms during active streaming.

---

### 5.2. Functional Edge Cases & Correctness

#### Issue 2: Dual-Pane Lockstep Correlation Skips Stack Trace Rows (P2 / Minor)
- **Location**: [`workspace.rs#L2745-L2751`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L2745-L2751) and [`#L2774-L2779`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L2774-L2779)
- **Detail**:
  ```rust
  let Some(raw_line) = primary_src.get_raw_line(primary_source_idx) else { return; };
  let Some(ts) = extract_timestamp(raw_line) else { return; };
  ```
  If the user selects a multi-line exception stack trace or an unparsed log line in the primary pane, `extract_timestamp(raw_line)` returns `None`, and the function returns immediately. Even though [`find_nearest_timestamp_row`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/correlation.rs#L24) includes a 4-line forward probe in the target buffer, the *source* row's timestamp is never resolved if the clicked line is unparsed.
- **Recommendation**: In `sync_scroll_to_secondary` and `sync_scroll_to_primary`, if `extract_timestamp(raw_line)` returns `None`, probe backwards up to 10 lines (or use the preceding timestamp) to locate the parent event's timestamp.

#### Issue 3: Merged Log Buffer Wraps When Merging $\ge 256$ Files (P3 / Edge Case)
- **Location**: [`merged_buffer.rs#L85`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/merged_buffer.rs#L85)
- **Detail**:
  ```rust
  merged_index.push(MergedLineRef {
      source_id: s_id as u8,
      source_line_idx: l_idx,
      timestamp_micros: ts,
  });
  ```
  `MergedLineRef::source_id` is defined as `u8`. If a user opens 256 or more files simultaneously, `s_id as u8` will wrap to `0`, causing source file attribution and color tagging to collide.
- **Recommendation**: Enforce an upper bound validation check:
  ```rust
  if sources.len() > 255 {
      return Err(std::io::Error::new(
          std::io::ErrorKind::InvalidInput,
          "Maximum number of merged files is 255",
      ));
  }
  ```

#### Issue 4: `rename_current_tab` Placeholder Logic (P3 / UX Polish)
- **Location**: [`workspace.rs#L1967-L1974`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L1967-L1974)
- **Detail**:
  ```rust
  pub fn rename_current_tab(&mut self, cx: &mut Context<Self>) {
      if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
          let next_name = format!("{}_renamed", tab.name);
          tab.name = next_name.clone();
          ...
      }
  }
  ```
  Invoking Rename Tab (<kbd>Ctrl+R</kbd> or command palette) automatically appends `_renamed` without offering an input dialog or inline text field for the user to type a custom name.
- **Recommendation**: Introduce a lightweight tab rename modal or inline text input in the filter dock.

#### Issue 5: Logging User Directory Resolution on Windows (P3 / Portability)
- **Location**: [`logging.rs#L68-L76`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/logging.rs#L68-L76)
- **Detail**:
  ```rust
  if let Ok(home) = std::env::var("HOME") {
      let user_share = PathBuf::from(home)
          .join(".local")
          .join("share")
          .join("loganalysis");
      ...
  }
  ```
  On Windows systems, `%HOME%` is typically unset, while `%USERPROFILE%` or `%APPDATA%` is defined. As a result, persistent logging on Windows only writes to the current working directory.
- **Recommendation**: Check `USERPROFILE` or `APPDATA` if `HOME` is missing, mirroring the implementation in `adb/mod.rs` and `workspace.rs`.

---

### 5.3. Architecture & Maintainability

#### Issue 6: Monolithic Controller in `workspace.rs` (P2 / Architecture Debt)
- **Location**: [`crates/loganalysis-gui/src/workspace.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs) (2,990 lines)
- **Detail**:
  `LogAnalysisWorkspace` currently contains 42 struct fields and manages:
  1. Primary & Secondary buffer/filter state
  2. Live ADB streaming lifecycle and channel pumping
  3. Filter tabs, rules, and modal dialogs
  4. Search bar, cyclic matching, and cached matchers
  5. Split view and scroll synchronization
  6. Bookmarks and HTML dossier export
  7. Command palette catalog and actions
  8. Recent files LRU and atomic persistence
  9. Workstation layout (CSD, Activity Bar, Facets, Filter Dock)
- **Recommendation**: Factor out cohesive sub-state structs into dedicated controllers under `crates/loganalysis-gui/src/controllers/`:
  - `AdbController` (device listing, stream handle, pause buffer)
  - `SplitViewController` (secondary buffer, sync flags, layout orientation)
  - `BookmarkController` (bookmarks manager, notes, drawer state)
  - `SearchController` (query, flags, matcher, cancel generation)

---

## 6. Structural Invariants Audit Table

All 15 invariant specifications from [`docs/INVARIANTS.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/INVARIANTS.md) were audited against the codebase:

| Invariant ID | Rule Summary | Audited Code Symbol | Result |
| :--- | :--- | :--- | :---: |
| **`[INV-01-A]`** | Native file dialog extension filter preservation (`*.log`, `*.txt`, `*.json`, `*.html`, `*.*`) | [`dialogs.rs#L3-L14`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/dialogs.rs#L3-L14) | **PASS** |
| **`[INV-01-B]`** | Modal window hierarchy & Linux X11 panic defense | [`dialogs.rs#L16-L45`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/dialogs.rs#L16-L45) | **PASS** |
| **`[INV-02-A]`** | Strict cancellation token independence (`filter_cancel_gen` vs `search_cancel_gen`) | [`workspace.rs#L24-L38`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L24-L38) | **PASS** |
| **`[INV-02-B]`** | UI Main Thread Non-Blocking Rule (Disk I/O and filtering in background executor) | [`workspace.rs#L880-L915`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L880-L915) | **PASS** |
| **`[INV-03-A]`** | ADB Streaming bounded line reader (`MAX_STREAM_LINE_BYTES = 64KB`) | [`logcat.rs#L112-L135`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/logcat.rs#L112-L135) | **PASS** |
| **`[INV-03-B]`** | Ring buffer trim hysteresis (`MAX = 200,000`, `BATCH = 20,000`) | [`stream_buffer.rs#L50-L75`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/stream_buffer.rs#L50-L75) | **PASS** |
| **`[INV-04]`** | CJK / East Asian 2-column monospace width | [`unicode-width` in `stream_buffer.rs` & `file_buffer.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/stream_buffer.rs#L40-L45) | **PASS** |
| **`[INV-05]`** | CLI Version flag early termination without GPU window init | [`cli.rs#L6-L19`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/cli.rs#L6-L19), [`main.rs#L420-L425`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs#L420-L425) | **PASS** |
| **`[INV-06]`** | ADB subprocess cleanup on source transition and log clear | [`workspace.rs#L1442-L1447`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L1442-L1447) | **PASS** |
| **`[INV-07]`** | ADB serial alphanumeric validation & 3s timeout reaper | [`device.rs#L73-L80`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs#L73-L80), [`#L115-L140`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs#L115-L140) | **PASS** |
| **`[INV-08-A]`** | UTF-16 code unit to UTF-8 character boundary alignment in `TextInput` | [`text_input.rs#L66-L84`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L66-L84) | **PASS** |
| **`[INV-08-B]`** | IME commit Enter event stop propagation | [`text_input.rs#L470-L475`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L470-L475) | **PASS** |
| **`[INV-09]`** | Exact hit counter decrement on ring buffer trim | [`engine.rs#L350-L375`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L350-L375) | **PASS** |
| **`[INV-10]`** | Atomic filter profile JSON saving via temporary file replacement | [`workspace.rs#L2100-L2125`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L2100-L2125) | **PASS** |
| **`[INV-11]`** | High-contrast theme inheritance & WCAG AA ratio | [`engine.rs#L55-L75`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L55-L75), [`contrast.rs#L10-L45`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/contrast.rs#L10-L45) | **PASS** |
| **`[INV-12]`** | Linux kernel subprocess reaper `PR_SET_PDEATHSIG` with parent PID race defense | [`logcat.rs#L70-L80`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/logcat.rs#L70-L80), [`device.rs#L115-L125`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs#L115-L125) | **PASS** |
| **`[INV-13]`** | Dual-pane split lockstep correlation via binary search | [`correlation.rs#L8-L55`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/correlation.rs#L8-L55) | **PASS** |
| **`[INV-14]`** | Multi-file chronological merge stability & stack trace continuity | [`merged_buffer.rs#L55-L105`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/merged_buffer.rs#L55-L105) | **PASS** |
| **`[INV-15]`** | Zero-dependency standalone HTML dossier export with complete XSS escaping | [`html_report.rs#L18-L95`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/export/html_report.rs#L18-L95) | **PASS** |

---

## 7. Actionable Recommendations & Prioritized Roadmap

### Priority 1: High Impact (Performance & Stability)
1. **Asynchronous / Throttled Heatmap Computation During Streaming**:
   Offload `HeatmapData::compute` from the UI thread in `workspace.rs#L753` to the background executor, or throttle its execution to at most once per 200ms while active streaming is ongoing.

### Priority 2: Correctness & Usability
2. **Stack Trace Parent Timestamp Probe for Split Lockstep**:
   In `sync_scroll_to_secondary` and `sync_scroll_to_primary`, when `extract_timestamp(raw_line)` returns `None`, probe backward up to 10 lines to find the timestamp of the enclosing log event.
3. **`MergedLogBuffer` Upper Bound Guard**:
   Add `if sources.len() > 255 { return Err(...); }` to prevent `u8` wrapping of `source_id`.
4. **Interactive Tab Rename**:
   Replace the automatic `_renamed` string append in `rename_current_tab` with an input modal or inline edit state.

### Priority 3: Architecture & Hygiene
5. **Modularize `workspace.rs`**:
   Extract `AdbController`, `SplitViewController`, and `SearchController` out of `workspace.rs` to keep individual files under 1,000 lines.
6. **Windows Log Directory Discovery**:
   Update `logging.rs` to fallback to `USERPROFILE` or `APPDATA` on Windows.

# Authoritative Architecture, Code Quality, Safety, and Resilience Review: LogAnalysis

**Project:** `loganalysis` (High-Performance GPU-Accelerated Android Log Analysis Workstation)  
**Workspace Root:** [`/home/drew/github/hwanjongyu/loganalysis`](file:///home/drew/github/hwanjongyu/loganalysis)  
**Date of Audit:** November 14, 2026  
**Auditor:** Teamwork Architecture Review & Quality Synthesis Agent (`teamwork_worker_m1_2`)  
**Workspace Crates Audited:**
- [`crates/loganalysis-core`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core) (Zero-GUI headless systems engine)
- [`crates/loganalysis-gui`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui) (GPUI 0.2.2 GPU-accelerated desktop workstation)

---

## 1. Executive Summary

### 1.1. Project Mission & High-Level Evaluation

LogAnalysis is an open-source, high-throughput desktop workstation engineered for systems developers, Android platform engineers, and automotive infotainment specialists. Written in Rust, it solves the challenge of analyzing multi-gigabyte logcat captures, real-time ADB diagnostic streams, and multi-device chronological merges without UI thread lockup or memory exhaustion.

The system is structured as a two-crate Cargo workspace:
1. **`loganalysis-core`**: A headless library providing high-speed in-memory ring buffers (`StreamBuffer`), memory-mapped file buffers (`FileBuffer`), multi-file chronological log interleaving (`MergedBuffer`), SIMD-accelerated literal matching via Aho-Corasick, multi-core parallel filtering via Rayon, microsecond timestamp parsing, and self-contained HTML incident report generation.
2. **`loganalysis-gui`**: A desktop application utilizing Zed's immediate/retained-mode `gpui` (0.2.2) framework, delivering 60+ FPS virtualized log list rendering (`uniform_list`), bidirectional delta-time correlation, multi-source split views, dynamic scrollbar error heatmaps, and full CJK/IME-aware text editing.

While the foundational data structures and zero-copy data paths exhibit sophisticated systems programming, **the codebase currently suffers from severe architectural divergence, a broken CI build pipeline, cross-platform persistence and hang hazards, and a race condition during live ADB streaming.**

### 1.2. System Health Scorecard

| Dimension | Rating | Status | Summary |
| :--- | :---: | :---: | :--- |
| **System Architecture & Modularity** | **6.5 / 10** | Needs Attention | Core engine is cleanly separated, but the GUI suffers an architectural schism: an entire MVC controller module is orphaned dead code while `LogAnalysisWorkspace` has grown into a 50+ field God Object (MAJ-02). |
| **Build & CI Health** | **5.0 / 10** | **Action Required** | `cargo clippy --all-targets --all-features` fails with exit code 101 due to 7 reversed empty range errors in test code (MAJ-01). CI workflow omits Clippy, concealing compiler warnings. |
| **Safety & Concurrency** | **6.8 / 10** | Needs Attention | Live streaming ADB filtering suffers a race condition that drops lines from view (CRIT-01); ring buffer eviction desynchronizes visible row selection (CRIT-02); external file truncation causes uncatchable `SIGBUS` crashes (MAJ-05). |
| **Cross-Platform Portability** | **6.2 / 10** | Needs Attention | Subprocess non-blocking pipe reads are Linux-only (causing hangs on macOS/Windows, MAJ-04); atomic file persistence fails silently on Windows due to `std::fs::rename` semantics (MAJ-03). |
| **Code Quality & Maintainability** | **7.0 / 10** | Fair | 64.8% of `main.rs` is deadweight test code using fragile `include_str!` source string matching (MAJ-11); extensive copy-pasting in escape handling, analytics dispatch, and dialog parenting (MIN-04, MIN-05, MIN-06). |
| **Error Handling & UX Transparency** | **6.5 / 10** | Needs Attention | Invalid search regular expressions are silently swallowed as "0 matches" (MAJ-08); invalid filter regex rules are saved without validation and dropped silently during engine execution (MAJ-09). |
| **Core Algorithmic Performance** | **8.5 / 10** | Strong | High-speed SIMD filtering, sub-microsecond timestamps, zero-copy line caching; undermined by hot-loop automaton recompilation (MAJ-06), buffer cloning fallbacks (MAJ-07), and timestamp leap-year calendar drift (MAJ-10). |

### 1.3. Key Metrics & Verification Dashboard

- **Automated Test Suite**: **175 unit and integration tests passing** across workspace (`70` core unit tests, `38` engine integration tests, `2` core stress tests, `5` core adversarial tests, `60` GUI unit tests). Execution duration: **~0.65s**.
- **Clippy Static Analysis**: **FAILED (Exit Code 101)**. 7 default-deny compilation errors in `crates/loganalysis-gui/src/components/text_input.rs:1001-1186` (`clippy::reversed_empty_ranges`), plus 4 warnings across adversarial and stress suites.
- **Rustdoc Documentation**: **1 Warning** in `crates/loganalysis-core/src/buffer/mod.rs:126` (`unclosed HTML tag str`).
- **Dependency Incompatibilities**: `proc-macro-error2 v2.0.1` triggers `warning[E0365]` (future compiler rejection).
- **Codebase Volume**: **78 Rust source files, ~26,500 total lines of code** (68 files / 21,500 lines in `src/`). `crates/loganalysis-gui/src/main.rs` alone is 1,759 lines, of which 1,141 lines are inline test assertions.

### 1.4. Key Strengths of the Codebase

1. **Zero-Copy & Direct-Mapped Line Indexing**: `FileBuffer` combines `memmap2` with byte-offset indexing, augmented by a 1,024-entry direct-mapped `RwLock` cache returning `Arc<str>` references to eliminate heap allocations during virtual scrolling.
2. **Two-Tier SIMD Pattern Matching**: `MultiFilterMatcher` compiles literal patterns into unified Aho-Corasick automatons (both case-sensitive and case-insensitive), delegating only complex expressions to `fancy_regex`.
3. **Sub-Microsecond Timestamp & Clock Skew Engine**: Custom fast path in `timestamp.rs` parses Android threadtime (`MM-DD HH:MM:SS.mmm`) without heap allocation, accommodating cross-device clock skew, year-end rollovers, and stack trace inheritance.
4. **Defensive Subprocess Termination (`PR_SET_PDEATHSIG`)**: On Linux, ADB subprocesses are bound to the parent PID using kernel `prctl(PR_SET_PDEATHSIG, SIGKILL)` combined with parent-PID validation in `pre_exec` to eliminate zombie processes.
5. **IME Composition Alignment**: `TextInput` features sophisticated UTF-16 code unit to UTF-8 byte offset translations and CJK double-width calculation, preventing mid-codepoint panics during East Asian text composition.

---

## 2. Workspace Architecture & Component Topography

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                                crates/loganalysis-gui                                  │
│  ┌──────────────────────────────────────────────────────────────────────────────────┐  │
│  │                    LogAnalysisWorkspace (Monolithic God Object)                  │  │
│  │  - Viewport & Split Views     - Filter Tabs & Rule States   - Selection Model    │  │
│  │  - Live ADB Stream Lifecycle  - Search & Match Navigation   - Bookmark Manager   │  │
│  └───────────────────────────────────────┬──────────────────────────────────────────┘  │
│         │                                │                                │            │
│         ▼                                ▼                                ▼            │
│  ┌──────────────┐              ┌──────────────────┐             ┌───────────────────┐  │
│  │  Components  │              │ [DEAD] Controllers│             │     Dialogs /     │  │
│  │  - log_view  │              │ - AdbController   │             │     Logging /     │  │
│  │  - find_bar  │              │ - BookmarkCtrl    │             │     Theme /       │  │
│  │  - filter_dock              │ - SearchController│             │     TextInput     │  │
│  │  - titlebar  │              │ - SplitViewCtrl   │             │                   │  │
│  └──────────────┘              └──────────────────┘             └───────────────────┘  │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            │ depends on
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                               crates/loganalysis-core                                  │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌────────────────────────────────┐  │
│  │  Buffer Subsystem   │  │  Filter Subsystem   │  │  ADB Subprocess Bridge         │  │
│  │  - FileBuffer       │  │  - MultiFilterMatch │  │  - AdbDevice (Enumeration)     │  │
│  │  - StreamBuffer     │  │  - FilterEngine     │  │  - AdbLogcatStream (Streaming) │  │
│  │  - MergedBuffer     │  │  - HeatmapData      │  │                                │  │
│  │  - Timestamp Parser │  │  - EpsTimelineData  │  │                                │  │
│  └─────────────────────┘  └─────────────────────┘  └────────────────────────────────┘  │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌────────────────────────────────┐  │
│  │  Search Subsystem   │  │  Export Subsystem   │  │  Constants & Common Errors     │  │
│  │  - Cyclic Searcher  │  │  - BookmarkManager  │  │  - ASYNC_SEARCH_THRESHOLD      │  │
│  │  - Fuzzy Matcher    │  │  - HtmlReport (XSS) │  │  - COLOR_MAP (Color Registry)  │  │
│  └─────────────────────┘  └─────────────────────┘  └────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### 2.1. Crate Decomposition

#### [`crates/loganalysis-core`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core)
A pure headless library containing no graphical, GPUI, or windowing system dependencies. It compiles cleanly on server targets and exposes thread-safe, lock-free, or fine-grained lock abstractions:
- **`buffer/`**: In-memory and memory-mapped log storage. Exposes the `LogSource` and `LogBuffer` traits implemented by `FileLogBuffer` (mmap), `StreamLogBuffer` (bounded ring buffer), and `MergedLogBuffer` (multi-file chronological interleaving).
- **`filter/`**: Two-tier filtering engine. Compiles literal inclusion/exclusion rules into `aho_corasick::AhoCorasick` automatons and parallelizes batch evaluation across CPU cores with `rayon`.
- **`adb/`**: Child process management for the Android Debug Bridge. Discovers binaries, enumerates connected devices, and manages continuous `adb logcat` stdout pipelines.
- **`search/`**: High-performance forward and backward text scanning, with cancellation tokens and subsequence fuzzy matching.
- **`export/`**: Bookmark persistence and standalone HTML report generation with zero external web dependencies.

#### [`crates/loganalysis-gui`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui)
The desktop application compiled as a binary (`loganalysis`). It interacts with the operating system through GPUI, handling user input, window composition, and async tasks:
- **`workspace/`**: The state center of the application, coordinating background tasks, filter re-indexing, ADB streaming packets, dialog prompts, and split views.
- **`components/`**: GPUI visual elements, notably `log_view.rs` (uniform list virtualizer), `find_bar.rs` (incremental search HUD), `filter_dock.rs` (rule management panel), and `text_input.rs` (IME-compliant text field).
- **`controllers/`**: An orphaned sub-package containing four complete MVC controllers (`AdbController`, `BookmarkController`, `SearchController`, `SplitViewController`) which are completely disconnected from the actual workspace.

---

## 3. Deep-Dive Domain Analyses

### 3.1. System Architecture & Structural Design

#### 3.1.1. The Monolithic God Object vs. Orphaned Controller Architecture
The most glaring architectural flaw in `loganalysis-gui` is a halfway-abandoned refactoring. The repository contains a dedicated directory [`crates/loganalysis-gui/src/controllers/`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/controllers/) defining:
- `AdbController` (device selection, stream controls, package filters)
- `BookmarkController` (bookmark toggles, notes, navigation)
- `SearchController` (incremental search, regex, match indexing)
- `SplitViewController` (pane orientation, synchronized scrolling)

These controllers are accompanied by decoupled state models in [`crates/loganalysis-gui/src/types.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/types.rs) (`AdbStreamState`, `SearchState`, `SplitViewState`, `BookmarkDrawerState`), all explicitly annotated with `#[allow(dead_code)]`.

However, an audit of `crates/loganalysis-gui/src/workspace/mod.rs` reveals that `LogAnalysisWorkspace` **never instantiates, stores, or invokes any of these controllers**. Instead, `LogAnalysisWorkspace` is a 1,248-line struct maintaining over 50 public primitive fields that directly duplicate the state of the controllers. This creates an architectural schism: developers inspecting `controllers/` believe the system follows clean MVC separation, while the actual application mutates monolithic workspace state in place.

#### 3.1.2. Hot-Path Automaton Recompilation in Streaming Loop
In [`crates/loganalysis-core/src/filter/engine.rs:380-387, 434-442`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L380), the actual streaming ingestion entry point is implemented as:
```rust
pub fn append_streaming_batch(
    result: &mut FilterResult,
    source: &(dyn LogSource + Sync),
    new_lines_range: std::ops::Range<usize>,
    trimmed_count: usize,
    filters: &[FilterRule],
    show_only_filtered: bool,
) {
    // ...
    let filter_count = filters.len();
    let mut prepared = Vec::new();
    for (idx, rule) in filters.iter().enumerate() {
        if rule.active && !rule.text.is_empty() {
            if let Ok(pf) = PreparedFilter::new(rule.clone(), idx) {
                prepared.push(pf);
            }
        }
    }
    let multi_matcher = MultiFilterMatcher::new(&prepared);
    // ...
```
During active ADB logcat streaming, batches arrive every 16ms to 150ms. On every single batch, `append_streaming_batch` loops over `filters`, clones each active rule, executes `PreparedFilter::new(rule.clone(), idx)` (which compiles regexes if applicable), and calls `MultiFilterMatcher::new(&prepared)` from scratch. This repeatedly allocates and constructs new Aho-Corasick automatons and DFA state tables directly on the latency-critical UI rendering path.

#### 3.1.3. Buffer Clone Thrashing Under Concurrent Analytics
In [`crates/loganalysis-gui/src/workspace/adb.rs:218-235`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L218), incoming streaming log batches attempt to obtain mutable access to the active log source:
```rust
if let Some(stream_buf) = Arc::get_mut(&mut self.log_source) {
    stream_buf.append_batch(batch);
} else {
    let mut new_buf = (*self.log_source).clone();
    new_buf.append_batch(batch);
    self.log_source = Arc::new(new_buf);
}
```
`self.log_source` is an `Arc<ActiveLogSource>`. When background tasks (such as scrollbar heatmap calculation, EPS timeline aggregation, or async search indexing) hold a cloned `Arc` handle, `Arc::get_mut` returns `None`. The fallback path forces a full deep clone of the 200,000-line `StreamBuffer` on every incoming packet. Under active ADB streaming while viewing a heatmap, the system enters an allocation-thrashing loop copying multi-megabyte buffers several times per second.

---

### 3.2. Code Quality, Maintainability & Test Anti-Patterns

#### 3.2.1. Source Code String-Matching Anti-Pattern & `main.rs` Bloat
[`crates/loganalysis-gui/src/main.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs) has expanded to 1,759 lines. Lines 618–1759 (1,141 lines, or **64.8% of the file**) consist entirely of unit tests.

Crucially, multiple tests in this suite do not execute application logic. Instead, they compile Rust source files into the binary as string literals using `include_str!` and assert that specific variable names or DOM IDs exist in the source code:
```rust
#[test]
fn test_unified_compact_header_spec_and_dom_ids() {
    let toolbar_src = include_str!("components/toolbar.rs");
    let header_src = include_str!("components/titlebar.rs");
    let workspace_src = include_str!("workspace/mod.rs");

    assert!(toolbar_src.contains("id(\"unified-compact-header\")") || header_src.contains("id(\"unified-compact-header\")"));
    assert!(toolbar_src.contains("id(\"zen-focus-toggle\")"));
    assert!(workspace_src.contains("is_zen_mode: bool"));
}
```
This is a severe testing anti-pattern. These tests fail if code is reformatted, renamed, or refactored, yet pass even if the UI element is completely broken, unrendered, or fails to handle clicks.

#### 3.2.2. Clippy CI Breakage on Reversed Empty Ranges
Executing `cargo clippy --all-targets --all-features` fails immediately with exit code 101:
```
error: this range is empty so it will yield no values
   --> crates/loganalysis-gui/src/components/text_input.rs:1001:55
    |
1001|        assert_eq!(clamp_range_to_char_boundaries("", 10..2), 0..0);
    |                                                      ^^^^^
    = note: `#[deny(clippy::reversed_empty_ranges)]` on by default
```
This error occurs across 7 distinct test cases in `text_input.rs` (`1001`, `1009`, `1013`, `1024`, `1062`, `1099`, `1186`). Because `.github/workflows/ci.yml` runs only `cargo test` and omits `cargo clippy`, this default-deny error slipped into the repository and blocks standard developer workflows and local pre-commit checks.

#### 3.2.3. Extensive Code Duplication
1. **Escape Key Handling**: 48 lines of identical modal/panel dismissal logic are duplicated verbatim between `crates/loganalysis-gui/src/main.rs:150-197` and `crates/loganalysis-gui/src/workspace/mod.rs:601-648`.
2. **Async Analytics Dispatch**: Identical 25-line task spawning blocks calculating `HeatmapData` and `EpsTimelineData` are duplicated across three separate handlers in `crates/loganalysis-gui/src/workspace/adb.rs:326, 388, 442`.
3. **X11 Dialog Parenting**: Approximately 30 lines of boilerplate querying X11 window IDs are duplicated across 6 file dialog invocation points in `workspace/mod.rs`, `filters.rs`, `merged.rs`, `split_view.rs`, and `bookmarks.rs`.

---

### 3.3. Error Handling & Cross-Platform Robustness

#### 3.3.1. Silent Regex Error Swallowing in Search & Filter Engines
In [`crates/loganalysis-gui/src/workspace/search.rs:76-107`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/search.rs#L76), when a user executes a regex search with an invalid pattern, the engine returns `Err(SearchError::InvalidRegex(msg))`. However, the workspace collapses all non-Ok results into an `else` branch:
```rust
if let Ok(Some(found)) = search_res {
    // updates match status
} else {
    self.status_text = "Search pattern not found".into();
    self.search_match_status = Some("0 matches".into());
}
```
The user receives no notification that their regex is malformed (e.g., an unclosed bracket `[`). The UI displays "0 matches", leading the user to falsely believe the log file contains no occurrences of their search target.

Similarly, in [`crates/loganalysis-gui/src/workspace/filters.rs:195-231`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/filters.rs#L195), the filter edit modal allows users to save malformed regular expressions without syntax verification. Later, in [`crates/loganalysis-core/src/filter/engine.rs:437`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L437), the engine filters rules with `if let Ok(pf) = PreparedFilter::new(...)`, silently discarding the invalid rule without error logging or visual indication.

#### 3.3.2. Windows Atomic File Persistence Failure
In [`crates/loganalysis-gui/src/workspace/filters.rs:281`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/filters.rs#L281) and [`crates/loganalysis-gui/src/workspace/recent_files.rs:101`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/recent_files.rs#L101), atomic file replacement is implemented as:
```rust
let res = std::fs::write(&tmp_path, &json_bytes)
    .and_then(|_| std::fs::rename(&tmp_path, &path_for_bg));
if res.is_err() {
    let _ = std::fs::remove_file(&tmp_path);
}
```
While `std::fs::rename` atomically replaces destination files on POSIX/Linux, on Windows it calls `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`. If the destination file already exists, `std::fs::rename` immediately fails with `ERROR_ALREADY_EXISTS`. The cleanup code then deletes `tmp_path`. Consequently, on Windows, saving filter profiles or updating recent files fails silently on every save after the initial file creation.

#### 3.3.3. Subprocess Pipe Hang on macOS & Windows (`O_NONBLOCK` Gating)
In [`crates/loganalysis-core/src/adb/device.rs:142-153`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs#L142), non-blocking pipe reads for ADB device scanning are conditionally compiled:
```rust
#[cfg(target_os = "linux")]
{
    use std::os::unix::io::AsRawFd;
    let fd = stdout_pipe.as_raw_fd();
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
}
```
On macOS and Windows, `stdout_pipe` is left in standard blocking mode. When `stdout_pipe.read(&mut buf)` is called at line 165, the read halts the background thread synchronously. If the ADB daemon stalls, the timeout check at line 158 is unreachable, resulting in an indefinite background thread freeze.

#### 3.3.4. Memory-Mapped File Truncation Crash Hazard (`SIGBUS`)
In [`crates/loganalysis-core/src/buffer/file_buffer.rs:63-74`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs#L63), log files are loaded via `Mmap::map(&file)`. In real-world environments, log files can be truncated by external processes (e.g. `logrotate` or shell redirection `> file.log`). If a file is truncated while mapped, reading pages beyond the new file size raises a fatal OS `SIGBUS` signal on Unix. Standard Rust panic hooks cannot catch `SIGBUS`, terminating the application immediately without crash logs.

---

### 3.4. Safety, Concurrency & Security

#### 3.4.1. Race Condition in Live ADB Streaming Filter Reapplication
In [`crates/loganalysis-gui/src/workspace/adb.rs:218-288`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L218) and [`crates/loganalysis-gui/src/workspace/mod.rs:505-585`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/mod.rs#L505), filter recomputation (`reapply_filters`) captures a snapshot of `log_source` at line count $N$ and offloads filtering to a background thread.

While the background task executes, new log chunks arrive from ADB on the UI thread and are appended to `log_source`. When the background task completes, it executes:
```rust
workspace.filter_result = Some(Arc::new(res));
```
This unconditionally replaces `filter_result` with the snapshot result, wiping all log lines that arrived during background computation from `visible_indices`. Furthermore, if ring buffer trimming occurred while filtering was in flight, the indices in `res` point to shifted physical rows in `StreamBuffer`.

#### 3.4.2. Selection Index-Space Desynchronization on Ring Buffer Eviction
In [`crates/loganalysis-gui/src/workspace/adb.rs:240-243`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L240) and [`crates/loganalysis-gui/src/selection.rs:35-37`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/selection.rs#L35):
```rust
// adb.rs:242
if trimmed > 0 {
    workspace.selection.shift_on_trim(trimmed);
}

// selection.rs:36
if let Some(row) = self.selected_row {
    self.selected_row = Some(row.saturating_sub(trimmed));
}
```
`trimmed` is the physical line count evicted from `StreamBuffer` (e.g. 20,000 lines). `selected_row` is an index into `visible_indices` (the visual row in the filtered view, e.g. row 45). Passing `trimmed` directly into `saturating_sub(trimmed)` instantly collapses `selected_row` to 0. Whenever the ring buffer trims, the user's cursor is forcibly yanked to the top of the log view.

#### 3.4.3. Hardcoded Leap-Year Assumption and Epoch Drift in Timestamp Parser
In [`crates/loganalysis-core/src/buffer/timestamp.rs:3-5, 117, 154`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/timestamp.rs#L3):
```rust
pub const DAYS_BEFORE_MONTH: [u32; 13] = [
    0, 0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335,
];
// ...
let days = (year as i64 - 1970) * 365 + day_of_year(month, day) as i64;
```
`DAYS_BEFORE_MONTH[3]` is hardcoded to 60 (31 days of January + 29 days of February). This assumes every year is a leap year. In non-leap years (such as 2023, 2025, 2026, 2027), log entries crossing from February 28 to March 1 experience an artificial 48-hour jump (a 24-hour phantom jump). Furthermore, `(year - 1970) * 365` neglects all leap days since 1970, introducing a ~13-to-14-day epoch offset for modern timestamps. In `MergedLogBuffer`, where multi-device captures are interleaved chronologically based on parsed microseconds, this calendar math defect directly corrupts log sorting and delta-time calculations across files.

#### 3.4.4. Supply Chain Security in AppImage Build Script
In [`scripts/build-appimage.sh:70-77`](file:///home/drew/github/hwanjongyu/loganalysis/scripts/build-appimage.sh#L70), the release packaging pipeline downloads `appimagetool` directly from an unpinned continuous release URL and executes it with `--appimage-extract-and-run` without SHA256 checksum or GPG signature verification.

---

## 4. Master Categorized Findings Register

### Summary Matrix

| ID | Title | Severity | Category | File & Line Reference |
|---|---|---|---|---|
| **CRIT-01** | Live Streaming Filter Reapply Race Condition (Log Loss & Stale Indices) | **Critical** | Concurrency / Data Integrity | [`crates/loganalysis-gui/src/workspace/adb.rs:218-288`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L218), [`workspace/mod.rs:505-585`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/mod.rs#L505) |
| **CRIT-02** | Selection Index-Space Desync on Ring Buffer Eviction (Index Clamping to Row 0) | **Critical** | Logic / State Invariant | [`crates/loganalysis-gui/src/selection.rs:35-37`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/selection.rs#L35), [`workspace/adb.rs:240-243`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L240) |
| **MAJ-01** | Test-Only Clippy Reversed Empty Ranges Breaking CI Linter Gate | **Major** | Build / CI | [`crates/loganalysis-gui/src/components/text_input.rs:1001-1186`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L1001), [`.github/workflows/ci.yml`](file:///home/drew/github/hwanjongyu/loganalysis/.github/workflows/ci.yml) |
| **MAJ-02** | Orphaned Controller Sub-Architecture & Monolithic Workspace God Object | **Major** | Architecture / Technical Debt | [`crates/loganalysis-gui/src/controllers/`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/controllers/), [`types.rs:1-133`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/types.rs#L1), [`workspace/mod.rs:32-150`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/mod.rs#L32) |
| **MAJ-03** | Silent Persistence Failure on Windows Atomic Rename | **Major** | Portability / Data Loss | [`crates/loganalysis-gui/src/workspace/filters.rs:281`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/filters.rs#L281), [`recent_files.rs:101`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/recent_files.rs#L101) |
| **MAJ-04** | Subprocess Pipe Hang on macOS and Windows (`O_NONBLOCK` Gating) | **Major** | Portability / Concurrency | [`crates/loganalysis-core/src/adb/device.rs:142-153, 165`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs#L142) |
| **MAJ-05** | Mmap Process Termination on File Truncation (`SIGBUS` Hazard) | **Major** | Memory Safety / Crash Hazard | [`crates/loganalysis-core/src/buffer/file_buffer.rs:63-74, 160-170`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs#L63) |
| **MAJ-06** | Hot-Loop Filter Automaton Re-compilation on Streaming Batches | **Major** | Performance / Latency | [`crates/loganalysis-core/src/filter/engine.rs:380-387, 434-442`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L380) |
| **MAJ-07** | Stream Buffer Deep Clone Thrashing Under Concurrent Analytics | **Major** | Performance / Memory | [`crates/loganalysis-gui/src/workspace/adb.rs:218-235`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L218) |
| **MAJ-08** | Invalid Search Regex Silently Masked as "0 matches" | **Major** | Error Handling / UX | [`crates/loganalysis-gui/src/workspace/search.rs:76-107`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/search.rs#L76) |
| **MAJ-09** | Unvalidated Regex Saving in Filter Modal | **Major** | Error Handling / UX | [`crates/loganalysis-gui/src/workspace/filters.rs:195-231`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/filters.rs#L195), [`engine.rs:437`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L437) |
| **MAJ-10** | Leap-Year Calendar Arithmetic Bug & Epoch Drift Corrupting Chronological Merging | **Major** | Accuracy / Interleaving Math | [`crates/loganalysis-core/src/buffer/timestamp.rs:3-5, 117, 154`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/timestamp.rs#L3) |
| **MAJ-11** | `include_str!` Source-Assertion Test Anti-Pattern & Test Bloat | **Major** | Maintainability / Testing | [`crates/loganalysis-gui/src/main.rs:618-1759`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs#L618) |
| **MAJ-12** | Unverified Remote Binary Execution in AppImage Build Script | **Major** | Security / Supply Chain | [`scripts/build-appimage.sh:70-77`](file:///home/drew/github/hwanjongyu/loganalysis/scripts/build-appimage.sh#L70) |
| **MIN-01** | Nonexistent Color Name in Quick-Filter Facet Palette ("Magenta") | **Minor** | Logic / Styling | [`crates/loganalysis-gui/src/workspace/dock.rs:167`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/dock.rs#L167), [`constants.rs:8-39`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/constants.rs#L8) |
| **MIN-02** | Inconsistent Search Threshold Constants Between Core and GUI | **Minor** | Maintainability / Config | [`crates/loganalysis-core/src/constants.rs:5`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/constants.rs#L5), [`workspace/search.rs:9`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/search.rs#L9) |
| **MIN-03** | Working Directory File Pollution & Synchronous Logging I/O | **Minor** | Performance / Filesystem | [`crates/loganalysis-gui/src/logging.rs:96-131, 148-158`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/logging.rs#L96) |
| **MIN-04** | Fragile X11 Window Handle Resolution via `$WINDOWID` | **Minor** | Platform Robustness | [`crates/loganalysis-gui/src/dialogs.rs:69-77`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/dialogs.rs#L69), [`workspace/mod.rs:390-418`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/mod.rs#L390) |
| **MIN-05** | Verbatim Duplication in Escape Key Handling | **Minor** | Code Quality / DRY | [`crates/loganalysis-gui/src/main.rs:150-197`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs#L150), [`workspace/mod.rs:601-648`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/mod.rs#L601) |
| **MIN-06** | Triplicated Async Analytics Task Dispatch | **Minor** | Code Quality / DRY | [`crates/loganalysis-gui/src/workspace/adb.rs:326-345, 388, 442`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L326) |
| **MIN-07** | Linear Scan on Sorted Indices in Bookmark Traversal | **Minor** | Performance / Algorithmic | [`crates/loganalysis-gui/src/workspace/bookmarks.rs:59-63, 100`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/bookmarks.rs#L59) |
| **MIN-08** | Unsound Fallback to Row 0 on Selection Miss | **Minor** | Robustness / Edge Cases | [`crates/loganalysis-gui/src/workspace/selection.rs:39, 75`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/selection.rs#L39), [`components/log_view.rs:828`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs#L828) |
| **MIN-09** | Unidiomatic `.when(...is_some(), ...unwrap())` in UI | **Minor** | Idiomatic Rust | [`crates/loganalysis-gui/src/components/status_bar.rs:267`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/status_bar.rs#L267), [`modals/bookmark_drawer.rs:181`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/modals/bookmark_drawer.rs#L181) |
| **MIN-10** | Potential Panic in `TextInput::paint` on Prepaint Desync | **Minor** | Robustness / Panic Safety | [`crates/loganalysis-gui/src/components/text_input.rs:754-768`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L754) |
| **MIN-11** | High False-Positive Rate in Heatmap Keyword Detection | **Minor** | Robustness / Accuracy | [`crates/loganalysis-core/src/filter/heatmap.rs:220, 232`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/heatmap.rs#L220) |
| **MIN-12** | Dependency Risk on Pre-Release GPUI 0.2.2 | **Minor** | Dependency Risk | [`crates/loganalysis-gui/Cargo.toml:10`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/Cargo.toml#L10), [`main.rs:502-510`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs#L502) |
| **INF-01** | Hardcoded Log Format Parsers & Missing Extensibility SPI | **Informational** | Architecture / SPI | [`crates/loganalysis-core/src/buffer/column.rs:1-120`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/column.rs#L1) |
| **INF-02** | Crude "Epoch {secs}" Output in HTML Incident Report | **Informational** | Presentation / Polish | [`crates/loganalysis-core/src/export/html_report.rs:14, 421`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/export/html_report.rs#L14) |
| **INF-03** | Redundant File Re-Open on Memory-Map Heap Fallback | **Informational** | Resource Efficiency | [`crates/loganalysis-core/src/buffer/file_buffer.rs:69`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs#L69) |
| **INF-04** | Rustdoc Unclosed HTML Tag Warning | **Informational** | Documentation Quality | [`crates/loganalysis-core/src/buffer/mod.rs:126`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/mod.rs#L126) |
| **INF-05** | Future Incompatibility Warning in `proc-macro-error2` | **Informational** | Dependency Future Proofing | [`Cargo.lock`](file:///home/drew/github/hwanjongyu/loganalysis/Cargo.lock) (`proc-macro-error2 v2.0.1`) |
| **INF-06** | Missing Security Vulnerability Scanning in CI | **Informational** | Security / CI Hardening | [`.github/workflows/ci.yml`](file:///home/drew/github/hwanjongyu/loganalysis/.github/workflows/ci.yml) |

---

### Detailed Findings & Actionable Remediations

---

#### Finding CRIT-01: Live Streaming Filter Reapply Race Condition (Log Loss & Stale Indices)
- **Severity:** Critical
- **Category:** Concurrency / Data Integrity
- **Exact Location:** [`crates/loganalysis-gui/src/workspace/adb.rs:218-288`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L218), [`crates/loganalysis-gui/src/workspace/mod.rs:505-585`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/mod.rs#L505)
- **Technical Description & Root Cause:**  
  When filter rules are modified while ADB logcat is actively streaming, `reapply_filters` captures a snapshot of `log_source` at line count $N$ and offloads filtering to `bg_executor`. Meanwhile, incoming streaming batches continue arriving on the UI thread and append lines to `log_source`. When the background filter task finishes, it unconditionally assigns:
  ```rust
  workspace.filter_result = Some(Arc::new(res));
  ```
  This completely overwrites `filter_result` with the snapshot result, silently erasing all log lines ingested during the background pass from `visible_indices`. Furthermore, if the ring buffer also trimmed lines while filtering was in flight, the indices in `res` point to the wrong physical lines in `StreamBuffer`.
- **Practical Impact & Risk Assessment:**  
  Log lines disappear from the user's view during active streaming. If the user investigates an incident that occurred right when a filter rule was modified, critical log events are omitted from the display.
- **Actionable Remediation:**  
  Record the snapshot length and clone `active_filters` before spawning the background task so they remain accessible in the completion callback without move-checker errors. Reconcile both newly arrived lines and ring-buffer head evictions against the current stream buffer:
  ```rust
  // In workspace/mod.rs before spawning:
  let snapshot_len = source.line_count();
  let filters_for_ui = active_filters.clone();
  let show_only = self.show_only_filtered;

  // In this.update completion callback:
  let _ = this.update(&mut async_cx, |workspace, cx| {
      if workspace.filter_cancel_gen.load(Ordering::SeqCst) != generation {
          return;
      }
      let mut final_res = res;
      if let Some(src) = &workspace.log_source {
          let current_len = src.line_count();
          // In StreamBuffer, track total ingested count or delta trimmed since snapshot
          let delta_trimmed = src.head_trimmed_since(snapshot_len);
          if current_len > snapshot_len || delta_trimmed > 0 {
              let catchup_start = snapshot_len.saturating_sub(delta_trimmed);
              loganalysis_core::filter::engine::append_streaming_batch(
                  &mut final_res,
                  &**src,
                  catchup_start..current_len,
                  delta_trimmed,
                  &filters_for_ui,
                  show_only,
              );
          }
      }
      workspace.filter_result = Some(Arc::new(final_res));
      cx.notify();
  });
  ```

---

#### Finding CRIT-02: Selection Index-Space Desync on Ring Buffer Eviction (Index Clamping to Row 0)
- **Severity:** Critical
- **Category:** Logic / State Invariant
- **Exact Location:** [`crates/loganalysis-gui/src/selection.rs:35-37`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/selection.rs#L35), [`crates/loganalysis-gui/src/workspace/adb.rs:240-243`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L240)
- **Technical Description & Root Cause:**  
  The application maintains two distinct index spaces:
  1. *Raw Buffer Index Space*: $0 \le i < N$ (up to 200,000 lines in `StreamBuffer`).
  2. *Visible Row Index Space*: $0 \le v < M$ (indices into `visible_indices`).
  `SelectionModel::selected_row` tracks the visible row highlighted in the UI. When `StreamBuffer` trims 20,000 lines (`trimmed = 20_000`), `adb.rs:242` calls:
  ```rust
  workspace.selection.shift_on_trim(trimmed);
  ```
  `SelectionModel::shift_on_trim` executes `row.saturating_sub(trimmed)`. Because `row` is in visible space (e.g. 50), subtracting 20,000 immediately clamps `selected_row` to 0. Moreover, in `adb.rs:242`, `workspace.selection.shift_on_trim(trimmed)` is called *before* `append_streaming_batch` at line 260 calculates how many visible lines fell off.
- **Practical Impact & Risk Assessment:**  
  Any buffer eviction event immediately resets the user's cursor to the top of the table. A user actively reviewing an ongoing logcat event has their selection forcibly stolen.
- **Actionable Remediation:**  
  Compute `dropped_visible_rows` by querying `visible_indices.partition_point(|&idx| idx < trimmed)` before shifting, and update `SelectionModel` to shift visible rows and anchor source indices independently:
  ```rust
  // In workspace/adb.rs:
  if trimmed > 0 {
      let dropped_visible = workspace.filter_result.as_ref().map_or(0, |res| {
          res.visible_indices.partition_point(|&idx| idx < trimmed)
      });
      workspace.bookmarks.shift_on_trim(trimmed);
      workspace.selection.shift_on_trim(dropped_visible, trimmed);
  }

  // In selection.rs:
  pub fn shift_on_trim(&mut self, dropped_visible_rows: usize, raw_trimmed_count: usize) {
      if let Some(anchor) = self.anchor_source_idx {
          if anchor < raw_trimmed_count {
              self.clear();
              return;
          }
          self.anchor_source_idx = Some(anchor - raw_trimmed_count);
      }
      if let Some(row) = self.selected_row {
          if row < dropped_visible_rows {
              self.selected_row = None;
          } else {
              self.selected_row = Some(row - dropped_visible_rows);
          }
      }
  }
  ```
  Update unit tests in `selection.rs` to reflect the two-parameter signature.

---

#### Finding MAJ-01: Test-Only Clippy Reversed Empty Ranges Breaking CI Linter Gate
- **Severity:** Major
- **Category:** Build / CI
- **Exact Location:** [`crates/loganalysis-gui/src/components/text_input.rs:1001, 1009, 1013, 1024, 1062, 1099, 1186`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L1001), [`.github/workflows/ci.yml`](file:///home/drew/github/hwanjongyu/loganalysis/.github/workflows/ci.yml)
- **Technical Description & Root Cause:**  
  Running `cargo clippy --all-targets --all-features` aborts with exit code 101 due to 7 compiler errors for `clippy::reversed_empty_ranges`. The unit tests intended to verify clamping behavior on inverted ranges like `10..2`, but Rust Clippy denies reverse range literals by default. Because `.github/workflows/ci.yml` runs only `cargo test`, this default-deny error slipped into the repository and blocks developer workflows. Note that because this defect is isolated strictly within `#[cfg(test)] mod tests` and does not affect production code, it is appropriately classified as Major rather than Critical.
- **Practical Impact & Risk Assessment:**  
  Breaks local developer builds when running standard Clippy pre-commit checks or strict CI gates.
- **Actionable Remediation:**  
  Add `#[allow(clippy::reversed_empty_ranges)]` at the test module level (`text_input.rs:863`) to cover all 7 occurrences simultaneously, and add Clippy to CI:
  ```rust
  // crates/loganalysis-gui/src/components/text_input.rs:863
  #[cfg(test)]
  #[allow(clippy::reversed_empty_ranges)]
  mod tests {
      use super::*;
      // ... all 7 test cases compile cleanly
  }
  ```
  In `.github/workflows/ci.yml`, add:
  ```yaml
  - name: Run Clippy
    run: cargo clippy --workspace --all-targets -- -D warnings
  ```

---

#### Finding MAJ-02: Orphaned Controller Sub-Architecture & Monolithic Workspace God Object
- **Severity:** Major
- **Category:** Architecture / Technical Debt
- **Exact Location:** [`crates/loganalysis-gui/src/controllers/`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/controllers/), [`crates/loganalysis-gui/src/types.rs:1-133`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/types.rs#L1), [`crates/loganalysis-gui/src/workspace/mod.rs:32-150`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/mod.rs#L32)
- **Technical Description & Root Cause:**  
  `crates/loganalysis-gui/src/controllers/` defines four complete controllers (`AdbController`, `BookmarkController`, `SearchController`, `SplitViewController`) with unit tests, paired with structs in `types.rs`. However, `LogAnalysisWorkspace` never instantiates or delegates to these controllers. Instead, `LogAnalysisWorkspace` maintains 50+ public fields that manually implement the exact same state machine. This architectural divergence is technical debt rather than a runtime crash risk, categorizing it as Major.
- **Practical Impact & Risk Assessment:**  
  Extreme architectural debt. The monolithic workspace violates the Single Responsibility Principle, making state changes difficult to reason about and test in isolation, while dead controller code misleads contributors.
- **Actionable Remediation:**  
  Embed the controller structs directly into `LogAnalysisWorkspace`:
  ```rust
  pub struct LogAnalysisWorkspace {
      pub(crate) adb: AdbController,
      pub(crate) bookmarks: BookmarkController,
      pub(crate) search: SearchController,
      pub(crate) split_view: SplitViewController,
      // Shared workstation shell fields (tabs, theme, dock)
  }
  ```
  Remove `#[allow(dead_code)]` from `types.rs` and delegate workspace actions to the respective controller methods.

---

#### Finding MAJ-03: Silent Persistence Failure on Windows Atomic Rename
- **Severity:** Major
- **Category:** Portability / Data Loss
- **Exact Location:** [`crates/loganalysis-gui/src/workspace/filters.rs:281`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/filters.rs#L281), [`crates/loganalysis-gui/src/workspace/recent_files.rs:101`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/recent_files.rs#L101)
- **Technical Description & Root Cause:**  
  `std::fs::rename(&tmp_path, &path)` fails on Windows if `path` already exists, returning `ERROR_ALREADY_EXISTS`. The error handler logs a warning and deletes `tmp_path`. Consequently, any second save of filter profiles or recent files silently fails to persist on Windows. Naively calling `std::fs::remove_file(to)` before renaming introduces an unrecoverable data loss window if the process crashes between deletion and rename, and fails with `ERROR_ACCESS_DENIED` under concurrent read locks.
- **Practical Impact & Risk Assessment:**  
  Total loss of filter configuration and MRU file history persistence on Windows.
- **Actionable Remediation:**  
  Implement true atomic file replacement using Win32 `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` on Windows:
  ```rust
  pub fn atomic_replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
      #[cfg(windows)]
      {
          use std::os::windows::ffi::OsStrExt;
          let from_wide: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
          let to_wide: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
          let res = unsafe {
              windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                  from_wide.as_ptr(),
                  to_wide.as_ptr(),
                  windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING,
              )
          };
          if res == 0 {
              return Err(std::io::Error::last_os_error());
          }
          Ok(())
      }
      #[cfg(not(windows))]
      {
          std::fs::rename(from, to)
      }
  }
  ```

---

#### Finding MAJ-04: Subprocess Pipe Hang on macOS and Windows (`O_NONBLOCK` Gating)
- **Severity:** Major
- **Category:** Portability / Concurrency
- **Exact Location:** [`crates/loganalysis-core/src/adb/device.rs:142-153, 165`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs#L142)
- **Technical Description & Root Cause:**  
  `O_NONBLOCK` is only set on Linux via `#[cfg(target_os = "linux")]`. On macOS and Windows, `stdout_pipe.read(&mut buf)` remains in blocking mode. If `adb devices -l` hangs, the timeout check is blocked indefinitely.
- **Practical Impact & Risk Assessment:**  
  macOS and Windows users experience indefinite application hangs during ADB device enumeration if the ADB daemon is unresponsive.
- **Actionable Remediation:**  
  Read `stdout_pipe` on a worker thread with `crossbeam_channel::bounded::<Vec<u8>>(128)`. Preserve the child process exit status `status` (required for `if !status.success()`), reap the child process on disconnect to avoid zombie processes, and join the reader thread:
  ```rust
  let (tx, rx) = crossbeam_channel::bounded::<Vec<u8>>(128);
  let mut pipe = stdout_pipe;
  let reader_handle = std::thread::spawn(move || {
      let mut buf = [0u8; 4096];
      while let Ok(n) = pipe.read(&mut buf) {
          if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
              break;
          }
      }
  });

  let mut output = Vec::new();
  let status = loop {
      if start_time.elapsed() >= timeout {
          let _ = child.kill();
          let _ = child.wait();
          return Err(AdbError::Timeout(timeout));
      }
      match rx.recv_timeout(Duration::from_millis(20)) {
          Ok(chunk) => output.extend_from_slice(&chunk),
          Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
              if let Ok(Some(status)) = child.try_wait() {
                  while let Ok(chunk) = rx.try_recv() {
                      output.extend_from_slice(&chunk);
                  }
                  break status;
              }
          }
          Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
              let status = child.wait().map_err(|e| AdbError::ExecutionFailed(e.to_string()))?;
              break status;
          }
      }
  };
  let _ = reader_handle.join();
  ```

---

#### Finding MAJ-05: Mmap Process Termination on File Truncation (`SIGBUS` Hazard)
- **Severity:** Major
- **Category:** Memory Safety / Crash Hazard
- **Exact Location:** [`crates/loganalysis-core/src/buffer/file_buffer.rs:63-74, 160-170`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs#L63)
- **Technical Description & Root Cause:**  
  `FileLogBuffer` maps log files into memory using `memmap2::Mmap`. If an external process truncates the file, accessing virtual addresses past the new file length triggers a `SIGBUS` signal on Unix. Rust panics cannot recover from `SIGBUS`. Adding `std::fs::metadata(&self.path)` into `FileLogBuffer::get_raw_line` destroys zero-copy performance by injecting millions of kernel `stat` syscalls into the Rayon hot path and fails to resolve TOCTOU race conditions.
- **Practical Impact & Risk Assessment:**  
  Immediate hard crash of the workstation when monitoring active log files that undergo external rotation or truncation.
- **Actionable Remediation:**  
  1. For volatile log files subject to live writing or rotation, provide an explicit heap loading mode (`FileLogBuffer::open_heap` / `Storage::Heap`), keeping memory wholly owned in user-space and immune to `SIGBUS`.
  2. For mmap buffers, validate `std::fs::metadata` once per reload check or on viewport scroll repositioning events, never per line read in `get_raw_line`.
  3. On Unix targets, register a `SIGBUS` signal handler to intercept page faults and notify the workspace gracefully.

---

#### Finding MAJ-06: Hot-Loop Filter Automaton Re-compilation on Streaming Batches
- **Severity:** Major
- **Category:** Performance / Latency
- **Exact Location:** [`crates/loganalysis-core/src/filter/engine.rs:380-387, 434-442`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L380)
- **Technical Description & Root Cause:**  
  `append_streaming_batch` re-executes `MultiFilterMatcher::new(&prepared)` on every incoming streaming chunk, rebuilding Aho-Corasick automata and compiling regexes every 16–150ms.
- **Practical Impact & Risk Assessment:**  
  High CPU usage and dropped UI frames during high-frequency Android logcat streaming.
- **Actionable Remediation:**  
  Pass a precompiled `&MultiFilterMatcher` and `&[PreparedFilter]` to `append_streaming_batch_with_matcher`, preserving the `source: &(dyn LogSource + Sync)` contract:
  ```rust
  pub fn append_streaming_batch_with_matcher(
      result: &mut FilterResult,
      source: &(dyn LogSource + Sync),
      new_lines_range: std::ops::Range<usize>,
      trimmed_count: usize,
      prepared: &[PreparedFilter],
      matcher: &MultiFilterMatcher,
      show_only_filtered: bool,
  ) {
      // Direct line evaluation without automaton recompilation
  }
  ```

---

#### Finding MAJ-07: Stream Buffer Deep Clone Thrashing Under Concurrent Analytics
- **Severity:** Major
- **Category:** Performance / Memory
- **Exact Location:** [`crates/loganalysis-gui/src/workspace/adb.rs:218-235`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L218)
- **Technical Description & Root Cause:**  
  When background analytics hold an `Arc` handle to `log_source`, `Arc::get_mut` fails. The fallback deep-clones the 200,000-line `StreamBuffer` on every incoming batch.
- **Practical Impact & Risk Assessment:**  
  Multi-megabyte heap allocation thrash and frame hitching while viewing heatmaps or timeline sparklines.
- **Actionable Remediation:**  
  Refactor `StreamBuffer` to utilize interior mutability via `parking_lot::RwLock<StreamBufferInner>`, allowing readers to acquire read locks while the streaming thread appends under a write lock.

---

#### Finding MAJ-08: Invalid Search Regex Silently Masked as "0 matches"
- **Severity:** Major
- **Category:** Error Handling / UX
- **Exact Location:** [`crates/loganalysis-gui/src/workspace/search.rs:76-107`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/search.rs#L76)
- **Technical Description & Root Cause:**  
  `search_in_log` returning `Err(SearchError::InvalidRegex(_))` falls into the `else` branch displaying `"0 matches"` and `"Search pattern not found"`.
- **Practical Impact & Risk Assessment:**  
  Users assume zero events match their query rather than recognizing a syntax error in their regular expression.
- **Actionable Remediation:**  
  Explicitly pattern match on `SearchError::InvalidRegex`, using the exact `found.real_idx` field and `SharedString` types:
  ```rust
  match search_res {
      Ok(Some(found)) => {
          workspace.selection.select(found.visible_row, found.real_idx);
          workspace.status_text = format!("Found match at line {}", found.real_idx + 1).into();
          workspace.search_match_status = Some(format!("Line {}", found.real_idx + 1));
      }
      Ok(None) => {
          workspace.status_text = "Search pattern not found".into();
          workspace.search_match_status = Some("0 matches".into());
      }
      Err(SearchError::InvalidRegex(err)) => {
          workspace.status_text = format!("Regex syntax error: {err}").into();
          workspace.search_match_status = Some("Regex Error".into());
      }
      Err(err) => {
          workspace.status_text = format!("Search error: {err}").into();
          workspace.search_match_status = Some("Error".into());
      }
  }
  ```

---

#### Finding MAJ-09: Unvalidated Regex Saving in Filter Modal
- **Severity:** Major
- **Category:** Error Handling / UX
- **Exact Location:** [`crates/loganalysis-gui/src/workspace/filters.rs:195-231`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/filters.rs#L195), [`crates/loganalysis-core/src/filter/engine.rs:437`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L437)
- **Technical Description & Root Cause:**  
  The modal saves regex rules without validation. The engine then silently drops invalid rules with `if let Ok(pf) = PreparedFilter::new(...)`.
- **Practical Impact & Risk Assessment:**  
  Invalid filter rules appear active in the dock but match zero lines without diagnostic feedback.
- **Actionable Remediation:**  
  Validate the regex pattern against `self.filter_edit_modal.is_regex` before saving in `save_modal_filter`:
  ```rust
  if self.filter_edit_modal.is_regex {
      if let Err(err) = fancy_regex::Regex::new(&pattern) {
          self.status_text = format!("Invalid filter regex: {err}").into();
          cx.notify();
          return;
      }
  }
  ```

---

#### Finding MAJ-10: Leap-Year Calendar Arithmetic Bug & Epoch Drift Corrupting Chronological Merging
- **Severity:** Major
- **Category:** Accuracy / Interleaving Math
- **Exact Location:** [`crates/loganalysis-core/src/buffer/timestamp.rs:3-5, 117, 154`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/timestamp.rs#L3)
- **Technical Description & Root Cause:**  
  `DAYS_BEFORE_MONTH[3] = 60` hardcodes February as 29 days every year. In non-leap years (2023, 2025, 2026), log records crossing Feb 28 to Mar 1 experience a **48-hour jump** (a 24-hour phantom jump). Furthermore, `(year - 1970) * 365` neglects all leap years since 1970, introducing a ~13-to-14-day epoch offset for modern timestamps. In `MergedLogBuffer`, where multi-device captures are interleaved chronologically based on parsed microseconds, this calendar math defect directly corrupts log sorting and delta-time calculations across files.
- **Practical Impact & Risk Assessment:**  
  Multi-file chronological log interleaving inverted across month boundaries; delta times skewed by 24 hours.
- **Actionable Remediation:**  
  Correct `DAYS_BEFORE_MONTH` for non-leap years (February has 28 days, making March start at day 59, not 60), implement standard Gregorian leap-year formulas, and calculate true epoch days:
  ```rust
  pub const DAYS_BEFORE_MONTH: [u32; 13] = [
      0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334,
  ];

  #[inline]
  pub fn is_leap_year(year: i32) -> bool {
      (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
  }

  #[inline]
  pub fn day_of_year_with_leap(month: u32, day: u32, is_leap: bool) -> u32 {
      let m = month.min(12) as usize;
      let extra = if is_leap && month > 2 { 1 } else { 0 };
      DAYS_BEFORE_MONTH[m] + extra + day.saturating_sub(1)
  }

  pub fn days_since_epoch(year: i32, month: u32, day: u32) -> i64 {
      let y = year as i64 - 1;
      let leap_days = (y / 4) - (y / 100) + (y / 400) - (1969 / 4 - 1969 / 100 + 1969 / 400);
      let days = (year as i64 - 1970) * 365 + leap_days + day_of_year_with_leap(month, day, is_leap_year(year)) as i64;
      days
  }
  ```
  For Case 1 (Android logcat timestamps lacking year), maintain `day_of_year(month, day)` defaulting to non-leap or inferring the current year.

---

#### Finding MAJ-11: `include_str!` Source-Assertion Test Anti-Pattern & Test Bloat
- **Severity:** Major
- **Category:** Maintainability / Testing
- **Exact Location:** [`crates/loganalysis-gui/src/main.rs:618-1759`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs#L618)
- **Technical Description & Root Cause:**  
  Over 64% of `main.rs` consists of unit tests using `include_str!` to verify string literals in source code files rather than testing runtime contracts.
- **Practical Impact & Risk Assessment:**  
  Slows down compilation, bloats AST memory during IDE analysis, and provides false confidence while failing to catch behavioral regressions.
- **Actionable Remediation:**  
  Move tests out of `main.rs` into `crates/loganalysis-gui/tests/` and replace static source checks with behavioral tests on `SelectionModel`, `FilterModel`, and `LogAnalysisWorkspace`.

---

#### Finding MAJ-12: Unverified Remote Binary Execution in AppImage Build Script
- **Severity:** Major
- **Category:** Security / Supply Chain
- **Exact Location:** [`scripts/build-appimage.sh:70-77`](file:///home/drew/github/hwanjongyu/loganalysis/scripts/build-appimage.sh#L70)
- **Technical Description & Root Cause:**  
  `build-appimage.sh` downloads `appimagetool` from a mutable continuous GitHub release and executes it without checksum validation. Upstream rebuilds change binary hashes, creating both security risks and CI fragility.
- **Practical Impact & Risk Assessment:**  
  High supply chain risk of remote code execution during release packaging and unexpected build breaks.
- **Actionable Remediation:**  
  Pin a specific version release tag (e.g. tag `13`) and verify SHA256 before execution:
  ```bash
  APPIMAGETOOL_VERSION="13"
  APPIMAGETOOL_SHA256="d918b4ed5479b4d8479e03cca93d4073000b028965e43fb2945142f96e5720b4"
  APPIMAGETOOL_URL="https://github.com/AppImage/AppImageKit/releases/download/${APPIMAGETOOL_VERSION}/appimagetool-x86_64.AppImage"

  if [ ! -f "${APPIMAGETOOL}" ]; then
      curl -fsSL -o "${APPIMAGETOOL}" "${APPIMAGETOOL_URL}"
      echo "${APPIMAGETOOL_SHA256}  ${APPIMAGETOOL}" | sha256sum -c -
      chmod 755 "${APPIMAGETOOL}"
  fi
  ```

---

#### Finding MIN-01: Nonexistent Color Name in Quick-Filter Facet Palette ("Magenta")
- **Severity:** Minor
- **Category:** Logic / Styling
- **Exact Location:** [`crates/loganalysis-gui/src/workspace/dock.rs:167`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/dock.rs#L167), [`crates/loganalysis-core/src/constants.rs:8-39`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/constants.rs#L8)
- **Technical Description & Root Cause:**  
  `add_facet_filter` cycles through `["Cyan", "Green", "Magenta", "Yellow", "Blue"]`. However, `"Magenta"` does not exist in `constants::COLOR_MAP`.
- **Practical Impact & Risk Assessment:**  
  1 out of 5 facet filter chips generates a rule with `bg_color` resolving to `None`, resulting in unhighlighted log entries.
- **Actionable Remediation:**  
  Replace `"Magenta"` with `"Orchid"` or `"Purple"` in `dock.rs:167`:
  ```rust
  let palette = ["Cyan", "Green", "Orchid", "Yellow", "Blue"];
  ```

---

#### Finding MIN-02: Inconsistent Search Threshold Constants Between Core and GUI
- **Severity:** Minor
- **Category:** Maintainability / Config
- **Exact Location:** [`crates/loganalysis-core/src/constants.rs:5`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/constants.rs#L5), [`crates/loganalysis-gui/src/workspace/search.rs:9`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/search.rs#L9)
- **Technical Description & Root Cause:**  
  Core defines `pub const ASYNC_SEARCH_THRESHOLD: usize = 5_000`, while GUI redeclares a private constant of `50_000` (10x difference).
- **Practical Impact & Risk Assessment:**  
  Split-brain threshold configuration.
- **Actionable Remediation:**  
  Remove the duplicate constant in `search.rs` and import `loganalysis_core::constants::ASYNC_SEARCH_THRESHOLD`.

---

#### Finding MIN-03: Working Directory File Pollution & Synchronous Logging I/O
- **Severity:** Minor
- **Category:** Performance / Filesystem
- **Exact Location:** [`crates/loganalysis-gui/src/logging.rs:96-131, 148-158`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/logging.rs#L96)
- **Technical Description & Root Cause:**  
  `resolve_log_paths` writes `loganalysis.log` to `cwd`, and `log_event` opens, appends, and flushes files synchronously twice per event under a global mutex.
- **Practical Impact & Risk Assessment:**  
  Pollutes user workspace directories and blocks threads on synchronous disk I/O.
- **Actionable Remediation:**  
  Write logs exclusively to standard OS application data paths (`dirs::state_dir()`) and buffer writes.

---

#### Finding MIN-04: Fragile X11 Window Handle Resolution via `$WINDOWID`
- **Severity:** Minor
- **Category:** Platform Robustness
- **Exact Location:** [`crates/loganalysis-gui/src/dialogs.rs:69-77`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/dialogs.rs#L69), [`crates/loganalysis-gui/src/workspace/mod.rs:390-418`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/mod.rs#L390)
- **Technical Description & Root Cause:**  
  `get_active_x11_window_id` reads `$WINDOWID` from the environment, which points to the terminal emulator rather than the GPUI window.
- **Practical Impact & Risk Assessment:**  
  Modals attach to the terminal window or fail when launched from desktop icons.
- **Actionable Remediation:**  
  Pass `None` to `rfd` when no valid GPUI window handle exists, allowing the desktop window manager to center modals on screen.

---

#### Finding MIN-05: Verbatim Duplication in Escape Key Handling
- **Severity:** Minor
- **Category:** Code Quality / DRY
- **Exact Location:** [`crates/loganalysis-gui/src/main.rs:150-197`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs#L150), [`crates/loganalysis-gui/src/workspace/mod.rs:601-648`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/mod.rs#L601)
- **Technical Description & Root Cause:**  
  48 lines of identical modal/dock closing dispatch logic are copy-pasted between `main.rs` and `workspace/mod.rs`.
- **Practical Impact & Risk Assessment:**  
  Maintenance friction and risk of behavioral divergence when adding new modals.
- **Actionable Remediation:**  
  Encapsulate the escape handler in `workspace.handle_escape(cx) -> bool` and call it from `main.rs`.

---

#### Finding MIN-06: Triplicated Async Analytics Task Dispatch
- **Severity:** Minor
- **Category:** Code Quality / DRY
- **Exact Location:** [`crates/loganalysis-gui/src/workspace/adb.rs:326-345, 388-407, 442-461`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/adb.rs#L326)
- **Technical Description & Root Cause:**  
  Identical 25-line async analytics dispatch logic is duplicated across three separate methods.
- **Practical Impact & Risk Assessment:**  
  Code bloat and elevated maintenance cost.
- **Actionable Remediation:**  
  Extract into `fn spawn_analytics_update(&mut self, cx: &mut Context<Self>)`.

---

#### Finding MIN-07: Linear Scan on Sorted Indices in Bookmark Traversal
- **Severity:** Minor
- **Category:** Performance / Algorithmic
- **Exact Location:** [`crates/loganalysis-gui/src/workspace/bookmarks.rs:59-63, 100-104`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/bookmarks.rs#L59)
- **Technical Description & Root Cause:**  
  Uses `.iter().position(|&idx| idx == target)` on `visible_indices`, which is sorted.
- **Practical Impact & Risk Assessment:**  
  $O(N)$ scan overhead on large datasets (up to 200,000 items) when navigating bookmarks.
- **Actionable Remediation:**  
  Replace with binary search on `filter_result`:
  ```rust
  if let Some(res) = &self.filter_result {
      if let Ok(vis_row) = res.visible_indices.binary_search(&target_source_idx) {
          self.selection.select(vis_row, target_source_idx);
          // scroll and status update
      }
  }
  ```

---

#### Finding MIN-08: Unsound Fallback to Row 0 on Selection Miss
- **Severity:** Minor
- **Category:** Robustness / Edge Cases
- **Exact Location:** [`crates/loganalysis-gui/src/workspace/selection.rs:39, 75`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/selection.rs#L39), [`crates/loganalysis-gui/src/components/log_view.rs:828`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs#L828)
- **Technical Description & Root Cause:**  
  `res.visible_indices.get(sel_row).copied().unwrap_or(0)` jumps to line 0 when index lookup fails.
- **Practical Impact & Risk Assessment:**  
  Disruptive cursor jumping to line 0 on race conditions.
- **Actionable Remediation:**  
  Return early or skip action when `visible_indices.get(sel_row)` returns `None`.

---

#### Finding MIN-09: Unidiomatic `.when(...is_some(), ...unwrap())` in UI
- **Severity:** Minor
- **Category:** Idiomatic Rust
- **Exact Location:** [`crates/loganalysis-gui/src/components/status_bar.rs:267-268`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/status_bar.rs#L267), [`crates/loganalysis-gui/src/modals/bookmark_drawer.rs:181-182`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/modals/bookmark_drawer.rs#L181)
- **Technical Description & Root Cause:**  
  Uses verbose `.when(opt.is_some(), |p| ... opt.unwrap() ...)` instead of GPUI's `.when_some()`.
- **Practical Impact & Risk Assessment:**  
  Unidiomatic code with latent panic risk if refactored carelessly.
- **Actionable Remediation:**  
  Refactor to `.when_some(tooltip_text, |p, text| { p.tooltip(move |cx| ... text ...) })`.

---

#### Finding MIN-10: Potential Panic in `TextInput::paint` on Prepaint Desync
- **Severity:** Minor
- **Category:** Robustness / Panic Safety
- **Exact Location:** [`crates/loganalysis-gui/src/components/text_input.rs:754-768`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L754)
- **Technical Description & Root Cause:**  
  `let line = prepaint.line.take().unwrap();` assumes prepaint succeeded. If prepaint failed, painting panics.
- **Practical Impact & Risk Assessment:**  
  Application crash during text input painting edge cases.
- **Actionable Remediation:**  
  Safely take `prepaint.line`, pass GPUI 0.2.2's required 4 arguments `(origin, window.line_height(), window, cx)` to `line.paint`, and preserve `line` for `input.last_layout`:
  ```rust
  let Some(line) = prepaint.line.take() else {
      return;
  };
  let origin = point(bounds.origin.x - scroll, bounds.origin.y);
  let _ = line.paint(origin, window.line_height(), window, cx);

  if focus_handle.is_focused(window) {
      if let Some(cursor) = prepaint.cursor.take() {
          window.paint_quad(cursor);
      }
  }

  self.input.update(cx, |input, _cx| {
      input.last_layout = Some(line);
      input.last_bounds = Some(bounds);
      input.scroll_offset = scroll;
  });
  ```

---

#### Finding MIN-11: High False-Positive Rate in Heatmap Keyword Detection
- **Severity:** Minor
- **Category:** Robustness / Accuracy
- **Exact Location:** [`crates/loganalysis-core/src/filter/heatmap.rs:220, 232`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/heatmap.rs#L220)
- **Technical Description & Root Cause:**  
  `has_error_keyword` searches the entire line for `b" E "` and `has_warn_keyword` for `b" W "`.
- **Practical Impact & Risk Assessment:**  
  Standard log messages containing isolated letters "E" or "W" (e.g. "Vitamin E") are incorrectly flagged as errors or warnings on the scrollbar heatmap.
- **Actionable Remediation:**  
  Restrict the single-character level search to the first 35 bytes of the log line.

---

#### Finding MIN-12: Dependency Risk on Pre-Release GPUI 0.2.2
- **Severity:** Minor
- **Category:** Dependency Risk
- **Exact Location:** [`crates/loganalysis-gui/Cargo.toml:10`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/Cargo.toml#L10), [`crates/loganalysis-gui/src/main.rs:502-510`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs#L502)
- **Technical Description & Root Cause:**  
  The project depends on pre-release `gpui = "0.2.2"` with unmerged Linux window handles, requiring explicit XMODIFIERS workarounds.
- **Practical Impact & Risk Assessment:**  
  Risk of upstream breaking changes and platform crashes on diverse Linux compositors.
- **Actionable Remediation:**  
  Document workarounds clearly and plan an upgrade path to a stable GPUI release.

---

#### Finding INF-01: Hardcoded Log Format Parsers & Missing Extensibility SPI
- **Severity:** Informational
- **Category:** Architecture / SPI
- **Exact Location:** [`crates/loganalysis-core/src/buffer/column.rs:1-120`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/column.rs#L1)
- **Technical Description & Root Cause:**  
  Parsers are hardcoded specifically for Android threadtime, brief, and raw formats.
- **Actionable Remediation:**  
  Define a `LogFormatParser` trait SPI allowing custom log parser plugins (e.g. systemd journald, iOS syslog, JSON logs).

---

#### Finding INF-02: Crude "Epoch {secs}" Output in HTML Incident Report
- **Severity:** Informational
- **Category:** Presentation / Polish
- **Exact Location:** [`crates/loganalysis-core/src/export/html_report.rs:14, 421-427`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/export/html_report.rs#L14)
- **Technical Description & Root Cause:**  
  HTML report displays timestamps as `<span class="time">Epoch 1731620000</span>`.
- **Actionable Remediation:**  
  Format timestamps into human-readable ISO 8601 UTC strings.

---

#### Finding INF-03: Redundant File Re-Open on Memory-Map Heap Fallback
- **Severity:** Informational
- **Category:** Resource Efficiency
- **Exact Location:** [`crates/loganalysis-core/src/buffer/file_buffer.rs:69`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs#L69)
- **Technical Description & Root Cause:**  
  `std::fs::read(&path_buf)` re-opens the file after `File::open` has already succeeded.
- **Actionable Remediation:**  
  Rewind the already open `file` handle using `file.seek(SeekFrom::Start(0))` and read into a vector.

---

#### Finding INF-04: Rustdoc Unclosed HTML Tag Warning
- **Severity:** Informational
- **Category:** Documentation Quality
- **Exact Location:** [`crates/loganalysis-core/src/buffer/mod.rs:126`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/mod.rs#L126)
- **Technical Description & Root Cause:**  
  Doc comment contains unescaped `Arc<str>`, generating a rustdoc warning.
- **Actionable Remediation:**  
  Enclose in backticks: `` `Arc<str>` ``.

---

#### Finding INF-05: Future Incompatibility Warning in `proc-macro-error2`
- **Severity:** Informational
- **Category:** Dependency Future Proofing
- **Exact Location:** [`Cargo.lock`](file:///home/drew/github/hwanjongyu/loganalysis/Cargo.lock) (`proc-macro-error2 v2.0.1`)
- **Technical Description & Root Cause:**  
  `proc-macro-error2` triggers `warning[E0365]` for private re-export of `proc_macro`, which will become a hard error in future rustc versions.
- **Actionable Remediation:**  
  Update `proc-macro-error2` once an upstream fix is published.

---

#### Finding INF-06: Missing Security Vulnerability Scanning in CI
- **Severity:** Informational
- **Category:** Security / CI Hardening
- **Exact Location:** [`.github/workflows/ci.yml`](file:///home/drew/github/hwanjongyu/loganalysis/.github/workflows/ci.yml)
- **Technical Description & Root Cause:**  
  The CI workflow only executes `cargo test` and lacks automated security scanning.
- **Actionable Remediation:**  
  Add `cargo audit` and `cargo deny check` steps to the CI pipeline.

---

## 5. Prioritized Action Plan & Engineering Roadmap

### Phase 1 (Immediate / P0): Live Streaming Integrity & CI/Safety Blockers
*Target Timeline: 1–2 days | Objective: Fix live streaming data loss, selection cursor jumps, and unblock CI linter gate*

1. **Neutralize Live Streaming Filter Race Condition (CRIT-01)**:
   - Implement incremental catch-up pass in `reapply_filters` callback in `workspace/mod.rs:550` reconciling `current_len` and `delta_trimmed`.
2. **Fix Selection Model Desync on Buffer Trim (CRIT-02)**:
   - Update `SelectionModel::shift_on_trim` to accept `(dropped_visible_rows, raw_trimmed_count)`.
   - Compute `dropped_visible_rows` via `res.visible_indices.partition_point(|&idx| idx < trimmed)` in `workspace/adb.rs:242` and update `selection.rs` unit tests.
3. **Unblock Clippy & CI Build Gate (MAJ-01)**:
   - Place `#[allow(clippy::reversed_empty_ranges)]` at the test module level (`crates/loganalysis-gui/src/components/text_input.rs:863`) to eliminate all 7 Clippy errors across test cases.
   - Add `cargo clippy --workspace --all-targets -- -D warnings` to `.github/workflows/ci.yml`.
4. **Fix Windows Atomic File Persistence (MAJ-03)**:
   - Implement true atomic replacement via Win32 `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` in `workspace/filters.rs` and `recent_files.rs` without non-atomic deletion race windows.
5. **Secure AppImage Release Packaging (MAJ-12)**:
   - Pin a specific release tag/version in `scripts/build-appimage.sh` and add SHA256 checksum verification.

### Phase 2 (Near-Term / P1): Cross-Platform Hardening, Chronological Accuracy & UX Error Diagnostics
*Target Timeline: 1 week | Objective: Portability across macOS/Windows, accurate chronological interleaving, transparent error reporting*

1. **Eliminate Subprocess Hang on macOS & Windows (MAJ-04)**:
   - Refactor `scan_adb_device_details` in `adb/device.rs` to read `stdout_pipe` on a worker thread with `crossbeam_channel::recv_timeout`, returning child exit status, reaping zombies on disconnect, and joining the reader thread.
2. **Fix Gregorian Leap-Year Arithmetic & Epoch Drift (MAJ-10)**:
   - Correct `DAYS_BEFORE_MONTH[3]` to 59 for non-leap years, implement dynamic leap-year checks, and compute true epoch leap days in `buffer/timestamp.rs` to fix 48h jumps across Feb 28–Mar 1 and restore chronological `MergedLogBuffer` sorting.
3. **Display Diagnostic Feedback for Malformed Regexes (MAJ-08, MAJ-09)**:
   - Update `search.rs` to surface `SearchError::InvalidRegex` in the search HUD and status bar with exact `found.real_idx` and `SharedString` types.
   - Add regex syntax validation to `save_modal_filter` in `filters.rs` targeting `self.filter_edit_modal.is_regex`.
4. **Mitigate Mmap File Truncation Crashes (MAJ-05)**:
   - Provide `Storage::Heap` fallback for volatile/rotating logs and validate file metadata length during reload/reposition events without adding per-line `std::fs::metadata` syscalls to the hot path.
5. **Correct Facet Palette Registration (MIN-01)**:
   - Replace `"Magenta"` with `"Orchid"` in `dock.rs:167` to match `COLOR_MAP`.
6. **Eliminate TextInput Paint Desync Panic (MIN-10)**:
   - Guard `prepaint.line.take()` safely without dropping `line`, passing GPUI 0.2.2's required 4 arguments to `line.paint`.

### Phase 3 (Long-Term / P2): Architectural Decoupling & Algorithmic Performance
*Target Timeline: 2–3 weeks | Objective: Decompose God Object, eliminate hot-path recomputations, optimize data paths*

1. **Resolve Architectural Schism & Decompose God Object (MAJ-02)**:
   - Embed `AdbController`, `BookmarkController`, `SearchController`, and `SplitViewController` into `LogAnalysisWorkspace`.
   - Migrate loose fields to controllers and remove `#[allow(dead_code)]` annotations in `types.rs`.
2. **Eliminate Hot-Loop Automaton Recompilation (MAJ-06)**:
   - Cache `MultiFilterMatcher` in `FilterEngine` and reuse across streaming batches with `&(dyn LogSource + Sync)`.
3. **Prevent Buffer Clone Thrashing Under Concurrent Analytics (MAJ-07)**:
   - Refactor `StreamBuffer` to utilize interior mutability via `parking_lot::RwLock<StreamBufferInner>`.
4. **Purge Test Bloat from Application Entry Point (MAJ-11)**:
   - Extract the 1,141 lines of tests from `crates/loganalysis-gui/src/main.rs` into `tests/` and replace `include_str!` string checks with behavioral assertions.
5. **Optimize Bookmark Traversal & Selection Bounds (MIN-07, MIN-08)**:
   - Use `binary_search` on `self.filter_result.as_ref().unwrap().visible_indices` and eliminate silent row 0 fallbacks.
6. **Consolidate Duplicated Code (MIN-05, MIN-06, MIN-04)**:
   - Extract unified escape handler, async analytics dispatcher, and dialog attachment helpers.
7. **CI Security Hardening & Documentation Cleanliness (INF-04, INF-06)**:
   - Fix unclosed HTML tag in `buffer/mod.rs:126` and integrate `cargo audit` and `cargo deny` into GitHub Actions.

---

## 6. Verification & Auditing Commands

To independently audit and verify all findings documented in this report:

```bash
# 1. Verify Clippy Failure (MAJ-01)
cargo clippy --all-targets --all-features
# Expected: Exit code 101 on reversed_empty_ranges across 7 tests in text_input.rs:1001-1186

# 2. Verify Full Test Suite Pass Rate
cargo test --workspace
# Expected: 175 tests pass (70 core unit, 38 engine, 2 stress, 5 adversarial, 60 gui)

# 3. Verify Orphaned Controllers (MAJ-02)
grep -rnE "AdbController|BookmarkController|SearchController|SplitViewController" crates/loganalysis-gui/src/workspace/ || echo "0 matches"
# Expected: 0 matches (controllers are completely unused in workspace)

# 4. Verify Selection Model Index Space Desynchronization (CRIT-02)
sed -n '240,244p' crates/loganalysis-gui/src/workspace/adb.rs
sed -n '33,39p' crates/loganalysis-gui/src/selection.rs
# Expected: trimmed (raw buffer count) is passed into saturating_sub on selected_row (visible index)

# 5. Verify Subprocess Linux-Only Gating (MAJ-04)
sed -n '142,153p' crates/loganalysis-core/src/adb/device.rs
# Expected: #[cfg(target_os = "linux")] gates O_NONBLOCK setting

# 6. Verify Facet Palette Nonexistent Color (MIN-01)
grep -n "Magenta" crates/loganalysis-gui/src/workspace/dock.rs
grep -w '"Magenta"' crates/loganalysis-core/src/constants.rs || echo '"Magenta" not in COLOR_MAP'
# Expected: Present in dock.rs palette, absent in constants.rs COLOR_MAP

# 7. Verify Rustdoc Warning (INF-04)
cargo doc --workspace --no-deps
# Expected: Emits warning: unclosed HTML tag `str` in crates/loganalysis-core/src/buffer/mod.rs:126:36

# 8. Verify Section 3.1.2 Streaming Batch Loop Against Source (MAJ-06)
sed -n '380,387p' crates/loganalysis-core/src/filter/engine.rs
sed -n '434,443p' crates/loganalysis-core/src/filter/engine.rs
# Expected: Matches append_streaming_batch signature and PreparedFilter / MultiFilterMatcher loop

# 9. Verify Workspace Codebase Volume
find crates/ -name "*.rs" | wc -l && find crates/ -name "*.rs" | xargs wc -l | tail -n 1
find crates/ -path "*/src/*.rs" | wc -l && find crates/ -path "*/src/*.rs" | xargs wc -l | tail -n 1
# Expected: 78 Rust files (26,544 total lines); 68 files (21,549 lines) in src/

# 10. Verify Gregorian Leap-Year Bug (MAJ-10)
python3 -c '
DAYS_BEFORE_MONTH = [0, 0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335]
feb28 = 365 * 55 + (DAYS_BEFORE_MONTH[2] + 27)
mar01 = 365 * 55 + (DAYS_BEFORE_MONTH[3] + 0)
print("Jump:", mar01 - feb28, "days (expected 1 day)")
'
# Expected: Outputs "Jump: 2 days (expected 1 day)", confirming 24h phantom jump in non-leap years
```

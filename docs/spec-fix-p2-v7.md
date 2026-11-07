# Specification: P2 Architecture, Invariant Completeness & UI Polish (v7)

**Target Codebase:** [`/home/drew/github/hwanjongyu/loganalysis`](file:///home/drew/github/hwanjongyu/loganalysis)  
**Author:** Software Architect & Concurrency Specialist  
**Status:** Approved for Implementation (Autonomous `/goal` Mode)  
**Date:** 2026-11-06  
**Audit Reference:** [`docs/multi_model_review_report.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/multi_model_review_report.md)

---

## 1. Problem Statements & Technical Root Causes

### P2-1: `[INV-09]` Streaming Hit Counter Synchronization on Buffer Trim
- **Location:** [`crates/loganalysis-core/src/filter/engine.rs:352-370`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L352-L370)
- **Root Cause:**
  When the live streaming ring buffer trims lines (`trimmed_count > 0`), `result.total_matched_lines` is decremented by `drop_count`. However, the per-filter and per-rule hit counters (`result.filter_counts` and `result.rule_counts`) are never decremented.
  Although a background periodic full reapply runs every 10k lines or 5 seconds, between intervals the pill counters on active filter badges drift upwards, violating strict `[INV-09]` invariant specifications.
- **Target Fix:**
  1. Record matched rule flat indices for each visible line inside `FilterResult` via `visible_filter_matches: Vec<smallvec::SmallVec<[usize; 2]>>`.
  2. In `append_streaming_batch`, when `drop_count > 0`, iterate through the dropped matches `0..drop_count` and decrement `result.filter_counts[idx]` via `saturating_sub(1)`.
  3. Drain `result.visible_filter_matches.drain(0..drop_count)`.
  4. Resynchronize `result.rule_counts` from `result.filter_counts`.

---

### P2-2: Scrollbar Error Heatmap Tick Quad Culling & Batching
- **Location:** [`crates/loganalysis-gui/src/components/log_view.rs:952-985`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs#L952-L985)
- **Root Cause:**
  In `render_scrollbar_track`, the heatmap generator iterates over all 200 buckets. For `LineSeverity::Normal` (empty of errors/warnings), it produces a transparent `div().opacity(0.0)` element. In typical logs, >90% of buckets are empty, resulting in ~180 invisible elements created, styled, and inserted into GPUI's element tree on every frame.
- **Target Fix:**
  1. Return `None` for `LineSeverity::Normal` buckets, immediately discarding ~90% of unnecessary GPUI elements.
  2. Coalesce consecutive buckets sharing the same severity into a single contiguous quad element with combined height, drastically reducing element tree overhead during smooth scrolling.

---

### P2-3: Streaming Ingestion EPS Profiling Throttling
- **Location:** [`crates/loganalysis-gui/src/workspace.rs:727-738`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L727-L738)
- **Root Cause:**
  On every incoming ADB streaming chunk, `EpsTimelineData::compute` is called synchronously on the UI thread across all visible lines, even when the EPS timeline ribbon is hidden (`workspace.is_timeline_visible == false`).
- **Target Fix:**
  Guard `workspace.timeline_data = Some(...)` with `if workspace.is_timeline_visible { ... }`.

---

### P2-4: Desktop File Directive Injection Sanitization
- **Location:** [`crates/loganalysis-gui/src/main.rs:296-316`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs#L296-L316)
- **Root Cause:**
  `std::env::var("APPIMAGE")` or `current_exe()` is interpolated directly into `Exec="{}" %F`. If the path contains embedded quotes or newlines, desktop entry directives could be injected.
- **Target Fix:**
  Strip control characters, newlines, and unescaped quotes from the executable path before generating the `.desktop` file.

---

### P2-5: Controller State Organization in `types.rs`
- **Location:** [`crates/loganalysis-gui/src/types.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/types.rs)
- **Target Fix:**
  Define structured state structs `SplitViewState`, `AdbStreamState`, `SearchControllerState`, and `BookmarkDrawerState` to modularize workspace state.

---

## 2. Invariants Conformance

- **`[INV-09]` Ring Buffer Trim Hit Counter Synchronization:**
  - Upgraded from **PARTIAL** to **FULL PASS** through exact per-dropped-line counter subtraction in `append_streaming_batch`.

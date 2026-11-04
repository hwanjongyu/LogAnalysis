# 🛠️ P1 Performance & Concurrency Remediation Specification (v3.0)

- **Version:** 3.0.0
- **Scope:** Priority 1 Performance, Concurrency & Responsiveness Enhancements
  - **P1-1:** Stream Ingestion Batching, Filter Throttling & Bounded Pause Buffer
  - **P1-2:** Pre-compiled Search Matcher Caching Outside Render Loop
  - **P1-3:** Asynchronous File Loading & Offset Indexing on Background Executor
  - **P1-4:** Zero-Allocation Virtualized Viewport Row Text Rendering (`SharedString`)
  - **P1-5:** Single-Pass In-Place Character Iteration in `delete_previous_word_at`
- **Status:** Approved for Autonomous Execution via `/goal`

---

## 1. Problem Statements & Technical Solutions

### 1.1 [P1-1] Stream Ingestion Batching, Throttling & Bounded Buffer
- **Location:** [`crates/loganalysis-gui/src/workspace.rs:328-372, 1253-1275`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L328-L372)
- **Problem:**
  1. `start_adb_stream` checks `rx.try_recv()` once per timer tick (16ms) and processes only one chunk. If ADB produces 20 chunks during high activity, it lags behind or triggers 20 separate Rayon tasks.
  2. Each chunk clones `StreamLogBuffer` (`VecDeque<Arc<str>>`), allocating and incrementing 200,000 refcounts.
  3. Pausing streaming pushes into an unbounded `paused_stream_buffer: Vec<Vec<String>>`, causing eventual OOM.
  4. Resuming flushes everything synchronously on the UI thread.
- **Solution:**
  1. Drain all pending chunks in a single batch: `let mut batch = Vec::new(); while let Ok(chunk) = rx.try_recv() { batch.extend(chunk); }`.
  2. Append the entire batch in a single call to `StreamLogBuffer`.
  3. Bound `paused_stream_buffer` to `MAX_MONITOR_LINES = 200,000` lines total, dropping oldest lines if exceeded.
  4. Throttle `reapply_filters` during streaming to at most once per 60ms to prevent Rayon thread pool starvation.

---

### 1.2 [P1-2] Cache Search Matcher Outside GPUI Viewport Render Loop
- **Location:** [`crates/loganalysis-gui/src/components/log_view.rs:26-35`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs#L26-L35)
- **Problem:**
  `render_log_viewport` is invoked on every single frame, scroll step, and mouse move. If search is active, it calls `CompiledMatcher::new(...)` inside `render()`, recompiling regular expressions at 60–120 FPS on the UI thread.
- **Solution:**
  1. Add `pub cached_search_matcher: Option<Arc<CompiledMatcher>>` to `LogAnalysisWorkspace`.
  2. Add helper `update_cached_search_matcher(&mut self)` invoked whenever `search_query`, `is_search_case_sensitive`, `is_search_regex`, or search visibility changes.
  3. In `render_log_viewport`, simply read `workspace.cached_search_matcher.clone()`.

---

### 1.3 [P1-3] Asynchronous File Loading & Offset Indexing
- **Location:** [`crates/loganalysis-gui/src/workspace.rs:158-173`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L158-L173)
- **Problem:**
  `FileLogBuffer::open(path)` runs synchronously on the main UI thread. For files over 1 GB, SIMD newline scanning and line offset vector allocation block the event loop for 200–500ms, creating visible UI freezing.
- **Solution:**
  1. Dispatch `FileLogBuffer::open` onto `cx.background_executor().spawn(...)`.
  2. Immediately set `status_text = "Opening log file...".into()` and clear selection.
  3. Upon completion, update `workspace.log_source = Some(Arc::new(ActiveLogSource::File(buf)))` and trigger `workspace.reapply_filters(cx)` on the UI context.

---

### 1.4 [P1-4] Eliminate Row String Allocations in Virtualized Viewport
- **Location:** [`crates/loganalysis-gui/src/components/log_view.rs:81-85`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs#L81-L85)
- **Problem:**
  `src.get_line(r_idx).unwrap_or_default().to_string()` converts `Cow<'_, str>` into an owned `String` for every visible line on every layout frame.
- **Solution:**
  Convert `Cow<'_, str>` directly into `SharedString` via `SharedString::from(s.into_owned())` or `SharedString::from(s.as_ref().to_string())` without extra intermediate copies.

---

### 1.5 [P1-5] Optimize `delete_previous_word_at` in `text_edit.rs`
- **Location:** [`crates/loganalysis-core/src/text_edit.rs:55-66`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/text_edit.rs#L55-L66)
- **Problem:**
  `delete_previous_word_at` executes `let chars: Vec<char> = s.chars().collect();` inside `while cur > 0`, allocating vectors repeatedly.
- **Solution:**
  Collect characters once upfront, or operate directly on character indices and rebuild the string slice in a single operation.

---

## 2. Verification Criteria
1. `cargo check --workspace` and `cargo test --workspace` pass cleanly.
2. Viewport rendering contains zero regex compilations.
3. Stream ingestion batches incoming chunks and throttles filtering.
4. Paused stream buffer is capped to `MAX_MONITOR_LINES`.
5. File loading is fully asynchronous without blocking GPUI main event thread.
6. QA report generated in `docs/qa-fix-p1-v3.md`.

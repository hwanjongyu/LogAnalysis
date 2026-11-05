# Specification: P1 Critical Performance & Concurrency Hotfixes

**Target Codebase:** [`/home/drew/github/hwanjongyu/loganalysis`](file:///home/drew/github/hwanjongyu/loganalysis)  
**Author:** Software Architect & Concurrency Specialist  
**Status:** Approved for Implementation (Autonomous `/goal` Mode)  
**Date:** 2026-11-04  

---

## 1. Problem Statements & Technical Root Causes

### P1-1: CJK IME Preemption in `TextInput` & Duplicate Modal Key Handlers
- **Location:**  
  - [`crates/loganalysis-gui/src/components/text_input.rs:702-711`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L702-L711)
  - [`crates/loganalysis-gui/src/modals/filter_edit_modal.rs:64-73`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/modals/filter_edit_modal.rs#L64-L73)
  - [`crates/loganalysis-gui/src/workspace.rs:664-672`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L664-L672)
- **Root Cause:**  
  1. In `TextInput`, `on_action(Submit)` and `on_action(Cancel)` do not check whether `self.marked_range.is_some()`. During active Korean/CJK syllable composition, pressing `Enter` to commit the preedit block prematurely triggers `cx.emit(TextInputEvent::Submit)`, closing the modal before the user has finished typing.
  2. `modal-card-filter-edit` attaches an `.on_key_down` handler that intercepts `escape` and `enter` during event bubbling, bypassing `TextInput`'s action handlers.
  3. `LogAnalysisWorkspace::handle_key_down` unconditionally intercepts `escape` and `enter` when `filter_edit_modal.is_open`.
- **Target Fix:**  
  1. In `TextInput::on_action(Submit)`: if `self.marked_range.is_some()`, ignore the submit action so the IME can commit the preedit text cleanly.
  2. In `TextInput::on_action(Cancel)`: if `self.marked_range.is_some()`, clear `self.marked_range` without emitting `Cancel`, aborting the IME preedit without closing the modal.
  3. Remove the redundant `.on_key_down` listener from `modal-card-filter-edit`.
  4. In `workspace.rs::handle_key_down`, do not preempt `enter` when the filter modal is open, allowing `TextInput` to handle submission deterministically.

---

### P1-2: Stale Search Result Race Condition in `workspace.search()`
- **Location:** [`crates/loganalysis-gui/src/workspace.rs:547-581`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L547-L581)
- **Root Cause:**  
  1. `search()` passes `self.cancel_generation.load(Ordering::SeqCst)` into the background search task without calling `fetch_add(1, Ordering::SeqCst) + 1`. Subsequent searches never invalidate earlier in-flight searches.
  2. In the async callback (`this.update(&mut async_cx, |workspace, cx| ...)`), there is no check comparing `workspace.cancel_generation` against `generation`. If an older, slower search task completes after a faster new search, the stale result overwrites the selection and viewport scroll position.
- **Target Fix:**  
  1. Atomically advance `let generation = self.cancel_generation.fetch_add(1, Ordering::SeqCst) + 1;` at the entry of `search()`.
  2. Inside `this.update`, verify `if workspace.cancel_generation.load(Ordering::SeqCst) != generation { return; }` before applying search results.

---

### P1-3: Viewport Line Rendering String Allocation Churn
- **Location:** [`crates/loganalysis-gui/src/components/log_view.rs:72-76`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs#L72-L76)
- **Root Cause:**  
  ```rust
  let t: SharedString = src
      .get_line(r_idx)
      .map(|s| s.into_owned().into())
      .unwrap_or_default();
  ```
  Calling `s.into_owned()` allocates a brand new heap `String` for every visible line on every 60Hz frame render or scroll event. For a 60-row viewport, this performs 3,600 heap allocations/sec.
- **Target Fix:**  
  Convert directly from `s.as_ref()` into `SharedString::from(s.as_ref())`, bypassing the intermediate owned `String` allocation.

---

### P1-4: Incremental Streaming Filtering (Eliminating Full Buffer Rayon Passes)
- **Location:**  
  - [`crates/loganalysis-core/src/filter/engine.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs)
  - [`crates/loganalysis-gui/src/workspace.rs:430-445`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L430-L445)
- **Root Cause:**  
  Currently, live streaming invokes `reapply_filters` every 60ms. `reapply_filters` executes a full Rayon multi-threaded pass across all 200,000 lines in the buffer, allocating new `visible_indices` and `visible_styles` vectors every 60ms. This saturates all CPU cores during ADB streaming.
- **Target Fix:**  
  1. Implement `append_streaming_chunk` in `loganalysis-core`:
     - Given a previous `FilterResult` and newly appended lines (`start_idx..end_idx`), evaluate only the newly added lines against the active filters.
     - Append newly matched visible indices and resolved styles directly to the existing `FilterResult`.
     - Increment filter counts and match metrics incrementally.
  2. If `StreamLogBuffer::trim_if_needed()` trimmed lines from the head (which only occurs once every 10,000 lines):
     - Slice off trimmed indices and decrement remaining visible indices by `trimmed_count`.
  3. In `workspace.rs`: When new stream chunks arrive, use the incremental append path when filters have not changed, reducing CPU utilization during live streaming from ~80% to <1%.

---

### P1-5: Single-Pass Multi-Pattern Aho-Corasick SIMD Automaton
- **Location:**  
  - [`crates/loganalysis-core/Cargo.toml`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/Cargo.toml)
  - [`crates/loganalysis-core/src/filter/matcher.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/matcher.rs)
  - [`crates/loganalysis-core/src/filter/engine.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs)
- **Root Cause:**  
  When multiple literal filters are active, `evaluate_and_style_line` loops through all $K$ filters sequentially ($O(N \cdot K)$).
- **Target Fix:**  
  1. Add `aho-corasick = "1.1"` to `loganalysis-core`.
  2. When multiple literal filters share the same case-sensitivity mode, compile them into an `AhoCorasick` multi-pattern automaton with Teddy SIMD acceleration, scanning each line in a single pass ($O(N)$).

---

## 2. Implementation Schedule & Verification Matrix

| Task | Target Files | Verification Method |
| :--- | :--- | :--- |
| **P1-1** | `crates/loganalysis-gui/src/components/text_input.rs`<br>`crates/loganalysis-gui/src/modals/filter_edit_modal.rs`<br>`crates/loganalysis-gui/src/workspace.rs` | Unit test: Enter/Cancel while `marked_range.is_some()` must NOT emit submit/cancel event. |
| **P1-2** | `crates/loganalysis-gui/src/workspace.rs` | Test: Stale search generation aborts and does not overwrite updated search result. |
| **P1-3** | `crates/loganalysis-gui/src/components/log_view.rs` | Build & benchmark: Zero heap `String` allocation in virtual row loop. |
| **P1-4** | `crates/loganalysis-core/src/filter/engine.rs`<br>`crates/loganalysis-gui/src/workspace.rs` | Integration test: Incremental chunk append produces identical `FilterResult` to full pass. |
| **P1-5** | `crates/loganalysis-core/Cargo.toml`<br>`crates/loganalysis-core/src/filter/matcher.rs` | Benchmark & test: Multi-literal pattern matching produces identical hit counts to sequential finder. |

---

## 3. Acceptance Criteria

- `cargo test --workspace`: 100% test pass rate with zero regressions.
- `cargo clippy --all-targets`: Clean with zero warnings.
- IME composition test: Typing Hangul with active preedit does not dismiss modal on Enter.
- Search cancellation test: Stale generation results are rejected cleanly.
- Incremental filtering test: Streaming throughput scales without full-buffer recomputation.

# 📋 Engineering Specification: Remediation of 5 Critical (P0) Concurrency, Rendering & IME Hazards

**Document:** `docs/spec-fix-p0-v5.md`  
**Target:** [`crates/loganalysis-core`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core), [`crates/loganalysis-gui`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui)  
**Lead Architect:** Gemini Pro (Flagship)  
**Verification:** Claude Sonnet (Extended Thinking) & Claude Opus (Ultra-Reasoning)

---

## 1. Overview & Objective

Following the multi-model consensus audit ([`docs/multi_model_review_report.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/multi_model_review_report.md)), this technical specification outlines the deterministic remediation plan for the 5 critical (P0) bugs and regressions identified in the LogAnalysis workspace:

1. **[P0.1] Buffer Clone Churn in GUI Streaming Loop**: Elimination of per-batch cloning of 200,000 `Arc<str>` pointers by removing `Arc` captures in `uniform_list` and utilizing `weak_self.upgrade()`.
2. **[P0.2] Monotonic Desync of Matched Line & Filter Counts**: Accurate decrementing of `total_matched_lines` on rolling buffer head trims and reactive resynchronization triggering.
3. **[P0.3] Orphaned ADB Subprocess on Source Switch & Clear**: Clean RAII child process termination during `open_file` and `clear_logs`.
4. **[P0.4] UTF-16 to UTF-8 Desync & Char Boundary Panic in `TextInput`**: Converting `new_selected_range_utf16` relative to `new_text` rather than `self.content`.
5. **[P0.5] Missing Event Interception on IME Enter Key**: Halting action propagation (`cx.stop_propagation()`) during Hangul/CJK preedit composition.

---

## 2. Technical Design & Implementation Details

### 2.1 [P0.1] Zero-Copy Streaming Ingestion via `weak_self` in `uniform_list`
* **Root Cause:** In [`crates/loganalysis-gui/src/components/log_view.rs:20-21`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs#L20-L21), cloning `workspace.log_source` and `workspace.filter_result` into the `uniform_list` closure keeps the `Arc` strong reference count $\ge 2$ between render frames. Consequently, `Arc::get_mut(src)` in `workspace.rs:423` fails, forcing a full copy of 200,000 `Arc<str>` pointers (~1.6 MB) per batch.
* **Remediation:**
  1. In `log_view.rs`, remove `let filter_res = workspace.filter_result.clone();` and `let source = workspace.log_source.clone();`.
  2. Inside `uniform_list("log-view", row_count, move |range, _window, cx| ...)`:
     Upgrade `weak_self.upgrade().map(|e| e.read(cx))` to borrow `&LogAnalysisWorkspace` dynamically for each rendered visible range.
  3. Because the closure retains only `WeakEntity<LogAnalysisWorkspace>`, `workspace.log_source` maintains `strong_count == 1` throughout streaming ingestion, allowing `Arc::get_mut` to succeed 100% of the time and mutate `StreamLogBuffer` in-place with zero allocations.

### 2.2 [P0.2] Matched Line Count Synchronization on Buffer Trim
* **Root Cause:** In [`crates/loganalysis-core/src/filter/engine.rs:352-361`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs#L352-L361), `append_streaming_batch` drains trimmed rows from `visible_indices` and `visible_styles`, but never decrements `total_matched_lines`.
* **Remediation:**
  1. In `append_streaming_batch`:
     - When `show_only_filtered` is `true`, every dropped line is a match: `result.total_matched_lines = result.total_matched_lines.saturating_sub(drop_count)`.
     - When `show_only_filtered` is `false`, inspect `result.visible_styles[0..drop_count]` to count styled lines and decrement `result.total_matched_lines` accordingly.
  2. In `workspace.rs`, track `accumulated_trimmed_lines`. When accumulated trims exceed `MONITOR_TRIM_BATCH * 2` or after 5 seconds of active streaming, trigger throttled `workspace.reapply_filters(cx)` to guarantee exact rule hit count parity.

### 2.3 [P0.3] ADB Process Lifecycle Teardown
* **Root Cause:** In [`crates/loganalysis-gui/src/workspace.rs:181`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L181) and [`L1306`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L1306), replacing `log_source` leaves `self.is_streaming == true` and orphans the child process.
* **Remediation:**
  Add `if self.is_streaming { self.stop_adb_stream(cx); }` at the entry of both `open_file` and `clear_logs`.

### 2.4 [P0.4] Correct UTF-16 to UTF-8 Byte Offset Mapping in `TextInput`
* **Root Cause:** In [`crates/loganalysis-gui/src/components/text_input.rs:428-435`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L428-L435), `range_from_utf16` was invoked on `self.content` instead of `new_text`.
* **Remediation:**
  Implement `utf16_range_to_utf8_in_str(new_text, r)` to convert `new_selected_range_utf16` within `new_text`, then offset by `start`. Add char boundary and clamp safety guards to `self.selected_range` and `text_for_range`.

### 2.5 [P0.5] IME Preedit Event Bubbling Interception
* **Root Cause:** In [`crates/loganalysis-gui/src/components/text_input.rs:702-710`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L702-L710), returning early on `this.marked_range.is_some()` skips `cx.stop_propagation()`.
* **Remediation:**
  Call `cx.stop_propagation(); return;` when `this.marked_range.is_some()`.

---

## 3. Verification & Testing Criteria

1. **Unit Tests**:
   - `test_ime_utf16_range_to_utf8_mixed_cjk`: Verify `replace_and_mark_text_in_range` with ASCII prefix and multi-byte Korean characters never panics and sets cursor at valid char boundaries.
   - `test_append_streaming_batch_trims_adjust_total_matched`: Verify `total_matched_lines` decrements when head trims occur.
   - `test_stop_adb_stream_on_source_switch`: Verify opening a file or clearing logs resets streaming state and drops the child handle.
2. **Regression Check**: All 41 existing workspace tests must pass.

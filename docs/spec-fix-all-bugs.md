# Specification: Full Bug Remediation (`fix-all-bugs`)

- **Target Workspace**: `/home/drew/github/hwanjongyu/loganalysis` (`v0.2.4`, `loganalysis-core` + `loganalysis-gui`)
- **Source Baseline**: Read-only audit in `REVIEW_REPORT.md` cross-verified against live workspace source and tests (`295/295` currently passing).
- **Scope**:
  - High Severity: `[HIGH-1]` through `[HIGH-5]`
  - Medium Severity: `[MEDIUM-1]` through `[MEDIUM-10]`
  - Low Severity: `[LOW-1]` through `[LOW-8]` (including `[LOW-8a]`–`[LOW-8f]`)
  - UI Geometry & CLI: Column View `24px` scrollbar height alignment (`log_view.rs:1352-1369`) & CLI `--help`/`-h`/multi-file handling (`cli.rs`, `lib.rs`)
  - Defective Core Tests: `engine_tests.rs:228-245` & `challenger_core_stress_tests.rs:607-631`
- **Hard Guardrails**:
  - Preserve all existing invariants in [`docs/INVARIANTS.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/INVARIANTS.md) (`[INV-01]`–`[INV-15]`).
  - Zero `std::thread::sleep` in any new or updated tests.
  - Maintain cross-platform CI compatibility across `ubuntu-22.04`, `macos-15`, and `windows-latest` (do **not** enable `gpui`'s `test-support` feature, which unconditionally pulls `wayland` and `x11` in `gpui 0.2.2`).

---

## 1. High Severity Fix Contracts

### `[HIGH-1]` Orphaned `search_cancel_gen` fails to cancel background searches on log source switch
- **Files & Lines**:
  - `crates/loganalysis-gui/src/workspace/mod.rs:73, 322, 1030-1045`
  - `crates/loganalysis-gui/src/controllers/search.rs:12, 91-93`
  - `crates/loganalysis-gui/src/workspace/search.rs:98-99, 122`
- **Root Cause**: `LogAnalysisWorkspace` owns an orphaned `search_cancel_gen: Arc<AtomicU64>` incremented inside `reset_derived_log_state()`, while `workspace/search.rs` uses `self.search_ctrl.cancel_gen`. Background searches (> `ASYNC_SEARCH_THRESHOLD` = 50,000 rows) are never cancelled when switching log sources and can apply stale row/selection state to the new source via `apply_search_result`.
- **Exact Fix Contract**:
  1. Remove `pub(crate) search_cancel_gen: Arc<AtomicU64>` from `LogAnalysisWorkspace` (`workspace/mod.rs:73`) and its initializer (`workspace/mod.rs:322`).
  2. In `LogAnalysisWorkspace::reset_derived_log_state(&mut self)` (`workspace/mod.rs:1030-1045`), replace `self.search_cancel_gen.fetch_add(1, Ordering::Relaxed);` with `self.search_ctrl.abort_ongoing_searches();`.
  3. Also ensure `reset_derived_log_state()` clears `self.search_ctrl.match_status = None;` (already present at line 1038) and preserves `filter_cancel_gen` independence (`[INV-02-A]`).
- **Edge Cases & Non-Regression Invariants**:
  - `[INV-02-A]`: `filter_cancel_gen` and `search_ctrl.cancel_gen` remain independent `Arc<AtomicU64>` tokens; typing in or closing the Find Bar never cancels filtering.
  - Combined with `[HIGH-3]`, calling `reset_derived_log_state()` synchronously before spawning `open_file`/`open_merged_files` cancels in-flight background searches immediately on source switch initiation.

---

### `[HIGH-2]` `run_command_with_timeout` blocks indefinitely on `child.wait()` if child closes stdout without exiting
- **Files & Lines**:
  - `crates/loganalysis-core/src/adb/device.rs:182-259` (specifically `:245-251`)
- **Root Cause**: When a subprocess closes/redirects stdout and hangs, the pipe-reader thread hits EOF and drops `tx`. `rx.recv_timeout(...)` returns `Err(RecvTimeoutError::Disconnected)`, and line 247 calls unbounded blocking `child.wait()`, defeating timeout enforcement.
- **Exact Fix Contract**:
  1. In `pub fn run_command_with_timeout(mut cmd: Command, timeout: Duration) -> Result<Vec<u8>, AdbError>`, replace lines 245-251 (`Err(crossbeam_channel::RecvTimeoutError::Disconnected)`) with a bounded `child.try_wait()` polling loop:
     ```rust
     Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
         pipe_disconnected = true;
         break loop {
             match child
                 .try_wait()
                 .map_err(|e| AdbError::ExecutionFailed(e.to_string()))
             {
                 Ok(Some(status)) => break status,
                 Ok(None) => {
                     let remaining = timeout.saturating_sub(start_time.elapsed());
                     if remaining.is_zero() {
                         cleanup_aborted_command(child, rx, reader_handle);
                         return Err(AdbError::Timeout(timeout));
                     }
                     std::thread::sleep(remaining.min(Duration::from_millis(5)));
                 }
                 Err(err) => {
                     cleanup_aborted_command(child, rx, reader_handle);
                     return Err(err);
                 }
             }
         };
     }
     ```
  2. Note: `cleanup_aborted_command(child, rx, reader_handle)` kills `child`, reaps via `child.wait()`, drops `rx`, and joins `reader_handle` (`device.rs:107-116`). On `Ok(Some(status))`, line 256 (`if pipe_disconnected { let _ = reader_handle.join(); }`) cleans up the pipe thread before checking `status.success()`.
- **Edge Cases & Non-Regression Invariants**:
  - Child closes stdout and exits normally (`0` or non-zero) within `timeout`: returns captured `Ok(output)` or `Err(AdbError::NonZeroExit(...))`.
  - Child closes stdout and sleeps/hangs: returns `Err(AdbError::Timeout(timeout))` promptly after `timeout` and reaps the child without zombies (`[INV-07]`).
  - Existing tests in `device.rs:291-424` continue passing; new unit test verifies stdout-closed hanging subprocess (`sh -c "exec 1>&-; sleep 30"` on Unix) times out deterministically with zero `std::thread::sleep` in the test body.

---

### `[HIGH-3]` Missing generation guards on async file/merge/secondary open tasks
- **Files & Lines**:
  - `crates/loganalysis-gui/src/workspace/mod.rs:47-121, 298-359, 437-486, 840-849` (`open_file`, `clear_logs`)
  - `crates/loganalysis-gui/src/workspace/merged.rs:37-93` (`open_merged_files`)
  - `crates/loganalysis-gui/src/workspace/split_view.rs:94-146` (`open_secondary_file`)
  - `crates/loganalysis-gui/src/workspace/adb.rs:168-472` (`start_adb_stream`)
  - `crates/loganalysis-gui/src/controllers/split_view.rs:7-59` (`SplitViewController`)
- **Root Cause**: `open_file`, `open_merged_files`, and `open_secondary_file` spawn `bg_executor` tasks and unconditionally overwrite `log_source`, `active_file_name`, `secondary_log_source`, and `status_text` when `.await` resolves without checking a monotonic source generation token, and defer `reset_derived_log_state()` until after `.await` (or omit it in `open_secondary_file`).
- **Exact Fix Contract**:
  1. Add monotonic generation tracking:
     - On `LogAnalysisWorkspace` (`workspace/mod.rs`): `pub(crate) primary_source_gen: u64` (initialized `0`), with helper `pub(crate) fn advance_primary_source_gen(&mut self) -> u64 { self.primary_source_gen = self.primary_source_gen.wrapping_add(1); self.primary_source_gen }`.
     - On `SplitViewController` (`controllers/split_view.rs`): `pub secondary_source_gen: u64` (initialized `0`), with helper `pub fn advance_source_gen(&mut self) -> u64 { self.secondary_source_gen = self.secondary_source_gen.wrapping_add(1); self.secondary_source_gen }` and increment inside `SplitViewController::clear(&mut self)`.
     - Extract a small pure helper struct/methods (e.g., `SourceGenerationGuard` or pure checks on `SplitViewController` / `LogAnalysisWorkspace`) if helpful so generation invalidation logic can be unit-tested without `Context<Self>`.
  2. In `open_file(&mut self, path: &Path, cx: &mut Context<Self>)` (`workspace/mod.rs:437-486`):
     - Synchronously before spawning:
       1. Stop active ADB stream (`if self.adb.is_streaming { self.stop_adb_stream(cx); }`, preserving `[INV-06]`).
       2. `let open_gen = self.advance_primary_source_gen();`
       3. Clear primary source and reset derived state synchronously pre-spawn:
          `self.log_source = None;`
          `self.active_file_name = None;`
          `self.reset_derived_log_state();`
       4. Update `status_text` and `cx.notify()`.
     - Inside `this.update(&mut async_cx, |workspace, cx| { ... })`:
       - Guard **both** `Ok(buf)` and `Err(e)` branches at the very top:
         `if workspace.primary_source_gen != open_gen { return; }`
       - Remove the post-`.await` `reset_derived_log_state()` (already performed pre-spawn; or assert clean state) and install `log_source`, `active_file_name`, `add_recent_file_async`, `refresh_facet_tags_async`, `status_text`, and `reapply_filters(cx)`.
  3. In `open_merged_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>)` (`workspace/merged.rs:37-93`):
     - Return early if `paths.is_empty()` **before** stopping a live ADB stream or clearing state.
     - Synchronously before spawning:
       1. Stop active ADB stream if `self.adb.is_streaming`.
       2. `let merge_gen = self.advance_primary_source_gen();`
       3. `self.log_source = None; self.active_file_name = None; self.reset_derived_log_state();`
     - Inside `this.update(...)`: check `if workspace.primary_source_gen != merge_gen { return; }` before handling `Ok(buf)` or `Err(e)`.
  4. In `start_adb_stream(&mut self, cx: &mut Context<Self>)` (`workspace/adb.rs:196-205, 250-462`):
     - On `Ok(handle)`, capture `let stream_gen = self.advance_primary_source_gen();` (synchronously superseding any in-flight `open_file`/`open_merged_files`), call `self.reset_derived_log_state()`, and install `ActiveLogSource::Stream`.
     - Inside the async stream ingestion loop (`workspace/adb.rs:250, 399, 424, 435, 455`), verify `workspace.primary_source_gen == stream_gen && workspace.adb.is_streaming`; if `primary_source_gen != stream_gen`, break cleanly without modifying `log_source`, `filter_result`, or `status_text`.
  5. In `clear_logs(&mut self, cx: &mut Context<Self>)` (`workspace/mod.rs:840-849`):
     - Call `self.advance_primary_source_gen();` so any pending async open/merge resolves as stale and is discarded.
  6. In `open_secondary_file(&mut self, path: &Path, cx: &mut Context<Self>)` (`workspace/split_view.rs:94-146`):
     - Synchronously before spawning:
       1. `let sec_gen = self.split_view.advance_source_gen();`
       2. `self.secondary_filter_cancel_gen.fetch_add(1, Ordering::SeqCst);`
       3. `self.split_view.secondary_log_source = None;`
       4. `self.split_view.secondary_file_name = None;`
       5. `self.split_view.secondary_filter_result = None;`
       6. `self.secondary_selection.clear();`
       7. `self.split_view.is_split_view = true;`
     - Inside `this.update(...)`: check `if workspace.split_view.secondary_source_gen != sec_gen { return; }` before handling `Ok(buf)` or `Err(e)`.
- **Edge Cases & Non-Regression Invariants**:
  - Opening slow file A -> fast file B -> A finishes late: `primary_source_gen` mismatch drops A (and A's error if A failed).
  - Starting ADB stream while file A is loading: `primary_source_gen` advances so A cannot overwrite the live stream or orphan `adb.stream_handle`.
  - `[INV-06]` preserved on all primary source transitions.

---

### `[HIGH-4]` `adb.is_paused` is never reset across stream stop, clear, or restart
- **Files & Lines**:
  - `crates/loganalysis-gui/src/controllers/adb.rs:14, 41-46`
  - `crates/loganalysis-gui/src/workspace/adb.rs:168-206, 362-365, 399-402, 506-514`
  - `crates/loganalysis-gui/src/workspace/mod.rs:1030-1045`
- **Root Cause**: Pausing sets `self.adb.is_paused = true`, but `stop_stream`, `stop_adb_stream`, `start_adb_stream`, stream disconnection, and `reset_derived_log_state` leave `is_paused == true`. Starting a subsequent stream silently diverts all lines into `paused_buffer`.
- **Exact Fix Contract**:
  1. In `AdbController::stop_stream(&mut self)` (`controllers/adb.rs:41-46`):
     ```rust
     pub fn stop_stream(&mut self) {
         if let Some(handle) = self.stream_handle.take() {
             handle.stop();
         }
         self.is_streaming = false;
         self.is_paused = false;
         self.clear_paused_buffer();
     }
     ```
  2. In `LogAnalysisWorkspace::reset_derived_log_state(&mut self)` (`workspace/mod.rs:1043-1044`):
     - Set `self.adb.is_paused = false;` and `self.adb.clear_paused_buffer();`.
  3. In `LogAnalysisWorkspace::stop_adb_stream(&mut self, cx)` (`workspace/adb.rs:506-514`):
     - Replace lines 507-510 with `self.adb.stop_stream();`.
  4. In `LogAnalysisWorkspace::start_adb_stream(&mut self, cx)` (`workspace/adb.rs:198-205`):
     - Explicitly reset `self.adb.is_paused = false;` and `self.adb.clear_paused_buffer();` (in addition to `reset_derived_log_state()`).
  5. On pipe disconnection in the ADB stream loop (`workspace/adb.rs:362-365` and `:399-402`):
     - Also reset `workspace.adb.is_paused = false;` and `workspace.adb.clear_paused_buffer();` when setting `workspace.adb.is_streaming = false;`.
- **Edge Cases & Non-Regression Invariants**:
  - `test_workspace_controller_snapshots_and_defaults` (`gui_integration_tests.rs:1097-1119`) preserves `auto_scroll = true` default and `clear_paused_buffer()` behavior.
  - Pausing during stream, stopping/clearing/disconnecting, and starting a fresh stream always starts unpaused (`is_paused == false`, `paused_line_count == 0`).

---

### `[HIGH-5]` Bookmark Drawer `Note` and `Delete` buttons bubble clicks to the parent row
- **Files & Lines**:
  - `crates/loganalysis-gui/src/modals/bookmark_drawer.rs:84-88, 130-168`
- **Root Cause**: Parent row `bookmark-item` registers `.on_click(...)` to jump and close the drawer (`84-88`). Child buttons `btn-drawer-note` (`145-147`) and `btn-drawer-del` (`160-165`) also use `.on_click(...)` without `cx.stop_propagation()`. Under GPUI 0.2.2 (`div.rs:2134-2247`, `window.rs:3705-3711`), bubble-phase mouse-up click listeners propagate to `bookmark-item` unless `cx.stop_propagation()` is invoked.
- **Exact Fix Contract**:
  1. On the action button container `div` (`bookmark_drawer.rs:131-135`), add:
     `.on_mouse_down(MouseButton::Left, |_, _window, cx| { cx.stop_propagation(); })`
     (in GPUI 0.2.2 paint/listener order, the child button's own `pending_mouse_down` runs before the container's `on_mouse_down` during reverse Bubble dispatch, so stopping propagation at the container prevents `bookmark-item` from recording a pending mouse-down while still allowing the clicked button to complete its click).
  2. Inside `btn-drawer-note`'s `.on_click` (`bookmark_drawer.rs:145-147`), call `cx.stop_propagation();` and `this.open_note_modal_for_source_idx(b_source_idx, cx);` (note: `lib.rs:136-139` focuses `note_input` when `is_note_modal_open && input_focus == InputFocusTarget::BookmarkNote`, and we can also focus `note_input` immediately via `window.focus(&this.note_input.read(cx).focus_handle)`).
  3. Inside `btn-drawer-del`'s `.on_click` (`bookmark_drawer.rs:160-165`), call `cx.stop_propagation();`, remove `b_source_idx`, clamp `this.bookmarks.drawer_selected_index` (`let len = this.bookmarks.len(); this.bookmarks.drawer_selected_index = if len == 0 { 0 } else { this.bookmarks.drawer_selected_index.min(len - 1) };`), update `status_text`, and `cx.notify()`.
- **Edge Cases & Non-Regression Invariants**:
  - Clicking anywhere on the row outside the action buttons still jumps to the bookmarked line and closes the drawer.
  - Clicking `📝 Note` opens the note modal while keeping the drawer in its intended state without jumping; clicking `✕` deletes only that bookmark without jumping or closing the drawer.
  - Keyboard click (`Enter`/`Space`) on focused buttons also stops propagation via `cx.stop_propagation()` inside `.on_click`.

---

## 2. Medium Severity Fix Contracts

### `[MEDIUM-1]` `find_nearest_timestamp_row` discards left half on unstructured blocks and returns `Some(0)` when no timestamps exist
- **Files & Lines**:
  - `crates/loganalysis-core/src/buffer/correlation.rs:5-63`
  - `crates/loganalysis-core/src/buffer/file_buffer.rs:29-37, 238-241`
  - `crates/loganalysis-core/src/buffer/stream_buffer.rs:24-33, 260-265`
  - `crates/loganalysis-core/src/buffer/timestamp.rs` (via `extract_timestamp_with_kind` from `[MEDIUM-10]`)
- **Root Cause**:
  1. `best_row = 0`, `best_diff = u64::MAX`, and unconditional `Some(best_row)` at line 62 returns `Some(0)` even when zero lines have timestamps.
  2. When `get_ts(mid)` is `None`, line 58 executes `low = mid + 1;`, discarding `[low..mid]` even when all timestamps live in `[low..mid]`.
  3. Single-file `FileLogBuffer` and `StreamLogBuffer` `get_timestamp_micros` return un-normalized `MonthDayTime` timestamps across Dec 31 -> Jan 01 rollover, violating binary-search monotonicity.
- **Exact Fix Contract**:
  1. In `pub fn find_nearest_timestamp_row(source: &dyn LogSource, visible_indices: &[usize], target_micros: i64) -> Option<usize>` (`correlation.rs:5-63`):
     - Track `let mut best: Option<(usize, u64)> = None;`. Return `best.map(|(row, _)| row)` at the end (guarantees `None` when zero timestamps exist).
     - Query direct `source.get_timestamp_micros(visible_indices[idx])` (never record an unstamped row index as `best_row`).
     - When `mid` has `None`: probe outward from `mid` within `[low..=high]` (alternating `mid - d` and `mid + d` for `d = 1..`) until the first timestamped `anchor_idx` in `[low..=high]` is found (or `[low..=high]` is exhausted -> `break`).
       - Evaluate `anchor_idx`: update `best`, return `Some(anchor_idx)` if `ts == target_micros`.
       - Partition safely using proven-empty `[anchor_idx..mid]` / `[mid..anchor_idx]`:
         - If `anchor_idx <= mid`:
           - If `ts < target_micros`: `low = mid + 1;`
           - Else (`ts > target_micros`): if `anchor_idx == 0 { break; } high = anchor_idx - 1;`
         - If `anchor_idx > mid`:
           - If `ts < target_micros`: `low = anchor_idx + 1;`
           - Else (`ts > target_micros`): if `mid == 0 { break; } high = mid - 1;`
  2. Normalize `MonthDayTime` year rollover in single-file `FileLogBuffer` and `StreamLogBuffer`:
     - In `FileLogBuffer`: add `first_month_day_ts: std::sync::OnceLock<Option<i64>>` (lazily finds the first line whose `extract_timestamp_with_kind` has `TimestampKind::MonthDayTime`). In `FileLogBuffer::get_timestamp_micros(idx)`: if `extract_timestamp_with_kind` returns `MonthDayTime` with `ts`, and `anchor >= YEAR_ROLLOVER_THRESHOLD_MICROS && ts < anchor - YEAR_ROLLOVER_THRESHOLD_MICROS`, return `Some(ts + ONE_YEAR_MICROS)`. (Note: `MergedLogBuffer` calls `extract_timestamp_with_kind` directly, never `FileLogBuffer::get_timestamp_micros`, avoiding double offset.)
     - In `StreamLogBuffer`: track `first_month_day_ts: Option<i64>` updated in `append_chunk` on the first `MonthDayTime` line; apply the same `MonthDayTime`-only rollover offset in `StreamLogBuffer::get_timestamp_micros(idx)` (preserving monotonic rollover even after head trims).
- **Edge Cases & Non-Regression Invariants**:
  - Empty `visible_indices` or 100% unstamped log returns `None`.
  - Large leading or trailing unstamped stack-trace/header blocks find the closest timestamped row without discarding either half.
  - `i64::MIN` and `i64::MAX` target queries use `abs_diff` with zero overflow (`challenger_core_stress_tests.rs:533-566`, `correlation.rs:122-153`).

---

### `[MEDIUM-2]` `FileLogBuffer` and `execute_filtering` choose widest line by raw byte count, and streaming trims never shrink `widest_visible_length`
- **Files & Lines**:
  - `crates/loganalysis-core/src/buffer/mod.rs:156-168, 191-200`
  - `crates/loganalysis-core/src/buffer/file_buffer.rs:61-101`
  - `crates/loganalysis-core/src/filter/engine.rs:17-30, 263-286, 324-358, 413-524`
  - `crates/loganalysis-core/tests/engine_tests.rs:1350-1423`
- **Root Cause**:
  1. `FileLogBuffer::from_storage` selects `longest_line_idx` by `raw_len` before trimming trailing whitespace and measuring CJK/UTF-8 column width.
  2. `execute_filtering` selects `local_longest_idx` by `raw_line.len()` instead of display width.
  3. `append_streaming_batch_with_matcher` never clears or recalculates `widest_visible_length` / `widest_visible_text` when `trimmed_count > 0` evicts the widest line.
- **Exact Fix Contract**:
  1. Add canonical `pub fn measure_display_width(bytes: &[u8]) -> usize` in `buffer/mod.rs`:
     - `let trimmed = trim_trailing_whitespace(bytes);`
     - Printable-ASCII fast path (zero allocation): if `trimmed.iter().all(|&b| (0x20..=0x7e).contains(&b))` -> `trimmed.len()`.
     - Otherwise: `let s = String::from_utf8_lossy(trimmed); unicode_width::UnicodeWidthStr::width(s.as_ref())`.
     - Update `LogSource::get_measured_length(&self, idx: usize)` default impl (`buffer/mod.rs:156-168`) to delegate to `self.get_raw_line(idx).map(measure_display_width).unwrap_or(0)` (also fixing the invalid-UTF-8 raw `.len()` fallback at line 163).
  2. In `FileLogBuffer::from_storage` (`file_buffer.rs:61-101`):
     - Compute `let width = measure_display_width(&bytes[prev_offset..newline_pos]);` (and similarly for the trailing line without `\n`), tracking `max_line_len` and `longest_line_idx` directly by `width`!
  3. In `execute_filtering` (`filter/engine.rs:263-286, 324-365`):
     - Measure `let width = crate::buffer::measure_display_width(raw_line);` for each visible line and track `(local_max_width, local_longest_idx)`.
     - Store `widest_visible_source_idx: longest_idx` on `FilterResult`.
  4. On `FilterResult` (`filter/engine.rs:17-30`), add `#[doc(hidden)] pub widest_visible_source_idx: Option<usize>` (defaults to `None` via `Default`).
  5. In `append_streaming_batch_with_matcher` (`filter/engine.rs:431-524`):
     - Before/during the `trimmed_count > 0` block:
       - Track `let mut widest_was_dropped = false;`
       - If `result.visible_indices.is_empty()` after draining `0..drop_count`: clear `result.widest_visible_length = 0; result.widest_visible_text.clear(); result.widest_visible_source_idx = None;`.
       - Else if `trimmed_count > 0`:
         - Match `result.widest_visible_source_idx`:
           - `Some(w_idx)` if `w_idx < trimmed_count` -> `widest_was_dropped = true; result.widest_visible_source_idx = None;`
           - `Some(w_idx)` if `w_idx >= trimmed_count` -> `result.widest_visible_source_idx = Some(w_idx - trimmed_count);` (even when `drop_count == 0`!)
           - `None` if `result.widest_visible_length > 0` -> `widest_was_dropped = true;`
       - If `widest_was_dropped`: recompute `widest_visible_length`, `widest_visible_text`, and `widest_visible_source_idx` across the surviving `result.visible_indices`.
     - When evaluating `new_lines_range`, update `result.widest_visible_source_idx = Some(line_idx)` whenever `line_len > result.widest_visible_length`.
- **Edge Cases & Non-Regression Invariants**:
  - Multi-byte UTF-8 / trailing-whitespace lines (e.g., `"한".repeat(10)` = 30 bytes / width 20 vs `"a".repeat(25)` = 25 bytes / width 25; or `"short" + " ".repeat(50)`) always report the visually widest line.
  - `engine_tests.rs:1280-1423`: `check_widest = true` now holds across trim phases (`batch2`, `trim-only`, `trim-excluded`) as well as initial batches!

---

### `[MEDIUM-3]` `stream_log_lines` carries malformed UTF-8 tail bytes across complete `\n`-terminated lines (and `\r\n` straddling 64KB)
- **Files & Lines**:
  - `crates/loganalysis-core/src/adb/logcat.rs:215-297`
- **Root Cause**:
  1. `stream_log_lines` strips `\n`/`\r` first (`245-247`) and unconditionally calls `trailing_incomplete_utf8_len(&raw_buf)` (`249`), carrying invalid trailing UTF-8 bytes from a complete `\n`-terminated line into the next line.
  2. When a line of `MAX_STREAM_LINE_BYTES - 1` (`65,535`) bytes ends with `\r\n`, the `\r` is read at byte 65,536 (without `\n`), stripped at lines 245-247, and the remaining `\n` is read on the next iteration and emitted as a spurious empty line `""`.
- **Exact Fix Contract**:
  1. Track `let mut skip_leading_orphan_lf = false;` outside the loop.
  2. Capture `let pre_len = raw_buf.len();` before `read_until`, and `Ok(bytes_read)`:
     - Check `let hit_newline = raw_buf.ends_with(b"\n");` and `let hit_read_limit = !hit_newline && bytes_read == max_to_read as usize;` **before** stripping `\n`/`\r`.
     - If `skip_leading_orphan_lf`:
       - Clear `skip_leading_orphan_lf = false;`
       - If `bytes_read == 1 && raw_buf == b"\n"` (only the trailing `\n` of a split `\r\n` was read): `continue;` without pushing a blank line!
     - Strip trailing `\n`/`\r`:
       - Note whether a `\r` was stripped when `hit_read_limit`:
         ```rust
         if hit_newline {
             raw_buf.pop(); // '\n'
             if raw_buf.ends_with(b"\r") {
                 raw_buf.pop();
             }
         } else if raw_buf.ends_with(b"\r") && hit_read_limit {
             raw_buf.pop();
             skip_leading_orphan_lf = true;
         }
         ```
         (Also strip any extra trailing `\r` if present via `while raw_buf.ends_with(b"\r") { raw_buf.pop(); }` when `hit_newline`, preserving existing `\r\n` / `\r\r\n` handling.)
     - **Only** when `hit_read_limit`: run `trailing_incomplete_utf8_len(&raw_buf)` and carry incomplete trailing UTF-8 bytes into `carry_buf`. On `\n`-terminated lines (`hit_newline`) or EOF (`!hit_read_limit`), never move bytes into `carry_buf` — emit `String::from_utf8_lossy(&raw_buf)` directly.
- **Edge Cases & Non-Regression Invariants**:
  - `b"bad\xed\x95\nnext\n"` emits `["bad\u{FFFD}", "next"]` (never `"bad"`, `"\u{FFFD}next"`).
  - `[b'A'; 65535] + b"\r\nnext\n"` emits `["AAA...", "next"]` (no blank line between them).
  - Existing 64KB CJK/emoji split tests (`logcat.rs:493-522`, `challenger_core_stress_tests.rs:265-386`, `engine_tests.rs:738-761`) pass unchanged.

---

### `[MEDIUM-4]` Non-ASCII case-insensitive literal matching diverges between `MultiFilterMatcher` and `CompiledMatcher`
- **Files & Lines**:
  - `crates/loganalysis-core/src/filter/matcher.rs:79-94, 149-164, 168-331, 382-473`
- **Root Cause**:
  1. `MultiFilterMatcher::new` builds `ci_automaton` using `AhoCorasick::builder().ascii_case_insensitive(true)` (ASCII-only folding) for all non-regex case-insensitive filters, whereas `CompiledMatcher::new` uses `regex::bytes::RegexBuilder` (full Unicode case-folding). Thus `"café"` vs `"CAFÉ"` matches in `CompiledMatcher` (search/highlighting/status line) but fails in `MultiFilterMatcher` (`execute_filtering` / `append_streaming_batch_with_matcher`).
  2. `MultiFilterMatcher::new` uses `.ok()` on `AhoCorasick::build(...)`, silently dropping rules if automaton construction fails.
  3. `CompiledMatcher::new` fallback (`158-162`) uses `to_ascii_lowercase()`, breaking Unicode case-insensitive matching if DFA compilation exceeds `REGEX_DFA_SIZE_LIMIT`.
- **Exact Fix Contract**:
  1. In `MultiFilterMatcher::new(filters: &[PreparedFilter])` (`matcher.rs:389-439`):
     - Route `!pf.rule.regex && !pf.rule.case_sensitive && !pf.rule.text.is_ascii()` directly to `individual_filters` so they execute `PreparedFilter::matches` (`CompiledMatcher` with Unicode case-folding).
     - Keep ASCII case-insensitive literals (`pf.rule.text.is_ascii()`) in `ci_patterns` (`AhoCorasick::ascii_case_insensitive(true)`).
     - Handle `AhoCorasick::builder()...build(...)` errors explicitly instead of `.ok()`: if `cs_automaton` build returns `Err(_)`, push all `cs_indices` into `individual_filters`; if `ci_automaton` build returns `Err(_)`, push all `ci_indices` into `individual_filters`.
  2. In `CompiledMatcher` (`matcher.rs:79-94, 149-331`):
     - For `!is_regex && !case_sensitive`:
       - When `text.is_ascii()`, configure `builder.unicode(false)` on `regex::bytes::RegexBuilder` so ASCII literal matching in `CompiledMatcher` agrees 100% with `AhoCorasick::ascii_case_insensitive(true)` (even on edge-case codepoints like U+212A Kelvin sign `'K'` / U+017F `'ſ'`).
       - When `!text.is_ascii()`, keep default `unicode(true)` on `regex::bytes::RegexBuilder`.
       - Replace the single `LiteralAsciiInsensitive` fallback with:
         - If `text.is_ascii()` -> `Self::LiteralAsciiInsensitive(...)`
         - If `!text.is_ascii()` -> `Self::LiteralUnicodeInsensitive(text.to_lowercase())` (or `fancy_regex::RegexBuilder` fallback on escaped pattern with `backtrack_limit(FANCY_REGEX_BACKTRACK_LIMIT)`), where `LiteralUnicodeInsensitive` performs Unicode case-folded substring matching in `matches()` and char-boundary-safe range extraction in `find_match_ranges()`.
- **Edge Cases & Non-Regression Invariants**:
  - `evaluate_and_style_line` and `evaluate_and_style_line_with_matcher` produce identical visibility, style, and match counts on both ASCII and non-ASCII (`"café"` vs `"CAFÉ"`, `"über"` vs `"ÜBER"`, `"пример"` vs `"ПРИМЕР"`, Turkish `'İ'`) literals.

---

### `[MEDIUM-5]` Light Theme selected row contrast is ~1.36:1 (`#FFFFFF` text on `#DBEAFE` background)
- **Files & Lines**:
  - `crates/loganalysis-gui/src/theme.rs:5-32, 105-168`
  - `crates/loganalysis-gui/src/components/log_view.rs:855-880`
- **Root Cause**: `log_view.rs:875` forces `final_fg = rgb(0xffffff)` whenever `is_selected && !has_custom_bg`, while Light Theme `selection_bg` is `rgb(0xdbeafe)`, yielding ~1.36:1 contrast. Moreover, when `has_custom_bg` is true, `final_bg` still switches to `row_theme.selection_bg` while keeping `fg_color` (e.g., `White` on `#DBEAFE` in Light Theme, or `Black` on `#233354` in Dark Theme).
- **Exact Fix Contract**:
  1. Add `pub selection_fg: Rgba` and `pub selection_bg_hex: u32` (or helper `rgba_to_hex(Rgba) -> u32`) to `Theme` (`theme.rs:5-32`):
     - `Theme::dark()`: `selection_bg: rgb(0x233354)`, `selection_bg_hex: 0x233354`, `selection_fg: rgb(0xffffff)`.
     - `Theme::light()`: `selection_bg: rgb(0xdbeafe)`, `selection_bg_hex: 0xdbeafe`, `selection_fg: rgb(0x0f172a)` (contrast ratio ~13.8:1 against `0xdbeafe`).
  2. Add a pure, unit-testable function in `theme.rs` (or `log_view.rs`):
     ```rust
     pub fn resolve_selected_row_fg(
         is_selected: bool,
         is_rule_matched: bool,
         fg_hex: Option<u32>,
         theme: Theme,
     ) -> Rgba
     ```
     - If `!is_selected`: return `fg_hex.map(rgb).unwrap_or(theme.text_primary)`.
     - If `is_selected`:
       - If `is_rule_matched`: if `let Some(custom_fg) = fg_hex` and `loganalysis_core::contrast_ratio(theme.selection_bg_hex, custom_fg) >= 4.5`, return `rgb(custom_fg)`.
       - Otherwise return `theme.selection_fg`.
  3. Update `log_view.rs:855-880` to use `resolve_selected_row_fg(is_selected, style.is_matched, style.fg_hex, row_theme)` (falling back cleanly to `row_theme.selection_fg` when `filter_res` or `visible_styles.get(idx)` is absent).
- **Edge Cases & Non-Regression Invariants**:
  - Every selected row in both Light and Dark themes (unstyled, default gray, custom fg, custom bg+fg) achieves $\ge 4.5:1$ WCAG AA contrast ratio.

---

### `[MEDIUM-6]` Duplicate GPUI `ElementId` `0` for `"Default"` and `"Black"` foreground swatches (and mixed Korean labels)
- **Files & Lines**:
  - `crates/loganalysis-gui/src/modals/filter_edit_modal.rs:375, 383-403, 451, 459-479`
- **Root Cause**:
  1. `("Default", None)` and `("Black", Some(0x000000))` both map to `ElementId::NamedInteger("fg-palette-color".into(), fg_val.unwrap_or(0) as u64)`, emitting duplicate `ElementId` `0` in the same frame. (`bg-palette-color` uses the same `bg_val.unwrap_or(0)` pattern.)
  2. Lines 375 and 451 display mixed-language labels `"Background Color (배경색): {}"` and `"Text Color (글자색): {}"` whereas the rest of the UI is English.
- **Exact Fix Contract**:
  1. Extract pure constant arrays or helper functions `bg_palette_swatches()` and `fg_palette_swatches()` plus `swatch_element_id(prefix: &'static str, idx: usize) -> ElementId` (`ElementId::NamedInteger(prefix.into(), idx as u64)`).
  2. Iterate both BG and FG swatch arrays with `.into_iter().enumerate()` so every swatch receives a unique index-based `ElementId` (`0..len`).
  3. Change `filter_edit_modal.rs:375` to `format!("Background Color: {}", bg_name)` and `filter_edit_modal.rs:451` to `format!("Text Color: {}", fg_name)`.
- **Edge Cases & Non-Regression Invariants**:
  - Unit tests can assert that all `bg-palette-color` IDs and all `fg-palette-color` IDs are strictly pairwise distinct (`HashSet::len() == swatches.len()`).

---

### `[MEDIUM-7]` `TextInput` placeholder uses hardcoded black in Dark Theme, and cursor moves leave `marked_range` stale
- **Files & Lines**:
  - `crates/loganalysis-gui/src/components/text_input.rs:193-222, 340-363, 466-526, 634-677`
- **Root Cause**:
  1. `prepaint` (`636-640`) hardcodes placeholder color `hsla(0., 0., 0., 0.35)`, making placeholders near-invisible on Dark Theme surfaces (`#13141c`, `#1c1e2a`).
  2. `move_to`, `select_to`, `select_word_at`, and `on_mouse_down` (`click_count >= 3`) never clear `self.marked_range`. Subsequent typing via `replace_text_in_range(None, ...)` (`466-470`) prioritizes `self.marked_range.clone()` over `self.selected_range`, overwriting the old IME composition span instead of inserting at the new cursor/selection.
  3. If `content.is_empty()` while `marked_range.is_some()`, lines `651-676` would slice `input.placeholder` using `marked_range`.
- **Exact Fix Contract**:
  1. Add helper `pub fn placeholder_color(foreground: Hsla) -> Hsla { foreground.opacity(0.45) }` and use `(input.placeholder.clone(), placeholder_color(style.color))` in `TextElement::prepaint` (`text_input.rs:637`).
  2. Clear `self.marked_range = None;` in:
     - `move_to(&mut self, offset: usize, cx: &mut Context<Self>)` (and extract core state mutation `pub(crate) fn move_cursor_state(&mut self, offset: usize)` so unit tests can exercise `TextInput` state directly without `Context<Self>`)
     - `select_to(&mut self, offset: usize, cx: &mut Context<Self>)` (via `pub(crate) fn select_to_state(&mut self, offset: usize)`)
     - `select_word_at(&mut self, offset: usize, cx: &mut Context<Self>)` (via `pub(crate) fn select_word_state(&mut self, offset: usize)`)
     - `on_mouse_down` `click_count >= 3` branch (`self.marked_range = None;`)
  3. In `TextElement::prepaint` (`651-676`), only apply `marked_range` underline runs when `!content.is_empty()`, clamping `marked_range` via `clamp_range_to_char_boundaries(&content, marked_range.clone())`.
  4. Also extract `pub(crate) fn replace_text_state(&mut self, range_utf16: Option<Range<usize>>, new_text: &str)` and `pub(crate) fn replace_and_mark_state(...)` called by `EntityInputHandler` and tested directly in `text_input::tests`.
- **Edge Cases & Non-Regression Invariants**:
  - `[INV-08-A]` and `[INV-08-B]` preserved: active Hangul/CJK preedit still blocks `Submit` (`Enter` commits preedit without submitting modal) and `Cancel` (`Escape` cancels preedit via `cancel_ime_preedit()`), while clicking or moving the cursor via mouse/arrows/Home/End/SelectAll clears stale `marked_range`.

---

### `[MEDIUM-8]` `refresh_status_line` recompiles `CompiledMatcher` on UI thread and uses inverted rule precedence
- **Files & Lines**:
  - `crates/loganalysis-gui/src/workspace/mod.rs:784-837`
- **Root Cause**:
  1. `refresh_status_line` calls `CompiledMatcher::new(...)` inside a loop on the UI thread on every row selection.
  2. It inspects only `self.filter_tabs.get(self.active_tab_index)` (ignoring other enabled tabs and ignoring `!tab.enabled`).
  3. It breaks on the **first** match instead of using **last-match** precedence (`resolve_style_from_matches` in `engine.rs:69-97`).
  4. It only checks `!rule.active`, allowing empty `rule.text` to match every line.
  5. Stale doc comment at line 784 (`/// Adds a new filter rule to the active tab`).
- **Exact Fix Contract**:
  1. Remove the stale doc comment at `workspace/mod.rs:784`.
  2. Extract a pure, unit-testable helper `pub(crate) fn resolve_status_line_for_selection(...) -> String` and in `refresh_status_line(&mut self, cx: &mut Context<Self>)`:
     - Zero `CompiledMatcher::new` per-rule loop on selection!
     - Obtain precompiled filters via `let compiled = self.ensure_cached_stream_filters();` (remember `reapply_filters` calls `rebuild_cached_stream_filters()` on every filter mutation, even before a log is loaded).
     - Determine matched flat indices for `(selected_row, source_idx)`:
       - Fast path $O(1)$: if `self.filter_result` has `visible_indices.get(sel_row) == Some(&source_idx)`, read `filter_res.visible_filter_matches.get(sel_row)`.
       - Fallback path (no `filter_result` or mismatched row): fetch `raw_line = src.get_raw_line(source_idx)` and evaluate `compiled.matcher().find_matches(raw_line, compiled.prepared(), &mut matched_indices)` (which skips inactive rules, empty `rule.text`, and disabled tabs using the cached automaton!). Note: `find_matches` appends `prepared` indices; map via `compiled.prepared()[p_idx].flat_index` and sort/take `.last()`.
     - Take the **last** matching active non-empty rule (`flat_matches.last().and_then(|&idx| compiled.rules().get(idx))`), look up `count = self.filter_result.as_ref().map(|r| r.count_for_rule(&rule.id)).unwrap_or(0)`, and format `"Line {} | Matched Rule: '{}' ({}) | {} matches in tab"`.
     - If no rule matches, format `"Line {} | No filter rule match"`.
- **Edge Cases & Non-Regression Invariants**:
  - Empty `rule.text`, inactive rules, and disabled tabs are never reported as matched.
  - Overlapping rules report the last matching rule, matching `resolve_style_from_matches` and `test_rule_reorder_precedence_inversion_and_anchor_stability`.

---

### `[MEDIUM-9]` `EpsTimelineData::compute` drops data on boundary clock skew, jumps empty buckets to row 0, and can invert `end_micros < start_micros`
- **Files & Lines**:
  - `crates/loganalysis-core/src/filter/timeline.rs:6-25, 42-218`
  - `crates/loganalysis-gui/src/components/timeline_sparkline.rs:42-80`
- **Root Cause**:
  1. `min_micros` and `max_micros` only inspect the first/last timestamped line, so boundary clock skew causes `offset < 0 || offset > total_duration_us` and silently drops lines.
  2. `min_micros - YEAR_ROLLOVER_THRESHOLD_MICROS` overflows/panics in debug builds when `min_micros` is near `i64::MIN`, and `max_micros - min_micros` / `end_micros - start_micros` can overflow `i64`.
  3. Ceiling division `(total_duration_us + actual_buckets - 1) / actual_buckets` makes `start = min_micros + i * bucket_span_us` exceed `max_micros` for trailing buckets while `end = max_micros`, producing `end_micros < start_micros`.
  4. `EpsTimelineBucket::start_visible_row: usize` defaults to `0`, so clicking empty bars in `timeline_sparkline.rs` jumps to row `0`.
- **Exact Fix Contract**:
  1. Change `EpsTimelineBucket::start_visible_row` (`timeline.rs:14`) from `usize` to `Option<usize>` (defaults to `None` via `#[derive(Default)]`).
  2. Update `EpsTimelineBucket::eps(&self)` (`timeline.rs:21-24`):
     `let duration_us = self.end_micros.abs_diff(self.start_micros).max(1);`
  3. In `EpsTimelineData::compute`:
     - **Pass 1**: Scan all `visible_indices` to find `first_ts` and compute true `min_micros` and `max_micros`. Normalize rollover only via overflow-safe check:
       ```rust
       let normalize_rollover = |micros: i64, anchor: i64| -> i64 {
           if anchor >= YEAR_ROLLOVER_THRESHOLD_MICROS
               && micros >= 0
               && micros < anchor - YEAR_ROLLOVER_THRESHOLD_MICROS
           {
               micros.saturating_add(ONE_YEAR_MICROS)
           } else {
               micros
           }
       };
       ```
     - Compute `let total_duration_us: u128 = max_micros.abs_diff(min_micros) as u128;`.
     - If `total_duration_us == 0`: return `Self { min_timestamp: min_micros, max_timestamp: max_micros, has_timestamps: true, total_duration_secs: 0.0, ..Default::default() }`.
     - Partition `[min_micros, max_micros]` with exact `u128`/`i128` linear interpolation across `actual_buckets = bucket_count.clamp(8, 128)`:
       ```rust
       let b_start_off = ((i as u128) * total_duration_us) / (actual_buckets as u128);
       let b_end_off = (((i + 1) as u128) * total_duration_us) / (actual_buckets as u128);
       let start_micros = ((min_micros as i128) + (b_start_off as i128)) as i64;
       let end_micros = ((min_micros as i128) + (b_end_off as i128)) as i64;
       ```
       Guarantees `start_micros <= end_micros` for every bucket `i`, even when `total_duration_us < actual_buckets` or spanning `[i64::MIN, i64::MAX]`!
     - **Pass 2**: Bin every timestamped visible row using `let offset = (micros.abs_diff(min_micros)) as u128;` -> `let b_idx = ((offset * (actual_buckets as u128)) / total_duration_us).min((actual_buckets - 1) as u128) as usize;`. Set `buckets[b_idx].start_visible_row = Some(vis_idx);` when `!first_seen[b_idx]`.
     - Use `(total_duration_us as f64) / 1_000_000.0` for `total_duration_secs`.
  4. In `components/timeline_sparkline.rs:42-80`:
     - Add pure helper `pub fn bucket_navigation_row(bucket: &EpsTimelineBucket) -> Option<usize> { (bucket.total_count > 0).then_some(bucket.start_visible_row).flatten() }`.
     - Only apply `.cursor_pointer()`, hover styling, and `.on_click(...)` when `bucket_navigation_row(bucket)` returns `Some(target_row)`. Empty bars (`total_count == 0` / `start_visible_row == None`) must be non-interactive and never jump to row `0`.
- **Edge Cases & Non-Regression Invariants**:
  - `i64::MIN` and `[i64::MIN, i64::MAX]` inputs never panic or overflow.
  - Boundary clock skew (e.g., `[t+5s, t+1s, t+10s, t+6s]`) includes 100% of events in `buckets`.

---

### `[MEDIUM-10]` `MergedLogBuffer` ensemble rollover misclassifies ISO-8601 + time-only logs, sorts header lines to the end, and omits `source_names.len()` check
- **Files & Lines**:
  - `crates/loganalysis-core/src/buffer/timestamp.rs:60-65, 132-235`
  - `crates/loganalysis-core/src/buffer/merged_buffer.rs:70-202`
- **Root Cause**:
  1. `extract_timestamp` erases the parsed format (`MonthDayTime`, `Iso8601`, `TimeOnly`), so `MergedLogBuffer::from_sources` treats epoch-scale `Iso8601` (`>= 300 days`) + `TimeOnly` (`<= 60 days`) as a Dec-to-Jan year rollover and adds `ONE_YEAR_MICROS` to time-only sources.
  2. Leading header lines (`timestamp_micros == None`) before the first timestamp sort via `(Some(_), None) => Less`, pushing headers to the bottom of the merged log.
  3. `from_sources` validates `sources.len() > 255` without checking `sources.len() == source_names.len()`.
- **Exact Fix Contract**:
  1. In `crates/loganalysis-core/src/buffer/timestamp.rs` (re-export `ParsedTimestamp` and `TimestampKind` from `buffer/mod.rs` and `lib.rs`):
     - Keep `LogTimestamp { pub total_micros: i64 }` unchanged for backward compatibility.
     - Define:
       ```rust
       #[derive(Clone, Copy, Debug, PartialEq, Eq)]
       pub enum TimestampKind {
           MonthDayTime,
           Iso8601,
           TimeOnly,
       }

       #[derive(Clone, Copy, Debug, PartialEq, Eq)]
       pub struct ParsedTimestamp {
           pub timestamp: LogTimestamp,
           pub kind: TimestampKind,
       }

       pub fn extract_timestamp_with_kind(line: &[u8]) -> Option<ParsedTimestamp>
       ```
       Implement Case 1 (`MonthDayTime`), Case 2 (`Iso8601`), Case 3 (`TimeOnly`) inside `extract_timestamp_with_kind`, and implement `extract_timestamp(line)` as `extract_timestamp_with_kind(line).map(|p| p.timestamp)`.
  2. In `MergedLogBuffer::from_sources(sources, source_names)` (`merged_buffer.rs:70-202`):
     - Validate `if sources.len() != source_names.len()` -> `return Err(io::Error::new(io::ErrorKind::InvalidInput, ...))`. (Also keep `if sources.len() > 255` check.)
     - **Pass 1**: Scan each source using `extract_timestamp_with_kind`:
       - Record `first_month_day_ts: Option<i64>` and `last_month_day_ts: Option<i64>` **only** from `TimestampKind::MonthDayTime` timestamps!
       - Also record `first_any_ts: Option<(usize, i64, TimestampKind)>` (line index + raw timestamp + kind of the first timestamped line in the source, so leading unstamped headers can anchor before it!).
     - Compute `has_late_year` / `has_early_year` / `ensemble_rollover` / `base_offsets` **exclusively** from `first_month_day_ts` and `last_month_day_ts`!
     - **Pass 2**: Build `Vec<(MergedLineRef, Option<i64> /* sort_ts */)>`:
       - Compute each source's `first_adjusted_ts: Option<i64>` (if `first_any_ts` has `kind == MonthDayTime`, `raw + base_offsets[s_id]`, else `raw`).
       - For each line `l_idx`:
         - If `extract_timestamp_with_kind` returns `Some(p)`:
           - If `p.kind == TimestampKind::MonthDayTime`: apply `base_offsets[s_id]` + intra-file rollover check against `prev_month_day_raw_ts`; update `prev_month_day_raw_ts`.
           - Else (`Iso8601` / `TimeOnly`): `adjusted = p.timestamp.total_micros` (never add `ONE_YEAR_MICROS`).
           - `last_ts = Some(adjusted);`
           - `MergedLineRef { ..., timestamp_micros: Some(adjusted) }`, `sort_ts = Some(adjusted)`.
         - Else (unstamped line):
           - If `last_ts.is_some()` (trailing stack trace): `MergedLineRef { ..., timestamp_micros: last_ts }`, `sort_ts = last_ts`.
           - Else (leading header before first timestamp, or completely unstamped file): `MergedLineRef { ..., timestamp_micros: None }`, `sort_ts = first_adjusted_ts` (so leading headers of a timestamped file stay directly in front of that file's first event, while a 100%-unstamped file has `sort_ts = None` and sorts to the very beginning instead of the bottom!).
     - **Sort**: Sort by key `(sort_ts, r.source_id, r.source_line_idx)` (Rust's `Ord` for `Option<i64>` places `None < Some(_)`, giving a strict total order!). Strip `sort_ts` into `merged_index: Vec<MergedLineRef>`.
- **Edge Cases & Non-Regression Invariants**:
  - `Iso8601` (`2026-...`) + `TimeOnly` (`00:00:01.000`) merge never adds `ONE_YEAR_MICROS`.
  - `[dec31, jan01]` and `[jan01, dec31]` `MonthDayTime` merges (`[INV-14]`, `year_rollover_tests.rs`, `challenger_core_stress_tests.rs`) continue passing identically.
  - Leading headers like `--------- beginning of main` remain directly before their own source's first timestamped line (`timestamp_micros` stays `None`), and completely unstamped files sort before timestamped events.

---

## 3. Low Severity Fix Contracts

### `[LOW-1]` Search query trimming mismatch
- **Files & Lines**: `crates/loganalysis-gui/src/workspace/mod.rs:215, 226`, `crates/loganalysis-gui/src/workspace/search.rs:64-73`, `crates/loganalysis-gui/src/controllers/search.rs:59-88`
- **Contract**:
  - In `workspace/search.rs:64-67`, do **not** `.trim()` non-whitespace search queries (e.g., searching `" E "` must preserve leading/trailing spaces to match highlighting in `SearchController::update_matcher_when_visible`):
    ```rust
    let raw_input = self.find_input.read(cx).text().to_string();
    if raw_input.trim().is_empty() {
        self.search_ctrl.query.clear();
        self.search_ctrl.match_status = None;
    } else {
        self.search_ctrl.query = raw_input;
    }
    ```
  - Note: Preserves `MAJ-02` contract (`gui_integration_tests.rs:1190-1208`) where whitespace-only strings clear `cached_matcher` in `update_matcher` and `update_matcher_when_visible`.

---

### `[LOW-2]` `compute_delta_micros` & `format_delta_micros` edge cases
- **Files & Lines**: `crates/loganalysis-core/src/buffer/timestamp.rs:91-98, 101-129`, `crates/loganalysis-gui/src/workspace/selection.rs:130-170`
- **Contract**:
  1. Keep `pub fn compute_delta_micros(t_prev: LogTimestamp, t_curr: LogTimestamp) -> i64` backward-compatible for existing `LogTimestamp` callers/tests, and add `pub fn compute_delta_micros_typed(t_prev: ParsedTimestamp, t_curr: ParsedTimestamp) -> i64` which applies `+ ONE_YEAR_MICROS` **only** when `t_prev.kind == TimestampKind::MonthDayTime && t_curr.kind == TimestampKind::MonthDayTime && diff < -300 * 86_400 * 1_000_000`.
  2. Update `LogAnalysisWorkspace::get_selected_delta_time` (`workspace/selection.rs:130-170`) to call `extract_timestamp_with_kind` and `compute_delta_micros_typed` for non-`Merged` sources so explicit-year `Iso8601` backward jumps never trigger fake 366-day rollovers.
  3. Update `format_delta_micros(micros: i64)` (`timestamp.rs:101-129`) promotion thresholds to account for rounding:
     - `abs_us < 1_000` -> `{sign}{abs_us}µs`
     - `abs_us < 999_950` -> `{sign}{:.1}ms` (`999_949` -> `+999.9ms`; `999_950..=999_999` promotes to `+1.00s` instead of `+1000.0ms`)
     - `abs_us < 59_995_000` -> `{sign}{:.2}s` (`59_994_999` -> `+59.99s`; `59_995_000..=59_999_999` promotes to `+1m 0.0s` instead of `+60.00s`)
     - Compute `total_tenths_sec = (abs_us as f64 / 100_000.0).round() as u64;` (for `abs_us < 3_600_000_000`): if `total_tenths_sec < 36_000`, `mins = total_tenths_sec / 600`, `rem_tenths = total_tenths_sec % 600` -> `format!("{sign}{mins}m {:.1}s", rem_tenths as f64 / 10.0)` (never `0m 60.0s` or `59m 60.0s`); else enter the hours branch using `total_sec = total_tenths_sec / 10` (`1h 0m 0s`). For `abs_us >= 3_600_000_000`, keep `u64` integer division (`total_sec = abs_us / 1_000_000`), preserving `i64::MIN` overflow safety.

---

### `[LOW-3]` `BookmarkManager::next_bookmark` debug overflow on `usize::MAX`
- **Files & Lines**: `crates/loganalysis-core/src/export/bookmark.rs:87-98`
- **Contract**:
  - Replace `self.index_set.range((current_source_idx + 1)..)` with `self.index_set.range((std::ops::Bound::Excluded(current_source_idx), std::ops::Bound::Unbounded))`.
  - Add unit test asserting `mgr.next_bookmark(usize::MAX)` wraps cleanly to the first bookmark without panicking in debug mode.

---

### `[LOW-4]` HTML report `RuleMatch` filter pill mismatch and unused `HtmlReportOptions::total_lines`
- **Files & Lines**: `crates/loganalysis-core/src/export/html_report.rs:6-48, 257-269, 306-358`
- **Contract**:
  1. Track `match_count` separately from `info_count`:
     - `LineSeverity::Error => error_count += 1`
     - `LineSeverity::Warning => warn_count += 1`
     - `LineSeverity::RuleMatch => match_count += 1`
     - `LineSeverity::Normal => info_count += 1`
  2. Add a summary card for `"Rule Matches"` (`{match_count}`) in `.summary-grid` and add `<button class="pill-btn" onclick="setSeverityFilter('match', this)">MATCHES</button>` inside `.pill-group` (`html_report.rs:331-336`) so `data-severity="match"` rows are filterable via UI pills while `INFO` filters `Normal` rows.
  3. Render `options.total_lines` inside the `"Log Source File"` summary card (`html_report.rs:307-310`, e.g. `<div style="font-size: 12px; color: var(--text-muted); margin-top: 4px;">{} total lines</div>`).
  4. **Invariant**: Do **not** modify `.footer-info` format `"Generated by <strong>LogAnalysis</strong> (GPU-Accelerated Rust + GPUI) • {now}"`, preserving all assertions in `html_report.rs:470-529` and `gui_integration_tests.rs:246-263`.

---

### `[LOW-5]` Command Palette label & shortcut inaccuracies
- **Files & Lines**:
  - `crates/loganalysis-gui/src/actions.rs:3-106`
  - `crates/loganalysis-gui/src/lib.rs:69-122, 325-331`
  - `crates/loganalysis-gui/src/modals/command_palette.rs:78, 205, 294-298, 309`
  - `crates/loganalysis-gui/src/workspace/mod.rs:852-858`
  - `crates/loganalysis-gui/src/workspace/command_palette.rs:118-122`
  - `crates/loganalysis-gui/tests/gui_integration_tests.rs:982-988`
- **Contract**:
  1. `modals/command_palette.rs:205`: Change `"View: Toggle Filter Dock Position (Bottom / Right)"` -> `"View: Toggle Filter Dock Position (Bottom / Left)"`.
  2. `modals/command_palette.rs:309`: Change `NextBookmark` shortcut `Some("F2")` -> `Some("Alt+F2")` (matching `lib.rs:96`).
  3. Register `ResetZoom` as a real GPUI action in `actions.rs`:
     - Add `ResetZoom` to `actions!(loganalysis, [...])` and `all_action_names()`; update `REGISTERED_ACTION_COUNT` from `45` to `46`.
     - Add `pub fn reset_zoom(&mut self, cx: &mut Context<Self>)` in `workspace/mod.rs` (setting `self.font_size = px(12.0)`, updating `status_text`, calling `cx.notify()`).
     - Wire `.on_action(cx.listener(|this, _: &ResetZoom, _window, cx| { this.reset_zoom(cx); }))` in `lib.rs` (and bind `("ctrl-0", ResetZoom)`, `("cmd-0", ResetZoom)` in `define_default_keybindings!`, updating `command_palette.rs:297` shortcut to `Some("Ctrl+0")` and `command_palette.rs:78` to `gpui::Action::name(&ResetZoom)`, and calling `self.reset_zoom(cx)` in `workspace/command_palette.rs:118`).
     - Update `gui_integration_tests.rs:982-988` (`REGISTERED_ACTION_COUNT` assertion from `45` to `46`, plus check `Action::name(&ResetZoom)` and `Action::name(&CloseFind)`).

---

### `[LOW-6]` Unwired `CloseFind` action & `SaveFilters`/`SaveFiltersAs` identical behavior with dead `FilterTabData::{modified, file_path}`
- **Files & Lines**:
  - `crates/loganalysis-gui/src/actions.rs:11, 17-18, 65, 71-72`
  - `crates/loganalysis-gui/src/lib.rs:283-288`
  - `crates/loganalysis-gui/src/workspace/mod.rs:47-121`
  - `crates/loganalysis-gui/src/workspace/search.rs:157-168`
  - `crates/loganalysis-gui/src/workspace/filters.rs:121-358, 360-498`
  - `crates/loganalysis-gui/src/workspace/command_palette.rs:84-86`
  - `crates/loganalysis-core/src/filter/model.rs:76-87`
- **Contract**:
  1. **`CloseFind`**:
     - Add `pub fn close_search_bar(&mut self, cx: &mut Context<Self>)` in `workspace/search.rs` (sets `self.search_ctrl.is_visible = false`, clears `InputFocusTarget::SearchBar`, calls `self.update_cached_search_matcher()`, calls `cx.notify()`).
     - Wire `.on_action(cx.listener(|this, _: &CloseFind, _window, cx| { this.close_search_bar(cx); }))` on `workspace-container` in `lib.rs` (and update `find_bar.rs:208` close button to call `this.close_search_bar(cx)`). (Do not bind global `escape` to `CloseFind`, preserving `handle_escape` z-order priority.)
  2. **`SaveFilters` vs `SaveFiltersAs` & `FilterTabData::{modified, file_path}`**:
     - Add `pub(crate) active_filter_profile_path: Option<std::path::PathBuf>` on `LogAnalysisWorkspace` (initialized `None`).
     - When `load_filters_from_path` or `import_filters_json` succeeds from a file path, set `active_filter_profile_path = Some(path_buf.clone())` and set `tab.file_path = Some(path_str); tab.modified = false;` across `filter_tabs`. (In `import_filters_json` without a path, clear `active_filter_profile_path = None` and set `tab.modified = false`.)
     - Add helper `pub(crate) fn mark_filter_tabs_modified(&mut self)` called on filter mutations (`add_filter_rule`, `toggle_rule_active`, `delete_rule`, `move_rule_up`, `move_rule_down`, `duplicate_rule`, `save_modal_filter`, `add_filter_tab`, `delete_current_tab`, `rename_current_tab`, `toggle_tab_enabled`) to mark `tab.modified = true`.
     - Add `pub fn save_filters(&mut self, parent: Option<&Window>, cx: &mut Context<Self>)`:
       - If `let Some(path) = self.active_filter_profile_path.clone().or_else(|| self.filter_tabs.get(self.active_tab_index).and_then(|t| t.file_path.as_ref().map(PathBuf::from)))`: write atomically via `bg_executor` (`build_unique_temp_path` + `atomic_replace_file`) directly to `path` without opening a dialog, and on `Ok(_)` clear `tab.modified = false`.
       - Otherwise fall back to `self.prompt_save_filters(parent, cx)`.
     - In `prompt_save_filters` (`SaveFiltersAs`): always prompt dialog, and on `Ok(_)` update `active_filter_profile_path = Some(path)`, update `tab.file_path` on all tabs, and clear `tab.modified = false`.
     - Wire `SaveFilters` action (`lib.rs:283`) and `PaletteCommandId::SaveFilters` (`workspace/command_palette.rs:84`) to `save_filters(...)`, while `SaveFiltersAs` continues calling `prompt_save_filters(...)`.

---

### `[LOW-7]` Synchronous UI/startup I/O (`facets_bar.rs:191`, `workspace/recent_files.rs:59, 86`)
- **Files & Lines**:
  - `crates/loganalysis-gui/src/components/facets_bar.rs:190-193`
  - `crates/loganalysis-gui/src/components/log_view.rs:1992`
  - `crates/loganalysis-gui/src/workspace/dock.rs:126-165`
  - `crates/loganalysis-gui/src/workspace/recent_files.rs:17-60, 82-91`
  - `crates/loganalysis-gui/src/workspace/mod.rs:14-15, 330`
- **Contract**:
  1. In `components/facets_bar.rs:190-193`: replace `this.refresh_facet_tags(); cx.notify();` with `this.refresh_facet_tags_async(cx);`. Remove unused `#[allow(dead_code)] pub fn refresh_facet_tags(&mut self)` (retaining `pub fn compute_facet_tags`).
  2. In `workspace/mod.rs:330` & `workspace/recent_files.rs`:
     - Add `pub(crate) recent_files_gen: u64` on `LogAnalysisWorkspace`.
     - Initialize `recent_files: Vec::new()` synchronously in `LogAnalysisWorkspace::new(cx)`, then spawn background `load_recent_files_async(cx)` on `cx.background_executor()` (guarded by `recent_files_gen`: only applies loaded paths if `recent_files_gen` has not been incremented by `add_recent_file_async` or `clear_recent_files_async`).
     - Add `pub fn clear_recent_files_async(&mut self, cx: &mut Context<Self>)` that increments `recent_files_gen`, clears `self.recent_files`, calls `cx.notify()`, and offloads `save_recent_files(&[])` to `cx.background_executor()`. Update `log_view.rs:1992` to call `this.clear_recent_files_async(cx);`.
     - Extract pure helper `pub(crate) fn push_recent_path(recent_files: &mut Vec<PathBuf>, path: PathBuf)` (max 5, LRU front-insert, dedup) for direct unit testing.

---

### `[LOW-8]` Minor validation and edge cases (`8a`–`8f`)

1. **`[LOW-8a]` `filter/matcher.rs:18-23`: `has_nested_repetition` rejects bounded small lookarounds like `(?=(?:\d{1,3}\.){3}\d{1,3})`**
   - **Contract**: Refine `has_nested_repetition` (`matcher.rs:16-45`) so a small fixed-count outer repetition (`*lo == *hi && *hi <= 16`) containing only small bounded inner repetitions (`*hi <= 16`, `*hi != usize::MAX`, and product of bounded multipliers $\le 64$, with zero variable `lo != hi` nesting and zero enclosing `in_repeat`) is permitted, while **all** variable-count nested repetitions (`*lo != *hi`, unbounded `*hi == usize::MAX`, `(a{2,3}){2,3}`, `((?:\d{1,3}){3})+`, `(?:a+){3}`) continue returning `true` (preserving every existing ReDoS test in `matcher.rs`, `challenger_m1_adversarial_suite.rs:373-401`, and `matcher_adversarial_tests.rs:422-467`).
2. **`[LOW-8b]` `filter/heatmap.rs:193`: Rule with `bg_color = None` and `text_color = Gray` (`0x808080`) classifies as `Normal`**
   - **Contract**:
     - Add `pub is_matched: bool` to `ResolvedStyle` (`filter/engine.rs:10-14`, default `false`).
     - Unmatched branches (`engine.rs:60, 115, 179`) emit `ResolvedStyle { bg_hex: None, fg_hex: Some(0x808080), is_matched: false }` (preserving `engine_tests.rs:19-20` asserting `fg_hex == Some(0x808080)`).
     - Matched branch (`engine.rs:99`) emits `ResolvedStyle { bg_hex, fg_hex, is_matched: true }`.
     - Update `detect_line_severity` (`filter/heatmap.rs:193`) to:
       `if style.is_matched || style.bg_hex.is_some() || (style.fg_hex.is_some() && style.fg_hex != Some(0x808080)) { return LineSeverity::RuleMatch; }`
     - Update the two direct struct literals in `heatmap.rs:316, 326` to use `..ResolvedStyle::default()`.
3. **`[LOW-8c]` `constants.rs:88-89`: `parse_color_spec` accepts `"+12345"` via `u32::from_str_radix`**
   - **Contract**: Guard hex parsing in `constants.rs:87-92` with:
     `if hex_part.len() == 6 && hex_part.bytes().all(|b| b.is_ascii_hexdigit()) { u32::from_str_radix(hex_part, 16).ok() } else { None }`
     Rejects `"+12345"`, `"#+12345"`, `"-12345"`, and non-hex strings.
4. **`[LOW-8d]` `buffer/column.rs:150-157`: `ThreadtimeColumnParser` can emit `tag: Some("")` on empty tags**
   - **Contract**: In `ThreadtimeColumnParser::parse` (`column.rs:149-158`), filter empty trimmed tags via `(!raw_tag.is_empty()).then_some(raw_tag)` in both the colon-delimited and space-delimited branches so empty tags produce `None` instead of `Some("")`.
5. **`[LOW-8e]` `workspace/filters.rs:126-130`: `import_filters_json` empty tab vector / out-of-bounds `active_tab_index`**
   - **Contract**: Extract pure helper `pub(crate) fn apply_imported_filter_tabs(tabs: Vec<FilterTabData>) -> Result<Vec<FilterTabData>, String>` called by `import_filters_json`: reject `tabs.is_empty()` with `Err("Filter profile contains no tabs".to_string())`, and clamp `self.active_tab_index = 0` strictly `< self.filter_tabs.len()`. (Note: `parse_filters_json("[]")` itself already wraps `[]` into a 1-element `"Imported Filters"` tab; if `import_filters_json` is handed an empty `Vec<FilterTabData>` from any source, it can never leave `filter_tabs` empty with `active_tab_index == 0` out of bounds.)
6. **`[LOW-8f]` `workspace/adb.rs:344-346`: Repeated 25k-line facet scans on tagless streams**
   - **Contract**: Add `pub(crate) has_auto_sampled_facets: bool` on `LogAnalysisWorkspace` (initialized `false`, reset to `false` in `reset_derived_log_state()`). In `workspace/adb.rs:344-346`, gate auto-sampling with:
     `if !workspace.has_auto_sampled_facets && workspace.facet_tags.is_empty() && new_total_len >= 100 { workspace.has_auto_sampled_facets = true; workspace.refresh_facet_tags_async(cx); }`
     Manual `↻ Rescan` still calls `refresh_facet_tags_async(cx)` unconditionally.

---

## 4. Column View Scrollbar Alignment & CLI Handling

### Column View `24px` Scrollbar Height Alignment
- **Files & Lines**:
  - `crates/loganalysis-gui/src/components/log_view.rs:666-755, 1352-1369`
  - `crates/loganalysis-gui/src/workspace/mod.rs:861-925`
- **Root Cause**: `render_column_header` (`24px` tall) sits inside `main_viewport` above `list_el`, while `scrollbar_track` sits in the outer horizontal flex row (`h_full()`), making `scrollbar_track` `24px` taller than `list_el` (`scroll_handle.0.borrow().base_handle.bounds()`). Scrollbar click/drag ratios (`scroll_to_ratio_from_mouse_y`, `scroll_secondary_to_ratio_from_mouse_y`) and heatmap tick/thumb coordinates are thus offset by `24px` in Column View.
- **Exact Fix Contract**:
  1. In `render_log_list` (`log_view.rs:1352-1369`), render `render_column_header(theme)` (with a `15px` right-side corner spacer matching `scrollbar_track`'s `w(px(15.0))`) **above** a horizontal flex container holding ONLY `list_el` (`flex_1().h_full()`) and `scrollbar_track` (`w(px(15.0)).h_full()`):
     ```rust
     let list_and_scrollbar = div()
         .flex_1()
         .w_full()
         .min_h(px(0.0))
         .flex()
         .flex_row()
         .overflow_hidden()
         .child(list_el)
         .child(scrollbar_track);

     div()
         .flex_1()
         .h_full()
         .flex()
         .flex_col()
         .overflow_hidden()
         .when(is_column_view, |s| {
             s.child(
                 div()
                     .w_full()
                     .h(px(24.0))
                     .flex()
                     .flex_row()
                     .child(div().flex_1().h_full().overflow_hidden().child(render_column_header(theme)))
                     .child(
                         div()
                             .w(px(15.0))
                             .h_full()
                             .bg(theme.bg_header)
                             .border_b_1()
                             .border_l_1()
                             .border_color(theme.border_subtle),
                     ),
             )
         })
         .child(list_and_scrollbar)
     ```
  2. Result: `scrollbar_track` bounds strictly equal `list_el` bounds (`track_top` and `track_height`) in both Raw Mode and Column View, across both primary and secondary panes, while column header widths stay pixel-aligned with row columns.

---

### CLI `--help` / `-h` & Multi-File Handling
- **Files & Lines**:
  - `crates/loganalysis-gui/src/cli.rs:1-27`
  - `crates/loganalysis-gui/src/lib.rs:539-607`
- **Root Cause**: `run_app` only checks `handle_cli_version_flag`, then reads `std::env::args().nth(1)` as a single file path, treating `--help`/`-h`/unknown flags as log file paths and dropping additional positional files.
- **Exact Fix Contract**:
  1. In `crates/loganalysis-gui/src/cli.rs`:
     ```rust
     #[derive(Debug, Clone, PartialEq, Eq)]
     pub enum CliAction {
         PrintVersion,
         PrintHelp,
         UnknownFlag(String),
         OpenPaths(Vec<std::path::PathBuf>),
     }

     pub fn parse_cli_args<I>(args: I) -> CliAction
     where
         I: IntoIterator<Item = String>
     ```
     - Handle `--` sentinel (all subsequent args are positional file paths).
     - Before `--`:
       - `"-h" | "--help"` -> `CliAction::PrintHelp`
       - `"--version" | "-V" | "-v"` -> `CliAction::PrintVersion`
       - Any other arg starting with `'-'` -> `CliAction::UnknownFlag(arg)`
       - Positional arg -> pushed to `paths`
     - Keep `pub fn handle_cli_version_flag<I>(args: I) -> bool` working cleanly (`[INV-05]`).
     - Add `pub fn help_text() -> String` documenting usage, `--help`/`-h`, `--version`/`-V`/`-v`, `--`, single-file open, and multi-file chronological merge.
  2. In `crates/loganalysis-gui/src/lib.rs:539-607` (`run_app`):
     - Evaluate `parse_cli_args(std::env::args().skip(1))` **before** `logging::init_crash_handler()` and `ensure_desktop_integration()`:
       - `PrintVersion` -> `println!("LogAnalysis {}", env!("CARGO_PKG_VERSION")); return;`
       - `PrintHelp` -> `println!("{}", cli::help_text()); return;`
       - `UnknownFlag(flag)` -> `eprintln!("Unknown option: {flag}\n\n{}", cli::help_text()); std::process::exit(2);`
       - `OpenPaths(paths)` -> proceed to GUI init.
     - Inside `cx.open_window(...)`:
       - `paths.is_empty()` -> `workspace.refresh_adb_devices(cx);`
       - `paths.len() == 1` -> `workspace.open_file(&paths[0], cx);`
       - `paths.len() >= 2` -> `workspace.open_merged_files(&paths, cx);`

---

## 5. Defective Core Tests Fix Contracts

### 5.1 `crates/loganalysis-core/tests/engine_tests.rs:228-245` (`test_automotive_preset_profile_parses`)
- **Defect**: Conditionally checks `if path.exists()` for a non-existent `examples/filters/automotive_main_log_profile.json`, silently executing zero assertions.
- **Contract**:
  - Replace the conditional file check with an embedded `const AUTOMOTIVE_PROFILE_JSON: &str = r#"[...]"#;` fixture matching the expected 3-tab structure (`"🚨 Issue Triage"` containing `" F "` and `exclude: true` `"ACCGYROCALIBRATION"`, `"🚗 Vehicle & Subsystems"`, `"⏱️ Performance & Latency"`).
  - Unconditionally parse with `parse_filters_json(AUTOMOTIVE_PROFILE_JSON)` and execute all assertions (`tabs.len() == 3`, tab names, rule fields).

### 5.2 `crates/loganalysis-core/tests/challenger_core_stress_tests.rs:607-631` (`test_challenge_4_timeline_i64_min_behavior`)
- **Defect**: Wraps `EpsTimelineData::compute` in `std::panic::catch_unwind` and only prints in `Ok` and `Err` branches with zero assertions.
- **Contract**:
  - Remove `std::panic::catch_unwind` completely.
  - Execute `EpsTimelineData::compute(&extreme_source, &vis_indices, &vis_styles, 32)` directly (any panic fails the test).
  - Assert `timeline.has_timestamps`, `timeline.min_timestamp == i64::MIN`, `timeline.max_timestamp == i64::MIN + 100`, `!timeline.buckets.is_empty()`, `timeline.buckets.iter().all(|b| b.start_micros <= b.end_micros)`, and total binned count `== 2`.
  - Add a second direct assertion case spanning `[i64::MIN + 1, i64::MAX - 1]` verifying overflow-free bucketing and `start_micros <= end_micros`.

---

## 6. Acceptance Criteria & Deterministic Verification Matrix

### 6.1 Global Gates (Required After Final Edit)
| Gate | Exact Command | Required Outcome |
|---|---|---|
| Typecheck | `cargo check --workspace --all-targets` | Exit `0` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | Exit `0`, zero warnings |
| Full Test Suite | `cargo test --workspace` | Exit `0`, `0` failed, `0` ignored |
| Doc Build | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | Exit `0`, zero doc warnings |
| Sleep Ban | Static scan of `git diff` across `tests/` & `#[cfg(test)]` | Zero `std::thread::sleep` in any new or modified tests |

### 6.2 Per-Item Deterministic Verification Matrix

| ID | Target Test Module / Location | Deterministic Test Scenario (Zero `thread::sleep`) |
|---|---|---|
| `[HIGH-1]` | `crates/loganalysis-gui/src/controllers/search.rs` / `workspace/mod.rs` | `SearchController::abort_ongoing_searches` increments `cancel_gen`; `reset_derived_log_state` contract / extracted reset helper invalidates active search token without touching `filter_cancel_gen`. |
| `[HIGH-2]` | `crates/loganalysis-core/src/adb/device.rs` (`#[cfg(test)]`) | Unix `sh -c "exec 1>&-; sleep 30"` with `50ms` timeout returns `Err(AdbError::Timeout(_))` well before `2s`; `sh -c "printf 'ok'; exec 1>&-; exit 0"` returns `Ok(b"ok")`. |
| `[HIGH-3]` | `crates/loganalysis-gui/src/controllers/split_view.rs` & `workspace/` unit tests | Monotonic `advance_primary_source_gen` / `advance_source_gen` / `clear` invalidates earlier generation tokens; stale `gen != current_gen` check discards completion results. |
| `[HIGH-4]` | `crates/loganalysis-gui/src/controllers/adb.rs` (`#[cfg(test)]`) | Setting `is_paused = true` and `paused_line_count = 10` then calling `stop_stream()` resets `is_paused == false`, `is_streaming == false`, `paused_line_count == 0`. |
| `[HIGH-5]` | `crates/loganalysis-gui/src/modals/bookmark_drawer.rs` | Verified code contract: action container stops `MouseDown` propagation and `btn-drawer-note` / `btn-drawer-del` call `cx.stop_propagation()`, plus drawer index clamping helper test after deletion. |
| `[MEDIUM-1]` | `crates/loganalysis-core/src/buffer/correlation.rs` (`#[cfg(test)]`) | 100% unstamped source returns `None`; leading & trailing unstructured blocks return nearest timestamped row; `FileLogBuffer` & `StreamLogBuffer` Dec 31 -> Jan 01 rollover produces monotonic `get_timestamp_micros` for binary search. |
| `[MEDIUM-2]` | `crates/loganalysis-core/src/buffer/file_buffer.rs`, `tests/engine_tests.rs` | `FileLogBuffer` selects CJK/printable line with wider display width over byte-longer UTF-8 or trailing-space line; `execute_filtering` selects widest by display width; `engine_tests.rs:1350-1423` passes with `check_widest = true` across trims. |
| `[MEDIUM-3]` | `crates/loganalysis-core/src/adb/logcat.rs` (`#[cfg(test)]`) | `b"bad\xed\x95\nnext\n"` does not carry `\xed\x95` into `"next"`; `[b'A'; 65535] + b"\r\nnext\n"` emits 2 lines with no spurious blank line; existing 64KB CJK/emoji split tests pass. |
| `[MEDIUM-4]` | `crates/loganalysis-core/src/filter/matcher.rs` & `tests/engine_tests.rs` | `"café"` vs `"CAFÉ"` and `"über"` vs `"ÜBER"` (`case_sensitive: false, regex: false`) match identically in `CompiledMatcher`, `MultiFilterMatcher`, `evaluate_and_style_line`, and `execute_filtering`. |
| `[MEDIUM-5]` | `crates/loganalysis-gui/src/theme.rs` (`#[cfg(test)]`) | `resolve_selected_row_fg` passes WCAG AA ($\ge 4.5:1$) via `contrast_ratio` in both `Theme::light()` and `Theme::dark()` for unstyled, gray, low-contrast custom, and high-contrast custom rows. |
| `[MEDIUM-6]` | `crates/loganalysis-gui/src/modals/filter_edit_modal.rs` (`#[cfg(test)]`) | All `bg-palette-color` and `fg-palette-color` `ElementId`s are unique (`HashSet::len() == len`); labels contain no Korean parentheticals. |
| `[MEDIUM-7]` | `crates/loganalysis-gui/src/components/text_input.rs` (`#[cfg(test)]`) | `placeholder_color(Theme::dark().text_primary.into())` retains light RGB + `0.45` alpha; `move_cursor_state` / `select_to_state` / `select_word_state` clear `marked_range` so subsequent `replace_text_state(None, "a")` inserts at cursor instead of overwriting old preedit. |
| `[MEDIUM-8]` | `crates/loganalysis-gui/src/workspace/mod.rs` (`#[cfg(test)]`) | `resolve_status_line_for_selection` skips empty/inactive rules and disabled tabs, evaluates enabled tabs using last-match precedence without compiling matchers per row. |
| `[MEDIUM-9]` | `crates/loganalysis-core/src/filter/timeline.rs` & `timeline_sparkline.rs` | Boundary clock skew bins 100% of stamped events; `total_duration_us < actual_buckets` keeps `start_micros <= end_micros` for all buckets; empty buckets have `start_visible_row == None` and `bucket_navigation_row == None`; `i64::MIN`..`i64::MAX` safe. |
| `[MEDIUM-10]` | `crates/loganalysis-core/src/buffer/merged_buffer.rs` & `timestamp.rs` | Mixed `Iso8601` + `TimeOnly` merge does not add `ONE_YEAR_MICROS`; leading headers stay before their source's first event (`timestamp_micros == None`); mismatched `sources.len() != source_names.len()` returns `Err(InvalidInput)`. |
| `[LOW-1]` | `crates/loganalysis-gui/src/controllers/search.rs` & `workspace/search.rs` | Non-whitespace queries with spaces (`" E "`) stay untrimmed consistently; whitespace-only clears matcher & match status (`MAJ-02` preserved). |
| `[LOW-2]` | `crates/loganalysis-core/src/buffer/timestamp.rs` (`#[cfg(test)]`) | `compute_delta_micros_typed` does not add `ONE_YEAR_MICROS` to out-of-order `Iso8601` timestamps; `format_delta_micros(999_999) == "+1.00s"` and `format_delta_micros(59_999_999) == "+1m 0.0s"`. |
| `[LOW-3]` | `crates/loganalysis-core/src/export/bookmark.rs` (`#[cfg(test)]`) | `BookmarkManager::next_bookmark(usize::MAX)` wraps cleanly without arithmetic overflow. |
| `[LOW-4]` | `crates/loganalysis-core/src/export/html_report.rs` (`#[cfg(test)]`) | `RuleMatch` bookmarks increment `Rule Matches` count and match `MATCHES` filter pill (`'match'`); `total_lines` appears in summary; footer timestamp test (`YYYY-MM-DD HH:MM:SS UTC`) still passes. |
| `[LOW-5]` | `crates/loganalysis-gui/tests/gui_integration_tests.rs` | `REGISTERED_ACTION_COUNT == 46` with `ResetZoom` and `CloseFind`; Command Palette shows `(Bottom / Left)` and `Alt+F2` for `NextBookmark`; `palette_command_action_name(ResetZoom)` matches `Action::name(&ResetZoom)`. |
| `[LOW-6]` | `crates/loganalysis-gui/src/workspace/filters.rs` & `actions.rs` | `CloseFind` action wired; `FilterTabData::{modified, file_path}` track dirty state and saved path across `SaveFilters` vs `SaveFiltersAs`. |
| `[LOW-7]` | `crates/loganalysis-gui/src/workspace/recent_files.rs` & `facets_bar.rs` | `push_recent_path` enforces 5-item LRU deduplication; startup recent-file loading and `btn-facets-rescan` execute asynchronously off the UI thread. |
| `[LOW-8a]` | `crates/loganalysis-core/src/filter/matcher.rs` (`#[cfg(test)]`) | `(?=(?:\d{1,3}\.){3}\d{1,3})` compiles and matches IPv4, while `(a{2,3}){2,3}` and `((?:\d{1,3}){3})+` remain rejected. |
| `[LOW-8b]` | `crates/loganalysis-core/src/filter/heatmap.rs` (`#[cfg(test)]`) | Rule with `bg_color = "None"` and `text_color = "Gray"` (`0x808080`) classifies as `LineSeverity::RuleMatch`, while unmatched lines remain `LineSeverity::Normal` and keep `fg_hex == Some(0x808080)`. |
| `[LOW-8c]` | `crates/loganalysis-core/src/constants.rs` (`#[cfg(test)]`) | `parse_color_spec("+12345", true)` and `"#+12345"` return `None`. |
| `[LOW-8d]` | `crates/loganalysis-core/src/buffer/column.rs` (`#[cfg(test)]`) | `"11-06 18:00:00.123  1000  1000 I : msg"` and `"11-06 18:00:00.123  1000  1000 I"` yield `parsed.tag == None`. |
| `[LOW-8e]` | `crates/loganalysis-gui/src/workspace/filters.rs` (`#[cfg(test)]`) | `apply_imported_filter_tabs(vec![])` returns `Err`, ensuring `filter_tabs` is never empty and `active_tab_index < filter_tabs.len()`. |
| `[LOW-8f]` | `crates/loganalysis-gui/src/workspace/mod.rs` / `adb.rs` | `has_auto_sampled_facets` prevents repeated 25k-line scans on tagless ADB streams while resetting on `reset_derived_log_state()`. |
| Column View | `crates/loganalysis-gui/src/components/log_view.rs` | `render_column_header` + `15px` right corner spacer render above `[list_el \| scrollbar_track]`, aligning `scrollbar_track` height with `list_el`. |
| CLI | `crates/loganalysis-gui/src/cli.rs` (`#[cfg(test)]`) | `parse_cli_args` handles `--help`/`-h` (`PrintHelp`), `--version`/`-V`/`-v` (`PrintVersion`), `-- -foo.log` (`OpenPaths`), unknown `--bad` (`UnknownFlag`), and 2+ positional paths (`OpenPaths` -> `open_merged_files`). |
| Defective Tests | `engine_tests.rs:228-245`, `challenger_core_stress_tests.rs:607-631` | Both tests unconditionally execute assertions with zero silent skips and zero `catch_unwind` panic swallowing. |

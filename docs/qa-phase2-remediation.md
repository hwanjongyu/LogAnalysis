# Phase 2 Remediation QA Verification Report

**Specification:** [`docs/spec-phase2-remediation.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/spec-phase2-remediation.md)  
**QA Attempt:** `Attempt 1/3`  
**Host Platform:** Linux (`x86_64`)  
**Status:** **PASS** — *Phase 2 QA & Phase 3 `reality-checker` Final Audit Certified*

---

## 1. Executive Summary

Deterministic unit tests and static contract verification were written and executed for all seven Phase 2 remediation items (`MAJ-04`, `MAJ-10`, `MAJ-08`, `MAJ-09`, `MAJ-05` + `INF-03`, `MIN-01`, `MIN-10`). All workspace compilation, Clippy (`-D warnings`), and test targets passed on the first attempt with zero failures, zero skipped/ignored tests, and zero arbitrary sleep-based synchronization.

- **New deterministic tests added:** 23 unit tests (13 in `loganalysis-core`, 10 in `loganalysis-gui`).
- **Workspace test pass count:** 268 passed, 0 failed, 0 ignored across 11 compiled test targets (plus 0 doc-tests in `loganalysis-core`).
- **Inherited-pipe daemon regression check:** `test_scan_adb_devices_mock_daemon_inherited_pipe_hang` completed in `0.03s` (asserted `< 150ms`).

---

## 2. Added Unit Tests & Verification Coverage by Item

### 2.1. `MAJ-04` — Cross-Platform Subprocess Pipe Reading
**Target File:** [`crates/loganalysis-core/src/adb/device.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs#L287-L427)  
**Added Unit Tests (6):**
1. `adb::device::tests::test_run_command_with_timeout_fast_stdout` — verifies `Ok(stdout)` capture for fast command output.
2. `adb::device::tests::test_run_command_with_timeout_multi_chunk_stdout` — verifies reassembly of `16,384` bytes (`> 4096`-byte reader chunk size) on Unix (`> 4096` bytes via 512 lines on Windows).
3. `adb::device::tests::test_run_command_with_timeout_nonzero_exit` — verifies `Err(AdbError::NonZeroExit(Some(42)))` propagation.
4. `adb::device::tests::test_run_command_with_timeout_enforces_timeout_on_hanging_command` — verifies `Err(AdbError::Timeout(50ms))` is returned promptly (`< 2s`) when child sleeps for `5s` (`sleep 5` on `#[cfg(unix)]`; `powershell -NoProfile -Command "Start-Sleep -Seconds 5"` on `#[cfg(windows)]`).
5. `adb::device::tests::test_run_command_with_timeout_zero_duration_immediate` — verifies immediate `Err(AdbError::Timeout(Duration::ZERO))` return (`< 1s`).
6. `adb::device::tests::test_run_command_with_timeout_missing_binary` — verifies `Err(AdbError::ExecutionFailed(_))` on nonexistent executable path.

**Existing Regression Check:**
- `test_scan_adb_devices_mock_daemon_inherited_pipe_hang` ([`crates/loganalysis-core/tests/adb_logcat_adversarial_tests.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/tests/adb_logcat_adversarial_tests.rs#L322-L371)): passed (`0.03s`, well under `< 150ms` limit).

---

### 2.2. `MAJ-10` — Gregorian Leap-Year Arithmetic & Epoch Drift
**Target File:** [`crates/loganalysis-core/src/buffer/timestamp.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/timestamp.rs#L425-L494)  
**Added Unit Tests (4):**
1. `buffer::timestamp::tests::test_is_leap_year_gregorian_rules` — asserts `is_leap_year` returns `true` for `2000`, `2024` and `false` for `1900`, `2100`, `2025`, `2026`.
2. `buffer::timestamp::tests::test_max_days_in_month_and_calendar_validation` — validates `max_days_in_month` across months `0..=13`, verifies Case 2 rejection of `2025-02-29`, `2100-02-29`, `1900-02-29`, `2025-02-30`, `2025-04-31`, `2025-02-00`, Case 2 acceptance of `2024-02-29` and `2000-02-29` (space and `T` separators), Case 1 rejection of `02-30` and `04-31`, and Case 1 continued acceptance of `02-29 12:00:00.000`.
3. `buffer::timestamp::tests::test_iso8601_feb_to_mar_transitions_no_phantom_day` — verifies `2025-02-28 23:59:59.000` → `2025-03-01 00:00:00.000` delta equals `1_000_000` µs (no 24h phantom jump), `2024-02-29 23:59:59.000` → `2024-03-01 00:00:00.000` equals `1_000_000` µs, and `2024-02-28 23:59:59.000` → `2024-02-29 00:00:00.000` equals `1_000_000` µs.
4. `buffer::timestamp::tests::test_days_since_epoch_accuracy` — asserts `days_since_epoch(1970, 1, 1) == 0`, `(1969, 12, 31) == -1`, `(2000, 1, 1) == 10_957`, `(2026, 11, 6) == 20_763`, and exact microsecond epochs from `extract_timestamp`.

**Existing Regression Suites Verified Passing:**
- `buffer::timestamp::tests::test_leap_year_feb29_distinct_from_mar01`
- `challenger_m1_adversarial_suite.rs` (13/13 passed)
- `year_rollover_tests.rs` (5/5 passed)
- `year_rollover_adversarial_stress.rs` (6/6 passed)
- `adversarial_rollover_tests.rs` (8/8 passed)

---

### 2.3. `MAJ-08` — Diagnostic Feedback for Malformed Search Regex
**Target File:** [`crates/loganalysis-gui/src/workspace/search.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/search.rs#L183-L250)  
**Added Unit Tests (4):**
1. `workspace::search::tests::test_format_search_outcome_match_found` — checks `Ok(Some(SearchResult { visible_row: 3, real_idx: 41 }))` maps to `matched_location: Some((3, 41))`, `status_text: "Found match at line 42"`, `match_status: "Line 42"`, and `is_find_status_error == false`.
2. `workspace::search::tests::test_format_search_outcome_not_found` — checks `Ok(None)` maps to `matched_location: None`, `status_text: "Search pattern not found"`, `match_status: "0 matches"`, and `is_find_status_error == true`.
3. `workspace::search::tests::test_format_search_outcome_invalid_regex` — checks `Err(SearchError::InvalidRegex("unclosed group".into()))` maps to `matched_location: None`, `status_text: "Regex syntax error: unclosed group"`, `match_status: "Regex Error"`, and `is_find_status_error(&outcome.match_status) == true`.
4. `workspace::search::tests::test_format_search_outcome_cancelled` — checks `Err(SearchError::Cancelled)` maps to `matched_location: None`, `status_text: "Search error: Search cancelled"`, `match_status: "Error"`, and `is_find_status_error == true`.

---

### 2.4. `MAJ-09` — Filter Modal Regex & ReDoS Validation
**Target File:** [`crates/loganalysis-gui/src/workspace/filters.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/filters.rs#L594-L647)  
**Added Unit Tests (4):**
1. `workspace::filters::tests::test_validate_filter_pattern_rejects_empty_and_whitespace` — verifies `""`, `"   "`, `"\t\n"` are rejected in both literal and regex modes.
2. `workspace::filters::tests::test_validate_filter_pattern_rejects_invalid_regex_syntax` — verifies `"[unclosed"` and `"(abc"` return `Err` containing `"Invalid filter regex:"`.
3. `workspace::filters::tests::test_validate_filter_pattern_rejects_redos_nested_repetition` — verifies `"(?=(a+)+)b"` returns `Err` containing `"Invalid filter regex:"` and `"ReDoS"`.
4. `workspace::filters::tests::test_validate_filter_pattern_accepts_valid_regex_and_literal_brackets` — verifies `r"^E/.*timeout\d+$"` (`is_regex: true`) and `"[ERROR"` / `"foo("` (`is_regex: false`) return `Ok(())`.

---

### 2.5. `MAJ-05` (+ `INF-03`) — Mmap Truncation Safety & Heap Fallback
**Target File:** [`crates/loganalysis-core/src/buffer/file_buffer.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs#L244-L337)  
**Added Unit Tests (3):**
1. `buffer::file_buffer::tests::test_file_log_buffer_open_heap_survives_truncation_to_zero` — verifies `FileLogBuffer::open_heap` yields `is_heap_backed() == true`, `is_truncated_on_disk() == false`, and retains all lines readable after backing file truncation to 0 bytes via `set_len(0)` + `sync_all()`.
2. `buffer::file_buffer::tests::test_file_log_buffer_open_mmap_detects_shrink_and_allows_growth` — verifies `FileLogBuffer::open` on a non-empty file yields `is_heap_backed() == false` and `is_truncated_on_disk() == false`; under `#[cfg(unix)]`, verifies file growth keeps `is_truncated_on_disk() == false` and shrinking below `byte_size()` sets `is_truncated_on_disk() == true`.
3. `buffer::file_buffer::tests::test_file_log_buffer_empty_file_open_and_open_heap` — verifies empty file opened via both `open` and `open_heap` produces `is_heap_backed() == true`, `line_count() == 0`, `byte_size() == 0`, `is_truncated_on_disk() == false`, and `get_raw_line(0) == None`.

*(All tests use `tempfile::NamedTempFile` for isolated temporary file creation and automatic cleanup on drop.)*

---

### 2.6. `MIN-01` — Facet Filter Palette Color Fix
**Target File:** [`crates/loganalysis-gui/src/workspace/dock.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/dock.rs#L224-L257)  
**Added Unit Test (1):**
1. `workspace::dock::tests::test_facet_filter_palette_colors_and_wcag_contrast` — verifies `FACET_FILTER_PALETTE.len() == 5`, contains `"Orchid"`, omits `"Magenta"` (and confirms `parse_color_spec("Magenta", true).is_none()`), resolves all 5 entries via `parse_color_spec(color, true).is_some()`, and confirms `evaluate_contrast` against `"Black"` achieves contrast ratio `>= 4.5` (`ContrastLevel::Pass`) for all 5 entries.

---

### 2.7. `MIN-10` — `TextElement::paint` Prepaint Desync Panic Elimination
**Target File:** [`crates/loganalysis-gui/src/components/text_input.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L731-L771)  
**Static Inspection + Source-Contract Test (1):**
- Direct inspection of `TextElement::paint` (`text_input.rs:731-771`) confirms:
  - `prepaint.line.take()` uses `let Some(line) = prepaint.line.take() else { return; };` (zero `.unwrap()`).
  - `line.paint(origin, window.line_height(), window, cx)` uses `let _ = line.paint(...);` (zero `.unwrap()`).
  - `window.handle_input(...)` executes prior to the early return so IME/input registration remains intact.
- Added source-contract regression test `components::text_input::tests::test_text_element_paint_has_no_unwrap_panics` ([`text_input.rs:1238-1263`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L1238-L1263)) verifying `TextElement::paint` contains zero `.unwrap()` calls and retains the graceful `let Some(line) = prepaint.line.take() else` guard.

---

## 3. Verification Command Results

All verification commands were executed after the final Rust source/test edit.

| Command | Exit Code | Notes |
| :--- | :---: | :--- |
| `cargo check --workspace --all-targets` | `0` | Clean build; upstream future-incompat warning only (`proc-macro-error2 v2.0.1`). |
| `cargo clippy --workspace --all-targets -- -D warnings` | `0` | Zero clippy warnings/errors across all workspace targets. |
| `cargo test --workspace` | `0` | 268 tests passed, 0 failed, 0 ignored across 11 test binaries. |
| `cargo test -p loganalysis-core --test adb_logcat_adversarial_tests test_scan_adb_devices_mock_daemon_inherited_pipe_hang -- --exact` | `0` | 1 passed, 0 failed, 6 filtered out, finished in `0.03s` (`< 150ms`). |

### Per-Target Test Counts (`cargo test --workspace`)

| Crate / Target | Passed | Failed | Ignored | Duration |
| :--- | :---: | :---: | :---: | :---: |
| `loganalysis-core` (`src/lib.rs` unit tests) | 83 | 0 | 0 | `0.05s` |
| `loganalysis-core` (`tests/adb_logcat_adversarial_tests.rs`) | 7 | 0 | 0 | `1.04s` |
| `loganalysis-core` (`tests/adversarial_rollover_tests.rs`) | 8 | 0 | 0 | `0.19s` |
| `loganalysis-core` (`tests/challenger_core_stress_tests.rs`) | 7 | 0 | 0 | `0.01s` |
| `loganalysis-core` (`tests/challenger_m1_adversarial_suite.rs`) | 13 | 0 | 0 | `0.11s` |
| `loganalysis-core` (`tests/engine_tests.rs`) | 39 | 0 | 0 | `0.09s` |
| `loganalysis-core` (`tests/matcher_adversarial_tests.rs`) | 10 | 0 | 0 | `2.68s` |
| `loganalysis-core` (`tests/stream_robustness_stress.rs`) | 18 | 0 | 0 | `1.00s` |
| `loganalysis-core` (`tests/year_rollover_adversarial_stress.rs`) | 6 | 0 | 0 | `0.07s` |
| `loganalysis-core` (`tests/year_rollover_tests.rs`) | 5 | 0 | 0 | `0.00s` |
| `loganalysis-gui` (`src/main.rs` unit tests) | 72 | 0 | 0 | `0.01s` |
| `loganalysis-core` (doc-tests) | 0 | 0 | 0 | `0.00s` |
| **Total** | **268** | **0** | **0** | — |

---

## 4. Platform-Specific Coverage & Caveats

- **Windows (`#[cfg(windows)]`) & macOS paths not compiled/executed on this Linux host:**
  - The Windows branches in `MAJ-04` (`adb::device::tests` `cmd /C` and `powershell -NoProfile -Command "Start-Sleep -Seconds 5"`), `crates/loganalysis-gui/src/workspace/filters.rs` (`MoveFileExW` in `atomic_replace_file`), and macOS `std::process::Command` pipe-reader behavior are written for cross-platform CI but were **not compiled or executed** on this Linux x86_64 host.
  - In `MAJ-05` (`test_file_log_buffer_open_mmap_detects_shrink_and_allows_growth`), the in-place file shrink (`set_len(5)`) and append checks while an `Mmap` is active are scoped to `#[cfg(unix)]`, because Windows blocks shrinking files that have an active mapped view (`ERROR_USER_MAPPED_FILE`). The `open_heap` truncation-to-zero resilience test runs cross-platform.
  - In `crates/loganalysis-core/src/adb/device.rs`, the Linux-only `pre_exec` parent-death signal block (`#[cfg(target_os = "linux")]`, `[INV-12]`) compiled and ran on this Linux host; non-Linux builds skip that block.

---

## 5. Phase 3 Final Audit (`reality-checker`)

**Verdict:** **PASS**  
**Audited Against:** [`docs/spec-phase2-remediation.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/spec-phase2-remediation.md), [`docs/review-report.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/review-report.md), [`docs/INVARIANTS.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/INVARIANTS.md)  
**Audit Host:** Linux (`x86_64`)

### 5.1. Per-Item Specification & Code Audit Checklist

| Item | Scope & Files Audited | Verdict | Verified Invariants & Evidence |
| :--- | :--- | :---: | :--- |
| **`MAJ-04`** | [`device.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs), [`adb/mod.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/mod.rs), [`lib.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/lib.rs) | **PASS** | `run_command_with_timeout` extracted and re-exported; `scan_adb_device_details` delegates to it. `O_NONBLOCK`/`fcntl` completely removed from `crates/`; Linux `[INV-12]` `pre_exec` (`PR_SET_PDEATHSIG` + `getppid() != parent_pid`) preserved. Bounded `crossbeam_channel::bounded::<Vec<u8>>(128)` with `recv_timeout(remaining.min(Duration::from_millis(20)))` avoids busy-wait polling. `cleanup_aborted_command` kills/waits child and drops `rx` before `reader_handle.join()`; normal exit detaches `reader_handle` when `pipe_disconnected == false` after bounded 5ms grace drain. `test_scan_adb_devices_mock_daemon_inherited_pipe_hang` passed in `0.03s` (`< 150ms`); 6 new deterministic unit tests pass. |
| **`MAJ-10`** | [`timestamp.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/timestamp.rs) | **PASS** | `DAYS_BEFORE_MONTH` and `ONE_YEAR_MICROS` remain untouched so Case 1 (`MM-DD HH:MM:SS.mmm`) preserves its 366-day invariant (`02-29` valid, `02-30`/`04-31` rejected). `NON_LEAP_DAYS_BEFORE_MONTH`, `is_leap_year`, `max_days_in_month`, `day_of_year_with_leap`, and `days_since_epoch` (with `div_euclid` pre-1970 safety) added and wired into Case 2 (`YYYY-MM-DD[T ]HH:MM:SS[.mmm]`). Epoch math verified independently (`2026-11-06 00:00:00` UTC = `20_763` days = `1_793_923_200` s). 4 new unit tests plus all existing rollover suites (`challenger_m1_adversarial_suite`, `year_rollover_tests`, `year_rollover_adversarial_stress`, `adversarial_rollover_tests`) pass. |
| **`MAJ-08`** | [`search.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/search.rs) | **PASS** | `SearchOutcomeUpdate`, `format_search_outcome`, and `apply_search_result` unify sync and async search outcome handling. `Err(SearchError::Cancelled)` returns without mutating status; `Err(SearchError::InvalidRegex(err))` surfaces `"Regex syntax error: {err}"` in `status_text` and `"Regex Error"` in `search_match_status` (verified `is_find_status_error == true` in [`find_bar.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/find_bar.rs#L217-L225)). 4 deterministic unit tests pass. |
| **`MAJ-09`** | [`filters.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/filters.rs) | **PASS** | `validate_filter_pattern` rejects empty/whitespace patterns and validates regexes via `loganalysis_core::CompiledMatcher::new` (enforcing syntax, `REGEX_DFA_SIZE_LIMIT`, and AST ReDoS rejection). `save_modal_filter` checks validation before mutating `filter_tabs`, closing `filter_edit_modal`, resetting `input_focus`, or calling `reapply_filters`. Literal mode accepts `[ERROR` and `foo(`. 4 deterministic unit tests pass. |
| **`MAJ-05` + `INF-03`** | [`file_buffer.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs) | **PASS** | `from_storage` shared constructor factored cleanly; `FileLogBuffer::open` rewinds the existing `File` handle with `file.seek(SeekFrom::Start(0))?` on `Mmap::map` fallback (`INF-03`, no redundant `File::open`). `open_heap`, `is_heap_backed`, and `is_truncated_on_disk` implemented and exposed. `LogSource::get_raw_line` remains a pure zero-syscall slice read. 3 deterministic unit tests using `tempfile::NamedTempFile` pass. |
| **`MIN-01`** | [`dock.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/dock.rs) | **PASS** | `FACET_FILTER_PALETTE = ["Cyan", "Green", "Orchid", "Yellow", "Blue"]` replaces `"Magenta"`. Unit test confirms all 5 palette entries resolve via `parse_color_spec(..., true)` and pass WCAG AA contrast (`>= 4.5:1`, `ContrastLevel::Pass`) against `"Black"` foreground (`[INV-11]`). |
| **`MIN-10`** | [`text_input.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs) | **PASS** | `TextElement::paint` executes `window.handle_input(...)` prior to `let Some(line) = prepaint.line.take() else { return; };`, uses `let _ = line.paint(origin, window.line_height(), window, cx);` (4-arg GPUI 0.2.2 API, zero `.unwrap()` calls), and preserves `input.last_layout = Some(line)`. Compiles and passes `clippy -D warnings` and source-contract test `test_text_element_paint_has_no_unwrap_panics`. |

### 5.2. Independent Verification Command Results (Phase 3)

All verification commands were independently re-executed in Phase 3 against the audited workspace state:

| Command | Exit Code | Observed Result |
| :--- | :---: | :--- |
| `cargo check --workspace --all-targets` | `0` | Clean check (upstream future-incompat notice only for `proc-macro-error2 v2.0.1`). |
| `cargo clippy --workspace --all-targets -- -D warnings` | `0` | Zero clippy warnings or errors across all workspace targets. |
| `cargo test --workspace` | `0` | **268 passed, 0 failed, 0 ignored** across 11 compiled test binaries (+ 0 doc-tests in `loganalysis-core`). |
| `cargo test -p loganalysis-core --test adb_logcat_adversarial_tests test_scan_adb_devices_mock_daemon_inherited_pipe_hang -- --exact --nocapture` | `0` | **1 passed, 0 failed** (6 filtered out), finished in **`0.03s`** (`< 150ms`). |

**Independently Observed Per-Target Counts (`cargo test --workspace`):**
- `loganalysis-core` (`src/lib.rs` unit tests): `83 passed; 0 failed; 0 ignored` (`0.05s`)
- `loganalysis-core` (`tests/adb_logcat_adversarial_tests.rs`): `7 passed; 0 failed; 0 ignored` (`1.04s`)
- `loganalysis-core` (`tests/adversarial_rollover_tests.rs`): `8 passed; 0 failed; 0 ignored` (`0.18s`)
- `loganalysis-core` (`tests/challenger_core_stress_tests.rs`): `7 passed; 0 failed; 0 ignored` (`0.01s`)
- `loganalysis-core` (`tests/challenger_m1_adversarial_suite.rs`): `13 passed; 0 failed; 0 ignored` (`0.11s`)
- `loganalysis-core` (`tests/engine_tests.rs`): `39 passed; 0 failed; 0 ignored` (`0.09s`)
- `loganalysis-core` (`tests/matcher_adversarial_tests.rs`): `10 passed; 0 failed; 0 ignored` (`2.66s`)
- `loganalysis-core` (`tests/stream_robustness_stress.rs`): `18 passed; 0 failed; 0 ignored` (`1.02s`)
- `loganalysis-core` (`tests/year_rollover_adversarial_stress.rs`): `6 passed; 0 failed; 0 ignored` (`0.07s`)
- `loganalysis-core` (`tests/year_rollover_tests.rs`): `5 passed; 0 failed; 0 ignored` (`0.00s`)
- `loganalysis-gui` (`src/main.rs` unit tests): `72 passed; 0 failed; 0 ignored` (`0.01s`)
- `Doc-tests loganalysis_core`: `0 passed; 0 failed; 0 ignored` (`0.00s`)

### 5.3. Audit Observations & Platform Caveats

- **Test Rigor:** Newly added tests use deterministic assertions, `tempfile::NamedTempFile`, or bounded command timeouts without arbitrary `thread::sleep` synchronization or trivial mocks.
- **Minor Informational Notes (Non-Blocking):**
  1. In Section 2 above, three EOF line-range links (`device.rs#L287-L427`, `search.rs#L183-L250`, `file_buffer.rs#L244-L337`) extend 1–2 lines past actual EOF (`425`, `249`, `336`), and `text_input.rs#L1238-L1263` omits the closing brace on line `1264`. All named test functions exist and pass.
  2. `test_text_element_paint_has_no_unwrap_panics` uses `include_str!("text_input.rs")` rather than live GPUI `Window` rendering because `TextElement::paint` cannot be invoked headlessly in unit tests; this satisfies the spec's compilation + inspection gate and may be revisited if `MAJ-11` refactors source-contract tests in Phase 3.
- **Platform Caveats (Unverified on Non-Linux Hosts):**
  - `#[cfg(windows)]` branches in `adb::device::tests` (`cmd /C` / `powershell -NoProfile -Command "Start-Sleep -Seconds 5"`), `MoveFileExW` in `workspace/filters.rs`, and macOS `std::process::Command` pipe-reader execution were **not compiled or executed** on this Linux (`x86_64`) host and remain gated on `.github/workflows/ci.yml` (`macos-15`, `windows-latest`).
  - `test_file_log_buffer_open_mmap_detects_shrink_and_allows_growth` restricts its active-mmap shrink/append sub-assertions to `#[cfg(unix)]` (Windows denies shrinking active mapped files with `ERROR_USER_MAPPED_FILE`).
  - Linux-only `[INV-12]` `pre_exec` parent-death signaling in `adb/device.rs` (`#[cfg(target_os = "linux")]`) compiled and ran on Linux; non-Linux builds omit this block.

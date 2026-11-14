# Phase 2 (Near-Term / P1) Remediation Specification

**Source Audit:** [`docs/review-report.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/review-report.md) (Section 5: Phase 2, lines 1034–1050)  
**System Invariants:** [`docs/INVARIANTS.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/INVARIANTS.md) (`[INV-02-A]`, `[INV-07]`, `[INV-11]`, `[INV-12]`, `[INV-14]`)  
**Scope:** Cross-Platform Hardening, Chronological Accuracy & UX Error Diagnostics (`MAJ-04`, `MAJ-10`, `MAJ-08`, `MAJ-09`, `MAJ-05` + `INF-03`, `MIN-01`, `MIN-10`)

---

## 1. Architectural Constraints & Dependency Direction

1. **Crate Boundary Discipline:**
   - `loganalysis-core` remains strictly UI-framework-agnostic (no `gpui` dependency) and owns subprocess management (`adb/device.rs`), calendar timestamp parsing (`buffer/timestamp.rs`), and file buffer storage (`buffer/file_buffer.rs`).
   - `loganalysis-gui` consumes `loganalysis-core` public APIs (`SearchError`, `CompiledMatcher`, `parse_color_spec`, `FileLogBuffer`) and owns workspace presentation logic (`workspace/search.rs`, `workspace/filters.rs`, `workspace/dock.rs`) and GPUI element rendering (`components/text_input.rs`).
2. **Existing Public & Test Contracts Protected:**
   - `DAYS_BEFORE_MONTH` and `ONE_YEAR_MICROS` in `buffer/timestamp.rs` MUST keep their exact existing values (`[0, 0, 31, 60, ...]` and `366 * 86_400 * 1_000_000`) because yearless Android logcat timestamps (`MM-DD HH:MM:SS.mmm`) intentionally model a 366-day calendar so `02-29` and `03-01` stay distinct and monotonic (`test_leap_year_feb29_distinct_from_mar01`, `challenger_m1_adversarial_suite.rs`, `year_rollover_tests.rs`, `adversarial_rollover_tests.rs`).
   - `FileLogBuffer::get_raw_line` MUST remain a pure zero-syscall slice read on `Storage` (no `std::fs::metadata` in the Rayon hot path).
   - Linux child-process parent-death safety (`[INV-12]`: `PR_SET_PDEATHSIG` + `getppid() != parent_pid`) MUST remain intact in `adb/device.rs`.
   - Existing `include_str!` tests in `crates/loganalysis-gui/src/main.rs` do not target any of the 7 files modified in Phase 2, and all public workspace/core signatures remain backwards-compatible.
3. **CI Gate:** Workspace must pass `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` on Linux, macOS, and Windows (`.github/workflows/ci.yml`).

---

## 2. Remediation Specifications

### 2.1. `MAJ-04`: Cross-Platform Non-Blocking Subprocess Pipe Reading
- **Affected File:** [`crates/loganalysis-core/src/adb/device.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/device.rs) (lines 107–218)
- **Dependency Already Present:** `crossbeam-channel = "0.5"` in `crates/loganalysis-core/Cargo.toml:19`.

#### Problem & Constraints
`scan_adb_device_details(timeout)` sets `O_NONBLOCK` on `stdout_pipe` under `#[cfg(target_os = "linux")]` only. On macOS and Windows, `stdout_pipe.read(&mut buf)` remains blocking, causing an indefinite hang if `adb devices -l` stalls or if a spawned ADB daemon inherits the stdout file descriptor. Conversely, `test_scan_adb_devices_mock_daemon_inherited_pipe_hang` (`crates/loganalysis-core/tests/adb_logcat_adversarial_tests.rs:322-371`) spawns a grandchild `( sleep 5 ) &` that keeps the write end of stdout open after the direct child exits `0`, asserting completion in `< 150ms`.

#### Trade-Off Analysis
- *Rejected:* Platform-specific `poll`/`kqueue`/Win32 `PeekNamedPipe` FFI wrappers — high maintenance surface and fragile across OS APIs.
- *Chosen:* A pure-Rust dedicated pipe-reader worker thread feeding a bounded `crossbeam_channel::bounded::<Vec<u8>>(128)` polled with `recv_timeout`, extracted into a testable `run_command_with_timeout` helper.

#### Exact Design & Signatures
1. Extract a testable public helper in `crates/loganalysis-core/src/adb/device.rs`:
   ```rust
   /// Executes `cmd` with captured stdout and an enforced wall-clock `timeout`.
   /// Uses a dedicated pipe-reader thread and bounded crossbeam channel so pipe reads
   /// cannot block timeout enforcement on any platform (Linux, macOS, Windows).
   pub fn run_command_with_timeout(
       mut cmd: Command,
       timeout: Duration,
   ) -> Result<Vec<u8>, AdbError>
   ```
2. Simplify `scan_adb_device_details(timeout: Duration) -> Result<Vec<AdbDeviceInfo>, AdbError>`:
   ```rust
   pub fn scan_adb_device_details(timeout: Duration) -> Result<Vec<AdbDeviceInfo>, AdbError> {
       let adb_path = super::resolve_adb_path();
       let mut cmd = Command::new(adb_path);
       cmd.args(["devices", "-l"]);
       let output = run_command_with_timeout(cmd, timeout)?;
       let stdout_str = String::from_utf8_lossy(&output);
       Ok(parse_adb_devices_output(&stdout_str))
   }
   ```
3. Inside `run_command_with_timeout`:
   - Capture `let start_time = Instant::now();`.
   - Enforce `cmd.stdout(Stdio::piped());` and `cmd.stderr(Stdio::null());`.
   - Preserve the existing `#[cfg(target_os = "linux")]` `cmd.pre_exec` block (`PR_SET_PDEATHSIG` + `getppid() != parent_pid`, `[INV-12]`).
   - Remove the `#[cfg(target_os = "linux")]` `libc::fcntl(... O_NONBLOCK)` block completely.
   - Spawn `let mut child = cmd.spawn().map_err(|e| AdbError::ExecutionFailed(e.to_string()))?;`.
   - Pre-reader timeout guard:
     ```rust
     if start_time.elapsed() >= timeout {
         let _ = child.kill();
         let _ = child.wait();
         return Err(AdbError::Timeout(timeout));
     }
     ```
   - Take stdout pipe (`let mut stdout_pipe = match child.stdout.take() { Some(p) => p, None => { let _ = child.kill(); let _ = child.wait(); return Err(AdbError::IoCaptureFailed); } };`).
   - Create channel `let (tx, rx) = crossbeam_channel::bounded::<Vec<u8>>(128);` and spawn `reader_handle`:
     ```rust
     let reader_handle = thread::spawn(move || {
         let mut buf = [0u8; 4096];
         loop {
             match stdout_pipe.read(&mut buf) {
                 Ok(0) => break,
                 Ok(n) => {
                     if tx.send(buf[..n].to_vec()).is_err() {
                         break;
                     }
                 }
                 Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                 Err(_) => break,
             }
         }
     });
     ```
   - Main wait loop tracking `let mut output = Vec::new();` and `let mut pipe_disconnected = false;`:
     - Compute `remaining = timeout.checked_sub(start_time.elapsed())`. If `None` or `remaining.is_zero()`:
       ```rust
       let _ = child.kill();
       let _ = child.wait();
       drop(rx);
       let _ = reader_handle.join();
       return Err(AdbError::Timeout(timeout));
       ```
     - Call `rx.recv_timeout(remaining.min(Duration::from_millis(20)))`:
       - `Ok(chunk)` => `output.extend_from_slice(&chunk);`
       - `Err(crossbeam_channel::RecvTimeoutError::Timeout)` => poll `child.try_wait()`:
         - `Err(e)` => `let _ = child.kill(); let _ = child.wait(); drop(rx); let _ = reader_handle.join(); return Err(AdbError::ExecutionFailed(e.to_string()));`
         - `Ok(None)` => continue loop.
         - `Ok(Some(status))` => drain buffered chunks:
           1. Loop `rx.try_recv()` appending `Ok(chunk)`, setting `pipe_disconnected = true` on `TryRecvError::Disconnected`.
           2. If `TryRecvError::Empty` and `pipe_disconnected` is still `false`, perform a bounded grace drain up to `timeout.saturating_sub(start_time.elapsed()).min(Duration::from_millis(5))` via `rx.recv_timeout(...)` so any chunk just read by the worker thread before child exit is not lost under CI scheduler load, breaking immediately on `Timeout` or `Disconnected` (setting `pipe_disconnected = true` on `Disconnected`).
           3. `break status;`
       - `Err(crossbeam_channel::RecvTimeoutError::Disconnected)` =>
         `pipe_disconnected = true;`
         `let status = child.wait().map_err(|e| AdbError::ExecutionFailed(e.to_string()))?;`
         `break status;`
   - After the loop exits:
     ```rust
     drop(rx);
     if pipe_disconnected {
         let _ = reader_handle.join();
     }
     ```
   - Status check:
     ```rust
     if !status.success() {
         return Err(AdbError::NonZeroExit(status.code()));
     }
     Ok(output)
     ```

#### Critical Edge Cases & Invariants
- **Inherited Pipe Daemon (`( sleep 5 ) &`):** When the direct child exits `0` while a detached grandchild still holds the stdout write end, `try_wait()` observes `Some(status)` while `pipe_disconnected` stays `false`. Skipping `reader_handle.join()` in that specific state detaches the pipe-blocked thread (matching `adb/logcat.rs` `Drop` behavior) so `scan_adb_device_details` finishes in ~25ms (`< 150ms` requirement) instead of hanging 5s.
- **Channel Backpressure on Timeout:** If `child` writes > 512 KB (128 × 4 KB chunks) before stalling and timing out, `tx.send(...)` blocks in the reader thread. Calling `child.kill()`, `child.wait()`, and `drop(rx)` **before** `reader_handle.join()` guarantees `tx.send` wakes immediately with `Err` and prevents deadlock on `join()`.
- **Sub-20ms Timeouts:** Capping `recv_timeout` to `remaining.min(Duration::from_millis(20))` guarantees `Duration::from_micros(50)` / `Duration::from_millis(5)` tests do not overshoot by 20ms.
- **Zombie Prevention:** Every non-error exit path calls either `child.wait()` (on `Disconnected` or `Timeout`) or observes `Some(status)` from `child.try_wait()` (which reaps the child).

---

### 2.2. `MAJ-10`: Gregorian Leap-Year Arithmetic & Epoch Drift
- **Affected File:** [`crates/loganalysis-core/src/buffer/timestamp.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/timestamp.rs)

#### Problem & Constraints
In `buffer/timestamp.rs`:
1. `Case 2` (`YYYY-MM-DD[T ]HH:MM:SS[.mmm]`, lines 124–157) uses `day_of_year(month, day)` which indexes `DAYS_BEFORE_MONTH` where `DAYS_BEFORE_MONTH[3] = 60` (29-day Feb). In non-leap years (e.g., 2023, 2025, 2026), crossing `02-28` to `03-01` inserts a **24-hour phantom jump** (`48h` between consecutive midnights).
2. `Case 2` computes `(year as i64 - 1970) * 365 + day_of_year(...)`, omitting all leap days since 1970 (14 days of epoch drift by 2026).
3. Neither `Case 1` nor `Case 2` validates per-month day maxima (`02-30`, `02-31`, `04-31`, or `2025-02-29`).
4. **Invariants:** `Case 1` (`MM-DD HH:MM:SS.mmm`) lacks a year and MUST retain `DAYS_BEFORE_MONTH` (`[0, 0, 31, 60, ...]`) and `ONE_YEAR_MICROS` (`366 * 86_400 * 1_000_000`) so `02-29` remains valid and distinct from `03-01` (`test_leap_year_feb29_distinct_from_mar01`, `challenger_m1_adversarial_suite.rs`, `year_rollover_tests.rs`, `adversarial_rollover_tests.rs`).

#### Exact Design & Signatures
Add the following `pub` items to `crates/loganalysis-core/src/buffer/timestamp.rs` (keeping `DAYS_BEFORE_MONTH` and `ONE_YEAR_MICROS` unchanged):

```rust
pub const NON_LEAP_DAYS_BEFORE_MONTH: [u32; 13] = [
    0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334,
];

#[inline]
pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[inline]
pub fn max_days_in_month(month: u32, is_leap: bool) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap { 29 } else { 28 },
        _ => 0,
    }
}

#[inline]
pub fn day_of_year_with_leap(month: u32, day: u32, is_leap: bool) -> u32 {
    let m = month.clamp(1, 12) as usize;
    let table = if is_leap {
        &DAYS_BEFORE_MONTH
    } else {
        &NON_LEAP_DAYS_BEFORE_MONTH
    };
    table[m] + day.saturating_sub(1)
}

#[inline]
fn leap_years_before(year: i64) -> i64 {
    let y = year - 1;
    y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400)
}

#[inline]
pub fn days_since_epoch(year: i32, month: u32, day: u32) -> i64 {
    let y = year as i64;
    let leap_days = leap_years_before(y) - leap_years_before(1970);
    (y - 1970) * 365
        + leap_days
        + day_of_year_with_leap(month, day, is_leap_year(year)) as i64
}
```

Update `extract_timestamp`:
1. **Case 1 (`MM-DD HH:MM:SS.mmm`):**
   - Replace `!(1..=31).contains(&day)` with `day == 0 || day > max_days_in_month(month, true)`.
   - Keep `let days = day_of_year(month, day);` unchanged (`day_of_year` continues to use `DAYS_BEFORE_MONTH`, or delegates to `day_of_year_with_leap(month, day, true)`).
2. **Case 2 (`YYYY-MM-DD[T ]HH:MM:SS[.mmm]`):**
   - Compute `let year_i32 = year as i32;` and `let is_leap = is_leap_year(year_i32);`.
   - Replace `!(1..=31).contains(&day)` with `day == 0 || day > max_days_in_month(month, is_leap)`.
   - Compute `let days = days_since_epoch(year_i32, month, day);`.

#### Critical Edge Cases & Invariants
- **Century Rules:** `1900` and `2100` are non-leap (`is_leap_year == false`, `02-29` rejected); `2000` and `2024` are leap (`is_leap_year == true`, `02-29` accepted).
- **Pre-1970 & Negative Years:** Using `div_euclid` in `leap_years_before` avoids Rust `/` truncation-toward-zero sign errors (`days_since_epoch(1969, 12, 31) == -1`, `days_since_epoch(1970, 1, 1) == 0`).
- **Epoch Accuracy:** `days_since_epoch(2026, 11, 6) == 20_763` (`1_793_923_200_000_000` µs at midnight UTC).
- **Non-Leap Boundary:** `2025-02-28 23:59:59.000` to `2025-03-01 00:00:00.000` yields delta `1_000_000` µs (1 second, not 24h + 1s).
- **Overflow Safety:** Across all 4-digit years `0000..=9999`, `days_since_epoch` stays within `[-719_528, 2_932_896]`, so `calc_micros` is bounded well inside `i64` range.

---

### 2.3. `MAJ-08`: Diagnostic Feedback for Malformed Search Regex
- **Affected File:** [`crates/loganalysis-gui/src/workspace/search.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/search.rs) (lines 13–108)
- **Related UI Contract:** [`crates/loganalysis-gui/src/components/find_bar.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/find_bar.rs) (`is_find_status_error`, lines 217–225)

#### Problem & Constraints
Both the async completion handler (lines 76–88) and synchronous search branch (lines 94–107) use `if let Ok(Some(found)) = search_res { ... } else { ... }`, collapsing `Err(SearchError::InvalidRegex(_))` into `"Search pattern not found"` / `"0 matches"` and duplicating identical UI update code.

#### Exact Design & Signatures
1. Extract a pure, GPUI-independent outcome formatter in `crates/loganalysis-gui/src/workspace/search.rs`:
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct SearchOutcomeUpdate {
       pub matched_location: Option<(usize, usize)>, // (visible_row, real_idx)
       pub status_text: String,
       pub match_status: String,
   }

   pub fn format_search_outcome(
       search_res: Result<Option<SearchResult>, SearchError>,
   ) -> SearchOutcomeUpdate {
       match search_res {
           Ok(Some(found)) => SearchOutcomeUpdate {
               matched_location: Some((found.visible_row, found.real_idx)),
               status_text: format!("Found match at line {}", found.real_idx + 1),
               match_status: format!("Line {}", found.real_idx + 1),
           },
           Ok(None) => SearchOutcomeUpdate {
               matched_location: None,
               status_text: "Search pattern not found".to_string(),
               match_status: "0 matches".to_string(),
           },
           Err(SearchError::InvalidRegex(err)) => SearchOutcomeUpdate {
               matched_location: None,
               status_text: format!("Regex syntax error: {err}"),
               match_status: "Regex Error".to_string(),
           },
           Err(err) => SearchOutcomeUpdate {
               matched_location: None,
               status_text: format!("Search error: {err}"),
               match_status: "Error".to_string(),
           },
       }
   }
   ```
2. Add a single workspace helper method used by both branches:
   ```rust
   fn apply_search_result(
       &mut self,
       search_res: Result<Option<SearchResult>, SearchError>,
       cx: &mut Context<Self>,
   ) {
       if matches!(search_res, Err(SearchError::Cancelled)) {
           return;
       }
       let outcome = format_search_outcome(search_res);
       if let Some((visible_row, real_idx)) = outcome.matched_location {
           self.selection.select(visible_row, real_idx);
           self.scroll_handle
               .scroll_to_item(visible_row, ScrollStrategy::Center);
       }
       self.status_text = outcome.status_text.into();
       self.search_match_status = Some(outcome.match_status);
       cx.notify();
   }
   ```
3. Replace lines 76–89 (inside the `cx.spawn` update closure after the `search_cancel_gen` check) with `workspace.apply_search_result(search_res, cx);`, and replace lines 94–107 with:
   ```rust
   } else {
       let search_res =
           search_in_log(&*source, &res.visible_indices, &query, start_row, None, 0, None);
       self.apply_search_result(search_res, cx);
   }
   ```

#### Critical Edge Cases & Invariants
- **Find Bar Error Pill Styling:** `"Regex Error"` and `"Error"` satisfy `find_bar::is_find_status_error(status) == true` (via `lower.contains("error")`), automatically rendering the red error pill in `render_find_bar`.
- **Typing Mid-Expression:** `update_cached_search_matcher` (called on `TextInputEvent::Change`) remains silent while the user is mid-typing `(foo`; errors surface cleanly when search is executed.
- **Cancellation Safety:** `Err(SearchError::Cancelled)` never overwrites `status_text` or `search_match_status` even if observed before generation mismatch.

---

### 2.4. `MAJ-09`: Filter Modal Regex & ReDoS Validation
- **Affected File:** [`crates/loganalysis-gui/src/workspace/filters.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/filters.rs) (lines 237–274)

#### Problem & Constraints
`save_modal_filter` checks only `pattern.is_empty()` and stores unvalidated regex strings in `FilterTabData::filters`. During `execute_filtering`, `PreparedFilter::new` silently ignores invalid regexes, leaving an apparently active rule that matches zero lines without user feedback.

#### Trade-Off Analysis
- *Rejected:* Calling `fancy_regex::Regex::new` directly in the GUI crate — misses `REGEX_DFA_SIZE_LIMIT` and `has_nested_repetition` static AST ReDoS checks enforced by `CompiledMatcher::new`, and pulls `fancy_regex` into GUI validation drift.
- *Chosen:* Reusing `loganalysis_core::CompiledMatcher::new(pattern, is_case_sensitive, true)` inside a pure `validate_filter_pattern` helper so UI validation and core filter compilation share identical acceptance criteria.

#### Exact Design & Signatures
1. Add a pure testable helper in `crates/loganalysis-gui/src/workspace/filters.rs`:
   ```rust
   /// Validates a filter rule pattern before saving.
   /// When `is_regex` is true, validates syntax, DFA size limits, and ReDoS nested-repetition rules
   /// identically to `loganalysis_core::CompiledMatcher`.
   pub fn validate_filter_pattern(
       pattern: &str,
       is_regex: bool,
       is_case_sensitive: bool,
   ) -> Result<(), String> {
       if pattern.trim().is_empty() {
           return Err("Filter pattern cannot be empty".to_string());
       }
       if is_regex {
           loganalysis_core::CompiledMatcher::new(pattern, is_case_sensitive, true).map_err(|e| {
               match e {
                   loganalysis_core::FilterError::InvalidRegex(msg) => {
                       format!("Invalid filter regex: {msg}")
                   }
                   other => format!("Invalid filter regex: {other}"),
               }
           })?;
       }
       Ok(())
   }
   ```
2. Update `LogAnalysisWorkspace::save_modal_filter`:
   ```rust
   pub fn save_modal_filter(&mut self, cx: &mut Context<Self>) {
       let pattern = self.modal_pattern_input.read(cx).text().trim().to_string();
       if let Err(err) = validate_filter_pattern(
           &pattern,
           self.filter_edit_modal.is_regex,
           self.filter_edit_modal.is_case_sensitive,
       ) {
           self.status_text = err.into();
           cx.notify();
           return;
       }
       // Proceed to mutate self.filter_tabs, close modal, reset input_focus, and reapply_filters
   ```

#### Critical Edge Cases & Invariants
- **Modal State Preservation on Error:** When `validate_filter_pattern` returns `Err(err)`, `save_modal_filter` returns *before* modifying `self.filter_tabs`, *before* setting `self.filter_edit_modal.is_open = false`, *before* resetting `self.input_focus`, and *before* calling `self.reapply_filters(cx)`, allowing the user to fix their pattern immediately.
- **Literal Mode Bypass:** When `is_regex == false`, regex metacharacters (e.g. `"[ERROR"`, `"foo("`) are valid literal substrings and pass `validate_filter_pattern` cleanly.
- **ReDoS Rejection Consistency:** Lookaround nested repetitions like `"(?=(a+)+)b"` fail `validate_filter_pattern` with `"Invalid filter regex: Catastrophic backtracking expression rejected (ReDoS protection)"`.

---

### 2.5. `MAJ-05` (+ `INF-03`): Mmap Truncation Safety & Heap Fallback
- **Affected File:** [`crates/loganalysis-core/src/buffer/file_buffer.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs)

#### Problem & Constraints
`FileLogBuffer::open` maps files via `memmap2::Mmap`. External truncation of an mmap-backed file can raise an uncatchable `SIGBUS` when reading past the new file length. However, calling `std::fs::metadata` inside `get_raw_line` would inject millions of `stat` syscalls into Rayon filter loops and still suffer TOCTOU races. Additionally, the existing heap fallback on `Mmap::map` error redundantly re-opens `&path_buf` (`INF-03`).

#### Trade-Off Analysis
- *Rejected:* Installing a global Unix `SIGBUS` signal handler with `siglongjmp` (unsafe across multi-threaded Rayon/Vulkan/GPUI threads) or adding `metadata()` checks inside `get_raw_line`.
- *Chosen:* Factoring `from_storage`, exposing explicit `open_heap` for volatile/rotating files, eliminating the redundant `File::open` in the `open` fallback via `SeekFrom::Start(0)`, and adding lightweight non-hot-path inspection methods (`is_heap_backed`, `is_truncated_on_disk`).

#### Exact Design & Signatures
1. Extract shared constructor helper on `FileLogBuffer`:
   ```rust
   fn from_storage(path_buf: PathBuf, storage: Storage) -> Self
   ```
   - Computes `line_offsets`, `max_line_len`, `longest_line_idx`, and `line_cache` from `storage.as_bytes()`.
   - When `storage.as_bytes().is_empty()`, initializes `line_offsets = vec![0]`, `max_line_len = 0`, `longest_line_idx = None`.
2. Update `FileLogBuffer::open<P: AsRef<Path>>(path: P) -> io::Result<Self>`:
   - `let mut file = File::open(&path_buf)?;`
   - If `file_len == 0`: return `Ok(Self::from_storage(path_buf, Storage::Heap(Vec::new())))`.
   - On `unsafe { Mmap::map(&file) }`:
     - `Ok(mmap)` -> `Storage::Mmap(mmap)`
     - `Err(_)` -> rewind the existing handle (`INF-03`) instead of re-opening `&path_buf`:
       ```rust
       use std::io::{Read, Seek, SeekFrom};
       file.seek(SeekFrom::Start(0))?;
       let mut bytes = Vec::with_capacity(file_len);
       file.read_to_end(&mut bytes)?;
       Storage::Heap(bytes)
       ```
   - Return `Ok(Self::from_storage(path_buf, storage))`.
3. Add explicit heap-backed loader:
   ```rust
   /// Loads a file completely into heap-owned memory (`Storage::Heap`), making all subsequent
   /// reads immune to external file truncation (`SIGBUS`). Intended for volatile/rotating logs.
   pub fn open_heap<P: AsRef<Path>>(path: P) -> io::Result<Self> {
       use std::io::Read;
       let path_buf = path.as_ref().to_path_buf();
       let mut file = File::open(&path_buf)?;
       let file_len = file.metadata()?.len() as usize;
       let mut bytes = Vec::with_capacity(file_len);
       file.read_to_end(&mut bytes)?;
       Ok(Self::from_storage(path_buf, Storage::Heap(bytes)))
   }
   ```
4. Add storage and truncation inspection methods:
   ```rust
   /// Returns `true` if this buffer is backed by heap memory rather than an OS memory mapping.
   #[inline]
   pub fn is_heap_backed(&self) -> bool {
       matches!(self.storage, Storage::Heap(_))
   }

   /// Checks whether an mmap-backed file has shrunk on disk below its mapped byte length.
   /// Always returns `false` for heap-backed buffers and on metadata lookup failure.
   /// MUST NOT be called on the per-line `get_raw_line` hot path.
   pub fn is_truncated_on_disk(&self) -> bool {
       match &self.storage {
           Storage::Heap(_) => false,
           Storage::Mmap(_) => std::fs::metadata(&self.path)
               .map(|m| m.len() < self.byte_size() as u64)
               .unwrap_or(false),
       }
   }
   ```

#### Critical Edge Cases & Invariants
- **Hot Path Untouched:** `LogSource::get_raw_line` (`file_buffer.rs:160-172`) performs zero syscalls and MUST NOT call `is_truncated_on_disk()` or `std::fs::metadata()`.
- **External Truncation Immunity:** A `FileLogBuffer::open_heap` instance survives subsequent truncation to 0 bytes or deletion of the source file with `is_truncated_on_disk() == false` and all lines readable.
- **Metadata Failure / Deleted File:** For `Storage::Mmap`, if the file was unlinked on Unix (`metadata` returns `Err(NotFound)`), `.unwrap_or(false)` returns `false` (unlinking an open file does not invalidate its kernel inode pages; shrinking it with `set_len`/`truncate` does, which returns `true`).
- **Empty File Handling:** Both `open` and `open_heap` on an empty file produce `is_heap_backed() == true`, `line_count() == 0`, `byte_size() == 0`, and `is_truncated_on_disk() == false`.

---

### 2.6. `MIN-01`: Facet Filter Palette Color Fix
- **Affected File:** [`crates/loganalysis-gui/src/workspace/dock.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/dock.rs) (lines 166–179)

#### Problem & Constraints
`add_facet_filter` defines `let palette = ["Cyan", "Green", "Magenta", "Yellow", "Blue"];`, but `"Magenta"` does not exist in `loganalysis_core::constants::COLOR_MAP`. Every tag whose byte sum modulo 5 equals 2 resolves `bg_color` to `None`, leaving matched lines unhighlighted.

#### Exact Design & Signatures
1. Extract a testable module-level constant in `crates/loganalysis-gui/src/workspace/dock.rs`:
   ```rust
   pub const FACET_FILTER_PALETTE: [&str; 5] = ["Cyan", "Green", "Orchid", "Yellow", "Blue"];
   ```
2. Update `LogAnalysisWorkspace::add_facet_filter`:
   ```rust
   let color_idx = tag.bytes().map(|b| b as usize).sum::<usize>() % FACET_FILTER_PALETTE.len();
   let bg_color = if is_exclude {
       "None".to_string()
   } else {
       FACET_FILTER_PALETTE[color_idx].to_string()
   };
   ```

#### Invariants
- Every entry in `FACET_FILTER_PALETTE` exists in `loganalysis_core::constants::COLOR_MAP` (`parse_color_spec(color, true).is_some()`).
- `"Orchid"` (`0xDA70D6`) paired with `"Black"` (`0x000000`) achieves contrast ratio $> 7.0 : 1$, satisfying `[INV-11]`.

---

### 2.7. `MIN-10`: `TextInput::paint` Prepaint Desync Panic Elimination
- **Affected File:** [`crates/loganalysis-gui/src/components/text_input.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs) (lines 754–768)

#### Problem & Constraints
`TextElement::paint` calls `prepaint.line.take().unwrap()` and `line.paint(origin, window.line_height(), window, cx).unwrap()`. Any GPUI layout/prepaint desync triggers an unrecoverable UI thread panic.

#### Exact Design & Changes
Replace lines 754–768 in `crates/loganalysis-gui/src/components/text_input.rs`:
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

#### Invariants
- `window.handle_input(...)` continues to execute before the early return so IME/keyboard input handling stays registered on `bounds`.
- `line` is moved into `input.last_layout = Some(line)` after `line.paint(origin, window.line_height(), window, cx)` (preserving GPUI 0.2.2's 4-argument signature), along with `last_bounds` and `scroll_offset`.
- Cursor quad painting when `focus_handle.is_focused(window)` is preserved.

---

## 3. Verification Plan & Deterministic Test Matrix

All new unit tests must be strictly deterministic (no arbitrary wall-clock sleep assumptions for correctness beyond generous timeout ceilings).

| Item | Target File | Required Unit Tests & Assertions |
| :--- | :--- | :--- |
| **`MAJ-04`** | `crates/loganalysis-core/src/adb/device.rs` | 1. `run_command_with_timeout` returns `Ok(stdout)` for fast output and reassembles multi-chunk (> 4096 bytes) output.<br>2. Returns `Err(AdbError::NonZeroExit(Some(code)))` on non-zero exit status.<br>3. Returns `Err(AdbError::Timeout(t))` promptly when a child sleeps past `t` (tested on both `#[cfg(unix)]` and `#[cfg(windows)]` without requiring `adb`).<br>4. Returns `Err(AdbError::Timeout(Duration::ZERO))` immediately for zero timeout.<br>5. Returns `Err(AdbError::ExecutionFailed(_))` for a non-existent executable.<br>6. Existing `test_scan_adb_devices_mock_daemon_inherited_pipe_hang` passes in `< 150ms`. |
| **`MAJ-10`** | `crates/loganalysis-core/src/buffer/timestamp.rs` | 1. `is_leap_year`: `2000` (true), `2024` (true), `1900` (false), `2100` (false), `2025` (false), `2026` (false).<br>2. `max_days_in_month` & Case 1/Case 2 rejection: `2025-02-29` (`None`), `2100-02-29` (`None`), `1900-02-29` (`None`), `2024-02-29` (`Some`), `2000-02-29` (`Some`), `02-30` (`None`), `04-31` (`None`), while `02-29 12:00:00.000` (Case 1) remains `Some`.<br>3. Non-leap transition: `2025-02-28 23:59:59.000` -> `2025-03-01 00:00:00.000` delta == `1_000_000` µs (1s, eliminating 24h phantom jump).<br>4. Leap transition: `2024-02-29 23:59:59.000` -> `2024-03-01 00:00:00.000` delta == `1_000_000` µs.<br>5. Epoch accuracy: `days_since_epoch(1970, 1, 1) == 0`, `days_since_epoch(1969, 12, 31) == -1`, `days_since_epoch(2000, 1, 1) == 10_957`, `days_since_epoch(2026, 11, 6) == 20_763`.<br>6. Existing `test_leap_year_feb29_distinct_from_mar01`, `challenger_m1_adversarial_suite`, `year_rollover_tests`, and `adversarial_rollover_tests` remain 100% passing. |
| **`MAJ-08`** | `crates/loganalysis-gui/src/workspace/search.rs` | Unit tests for `format_search_outcome`:<br>1. `Ok(Some(SearchResult { visible_row: 3, real_idx: 41 }))` -> `matched_location: Some((3, 41))`, `status_text == "Found match at line 42"`, `match_status == "Line 42"`.<br>2. `Ok(None)` -> `matched_location: None`, `status_text == "Search pattern not found"`, `match_status == "0 matches"`.<br>3. `Err(SearchError::InvalidRegex("unclosed group".into()))` -> `status_text == "Regex syntax error: unclosed group"`, `match_status == "Regex Error"`, and `is_find_status_error(&outcome.match_status) == true`.<br>4. `Err(SearchError::Cancelled)` -> `status_text == "Search error: Search cancelled"`, `match_status == "Error"`, and `is_find_status_error(&outcome.match_status) == true`. |
| **`MAJ-09`** | `crates/loganalysis-gui/src/workspace/filters.rs` | Unit tests for `validate_filter_pattern`:<br>1. Empty or whitespace-only pattern returns `Err` for both regex and literal modes.<br>2. Invalid regex syntax (`"[unclosed"`, `"(abc"`) returns `Err` containing `"Invalid filter regex:"`.<br>3. Lookaround ReDoS nested repetition (`"(?=(a+)+)b"`) returns `Err`.<br>4. Valid regex (`"^E/.*timeout\\d+$"`) and literal containing `[` when `is_regex == false` both return `Ok(())`. |
| **`MAJ-05`** | `crates/loganalysis-core/src/buffer/file_buffer.rs` | 1. `FileLogBuffer::open_heap` sets `is_heap_backed() == true`, `is_truncated_on_disk() == false`, and survives truncating the backing file to 0 bytes with all lines still readable.<br>2. `FileLogBuffer::open` on a non-empty file sets `is_heap_backed() == false` (when mmap succeeds) and `is_truncated_on_disk() == false`; after shrinking the file on disk below `byte_size()`, `is_truncated_on_disk() == true`; if file grows, `is_truncated_on_disk() == false`.<br>3. Empty file via `open` and `open_heap` yields `is_heap_backed() == true`, `line_count() == 0`, `is_truncated_on_disk() == false`. |
| **`MIN-01`** | `crates/loganalysis-gui/src/workspace/dock.rs` | Verify `FACET_FILTER_PALETTE.len() == 5`, contains `"Orchid"` (not `"Magenta"`), every entry satisfies `loganalysis_core::parse_color_spec(color, true).is_some()`, and every entry meets WCAG AA contrast ($\ge 4.5:1$) with `"Black"` foreground via `loganalysis_core::evaluate_contrast`. |
| **`MIN-10`** | `crates/loganalysis-gui/src/components/text_input.rs` | Verified by compilation/clippy clean pass and inspection of `TextElement::paint` having zero `.unwrap()` calls on `prepaint.line` or `line.paint`. |

### Global Verification Commands
```bash
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
All three commands must exit with status `0` and zero warnings or skipped/ignored tests.

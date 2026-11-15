# Phase 4 Backlog & QA Refinement Architectural Specification

**Status:** Proposed for Pre-Approval (`No Unsolicited Build`)  
**Target Branch:** `master`  
**Author:** Software Architect  
**Scope Sources:** [docs/review-report.md](file:///home/drew/github/hwanjongyu/loganalysis/docs/review-report.md), [docs/qa-phase3-remediation.md](file:///home/drew/github/hwanjongyu/loganalysis/docs/qa-phase3-remediation.md), [docs/INVARIANTS.md](file:///home/drew/github/hwanjongyu/loganalysis/docs/INVARIANTS.md)  
**Preservation Guarantee:** All Phase 1–3 deliverables, invariants, and existing public/test contracts MUST remain intact.

---

## 1. Overview, Trade-off Summary & Scope Boundary

Phase 4 resolves the remaining code-level backlog items (`MIN-03`, `INF-01`), hardens two runtime/QA invariants uncovered during Phase 3 verification (`[INV-09]` excluded-line trim counter parity and non-blocking child reaping in `LogcatStreamHandle::stop`), closes the explicit registry allowlist caveat in `deny.toml` (`INF-06`), and formally documents the external dependency deferrals (`MIN-12`, `INF-05`).

### 1.1 Architectural Constraints & Dependency Direction

1. **`loganalysis-core` (`crates/loganalysis-core/`)**:
   - Remains pure Rust domain, parsing, filtering, streaming, and subprocess orchestration logic with **zero** dependency on `gpui`, `rfd`, or OS windowing APIs.
   - Backward compatibility of `parse_logcat_column_line` and `FilterResult::default()` MUST be preserved across all unit, integration, and adversarial tests.
2. **`loganalysis-gui` (`crates/loganalysis-gui/`)**:
   - Retains the Phase 3 `lib.rs` / thin `main.rs` split and headless `controllers/` boundaries.
   - Logging changes MUST require **zero** new Cargo dependencies (using std `BufWriter<File>`, `Mutex<LogWriterState>`, and `std::env::temp_dir()` test isolation without modifying `Cargo.toml` or `Cargo.lock`).
3. **Invariants Protected**:
   - `[INV-06]` / `[INV-12]` Subprocess termination without UI thread blocking and without leaking zombie processes.
   - `[INV-09]` Exact synchronization of `total_matched_lines`, `filter_counts`, and `rule_counts` across initial filtering, streaming append, and ring-buffer trimming—now extended to excluded (`exclude = true`) lines.
   - Zero workspace/repository file pollution (`crates/loganalysis-gui/loganalysis.log`) during test execution or normal desktop launches.

### 1.2 Pragmatic Trade-off Summary

| Item ID | Scope / Target Files | Chosen Design | Trade-off & Reversibility |
|---|---|---|---|
| `MIN-03` | `crates/loganalysis-gui/src/logging.rs`, `src/lib.rs` | Prefer OS user log directory exclusively; fallback to `cwd` only when user env lookup returns `None`; replace per-call open/flush with `Mutex<LogWriterState>` (`BufWriter<File>`) + explicit `flush_logs()` + immediate flush on error/panic. | Avoids adding `dirs` crate dependency while preserving existing `resolve_user_log_dir_from_env` precedence tests (`HOME`, `LOCALAPPDATA`, `APPDATA`, `USERPROFILE`). 100% localized to `logging.rs` (+ 1 flush call on exit in `lib.rs`). |
| `INF-01` | `crates/loganalysis-core/src/buffer/{column.rs,mod.rs}`, `src/lib.rs` (+ optional GUI `log_view.rs`, `workspace/dock.rs`) | Introduce zero-copy `LogFormatParser` trait SPI, `ThreadtimeColumnParser`, `BriefColumnParser`, `DEFAULT_COLUMN_PARSERS`, `parse_column_line_with_parsers`, and `parse_column_line_auto`. Keep `parse_logcat_column_line` delegating strictly to `ThreadtimeColumnParser`. | Preserves 100% behavioral parity for all existing `parse_logcat_column_line` callers & adversarial fuzz tests while enabling zero-copy multi-format parsing and custom user parsers via `&[&dyn LogFormatParser]`. |
| `[INV-09]` | `crates/loganalysis-core/src/filter/engine.rs`, `tests/engine_tests.rs` | Add `#[doc(hidden)] pub excluded_filter_matches: Vec<(usize, SmallVec<[usize; 2]>)>` on `FilterResult` storing only excluded lines that matched `>= 1` filter rule. Trim & shift independently of `visible_indices.is_empty()`. | Uses ~1 entry per *matched* excluded line (zero allocation for unmatched lines even when `show_only_filtered = true`), keeping incremental trim $O(\text{dropped})$ instead of rescanning the buffer or breaking `FilterResult::default()`. |
| Non-blocking `stop` | `crates/loganalysis-core/src/adb/logcat.rs` | In `LogcatStreamHandle::stop`, issue `child.kill()` + `child.try_wait()`, spawning a detached `"adb-child-reaper"` thread only when `try_wait()` returns `Ok(None) \| Err(_)`. | Eliminates 50ms–1.5s UI/Drop stalls under Linux scheduler contention while guaranteeing zero zombies (detached thread or synchronous fallback if spawn fails). |
| `INF-06` | `deny.toml` | Explicitly set `allow-registry = ["https://github.com/rust-lang/crates.io-index"]` and `allow-git = []` under `[sources]`. | Two-line config hardening matching verified `Cargo.lock` sources without altering CI scripts. |
| `MIN-12`, `INF-05` | Documentation only (no code edits) | Formal deferral record documenting active runtime workarounds and upstream upgrade criteria for `gpui 0.2.2` and transitive `proc-macro-error2 v2.0.1`. | Avoids risky local forks/patches of upstream GPUI macros while keeping builds green and workarounds auditable. |

---

## 2. Detailed Remediation Designs & Exact Rust Signatures

---

### 2.1 `MIN-03` — Logging Directory Hygiene & Buffered I/O

**Target Files:**
- [crates/loganalysis-gui/src/logging.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/logging.rs)
- [crates/loganalysis-gui/src/lib.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/lib.rs)

#### Problem & Invariants
1. `resolve_log_paths_with_lookup` (`logging.rs:63-87`) unconditionally pushes `cwd.join(filename)` **before** `user_share.join(filename)`. Launching the app from a workspace or running `cargo test` (`logging.rs:282`) pollutes the repository checkout (`crates/loganalysis-gui/loganalysis.log` was created at 23,725 bytes on disk).
2. `log_event` (`logging.rs:104-114`) acquires `LOG_MUTEX` and invokes `append_to_file` (`OpenOptions::new().create(true).append(true).open(path)` + `write_all` + `flush`) on every single call across every path, causing synchronous open/syscall/flush overhead per event.
3. `init_crash_handler` (`logging.rs:117-187`) writes directly via `append_to_file` without flushing buffered `loganalysis.log` entries first, risking out-of-order or lost pre-crash diagnostics.

#### Exact Changes in `crates/loganalysis-gui/src/logging.rs`

1. **Remove `#[allow(dead_code)]` on `resolve_user_log_dir` (`logging.rs:27`):**
   - Since `pub mod logging;` is exported from `lib.rs:8`, `pub fn resolve_user_log_dir() -> Option<PathBuf>` triggers no `dead_code` lint; remove `#[allow(dead_code)]`.

2. **Exclusive User Directory Precedence (`resolve_log_paths_with_lookup`, `logging.rs:49-87`):**
   - Update `resolve_log_paths` doc comment to specify:
     1. User data directory (`~/.local/share/loganalysis` on Unix, `%LOCALAPPDATA%\loganalysis` / `%APPDATA%\loganalysis` / `%USERPROFILE%\AppData\Local\loganalysis` on Windows) exclusively when resolved.
     2. Current working directory fallback **only** when no user directory environment variable resolves (`None`).
   - Replace body of `resolve_log_paths_with_lookup`:
     ```rust
     pub(crate) fn resolve_log_paths_with_lookup<F, C>(
         filename: &str,
         cwd: Option<PathBuf>,
         get_env: F,
         mut create_dir: C,
     ) -> Vec<PathBuf>
     where
         F: Fn(&str) -> Option<String>,
         C: FnMut(&Path),
     {
         let mut paths = Vec::new();

         if let Some(user_share) = resolve_user_log_dir_from_env(get_env) {
             create_dir(&user_share);
             paths.push(user_share.join(filename));
         } else if let Some(cwd) = cwd {
             paths.push(cwd.join(filename));
         }

         paths
     }
     ```

3. **Cached Buffered Writer (`LogWriterState` & `LOG_STATE`):**
   - Replace `static LOG_MUTEX: Mutex<()> = Mutex::new(());` (`logging.rs:8`) with:
     ```rust
     use std::fs::{File, OpenOptions};
     use std::io::{BufWriter, Write};

     #[derive(Default)]
     pub(crate) struct LogWriterState {
         cached: Option<(PathBuf, BufWriter<File>)>,
     }

     static LOG_STATE: Mutex<LogWriterState> = Mutex::new(LogWriterState { cached: None });
     ```
   - Implement methods on `LogWriterState`:
     ```rust
     impl LogWriterState {
         fn write_line(&mut self, path: &Path, line: &str, immediate_flush: bool) {
             let needs_reopen = match &self.cached {
                 Some((cached_path, _)) => cached_path.as_path() != path,
                 None => true,
             };

             if needs_reopen {
                 if let Some((_, mut prev_writer)) = self.cached.take() {
                     let _ = prev_writer.flush();
                 }
                 if let Some(parent) = path.parent() {
                     if !parent.as_os_str().is_empty() {
                         let _ = std::fs::create_dir_all(parent);
                     }
                 }
                 if let Ok(file) = OpenOptions::new().create(true).append(true).open(path) {
                     self.cached = Some((path.to_path_buf(), BufWriter::new(file)));
                 }
             }

             if let Some((_, writer)) = self.cached.as_mut() {
                 if writer.write_all(line.as_bytes()).is_err() {
                     self.cached = None;
                     return;
                 }
                 if immediate_flush && writer.flush().is_err() {
                     self.cached = None;
                 }
             }
         }

         fn flush(&mut self) {
             if let Some((_, writer)) = self.cached.as_mut() {
                 if writer.flush().is_err() {
                     self.cached = None;
                 }
             }
         }

         fn close(&mut self) {
             if let Some((_, mut writer)) = self.cached.take() {
                 let _ = writer.flush();
             }
         }
     }
     ```
   - **Why `close()` matters on Windows:** Clearing `self.cached` drops the open `File` handle so Windows unit tests deleting temporary directories via `Drop` do not hit `ERROR_SHARING_VIOLATION` / locked-file errors.

4. **Immediate Flush Policy, `flush_logs()`, and Test Path Injection:**
   ```rust
   #[inline]
   fn should_flush_immediately(level: &str) -> bool {
       level.eq_ignore_ascii_case("ERROR")
           || level.eq_ignore_ascii_case("CRITICAL")
           || level.eq_ignore_ascii_case("CRITICAL_PANIC")
           || level.eq_ignore_ascii_case("FATAL")
   }

   /// Flushes any buffered log entries held by the global log writer to disk.
   pub fn flush_logs() {
       let mut state = LOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
       state.flush();
   }

   /// Appends a structured log event using an optional explicit target path override.
   /// When `path_override` is `None`, resolves the standard application log path.
   pub(crate) fn log_event_to_path(level: &str, message: &str, path_override: Option<&Path>) {
       let timestamp = now_timestamp();
       let formatted = format!("[{timestamp}] [{level}] {message}\n");
       let immediate_flush = should_flush_immediately(level);

       let target_path = path_override
           .map(PathBuf::from)
           .or_else(|| resolve_log_paths("loganalysis.log").into_iter().next());

       let mut state = LOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
       safe_write_stderr(formatted.as_bytes());
       if let Some(path) = target_path {
           state.write_line(&path, &formatted, immediate_flush);
       }
   }

   /// Appends a structured log event to the persistent `loganalysis.log` file.
   pub fn log_event(level: &str, message: &str) {
       log_event_to_path(level, message, None);
   }

   #[cfg(test)]
   pub(crate) fn reset_log_writer_for_test() {
       let mut state = LOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
       state.close();
   }
   ```
   - Note: Keep `append_to_file(path: &Path, content: &str)` solely for one-shot `crash.log` report writes (and ensure `parent` dir creation if needed, already handled by `resolve_log_paths`).

5. **Panic Crash Handler Synchronization (`init_crash_handler` & `record_crash_report`):**
   - Extract testable helper:
     ```rust
     pub(crate) fn record_crash_report(
         crash_paths: &[PathBuf],
         log_paths: &[PathBuf],
         timestamp: &str,
         thread_name: &str,
         location: &str,
         payload: &str,
         report: &str,
     ) {
         for path in crash_paths {
             append_to_file(path, report);
         }

         let error_line = format!(
             "[{timestamp}] [CRITICAL_PANIC] Thread '{thread_name}' at {location}: {payload}\n"
         );

         let mut state = LOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
         // Flush any previously buffered events before appending and flushing CRITICAL_PANIC
         state.flush();
         for path in log_paths {
             state.write_line(path, &error_line, true);
         }
         state.flush();
     }
     ```
   - Update `init_crash_handler()` (`logging.rs:157-186`) to call `record_crash_report(&crash_paths, &resolve_log_paths("loganalysis.log"), ...)` and `safe_write_stderr(notice.as_bytes())` **before** calling `default_hook(panic_info)`.

6. **Clean Shutdown Flush (`crates/loganalysis-gui/src/lib.rs:613`):**
   - Immediately after `Application::new().run(move |cx: &mut App| { ... });` at `lib.rs:613`, call:
     ```rust
     logging::flush_logs();
     ```
     so buffered `INFO`/`WARN`/`CMD_PALETTE` entries are persisted on clean exit.

7. **Migration of Existing & New Tests in `logging.rs`:**
   - Guard all global `LOG_STATE` test cases (`test_safe_write_stderr_resilience`, `test_buffered_logging_defers_until_flush_and_flushes_errors_immediately`, `test_record_crash_report_flushes_buffered_logs_first`) with a shared `static LOG_TEST_MUTEX: Mutex<()> = Mutex::new(());` and an RAII `struct TestTempDir(PathBuf)` under `#[cfg(test)]` using `std::env::temp_dir()` (calling `reset_log_writer_for_test()` before `TestTempDir` removes the directory).
   - **`test_resolve_log_paths_non_empty` (`logging.rs:208-212`):** Verify `resolve_log_paths("test.log")` returns a non-empty `Vec` whose single element ends with `"test.log"` and is not under `std::env::current_dir()` when user env vars are set (without creating any files on disk).
   - **`test_resolve_log_paths_with_lookup_windows` (`logging.rs:257-275`):** Update assertions from `paths.len() == 2` (`[cwd, user]`) to **`paths.len() == 1`** and `paths == vec![expected_user_dir.join("app.log")]`, explicitly asserting `!paths.contains(&cwd.join("app.log"))`.
   - **`test_safe_write_stderr_resilience` (`logging.rs:278-283`):** Replace `log_event("TEST", ...)` at line 282 with `log_event_to_path("TEST", "Unit test message via safe_write_stderr", Some(&temp_log))`, followed by `flush_logs()` and `reset_log_writer_for_test()`, verifying `temp_log` receives the line and `cwd.join("loganalysis.log")` is untouched.
   - **New Deterministic Tests:**
     - `test_resolve_log_paths_with_lookup_cwd_fallback_when_no_env`
     - `test_buffered_logging_defers_until_flush_and_flushes_errors_immediately` (tests `LogWriterState`: `"INFO"` stays buffered until `flush()`, `"ERROR"` / `"CRITICAL"` flush immediately, switching paths flushes the prior path)
     - `test_record_crash_report_flushes_buffered_logs_first` (buffers `"INFO"`, invokes `record_crash_report`, asserts both `"INFO"` and `"[CRITICAL_PANIC]"` appear in chronological order on disk in `loganalysis.log` and full report in `crash.log`)
   - **Workspace Cleanup:** Remove the stale untracked `crates/loganalysis-gui/loganalysis.log` file and assert via verification gate that running `cargo test --workspace` creates **no** `loganalysis.log` anywhere in the repo tree.

---

### 2.2 `INF-01` — Extensible Log Parser SPI

**Target Files:**
- [crates/loganalysis-core/src/buffer/column.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/column.rs)
- [crates/loganalysis-core/src/buffer/mod.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/mod.rs)
- [crates/loganalysis-core/src/lib.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/lib.rs)
- *(Recommended Live Production Integration)* [crates/loganalysis-gui/src/components/log_view.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs#L998-L1005), [crates/loganalysis-gui/src/workspace/dock.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace/dock.rs#L21)

#### Problem & Backward-Compatibility Contract
`column.rs` currently hardcodes a single function `parse_logcat_column_line<'a>(line: &'a str) -> ParsedColumnLine<'a>` handling only threadtime logs with unstructured fallback. Callers cannot plug in additional log formats (e.g., Android `brief`, systemd journald, JSON logs) without modifying core code.

**Protected Contract Verification:**
- Every assertion in `column::tests` (`column.rs:130-262`), `challenger_m1_adversarial_suite.rs` (`L24-116`: prefix slicing `len < 18`, UTF-8/emoji boundary safety, `!parsed.message.is_empty()`, 10k fuzzer), and `gui_integration_tests.rs` (`L359, 369, 377, 565`) calls `parse_logcat_column_line(line)`.
- By keeping `parse_logcat_column_line` delegating **solely** to `ThreadtimeColumnParser` with unstructured fallback, 100% of existing `parse_logcat_column_line` behavior is preserved identically.

#### Exact Signatures in `crates/loganalysis-core/src/buffer/column.rs`

```rust
use crate::filter::LineSeverity;

/// Structured representation of a single log line parsed into tabular fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedColumnLine<'a> {
    pub time: Option<&'a str>,
    pub pid: Option<&'a str>,
    pub tid: Option<&'a str>,
    pub level_char: Option<char>,
    pub level_severity: LineSeverity,
    pub tag: Option<&'a str>,
    pub message: &'a str,
}

/// Extensible zero-copy Service Provider Interface (SPI) for structured log line parsing.
///
/// Implementations borrow slices directly from `line` without heap allocation, returning `Some`
/// when `line` matches their format or `None` so downstream parsers in a chain can inspect it.
pub trait LogFormatParser: Send + Sync {
    /// Stable format identifier (e.g. `"logcat-threadtime"`, `"logcat-brief"`).
    fn name(&self) -> &'static str;

    /// Attempts to parse `line` into structured columns. Returns `None` if unrecognized.
    fn parse<'a>(&self, line: &'a str) -> Option<ParsedColumnLine<'a>>;
}

/// Zero-copy parser for Android `logcat -v threadtime` lines:
/// `MM-DD HH:MM:SS.mmm  PID  TID L Tag: message`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadtimeColumnParser;

/// Zero-copy parser for Android `logcat -v brief` (`L/Tag(PID): message`) and tag-prefixed
/// (`L/Tag: message`) lines.
#[derive(Debug, Clone, Copy, Default)]
pub struct BriefColumnParser;

/// Shared static instance of [`ThreadtimeColumnParser`].
pub static THREADTIME_COLUMN_PARSER: ThreadtimeColumnParser = ThreadtimeColumnParser;

/// Shared static instance of [`BriefColumnParser`].
pub static BRIEF_COLUMN_PARSER: BriefColumnParser = BriefColumnParser;

/// Default parser fallback chain evaluated by [`parse_column_line_auto`].
pub static DEFAULT_COLUMN_PARSERS: [&dyn LogFormatParser; 2] = [
    &THREADTIME_COLUMN_PARSER,
    &BRIEF_COLUMN_PARSER,
];
```

**Helper & Entrypoint Functions:**
```rust
#[inline]
fn unstructured_column_line<'a>(line: &'a str) -> ParsedColumnLine<'a> {
    ParsedColumnLine {
        time: None,
        pid: None,
        tid: None,
        level_char: None,
        level_severity: LineSeverity::Normal,
        tag: None,
        message: line,
    }
}

/// Attempts each parser in `parsers` in order, returning the first successful match
/// or an unstructured fallback containing the original `line` as `message`.
pub fn parse_column_line_with_parsers<'a>(
    line: &'a str,
    parsers: &[&dyn LogFormatParser],
) -> ParsedColumnLine<'a> {
    for parser in parsers {
        if let Some(parsed) = parser.parse(line) {
            return parsed;
        }
    }
    unstructured_column_line(line)
}

/// Parses `line` using [`DEFAULT_COLUMN_PARSERS`] (`threadtime` then `brief`),
/// falling back to an unstructured line when no parser matches.
pub fn parse_column_line_auto<'a>(line: &'a str) -> ParsedColumnLine<'a> {
    parse_column_line_with_parsers(line, &DEFAULT_COLUMN_PARSERS)
}

/// Zero-copy fast parser for Android logcat (`threadtime`) and tabular logs.
///
/// Preserves exact `threadtime`-only semantics with unstructured fallback for backward
/// compatibility with existing callers; use [`parse_column_line_auto`] or
/// [`parse_column_line_with_parsers`] for extensible multi-format parsing.
pub fn parse_logcat_column_line<'a>(line: &'a str) -> ParsedColumnLine<'a> {
    THREADTIME_COLUMN_PARSER
        .parse(line)
        .unwrap_or_else(|| unstructured_column_line(line))
}
```

#### `BriefColumnParser::parse` Exact Parsing Rules & Edge Cases
1. `let trimmed = line.trim_start();`
2. Verify `let bytes = trimmed.as_bytes();` has `bytes.len() >= 3`, `matches!(bytes[0], b'V' | b'D' | b'I' | b'W' | b'E' | b'F')`, and `bytes[1] == b'/'`. (Rejects lowercase paths like `d/foo: bar`, invalid levels like `X/Tag: msg`, and short strings.)
3. Because bytes `0..2` are pure ASCII, `let after_slash = &trimmed[2..];` is guaranteed to start on a valid UTF-8 char boundary.
4. Locate the first `':'` in `after_slash` (`let colon_pos = after_slash.find(':')?;`).
5. Split `before_colon = &after_slash[..colon_pos]` and `after_colon = &after_slash[colon_pos + 1..]`.
6. Inspect `let header = before_colon.trim_end();`:
   - If `header.ends_with(')')`, search for `rfind('(')`:
     - Let `inner = header[open_idx + 1..header.len() - 1].trim()`.
     - If `!inner.is_empty() && inner.bytes().all(|b| b.is_ascii_digit())`, set `tag_raw = header[..open_idx].trim()` and `pid = Some(inner)`.
     - Otherwise (non-digit parenthesized suffix like `I/Audio(v2): msg`), treat `header.trim()` as `tag_raw` and `pid = None`.
   - Else set `tag_raw = header.trim()` and `pid = None`.
7. If `tag_raw.is_empty()`, return `None` (rejects malformed `"I/(123): msg"` or `"I/: msg"`).
8. Map severity identically to `ThreadtimeColumnParser`:
   - `b'E' | b'F'` -> `LineSeverity::Error`
   - `b'W'` -> `LineSeverity::Warning`
   - `_` -> `LineSeverity::Normal`
9. Return `Some(ParsedColumnLine { time: None, pid, tid: None, level_char: Some(bytes[0] as char), level_severity, tag: Some(tag_raw), message: after_colon.trim_start() })`.

#### Re-exports & GUI Call-Site Integration
- **`crates/loganalysis-core/src/buffer/mod.rs:11`:**
  ```rust
  pub use column::{
      parse_column_line_auto, parse_column_line_with_parsers, parse_logcat_column_line,
      BriefColumnParser, LogFormatParser, ParsedColumnLine, ThreadtimeColumnParser,
      BRIEF_COLUMN_PARSER, DEFAULT_COLUMN_PARSERS, THREADTIME_COLUMN_PARSER,
  };
  ```
- **`crates/loganalysis-core/src/lib.rs:14-16`:**
  Re-export those same symbols from `buffer::column::*` alongside `parse_logcat_column_line` and `ParsedColumnLine`.
- **`crates/loganalysis-gui/src/components/log_view.rs:999-1000` & `crates/loganalysis-gui/src/workspace/dock.rs:21`:**
  - In `log_view.rs:999-1000`: Switch `parse_logcat_column_line(text.as_ref())` to `parse_column_line_auto(text.as_ref())`, and change line 1000 from `if let Some(time_str) = parsed.time` to `if parsed.time.is_some() || parsed.level_char.is_some() { let time_str = parsed.time.unwrap_or("-"); ... }` so `brief` logs render cleanly in Structured Column View (`-` in the Time column, severity badge, `pid/-`, colored `tag`, `message`) while stack traces (`"\tat ..."`) continue taking the fallback indentation branch.
  - In `workspace/dock.rs:21`: Switch `parse_logcat_column_line(&line)` to `parse_column_line_auto(&line)` so forensic facet tag sampling works for both `threadtime` and `brief` log files.

---

### 2.3 `[INV-09]` — Excluded-Line Trim Counter Parity

**Target Files:**
- [crates/loganalysis-core/src/filter/engine.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/filter/engine.rs)
- [crates/loganalysis-core/tests/engine_tests.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/tests/engine_tests.rs)

#### Root Cause & Invariant Violation
In `engine.rs`:
1. `evaluate_and_style_line_with_matcher_full` (`engine.rs:120-129`) increments `filter_counts[flat_idx]` for **all** matched rules (including `exclude = true` rules and any earlier include rules on the same line) **before** `resolve_style_from_matches` sets `is_visible = false`.
2. Both `execute_filtering` (`engine.rs:262`) and `append_streaming_batch_with_matcher` (`engine.rs:463`) only push lines to `visible_indices`, `visible_styles`, and `visible_filter_matches` when `is_visible == true`.
3. On ring-buffer eviction (`trimmed_count > 0`, `engine.rs:399-440`):
   - Matches from trimmed excluded lines are absent from `visible_filter_matches`, so their `filter_counts` / `rule_counts` remain inflated relative to `execute_filtering` on the trimmed buffer.
   - Worse, the guard `if trimmed_count > 0 && !result.visible_indices.is_empty()` (`engine.rs:399`) skips the entire trim block if `visible_indices` happens to be empty, leaving stale excluded-line counts behind even when all lines are drained or only excluded lines exist.

#### Exact Changes in `crates/loganalysis-core/src/filter/engine.rs`

1. **Extend `FilterResult` (`engine.rs:16-26`):**
   ```rust
   #[derive(Clone, Debug, Default)]
   pub struct FilterResult {
       pub visible_indices: Vec<usize>,
       pub visible_styles: Vec<ResolvedStyle>,
       pub visible_filter_matches: Vec<SmallVec<[usize; 2]>>,
       /// Chronologically ordered `(source_line_idx, matched_flat_filter_indices)` for
       /// exclude-suppressed lines whose matches incremented `filter_counts` (`[INV-09]`).
       #[doc(hidden)]
       pub excluded_filter_matches: Vec<(usize, SmallVec<[usize; 2]>)>,
       pub filter_counts: Vec<usize>,
       pub rule_counts: std::collections::HashMap<uuid::Uuid, usize>,
       pub total_matched_lines: usize,
       pub widest_visible_text: String,
       pub widest_visible_length: usize,
   }
   ```
   - Update both `FilterResult { ... }` literals in `engine.rs:216-225` (`excluded_filter_matches: Vec::new()`) and `engine.rs:334-343` (`excluded_filter_matches: combined_excluded_matches`). All 7 `FilterResult::default()` usages across tests continue compiling unchanged.

2. **Populate `excluded_filter_matches` in `execute_filtering` (`engine.rs:244-316`):**
   - Inside each Rayon parallel chunk (`start..end`):
     - Allocate `let mut excluded_filter_matches: Vec<(usize, SmallVec<[usize; 2]>)> = Vec::new();`.
     - Inside the per-line loop (`engine.rs:262-273`):
       ```rust
       if is_visible {
           visible_indices.push(line_idx);
           visible_styles.push(style);
           visible_filter_matches.push(flat_matches);

           let len = raw_line.len();
           if len > local_max_len {
               local_max_len = len;
               local_longest_idx = Some(line_idx);
           }
       } else if !flat_matches.is_empty() {
           excluded_filter_matches.push((line_idx, flat_matches));
       }
       ```
     - Include `excluded_filter_matches` in the chunk tuple and extend `combined_excluded_matches` sequentially in chunk order (`0..num_chunks` collected by Rayon preserves chunk index order, and each chunk iterates `start..end` ascending, guaranteeing `combined_excluded_matches` is strictly ascending by `line_idx`).

3. **Decoupled Visible & Excluded Trim + Shift in `append_streaming_batch_with_matcher` (`engine.rs:399-475`):**
   - Replace line 399–440 trim block with:
     ```rust
     if trimmed_count > 0 {
         if !result.visible_indices.is_empty() {
             let drop_count = result.visible_indices.partition_point(|&idx| idx < trimmed_count);
             if drop_count > 0 {
                 if show_only_filtered {
                     result.total_matched_lines =
                         result.total_matched_lines.saturating_sub(drop_count);
                 } else {
                     let dropped_matches = result
                         .visible_filter_matches
                         .get(..drop_count)
                         .map(|slice| slice.iter().filter(|m| !m.is_empty()).count())
                         .unwrap_or_else(|| {
                             result
                                 .visible_filter_matches
                                 .iter()
                                 .filter(|m| !m.is_empty())
                                 .count()
                         });
                     result.total_matched_lines =
                         result.total_matched_lines.saturating_sub(dropped_matches);
                 }

                 // Exact per-rule counter decrement for visible trimmed lines [INV-09]
                 if drop_count <= result.visible_filter_matches.len() {
                     for matches in result.visible_filter_matches.drain(0..drop_count) {
                         for flat_idx in matches {
                             if flat_idx < result.filter_counts.len() {
                                 result.filter_counts[flat_idx] =
                                     result.filter_counts[flat_idx].saturating_sub(1);
                             }
                         }
                     }
                 } else {
                     result.visible_filter_matches.clear();
                 }

                 result.visible_indices.drain(0..drop_count);
                 result.visible_styles.drain(0..drop_count);
             }
             for idx in &mut result.visible_indices {
                 *idx = idx.saturating_sub(trimmed_count);
             }
         }

         // Exact per-rule counter decrement for excluded trimmed lines [INV-09]
         // Runs unconditionally even when `visible_indices` is empty.
         if !result.excluded_filter_matches.is_empty() {
             let drop_excluded = result
                 .excluded_filter_matches
                 .partition_point(|(idx, _)| *idx < trimmed_count);
             if drop_excluded > 0 {
                 for (_, matches) in result.excluded_filter_matches.drain(0..drop_excluded) {
                     for flat_idx in matches {
                         if flat_idx < result.filter_counts.len() {
                             result.filter_counts[flat_idx] =
                                 result.filter_counts[flat_idx].saturating_sub(1);
                         }
                     }
                 }
             }
             for (idx, _) in &mut result.excluded_filter_matches {
                 *idx = idx.saturating_sub(trimmed_count);
             }
         }
     }
     ```
   - In the append loop (`engine.rs:463-475`), add the `else if` branch:
     ```rust
     } else if !flat_matches.is_empty() {
         result.excluded_filter_matches.push((line_idx, flat_matches));
     }
     ```
   - **Invariants Verified:**
     - Excluded lines never increment `total_matched_lines` on append (`if is_matched && is_visible`), so draining `excluded_filter_matches` MUST NOT decrement `total_matched_lines`.
     - Unmatched lines when `show_only_filtered == true` yield `flat_matches.is_empty()` and are **not** stored in `excluded_filter_matches` (zero memory overhead).
     - `CompiledStreamFilters::append_streaming_batch` (`engine.rs:558-560`) immediately resyncs `rule_counts` from `result.filter_counts`, giving exact parity across both `filter_counts` and `rule_counts`.

4. **Test Updates in `crates/loganalysis-core/tests/engine_tests.rs`:**
   - In `test_compiled_stream_filters_parity_with_append_streaming_batch` Step 4 (`trim-excluded`, `engine_tests.rs:1384-1416`): compute `let full_4 = execute_filtering(&drop_excl_stream, &rules, show_only_filtered, None, 0).expect("full filtering phase 4");` and assert `assert_incremental_eq(&compiled_res, &full_4, false, "trim-excluded compiled vs full");`!
   - Also assert `a.excluded_filter_matches == b.excluded_filter_matches` inside `assert_incremental_eq` (so `compiled_res`, `standalone_res`, and `full_1..4` verify identical internal tracking too).
   - Add `test_excluded_line_trim_counter_parity_when_visible_empty` covering:
     1. Buffer populated exclusively with excluded lines (`show_only_filtered = true` -> `visible_indices.is_empty()`),
     2. Partial trim retaining shifted excluded lines (verifies exact `filter_counts`, `rule_counts`, and full `execute_filtering` parity),
     3. Second partial/full trim draining remaining shifted excluded lines to `0`.

---

### 2.4 Non-Blocking Child Reap in `LogcatStreamHandle::stop`

**Target File:**
- [crates/loganalysis-core/src/adb/logcat.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/logcat.rs)

#### Root Cause
`LogcatStreamHandle::stop` (`logcat.rs:22-29`) kills the child and immediately calls synchronous blocking `child.wait()` on the calling thread—including when `Drop` runs on the GPUI UI thread (`workspace/adb.rs:508`) or inside `test_stress_handle_drop_safety` (`< 50ms` budget). Under Linux scheduler / adb teardown contention, `wait()` caused `test_stress_handle_drop_safety` to flake (`1.51s >= 50ms`).

#### Exact Changes in `crates/loganalysis-core/src/adb/logcat.rs`

1. **Add `reap_child_nonblocking(mut child: Child)`:**
   ```rust
   /// Terminates `child` and reaps its exit status without blocking the calling thread.
   ///
   /// Fast-paths via `try_wait()` when the process exits immediately after `kill()`,
   /// otherwise hands ownership to a detached `"adb-child-reaper"` background thread.
   /// Falls back to synchronous `wait()` only if spawning the reaper thread fails.
   fn reap_child_nonblocking(mut child: Child) {
       let _ = child.kill();
       match child.try_wait() {
           Ok(Some(_)) => {}
           Ok(None) | Err(_) => {
               let child_slot = Arc::new(Mutex::new(Some(child)));
               let reaper_slot = Arc::clone(&child_slot);
               if thread::Builder::new()
                   .name("adb-child-reaper".to_string())
                   .spawn(move || {
                       let child = {
                           let mut lock = reaper_slot.lock().unwrap_or_else(|e| e.into_inner());
                           lock.take()
                       };
                       if let Some(mut c) = child {
                           let _ = c.wait();
                       }
                   })
                   .is_err()
               {
                   let child = {
                       let mut lock = child_slot.lock().unwrap_or_else(|e| e.into_inner());
                       lock.take()
                   };
                   if let Some(mut c) = child {
                       let _ = c.wait();
                   }
               }
           }
       }
   }
   ```
   *(Design Note: Storing `child` in `Arc<Mutex<Option<Child>>>` before `Builder::spawn` ensures `child` is not dropped un-reaped if `spawn` returns `Err(io::Error)` due to OS thread-limit exhaustion.)*

2. **Update `LogcatStreamHandle::stop` (`logcat.rs:22-29`):**
   ```rust
   /// Signals the background thread and terminates the child process without blocking
   /// the caller/UI thread while the OS reaps the subprocess.
   pub fn stop(&self) {
       self.stop_flag.store(true, Ordering::SeqCst);
       let child = {
           let mut lock = self.child.lock().unwrap_or_else(|e| e.into_inner());
           lock.take()
       };
       if let Some(child) = child {
           reap_child_nonblocking(child);
       }
   }
   ```
   - Dropping `lock` before calling `reap_child_nonblocking(child)` ensures `self.child` mutex is never held across OS syscalls or thread spawning.

3. **Lifecycle & Deadlock Verification (`start_logcat_stream` & `join`):**
   - `start_logcat_stream` error path (`logcat.rs:100-108`) and worker cleanup path (`logcat.rs:120-124`) execute strictly on the background spawn/worker thread: if `stop()` did not already `take()` `child`, the worker thread owns `child`, calls `kill()` + `wait()`, and leaves zero zombies.
   - `LogcatStreamHandle::join` (`logcat.rs:32-39`) calls `self.stop()` (killing child & closing stdout pipe immediately) and joins `self.thread_handle`; the worker never waits on `"adb-child-reaper"`, preventing any circular wait or deadlock.
   - Add unit tests in `logcat::tests`:
     - `test_stop_with_live_child_is_nonblocking_and_clears_slot` (`#[cfg(unix)]`, spawns `sh -c "sleep 30"` with `Stdio::piped()`, asserts `handle.stop()` clears `child` slot, sets `stop_flag`, and completes in `< 50ms`; on Linux `#[cfg(target_os = "linux")]` polls `/proc/{pid}/status` with `thread::yield_now()` up to 2s to verify zero zombie state without any `thread::sleep`).
     - `test_reap_child_nonblocking_already_exited` (verifies `try_wait()` fast path on exited process).

---

### 2.5 `INF-06` — Explicit Registry Allowlist in `deny.toml`

**Target File:**
- [deny.toml](file:///home/drew/github/hwanjongyu/loganalysis/deny.toml#L28-L30)

#### Exact Change
Update `[sources]` (`deny.toml:28-30`) to:
```toml
[sources]
unknown-registry = "deny"
unknown-git = "warn"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
allow-git = []
```

---

### 2.6 Documented External Deferrals (`MIN-12`, `INF-05`)

No source modifications are performed for these external upstream dependencies; both are formally tracked with active production mitigations and objective closure triggers:

1. **`MIN-12` — Pre-Release `gpui = "0.2.2"` Dependency Risk**
   - **Location:** [crates/loganalysis-gui/Cargo.toml:10](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/Cargo.toml#L10) (`gpui = "0.2.2"`).
   - **Deferral Rationale:** Upgrading to a stable GPUI release is gated by upstream `crates.io` publication and introduces breaking API changes across GPUI windowing/element traits without an available `0.2.x` patch fixing X11/Win32 `raw-window-handle` gaps.
   - **Active In-Repo Mitigations:**
     - `XMODIFIERS` sanitization (`crates/loganalysis-gui/src/lib.rs:554-562`) prevents `zed-xim` `PreeditDraw UnsupportedEncoding` panics under Linux X11 CJK IMEs (opt-out via `LOGANALYSIS_KEEP_XMODIFIERS`).
     - `detect_initial_window_decorations()` (`crates/loganalysis-gui/src/lib.rs:507-536`) enables `WindowDecorations::Client` automatically on Wayland compositors.
     - `CustomX11WindowHandle` (`crates/loganalysis-gui/src/dialogs.rs:15-33, 70-120`) avoids calling GPUI 0.2.2's `unimplemented!()` `X11Window::window_handle()` on Linux X11.
     - `SafeDialogParent` (`crates/loganalysis-gui/src/dialogs.rs:35-66, 98-107`) supplies a valid `WindowsDisplayHandle` so `rfd` does not trigger GPUI 0.2.2's `unimplemented!()` `WindowsWindow::display_handle()` panic inside Win32 `CallWindowProcW`.
   - **Closure Trigger:** Upgrade when a stable `gpui` release implements both `X11Window::window_handle()` and `WindowsWindow::display_handle()` cleanly and fixes `zed-xim` compound-text handling, verified via X11/Wayland/macOS/Windows smoke tests before removing shims.

2. **`INF-05` — Future-Incompatibility Notice in `proc-macro-error2 v2.0.1`**
   - **Location:** [Cargo.lock:3935-3940](file:///home/drew/github/hwanjongyu/loganalysis/Cargo.lock#L3935-L3940) (`gpui 0.2.2` -> `stacksafe 0.1.4` -> `stacksafe-macro 0.1.4` -> `proc-macro-error2 v2.0.1`).
   - **Deferral Rationale:** `proc-macro-error2` is purely transitive through `gpui 0.2.2`; forking/patching `stacksafe-macro` or `proc-macro-error2` locally via `[patch.crates-io]` creates unwarranted supply-chain maintenance for a non-error compiler future-incompatibility notice (`cargo check` and `cargo clippy -- -D warnings` pass with exit `0`).
   - **Closure Trigger:** Update via `cargo update -p proc-macro-error2` / `stacksafe-macro` once upstream removes the private `proc_macro` re-export, or resolve alongside the `MIN-12` GPUI upgrade.

---

## 3. Deterministic Test Verification Matrix & Global Gates

All new and updated tests MUST be deterministic with **strictly zero `thread::sleep`** calls and **zero writes to repository working directories**.

| Item ID | Target File(s) | Deterministic Test / Verification Case | Exact Assertions & Pass Criteria |
|---|---|---|---|
| `MIN-03` | `crates/loganalysis-gui/src/logging.rs`, `src/lib.rs` | `test_resolve_log_paths_non_empty`, `test_resolve_log_paths_with_lookup_windows`, `test_resolve_log_paths_with_lookup_cwd_fallback_when_no_env`, `test_safe_write_stderr_resilience`, `test_buffered_logging_defers_until_flush_and_flushes_errors_immediately`, `test_record_crash_report_flushes_buffered_logs_first` | User dir env resolves **only** 1 path (`user_share.join(filename)`), never `cwd`; `cwd` used **only** when user env lookup returns `None`; `LogWriterState` buffers `"INFO"` until `flush()`, flushes `"ERROR"`/`"CRITICAL"` immediately, flushes on path switch, and `record_crash_report` flushes prior buffered entries before `[CRITICAL_PANIC]`; `crates/loganalysis-gui/loganalysis.log` is absent before and after `cargo test --workspace`. |
| `INF-01` | `crates/loganalysis-core/src/buffer/{column.rs,mod.rs}`, `src/lib.rs` (+ `log_view.rs`, `dock.rs`) | Existing 10 `column::tests` + 3 `challenger_m1_adversarial_suite` tests + new tests: `test_brief_column_parser_with_and_without_pid`, `test_brief_column_parser_rejects_malformed_and_utf8_safe`, `test_parse_column_line_with_parsers_custom_spi`, `test_parse_column_line_auto_mixed_formats` | All existing `parse_logcat_column_line` assertions remain 100% unchanged; `BriefColumnParser` extracts level, severity, tag, optional PID, and message across `L/Tag(PID): msg` and `L/Tag: msg`; custom `LogFormatParser` implementation works via `&[&dyn LogFormatParser]` without core edits; `DEFAULT_COLUMN_PARSERS` / `parse_column_line_auto` handles mixed threadtime + brief + stack traces cleanly. |
| `[INV-09]` | `crates/loganalysis-core/src/filter/engine.rs`, `tests/engine_tests.rs` | Updated `test_compiled_stream_filters_parity_with_append_streaming_batch` (Step 4 full `execute_filtering` parity) + new `test_excluded_line_trim_counter_parity_when_visible_empty` + `test_excluded_line_partial_trim_and_shift_parity` | Step 4 (`trim-excluded`) matches `execute_filtering` across all 6 non-watermark fields (`visible_indices`, `visible_styles`, `visible_filter_matches`, `filter_counts`, `rule_counts`, `total_matched_lines`) + `excluded_filter_matches`; trimming excluded lines when `visible_indices` is empty accurately decrements `filter_counts` and `rule_counts` down to `0` and shifts surviving excluded line indices across consecutive trims. |
| Non-blocking `stop` | `crates/loganalysis-core/src/adb/logcat.rs`, `tests/stream_robustness_stress.rs`, `tests/adb_logcat_adversarial_tests.rs` | Existing drop/join latency tests + new `test_stop_with_live_child_is_nonblocking_and_clears_slot` & `test_reap_child_nonblocking_already_exited` | `LogcatStreamHandle::stop()` returns in `< 50ms` on a live 30s subprocess, sets `stop_flag`, empties `child` slot, leaves zero zombie processes (`yield_now` bounded poll on Linux `/proc/{pid}/status`, zero `thread::sleep`), and passes `test_stress_handle_drop_safety`. |
| `INF-06` | `deny.toml` | TOML syntax + key validation (`python3` `tomllib`) & `cargo deny check` (if installed locally or on CI) | `[sources]` contains `unknown-registry = "deny"`, `unknown-git = "warn"`, `allow-registry = ["https://github.com/rust-lang/crates.io-index"]`, and `allow-git = []`; `Cargo.lock` contains only local packages and `crates.io-index` registry packages. |

### 3.1 Global Verification Gate Commands

Downstream `test-automation-engineer` and `reality-checker` MUST execute and verify exit status `0` on **all** gates below after the final edit:

```bash
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test --workspace
```

Plus structural/hygiene assertions:
```bash
# 1. Verify MIN-03 left zero loganalysis.log pollution in repo checkout after cargo test
test ! -e loganalysis.log && test ! -e crates/loganalysis-gui/loganalysis.log

# 2. Verify MIN-03 removed dead_code allow on resolve_user_log_dir
grep -n "allow(dead_code)" crates/loganalysis-gui/src/logging.rs   # expects 0 matches (exit 1)

# 3. Verify zero thread::sleep added in Phase 4 code/tests (only pre-existing stress tests)
grep -rn "thread::sleep" crates/

# 4. Validate deny.toml & .github/workflows/ci.yml syntax + [sources] allowlist
python3 -c 'import tomllib; d = tomllib.load(open("deny.toml","rb")); assert d["sources"]["allow-registry"] == ["https://github.com/rust-lang/crates.io-index"]; assert d["sources"]["allow-git"] == []'
```

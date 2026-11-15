# QA Verification Report — Phase 4 Backlog & QA Refinement

**Workspace:** `/home/drew/github/hwanjongyu/loganalysis`  
**Specification:** [docs/spec-phase4-backlog-remediation.md](file:///home/drew/github/hwanjongyu/loganalysis/docs/spec-phase4-backlog-remediation.md)  
**QA Loop Status:** **Attempt 1/3 — Audited & Certified**  
**Overall Result:** **PASS (Certified by `reality-checker`)**

---

## Executive Summary

Phase 4 remediation implemented by `backend-architect` (`MIN-03`, `INF-01`, `[INV-09]`, non-blocking child reap in `LogcatStreamHandle::stop`, and `INF-06`) was verified against deterministic, zero-`thread::sleep` automated tests added and updated by `test-automation-engineer`:

- **11 newly added deterministic tests** (`8` in `loganalysis-core`, `3` in `loganalysis-gui`) plus **1 re-enabled/expanded full-parity test** (`test_compiled_stream_filters_parity_with_append_streaming_batch` Step 4 `trim-excluded` in `engine_tests.rs`) and updated isolated `logging.rs` tests — all passing cleanly with zero `thread::sleep` calls added and zero `loganalysis.log` files created in the repository checkout.
- `cargo check --workspace --all-targets`: **PASS** (exit `0`, `5.21s`)
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** (exit `0`, `4.24s`, zero warnings)
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`: **PASS** (exit `0`, `5.50s`, zero rustdoc warnings)
- `cargo test --workspace`: **PASS** (exit `0`, compile+run `1m 06s` — **295 passed; 0 failed; 0 ignored; 0 measured** across 13 test targets and 2 doc-test targets)
- All Phase 4 hygiene and config gates (`loganalysis.log` absence, `logging.rs` `allow(dead_code)` removal, `thread::sleep` baseline check, `deny.toml` / `Cargo.lock` / `.github/workflows/ci.yml` validation): **PASS** (`cargo-deny` / `cargo-audit` binaries are not installed locally and remain CI-enforced on `ubuntu-22.04`).

---

## Added & Updated Deterministic Tests (Phase 4 QA Loop — Attempt 1/3)

### `loganalysis-core` (+8 new tests, +1 updated parity test)

1. `buffer::column::tests::test_brief_column_parser_with_and_without_pid` — [crates/loganalysis-core/src/buffer/column.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/column.rs) — **PASS**
   - Verifies `THREADTIME_COLUMN_PARSER.name()` (`"logcat-threadtime"`), `BRIEF_COLUMN_PARSER.name()` (`"logcat-brief"`), `L/Tag(PID): msg` with padded/compact PIDs, spaces in tags, `F/` mapping to `LineSeverity::Error`, `V`/`D`/`I` without PID, non-digit parenthesized suffixes (`D/Audio(v2): ...`), and empty messages.
2. `buffer::column::tests::test_brief_column_parser_rejects_malformed_and_utf8_safe` — [crates/loganalysis-core/src/buffer/column.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/column.rs) — **PASS**
   - Verifies rejection of lowercase levels (`d/Tag: ...`, `i/usr/bin: ...`), invalid levels (`X/Tag(10): ...`), missing slash/colon, empty tags (`I/: ...`, `I/(123): ...`), multi-byte UTF-8 CJK/emoji safety across all char-boundary prefixes, and backward compatibility of `parse_logcat_column_line`.
3. `buffer::column::tests::test_parse_column_line_with_parsers_custom_spi` — [crates/loganalysis-core/src/buffer/column.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/column.rs) — **PASS**
   - Exercises a custom `LogFormatParser` implementation via `&[&dyn LogFormatParser]` verifying zero-copy pointer-range borrowing, fallback through custom -> `ThreadtimeColumnParser` -> `BriefColumnParser`, and unstructured fallback on empty `&[]`.
4. `buffer::column::tests::test_parse_column_line_auto_mixed_formats` — [crates/loganalysis-core/src/buffer/column.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/column.rs) — **PASS**
   - Verifies `DEFAULT_COLUMN_PARSERS` order and `parse_column_line_auto` across mixed `threadtime`, `brief` with PID, `brief` without PID, Java stack trace lines, and plain unstructured lines.
5. `test_compiled_stream_filters_parity_with_append_streaming_batch` *(Updated Step 4 + `excluded_filter_matches` check)* — [crates/loganalysis-core/tests/engine_tests.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/tests/engine_tests.rs) — **PASS**
   - Extended `assert_incremental_eq` to assert `a.excluded_filter_matches == b.excluded_filter_matches` and re-enabled Step 4 (`trim-excluded`) full `execute_filtering` parity across `compiled_res`, `standalone_res`, and `full_4` for both `show_only_filtered = true` and `false`.
6. `test_excluded_line_trim_counter_parity_when_visible_empty` — [crates/loganalysis-core/tests/engine_tests.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/tests/engine_tests.rs) — **PASS**
   - Populates buffer exclusively with `exclude = true` matched lines (`visible_indices.is_empty()`), executes consecutive partial trims and a final full drain to `0`, and verifies exact parity of `filter_counts`, `rule_counts`, `excluded_filter_matches`, and `total_matched_lines` against `execute_filtering`.
7. `test_excluded_line_partial_trim_and_shift_parity` — [crates/loganalysis-core/tests/engine_tests.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/tests/engine_tests.rs) — **PASS**
   - Tests interleaved unmatched, visible-matched, and excluded-matched lines across consecutive rolling `StreamLogBuffer::trim_if_needed()` batches, trim-only shrinking to visible-empty, and final drain for both `show_only_filtered = true` and `false` across `CompiledStreamFilters`, `append_streaming_batch`, and `execute_filtering`.
8. `adb::logcat::tests::test_stop_with_live_child_is_nonblocking_and_clears_slot` — [crates/loganalysis-core/src/adb/logcat.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/logcat.rs) — **PASS**
   - `#[cfg(unix)]`, spawns `sh -c "exec sleep 30"`, asserts `LogcatStreamHandle::stop()` returns in `< 50ms`, sets `stop_flag`, empties `child` slot, and on Linux polls `/proc/{pid}/status` with `thread::yield_now()` (bounded `2s` deadline, zero `thread::sleep`) to confirm zero zombie state.
9. `adb::logcat::tests::test_reap_child_nonblocking_already_exited` — [crates/loganalysis-core/src/adb/logcat.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/logcat.rs) — **PASS**
   - Deterministically waits for an exit-0 subprocess via `try_wait()` + `thread::yield_now()` (and on Linux also tests an unreaped OS zombie `State:\tZ` via `/proc/{pid}/status` before any `try_wait()`), asserting `reap_child_nonblocking` completes in `< 50ms` and leaves zero `/proc/{pid}/status` entry.

### `loganalysis-gui` (+3 new tests; isolated `logging.rs` suite)

10. `logging::tests::test_resolve_log_paths_with_lookup_cwd_fallback_when_no_env` — [crates/loganalysis-gui/src/logging.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/logging.rs) — **PASS**
    - Verifies `cwd` fallback applies only when all user env vars are unset or empty, without invoking `create_dir`, and returns empty when both env and `cwd` are absent.
11. `logging::tests::test_buffered_logging_defers_until_flush_and_flushes_errors_immediately` — [crates/loganalysis-gui/src/logging.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/logging.rs) — **PASS**
    - Guarded by `LOG_TEST_MUTEX`, `TestTempDir`, and `ResetLogWriterOnDrop`; verifies `INFO` remains buffered until `flush_logs()`, `ERROR`/`CRITICAL` flush immediately, switching paths flushes the prior writer, and `LogWriterState` transitions cleanly without touching `cwd/loganalysis.log`.
12. `logging::tests::test_record_crash_report_flushes_buffered_logs_first` — [crates/loganalysis-gui/src/logging.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/logging.rs) — **PASS**
    - Guarded by `LOG_TEST_MUTEX`, `TestTempDir`, and `ResetLogWriterOnDrop`; buffers an `INFO` entry, invokes `record_crash_report`, and verifies `crash.log` receives the report while `loganalysis.log` receives the buffered `INFO` line chronologically before `[CRITICAL_PANIC]`.

*(Pre-existing updated `MIN-03` tests in [crates/loganalysis-gui/src/logging.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/logging.rs) — `test_resolve_log_paths_non_empty`, `test_resolve_log_paths_with_lookup_windows`, and `test_safe_write_stderr_resilience` — also verified passing with temp-dir isolation and `LOG_TEST_MUTEX` + `ResetLogWriterOnDrop`.)*

---

## Section 3 Test Matrix Audit

| Item ID | Target File(s) | Verification Cases | Observed Result | Notes |
|---|---|---|---|---|
| `MIN-03` | `crates/loganalysis-gui/src/logging.rs`, `src/lib.rs` | `test_resolve_log_paths_non_empty`, `test_resolve_log_paths_with_lookup_windows`, `test_resolve_log_paths_with_lookup_cwd_fallback_when_no_env`, `test_safe_write_stderr_resilience`, `test_buffered_logging_defers_until_flush_and_flushes_errors_immediately`, `test_record_crash_report_flushes_buffered_logs_first` | **PASS** | `63/63` `loganalysis-gui` unit tests pass; `#[allow(dead_code)]` absent from `logging.rs`; zero `loganalysis.log` files in repo root or `crates/loganalysis-gui/`. |
| `INF-01` | `crates/loganalysis-core/src/buffer/{column.rs,mod.rs}`, `src/lib.rs`, `log_view.rs`, `dock.rs` | 10 existing `column::tests` + 3 `challenger_m1_adversarial_suite` + 4 new `column::tests` (`test_brief_column_parser_with_and_without_pid`, `test_brief_column_parser_rejects_malformed_and_utf8_safe`, `test_parse_column_line_with_parsers_custom_spi`, `test_parse_column_line_auto_mixed_formats`) | **PASS** | All `parse_logcat_column_line` backward-compatibility tests and `gui_integration_tests` (`20/20`) pass alongside new SPI and `BriefColumnParser` coverage. |
| `[INV-09]` | `crates/loganalysis-core/src/filter/engine.rs`, `tests/engine_tests.rs` | Updated `test_compiled_stream_filters_parity_with_append_streaming_batch` (Step 4 + `excluded_filter_matches`) + `test_excluded_line_trim_counter_parity_when_visible_empty` + `test_excluded_line_partial_trim_and_shift_parity` | **PASS** | `43/43` `engine_tests` pass; full parity verified across `filter_counts`, `rule_counts`, `excluded_filter_matches`, `visible_indices`, `visible_styles`, `visible_filter_matches`, and `total_matched_lines`. |
| Non-blocking `stop` | `crates/loganalysis-core/src/adb/logcat.rs`, `tests/stream_robustness_stress.rs`, `tests/adb_logcat_adversarial_tests.rs` | `test_stop_with_live_child_is_nonblocking_and_clears_slot`, `test_reap_child_nonblocking_already_exited`, `test_stress_handle_drop_safety` (`18/18`), `adb_logcat_adversarial_tests` (`7/7`) | **PASS** | `< 50ms` caller return verified on live `exec sleep 30` and exited/zombie child processes; zero zombie `/proc/{pid}/status` entries remain; zero `thread::sleep` added. |
| `INF-06` | `deny.toml`, `Cargo.lock`, `.github/workflows/ci.yml` | Python `tomllib` assertion on `deny.toml` & `Cargo.lock`; `yaml.safe_load` on `.github/workflows/ci.yml`; `which cargo-deny cargo-audit` | **PASS (CONFIG) / CI-ENFORCED (EXEC)** | `deny.toml` `[sources]` explicitly pins `allow-registry = ["https://github.com/rust-lang/crates.io-index"]` and `allow-git = []`; `Cargo.lock` contains only local packages and `crates.io-index`. `cargo-deny` and `cargo-audit` are absent locally (enforced in CI). |

---

## Verification Gate Outputs (Post-Final-Edit Run)

### 1. Build, Clippy, Rustdoc Gates

| Command | Exit Code | Observed Output / Duration |
|---|---:|---|
| `cargo check --workspace --all-targets` | `0` | `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 5.21s` (only pre-existing transitive future-incompat notice on `proc-macro-error2 v2.0.1`) |
| `cargo clippy --workspace --all-targets -- -D warnings` | `0` | `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 4.24s` — zero warnings |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | `0` | `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 5.50s` — `Generated .../loganalysis_core/index.html and 2 other files` (zero rustdoc warnings) |

### 2. Full Workspace Test Pyramid (`cargo test --workspace`)

- Command: `cargo test --workspace` -> **Exit Code `0`** (`Finished 'test' profile [unoptimized + debuginfo] target(s) in 1m 06s`, **295 passed; 0 failed; 0 ignored; 0 measured**)

| Crate / Test Binary Target | Passed | Failed | Ignored | Measured | Duration | Status |
|---|---:|---:|---:|---:|---:|---|
| `loganalysis_core` (`src/lib.rs`) | 95 | 0 | 0 | 0 | `0.09s` | **PASS** |
| `loganalysis-core` (`tests/adb_logcat_adversarial_tests.rs`) | 7 | 0 | 0 | 0 | `1.04s` | **PASS** |
| `loganalysis-core` (`tests/adversarial_rollover_tests.rs`) | 8 | 0 | 0 | 0 | `0.19s` | **PASS** |
| `loganalysis-core` (`tests/challenger_core_stress_tests.rs`) | 7 | 0 | 0 | 0 | `0.02s` | **PASS** |
| `loganalysis-core` (`tests/challenger_m1_adversarial_suite.rs`) | 13 | 0 | 0 | 0 | `0.12s` | **PASS** |
| `loganalysis-core` (`tests/engine_tests.rs`) | 43 | 0 | 0 | 0 | `0.17s` | **PASS** |
| `loganalysis-core` (`tests/matcher_adversarial_tests.rs`) | 10 | 0 | 0 | 0 | `2.69s` | **PASS** |
| `loganalysis-core` (`tests/stream_robustness_stress.rs`) | 18 | 0 | 0 | 0 | `1.00s` | **PASS** |
| `loganalysis-core` (`tests/year_rollover_adversarial_stress.rs`) | 6 | 0 | 0 | 0 | `0.08s` | **PASS** |
| `loganalysis-core` (`tests/year_rollover_tests.rs`) | 5 | 0 | 0 | 0 | `0.01s` | **PASS** |
| `loganalysis_gui` (`src/lib.rs`) | 63 | 0 | 0 | 0 | `0.01s` | **PASS** |
| `loganalysis_gui` (`src/main.rs`) | 0 | 0 | 0 | 0 | `0.00s` | **PASS** |
| `loganalysis-gui` (`tests/gui_integration_tests.rs`) | 20 | 0 | 0 | 0 | `0.01s` | **PASS** |
| `Doc-tests loganalysis_core` | 0 | 0 | 0 | 0 | `0.00s` | **PASS** |
| `Doc-tests loganalysis_gui` | 0 | 0 | 0 | 0 | `0.00s` | **PASS** |
| **Workspace Total** | **295** | **0** | **0** | **0** | **N/A** *(compile+run: `1m 06s`)* | **PASS** |

### 3. Structural Hygiene & Config Verification Outputs

- `test ! -e loganalysis.log && test ! -e crates/loganalysis-gui/loganalysis.log` (executed after `cargo test --workspace`) -> Exit `0` (**PASS**: zero `loganalysis.log` files in repo checkout).
- `grep -n "allow(dead_code)" crates/loganalysis-gui/src/logging.rs` -> Exit `1` (**0 matches**).
- `grep -rn "thread::sleep" crates/` -> Exit `0` (**11 matches**, exclusively pre-existing tests):
  - `crates/loganalysis-core/tests/stream_robustness_stress.rs:212, 285, 343, 375, 377` (5 pre-existing lines)
  - `crates/loganalysis-core/tests/adb_logcat_adversarial_tests.rs:72, 131, 178, 252, 297` (5 pre-existing lines)
  - `crates/loganalysis-core/src/adb/logcat.rs:358` (1 pre-existing line)
  - **0** `thread::sleep` calls in any Phase 4 production or test code.
- `python3 -c 'import tomllib; d = tomllib.load(open("deny.toml","rb")); assert d["sources"]["allow-registry"] == ["https://github.com/rust-lang/crates.io-index"]; assert d["sources"]["allow-git"] == []'` -> Exit `0` (**PASS**).
- `python3 -c 'import tomllib; d = tomllib.load(open("Cargo.lock","rb")); srcs = {p.get("source","") for p in d["package"]}; assert srcs <= {"", "registry+https://github.com/rust-lang/crates.io-index"}'` -> Exit `0` (**PASS**).
- `python3 -c 'import yaml; yaml.safe_load(open(".github/workflows/ci.yml", "rb"))'` -> Exit `0` (**PASS**).
- `which cargo-deny cargo-audit` -> Exit `1` (absent locally; enforced in `.github/workflows/ci.yml`).

---

## Phase 3 Final Audit (`reality-checker`)

**Verdict:** **PASS**  
**Auditor:** `TestingRealityChecker` (`reality-checker`)  
**Audit Scope:** Full cross-document alignment, `git diff` / `git status` code inspection, and independent execution of all Phase 4 verification gates in `/home/drew/github/hwanjongyu/loganalysis` with zero modifications to production source code, tests, `Cargo.toml`/`Cargo.lock`, or Phase 1–3 spec/QA artifacts.

### 1. Independently Observed Gate Results

| Verification Gate | Command | Exit Code | Direct Observed Output |
|---|---|---:|---|
| Workspace check | `cargo check --workspace --all-targets` | `0` | `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 1.27s` (only pre-existing transitive future-incompat notice on `proc-macro-error2 v2.0.1`) |
| Workspace clippy | `cargo clippy --workspace --all-targets -- -D warnings` | `0` | `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 0.30s` — zero warnings |
| Workspace rustdoc | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | `0` | `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 0.31s` (`Generated .../loganalysis_core/index.html and 2 other files`, zero warnings) |
| Full workspace tests | `cargo test --workspace` | `0` | **295 passed; 0 failed; 0 ignored; 0 measured** across 13 test binary targets (`212` `loganalysis-core`, `83` `loganalysis-gui`) and 2 doc-test targets (`0` doc-tests) |
| Checkout log hygiene | `test ! -e loganalysis.log && test ! -e crates/loganalysis-gui/loganalysis.log` | `0` | `LOG_CLEAN=OK` (neither root nor `crates/loganalysis-gui/` contains `loganalysis.log` before or after `cargo test --workspace`) |
| Dead-code lint removal | `! grep -n "allow(dead_code)" crates/loganalysis-gui/src/logging.rs` | `0` | `DEAD_CODE_REMOVED=OK` (0 matches) |
| Hard-sleep check | `grep -rn "thread::sleep" crates/` | `0` | `11` pre-existing matches only (`stream_robustness_stress.rs:212,285,343,375,377`, `adb_logcat_adversarial_tests.rs:72,131,178,252,297`, `adb/logcat.rs:358`); **0** `thread::sleep` calls in Phase 4 code or tests |
| `deny.toml` `[sources]` | `python3 -c 'import tomllib; d = tomllib.load(open("deny.toml","rb")); assert d["sources"]["allow-registry"] == ["https://github.com/rust-lang/crates.io-index"]; assert d["sources"]["allow-git"] == []'` | `0` | `DENY_TOML=OK` (`unknown-registry = "deny"`, `unknown-git = "warn"`, `allow-registry = ["https://github.com/rust-lang/crates.io-index"]`, `allow-git = []`) |
| `Cargo.lock` & CI YAML | Python `tomllib` on `Cargo.lock` & `yaml.safe_load` on `.github/workflows/ci.yml` | `0` | `CARGO_LOCK_SRCS= ['', 'registry+https://github.com/rust-lang/crates.io-index']`, `CI_YAML=OK` (`which cargo-deny cargo-audit` absent locally; CI-enforced on `ubuntu-22.04`) |
| Protected files & deferrals | `git diff -- Cargo.toml crates/*/Cargo.toml Cargo.lock docs/spec-phase{1,2,3}-remediation.md docs/qa-phase{1,2,3}-remediation.md` | `0` | Empty diff (zero unauthorized manifest/lockfile edits; Phase 1–3 spec/QA files untouched) |

### 2. Per-Target Test Counts Independently Observed

| Crate / Test Target | Passed | Failed | Ignored | Duration |
|---|---:|---:|---:|---:|
| `loganalysis_core` (`src/lib.rs`) | 95 | 0 | 0 | `0.05s` |
| `loganalysis-core` (`tests/adb_logcat_adversarial_tests.rs`) | 7 | 0 | 0 | `1.03s` |
| `loganalysis-core` (`tests/adversarial_rollover_tests.rs`) | 8 | 0 | 0 | `0.19s` |
| `loganalysis-core` (`tests/challenger_core_stress_tests.rs`) | 7 | 0 | 0 | `0.01s` |
| `loganalysis-core` (`tests/challenger_m1_adversarial_suite.rs`) | 13 | 0 | 0 | `0.11s` |
| `loganalysis-core` (`tests/engine_tests.rs`) | 43 | 0 | 0 | `0.15s` |
| `loganalysis-core` (`tests/matcher_adversarial_tests.rs`) | 10 | 0 | 0 | `2.58s` |
| `loganalysis-core` (`tests/stream_robustness_stress.rs`) | 18 | 0 | 0 | `1.00s` |
| `loganalysis-core` (`tests/year_rollover_adversarial_stress.rs`) | 6 | 0 | 0 | `0.07s` |
| `loganalysis-core` (`tests/year_rollover_tests.rs`) | 5 | 0 | 0 | `0.00s` |
| `loganalysis_gui` (`src/lib.rs`) | 63 | 0 | 0 | `0.01s` |
| `loganalysis_gui` (`src/main.rs`) | 0 | 0 | 0 | `0.00s` |
| `loganalysis-gui` (`tests/gui_integration_tests.rs`) | 20 | 0 | 0 | `0.01s` |
| `Doc-tests` (`loganalysis_core` + `loganalysis_gui`) | 0 | 0 | 0 | `0.00s` |
| **Total** | **295** | **0** | **0** | — |

### 3. Specification, Invariant & Diff Compliance Summary

- **`MIN-03` (`crates/loganalysis-gui/src/logging.rs`, `src/lib.rs`):** `resolve_log_paths_with_lookup` exclusively emits the OS user log directory when resolved and falls back to `cwd` only on `None`; `#[allow(dead_code)]` is removed from `resolve_user_log_dir`; `LogWriterState` caches `BufWriter<File>` in `LOG_STATE`, flushes prior writers on path switches, immediately flushes `ERROR`/`CRITICAL`/`CRITICAL_PANIC`/`FATAL`, synchronizes pre-crash buffered logs ahead of `[CRITICAL_PANIC]` in `record_crash_report`, flushes via `logging::flush_logs()` on clean exit in `run_app()`, and isolates tests via `LOG_TEST_MUTEX`, `TestTempDir`, `ResetLogWriterOnDrop`, and `log_event_to_path`.
- **`INF-01` (`crates/loganalysis-core/src/buffer/{column.rs,mod.rs}`, `src/lib.rs`, `crates/loganalysis-gui/src/{components/log_view.rs,workspace/dock.rs}`):** `LogFormatParser: Send + Sync`, `ThreadtimeColumnParser`, `BriefColumnParser`, `DEFAULT_COLUMN_PARSERS`, `parse_column_line_with_parsers`, and `parse_column_line_auto` are implemented and re-exported with zero-copy slicing and UTF-8 boundary safety. `parse_logcat_column_line` strictly delegates to `THREADTIME_COLUMN_PARSER` with unstructured fallback, preserving all 10 `column::tests` and 13 `challenger_m1_adversarial_suite` tests. GUI structured view (`log_view.rs:999-1001`) and facet sampling (`workspace/dock.rs:21`) are wired to `parse_column_line_auto`.
- **`[INV-09]` (`crates/loganalysis-core/src/filter/engine.rs`, `tests/engine_tests.rs`):** `FilterResult::excluded_filter_matches` records `(line_idx, flat_matches)` whenever `!is_visible && !flat_matches.is_empty()`, preserving Rayon chunk order in `execute_filtering` and batch order in `append_streaming_batch_with_matcher`. Ring-buffer trimming (`trimmed_count > 0`) unconditionally drains and shifts `excluded_filter_matches` even when `visible_indices.is_empty()`. `test_compiled_stream_filters_parity_with_append_streaming_batch` Step 4 (`trim-excluded`) full `execute_filtering` parity is enabled and passes alongside `test_excluded_line_trim_counter_parity_when_visible_empty` and `test_excluded_line_partial_trim_and_shift_parity`.
- **Non-blocking `stop` (`crates/loganalysis-core/src/adb/logcat.rs`):** `LogcatStreamHandle::stop` releases the `self.child` mutex prior to invoking `reap_child_nonblocking`, which kills the child, fast-paths `try_wait()`, and otherwise delegates reaping to a detached `"adb-child-reaper"` thread (with synchronous fallback only on thread-spawn error). Deterministic tests verify `< 50ms` return and zero zombie `/proc/{pid}/status` entries without `thread::sleep`.
- **`INF-06`, `MIN-12`, `INF-05` & Phase 1–3 Preservation:** `deny.toml` pins `[sources]` explicitly; `MIN-12` (`gpui = "0.2.2"`) and `INF-05` (`proc-macro-error2 v2.0.1`) remain formally deferred in [docs/spec-phase4-backlog-remediation.md](file:///home/drew/github/hwanjongyu/loganalysis/docs/spec-phase4-backlog-remediation.md) with zero unauthorized Cargo edits; all Phase 1–3 fixes remain intact (`295/295` passing). Note: `cargo-deny` and `cargo-audit` binaries are absent locally and remain CI-enforced via `.github/workflows/ci.yml` (`ubuntu-22.04`).

**Final Certification:** **PASS** — Phase 4 backlog remediation (`MIN-03`, `INF-01`, `INF-06`), `[INV-09]` excluded-line trim parity, non-blocking `LogcatStreamHandle::stop`, formal deferrals (`MIN-12`, `INF-05`), and Phase 1–3 preservations are all verified complete and passing.

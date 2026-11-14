# Phase 1 (Immediate / P0) Remediation Specification

**Source Audit:** [`docs/review-report.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/review-report.md) (Section 5: Phase 1)  
**Scope:** Live Streaming Integrity & CI/Safety Blockers (`CRIT-01`, `CRIT-02`, `MAJ-01`, `MAJ-03`, `MAJ-12`)

---

## 1. Remediation Targets & Invariants

### 1.1. `CRIT-01`: Live Streaming Filter Reapply Race Condition
- **Affected Files:**
  - `crates/loganalysis-core/src/buffer/stream_buffer.rs`
  - `crates/loganalysis-core/src/buffer/mod.rs`
  - `crates/loganalysis-gui/src/workspace/mod.rs`
- **Problem:** `LogAnalysisWorkspace::reapply_filters` snapshots `log_source` and runs `execute_filtering` on a background executor. While filtering runs, live ADB log chunks can append to `log_source` and trigger ring-buffer head evictions (`trim_if_needed`). Assigning `workspace.filter_result = Some(Arc::new(res))` unconditionally overwrites newly ingested lines and introduces stale indices.
- **Invariant & Fix:**
  - Track cumulative evicted lines in `StreamLogBuffer::total_trimmed` (reset on `clear()`, exposed via `total_trimmed()` and `head_trimmed_since(prev)`), and expose wrappers on `ActiveLogSource`.
  - Capture `(snapshot_len, snapshot_trimmed, filters_for_ui)` before spawning the background task.
  - On completion (if `filter_cancel_gen == generation`), reconcile `final_res` against the live `workspace.log_source` using `append_streaming_batch` for `catchup_start..current_len` and `delta_trimmed = src.head_trimmed_since(snapshot_trimmed)` where `catchup_start = snapshot_len.saturating_sub(delta_trimmed)`.
  - Report `total_count` in `status_text` from the live `workspace.log_source` when available.

### 1.2. `CRIT-02`: Selection Index-Space Desync on Ring Buffer Eviction
- **Affected Files:**
  - `crates/loganalysis-gui/src/selection.rs`
  - `crates/loganalysis-gui/src/workspace/adb.rs`
- **Problem:** `SelectionModel::shift_on_trim` accepted a single `trimmed` raw line count and subtracted it directly from both `anchor_source_idx` (raw index space) and `selected_row` (visible filtered index space). Evicting e.g. 20,000 raw lines collapsed `selected_row` to `0` even when the user's selected row survived in `visible_indices`.
- **Invariant & Fix:**
  - Update `SelectionModel::shift_on_trim(&mut self, dropped_visible_rows: usize, raw_trimmed_count: usize)` so `anchor_source_idx` shifts by `raw_trimmed_count` and `selected_row` shifts by `dropped_visible_rows` (clearing `selected_row` to `None` if `row < dropped_visible_rows`).
  - At both `adb.rs` trim sites (live streaming batch ingestion and `toggle_pause_stream`), compute `dropped_visible` via `visible_indices.partition_point(|&idx| idx < trimmed)` *before* `append_streaming_batch` mutates `visible_indices`.

### 1.3. `MAJ-01`: Unblock Clippy & CI Build Gate
- **Affected Files:**
  - `crates/loganalysis-gui/src/components/text_input.rs`
  - `crates/loganalysis-core/tests/challenger_m1_adversarial_suite.rs`
  - `crates/loganalysis-core/tests/challenger_core_stress_tests.rs`
  - `.github/workflows/ci.yml`
- **Problem:** `cargo clippy --workspace --all-targets -- -D warnings` failed due to 7 `clippy::reversed_empty_ranges` errors in `text_input.rs` test cases, 2 warnings in `challenger_m1_adversarial_suite.rs`, and 2 warnings in `challenger_core_stress_tests.rs`. CI did not run `cargo clippy`.
- **Invariant & Fix:**
  - Add `#[allow(clippy::reversed_empty_ranges)]` on `text_input.rs` test module.
  - Clean up `octal_escapes`, `needless_range_loop`, and `manual_repeat_n` lints in the core test suites.
  - Add `cargo clippy --workspace --all-targets -- -D warnings` to `.github/workflows/ci.yml`.

### 1.4. `MAJ-03`: Windows Atomic File Persistence
- **Affected Files:**
  - `crates/loganalysis-gui/src/workspace/filters.rs`
  - `crates/loganalysis-gui/src/workspace/recent_files.rs`
- **Problem:** `std::fs::rename` fails on Windows when the destination file already exists (`ERROR_ALREADY_EXISTS`), breaking saves of filter profiles and recent files after the first write.
- **Invariant & Fix:**
  - Introduce `atomic_replace_file(from, to)` in `workspace/filters.rs`, using Win32 `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` on `#[cfg(windows)]` and `std::fs::rename` on `#[cfg(not(windows))]`.
  - Replace `std::fs::rename` calls in `filters.rs` and `recent_files.rs` with `atomic_replace_file`.

### 1.5. `MAJ-12`: Secure AppImage Release Packaging
- **Affected Files:**
  - `scripts/build-appimage.sh`
- **Problem:** `scripts/build-appimage.sh` downloaded `appimagetool` from the unpinned `continuous` tag and ran it without SHA256 verification.
- **Invariant & Fix:**
  - Pin `APPIMAGETOOL_VERSION="13"` and verify `sha256sum` (`d918b4ed5479b4d8479e03cca93d4073000b028965e43fb2945142f96e5720b4`) before marking executable, removing the downloaded artifact and exiting non-zero on mismatch.

---

## 2. Verification Criteria
- `cargo clippy --workspace --all-targets -- -D warnings` exits with code `0` and zero warnings/errors.
- `cargo test --workspace` exits with code `0`, including new/updated unit tests for `CRIT-01`, `CRIT-02`, and `MAJ-03`.
- `bash -n scripts/build-appimage.sh` exits with code `0`.

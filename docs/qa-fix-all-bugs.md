# QA Verification Report: `fix-all-bugs`

- **Workspace**: `/home/drew/github/hwanjongyu/loganalysis` (`v0.2.4`)
- **Specification**: [spec-fix-all-bugs.md](file:///home/drew/github/hwanjongyu/loganalysis/docs/spec-fix-all-bugs.md)
- **Overall Status**: **PASS** (`334` workspace tests passed, `0` failed, `0` ignored; all 4 Cargo gates exit `0`; zero `std::thread::sleep` calls in added/modified tests)

---

## 1. Cargo Quality Gates (Executed After Final Edit)

| Gate | Command | Exit Code | Observed Output Summary |
|---|---|---|---|
| Typecheck | `cargo check --workspace --all-targets` | `0` | `Finished dev profile [unoptimized + debuginfo] target(s) in 4.59s`; `0` crate errors/warnings (upstream `proc-macro-error2 v2.0.1` future-incompat notice only) |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | `0` | `Finished dev profile [unoptimized + debuginfo] target(s) in 2.81s`; `0` clippy warnings across all targets |
| Full Test Suite | `cargo test --workspace` | `0` | `334 passed; 0 failed; 0 ignored` across `loganalysis-core` (`234`) and `loganalysis-gui` (`100`) |
| Doc Build | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | `0` | `Finished dev profile [unoptimized + debuginfo] target(s) in 6.06s`; `0` rustdoc warnings |

### Per-Crate / Per-Test-Binary Breakdown (`cargo test --workspace`)

| Crate | Test Target / Binary | Passed | Failed | Ignored | Duration |
|---|---|---:|---:|---:|---:|
| `loganalysis-core` | `unittests src/lib.rs` | 115 | 0 | 0 | 0.11s |
| `loganalysis-core` | `tests/adb_logcat_adversarial_tests.rs` | 7 | 0 | 0 | 1.03s |
| `loganalysis-core` | `tests/adversarial_rollover_tests.rs` | 8 | 0 | 0 | 0.32s |
| `loganalysis-core` | `tests/challenger_core_stress_tests.rs` | 7 | 0 | 0 | 0.03s |
| `loganalysis-core` | `tests/challenger_m1_adversarial_suite.rs` | 13 | 0 | 0 | 0.14s |
| `loganalysis-core` | `tests/engine_tests.rs` | 45 | 0 | 0 | 0.22s |
| `loganalysis-core` | `tests/matcher_adversarial_tests.rs` | 10 | 0 | 0 | 2.88s |
| `loganalysis-core` | `tests/stream_robustness_stress.rs` | 18 | 0 | 0 | 1.00s |
| `loganalysis-core` | `tests/year_rollover_adversarial_stress.rs` | 6 | 0 | 0 | 0.14s |
| `loganalysis-core` | `tests/year_rollover_tests.rs` | 5 | 0 | 0 | 0.01s |
| `loganalysis-core` | `Doc-tests loganalysis_core` | 0 | 0 | 0 | 0.00s |
| **`loganalysis-core` Subtotal** | | **234** | **0** | **0** | |
| `loganalysis-gui` | `unittests src/lib.rs` | 80 | 0 | 0 | 0.03s |
| `loganalysis-gui` | `unittests src/main.rs` | 0 | 0 | 0 | 0.00s |
| `loganalysis-gui` | `tests/gui_integration_tests.rs` | 20 | 0 | 0 | 0.01s |
| `loganalysis-gui` | `Doc-tests loganalysis_gui` | 0 | 0 | 0 | 0.00s |
| **`loganalysis-gui` Subtotal** | | **100** | **0** | **0** | |
| **Workspace Total** | | **334** | **0** | **0** | |

---

## 2. Static `std::thread::sleep` Ban Scan

- **Command 1**: `git diff -- 'crates/*/tests/' 'crates/*/src/' | grep -E "^\+.*thread::sleep" || echo "ZERO_SLEEP_IN_DIFF"`
- **Command 2**: `git diff | grep -n -C 2 -E "^\+.*sleep" || true`
- **Findings**:
  - **Test Files & `#[cfg(test)]` Blocks**: **0** `std::thread::sleep` / `thread::sleep` calls added or modified anywhere in `crates/*/tests/` or `#[cfg(test)]` blocks.
  - **Other Diff Occurrences (Non-Test-Sleep)**:
    1. Production polling in `crates/loganalysis-core/src/adb/device.rs:259` (`std::thread::sleep(remaining.min(Duration::from_millis(5)));`) inside `run_command_with_timeout`'s bounded `try_wait()` loop (Spec `[HIGH-2]`).
    2. Child shell fixture string `"exec 1>&-; sleep 30"` passed to `sh -c` in `crates/loganalysis-core/src/adb/device.rs:446` (`test_run_command_with_timeout_stdout_closed_hang_enforces_timeout`), simulating a hung child process while the Rust test asserts timeout enforcement in `< 2s`.

---

## 3. Complete Verification Matrix (Spec Sections 5 & 6.2)

| Spec ID | Test File | Test Function(s) | Status |
|---|---|---|---|
| `[HIGH-1]` | `crates/loganalysis-gui/src/controllers/search.rs`<br>`crates/loganalysis-gui/src/workspace/mod.rs`<br>`crates/loganalysis-gui/tests/gui_integration_tests.rs` | `controllers::search::tests::test_search_controller_lifecycle`<br>`workspace::tests::test_derived_state_reset_source_gen_and_facet_auto_sampling`<br>`test_workspace_controller_snapshots_and_defaults` | **PASS** |
| `[HIGH-2]` | `crates/loganalysis-core/src/adb/device.rs` | `adb::device::tests::test_run_command_with_timeout_stdout_closed_hang_enforces_timeout`<br>`adb::device::tests::test_run_command_with_timeout_stdout_closed_before_clean_exit` | **PASS** |
| `[HIGH-3]` | `crates/loganalysis-gui/src/controllers/split_view.rs`<br>`crates/loganalysis-gui/src/workspace/mod.rs` | `controllers::split_view::tests::test_split_view_controller_toggles`<br>`controllers::split_view::tests::test_split_view_source_gen_advance_and_clear_invalidation`<br>`workspace::tests::test_derived_state_reset_source_gen_and_facet_auto_sampling` | **PASS** |
| `[HIGH-4]` | `crates/loganalysis-gui/src/controllers/adb.rs`<br>`crates/loganalysis-gui/src/workspace/mod.rs` | `controllers::adb::tests::test_adb_controller_stop_stream_resets_pause_and_buffer`<br>`workspace::tests::test_derived_state_reset_source_gen_and_facet_auto_sampling` | **PASS** |
| `[HIGH-5]` | `crates/loganalysis-gui/src/modals/bookmark_drawer.rs` | `modals::bookmark_drawer::tests::test_clamp_drawer_selected_index` (+ verified `.on_mouse_down` & `.on_click` `cx.stop_propagation()` in `render_bookmark_drawer`) | **PASS** |
| `[MEDIUM-1]` | `crates/loganalysis-core/src/buffer/correlation.rs` | `buffer::correlation::tests::test_nearest_timestamp_unstamped_and_block_boundaries`<br>`buffer::correlation::tests::test_single_file_and_stream_rollover_monotonic_correlation` | **PASS** |
| `[MEDIUM-2]` | `crates/loganalysis-core/src/buffer/file_buffer.rs`<br>`crates/loganalysis-core/tests/engine_tests.rs` | `buffer::file_buffer::tests::test_file_log_buffer_widest_line_uses_display_width`<br>`test_execute_filtering_cjk_display_width_and_streaming_trim_shrink`<br>`test_compiled_stream_filters_parity_with_append_streaming_batch` | **PASS** |
| `[MEDIUM-3]` | `crates/loganalysis-core/src/adb/logcat.rs` | `adb::logcat::tests::test_stream_malformed_utf8_does_not_bleed_across_newline`<br>`adb::logcat::tests::test_stream_crlf_straddling_64kb_boundary_no_spurious_blank_line`<br>`adb::logcat::tests::test_stream_split_utf8_cjk_across_64kb_boundary` | **PASS** |
| `[MEDIUM-4]` | `crates/loganalysis-core/src/filter/matcher.rs`<br>`crates/loganalysis-core/tests/engine_tests.rs` | `filter::matcher::tests::test_non_ascii_case_insensitive_literal_parity`<br>`test_execute_filtering_and_streaming_non_ascii_case_insensitive_parity` | **PASS** |
| `[MEDIUM-5]` | `crates/loganalysis-gui/src/theme.rs` | `theme::tests::test_resolve_selected_row_fg_wcag_aa_contrast` | **PASS** |
| `[MEDIUM-6]` | `crates/loganalysis-gui/src/modals/filter_edit_modal.rs` | `modals::filter_edit_modal::tests::test_palette_swatch_element_ids_are_distinct` | **PASS** |
| `[MEDIUM-7]` | `crates/loganalysis-gui/src/components/text_input.rs` | `components::text_input::tests::test_placeholder_color_derives_from_theme_foreground`<br>`components::text_input::tests::test_cursor_moves_and_selections_clear_stale_marked_range` | **PASS** |
| `[MEDIUM-8]` | `crates/loganalysis-gui/src/workspace/mod.rs` | `workspace::tests::test_resolve_status_line_uses_last_match_and_skips_empty_inactive` | **PASS** |
| `[MEDIUM-9]` | `crates/loganalysis-core/src/filter/timeline.rs`<br>`crates/loganalysis-gui/src/components/timeline_sparkline.rs`<br>`crates/loganalysis-core/tests/challenger_core_stress_tests.rs` | `filter::timeline::tests::test_timeline_clock_skew_tiny_duration_and_empty_bucket_rows`<br>`components::timeline_sparkline::tests::test_bucket_navigation_row_ignores_empty_buckets`<br>`test_challenge_4_timeline_i64_min_behavior` | **PASS** |
| `[MEDIUM-10]` | `crates/loganalysis-core/src/buffer/merged_buffer.rs`<br>`crates/loganalysis-core/tests/adversarial_rollover_tests.rs` | `buffer::merged_buffer::tests::test_merge_iso8601_and_time_only_no_spurious_year_rollover`<br>`buffer::merged_buffer::tests::test_merge_leading_headers_stay_before_first_event`<br>`buffer::merged_buffer::tests::test_from_sources_rejects_mismatched_source_names`<br>`test_adversarial_leading_untimestamped_lines_cross_file_rollover` | **PASS** |
| `[LOW-1]` | `crates/loganalysis-gui/src/workspace/search.rs`<br>`crates/loganalysis-gui/src/controllers/search.rs`<br>`crates/loganalysis-gui/tests/gui_integration_tests.rs` | `workspace::search::tests::test_apply_find_input_query_preserves_spaces_and_clears_whitespace`<br>`controllers::search::tests::test_search_controller_preserves_boundary_spaces_and_clears_whitespace`<br>`test_workspace_controller_snapshots_and_defaults` | **PASS** |
| `[LOW-2]` | `crates/loganalysis-core/src/buffer/timestamp.rs` | `buffer::timestamp::tests::test_compute_delta_micros_typed_skips_rollover_on_iso8601`<br>`buffer::timestamp::tests::test_format_delta_micros_rounding_promotions` | **PASS** |
| `[LOW-3]` | `crates/loganalysis-core/src/export/bookmark.rs` | `export::bookmark::tests::test_next_bookmark_usize_max_wraps_cleanly` | **PASS** |
| `[LOW-4]` | `crates/loganalysis-core/src/export/html_report.rs` | `export::html_report::tests::test_html_report_rule_match_pill_and_total_lines`<br>`export::html_report::tests::test_html_report_contains_utc_timestamp` | **PASS** |
| `[LOW-5]` | `crates/loganalysis-gui/tests/gui_integration_tests.rs` | `test_milestone4_collapsible_timeline_and_zen_focus_mode`<br>`test_command_palette_catalog_and_fuzzy_search` | **PASS** |
| `[LOW-6]` | `crates/loganalysis-gui/src/workspace/filters.rs`<br>`crates/loganalysis-gui/tests/gui_integration_tests.rs` | `workspace::filters::tests::test_filter_tab_dirty_and_save_target_transitions`<br>`test_milestone4_collapsible_timeline_and_zen_focus_mode` | **PASS** |
| `[LOW-7]` | `crates/loganalysis-gui/src/workspace/recent_files.rs` | `workspace::recent_files::tests::test_push_recent_path_lru_dedup_and_cap` | **PASS** |
| `[LOW-8a]` | `crates/loganalysis-core/src/filter/matcher.rs` | `filter::matcher::tests::test_small_fixed_bounded_lookaround_ipv4_allowed_while_nested_rejected` | **PASS** |
| `[LOW-8b]` | `crates/loganalysis-core/src/filter/heatmap.rs` | `filter::heatmap::tests::test_gray_fg_rule_match_classifies_as_rule_match` | **PASS** |
| `[LOW-8c]` | `crates/loganalysis-core/src/constants.rs` | `constants::tests::test_parse_color_spec_rejects_signed_or_invalid_hex` | **PASS** |
| `[LOW-8d]` | `crates/loganalysis-core/src/buffer/column.rs` | `buffer::column::tests::test_threadtime_empty_tag_produces_none` | **PASS** |
| `[LOW-8e]` | `crates/loganalysis-gui/src/workspace/filters.rs` | `workspace::filters::tests::test_apply_imported_filter_tabs_rejects_empty` | **PASS** |
| `[LOW-8f]` | `crates/loganalysis-gui/src/workspace/mod.rs` | `workspace::tests::test_derived_state_reset_source_gen_and_facet_auto_sampling` | **PASS** |
| Column View | `crates/loganalysis-gui/src/components/log_view.rs`<br>`crates/loganalysis-gui/tests/gui_integration_tests.rs` | `components::log_view::tests::test_column_view_scrollbar_alignment_constants_and_heights`<br>`test_structured_column_view_parsing_and_tag_quick_filter` | **PASS** |
| CLI | `crates/loganalysis-gui/src/cli.rs` | `cli::tests::test_parse_cli_args_variants`<br>`cli::tests::test_handle_cli_version_flag` | **PASS** |
| Defective Test 1 | `crates/loganalysis-core/tests/engine_tests.rs` | `test_automotive_preset_profile_parses` (embedded `AUTOMOTIVE_PROFILE_JSON`, unconditional assertions, zero `path.exists()` skip) | **PASS** |
| Defective Test 2 | `crates/loganalysis-core/tests/challenger_core_stress_tests.rs` | `test_challenge_4_timeline_i64_min_behavior` (zero `catch_unwind`, direct `EpsTimelineData::compute` assertions on `[i64::MIN, i64::MIN+100]` and `[i64::MIN+1, i64::MAX-1]`) | **PASS** |


---

## 4. Phase 3 Reality Check Audit Report (`reality-checker`)

- **Audit Timestamp**: `2026-11-18T06:34:00Z`
- **Audited Workspace**: `/home/drew/github/hwanjongyu/loganalysis` (`46` modified files + `2` untracked `docs/` files)
- **Reference Baseline**: `REVIEW_REPORT.md`, [spec-fix-all-bugs.md](file:///home/drew/github/hwanjongyu/loganalysis/docs/spec-fix-all-bugs.md), [INVARIANTS.md](file:///home/drew/github/hwanjongyu/loganalysis/docs/INVARIANTS.md)

### 4.1 Independent Command & Guardrail Execution

| Check | Exact Command | Exit Code | Observed Output / Evidence |
|---|---|---:|---|
| Git working tree | `git status && git diff --stat` | `0` | `46` tracked files modified (`3205` insertions, `837` deletions); `Cargo.toml` and `crates/*/Cargo.toml` untouched (`gpui = "0.2.2"` without `test-support`) |
| Typecheck | `cargo check --workspace --all-targets` | `0` | `Finished dev profile [unoptimized + debuginfo] target(s) in 0.33s`; `0` crate errors/warnings (upstream `proc-macro-error2 v2.0.1` future-incompat notice only) |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | `0` | `Finished dev profile [unoptimized + debuginfo] target(s) in 0.34s`; `0` clippy warnings |
| Workspace Tests | `cargo test --workspace` | `0` | **`334 passed; 0 failed; 0 ignored`** (`loganalysis-core`: `115` unit + `119` integration = `234`; `loganalysis-gui`: `80` unit + `0` bin + `20` integration = `100`; `0` doc-tests) |
| Doc Build | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | `0` | `Finished dev profile [unoptimized + debuginfo] target(s) in 0.32s`; `0` rustdoc warnings |
| CLI Binary Smoke | `cargo run -q -p loganalysis-gui -- --version` / `--help` / `--unknown-flag` | `0` / `0` / `2` | `--version` printed `LogAnalysis 0.2.4`; `--help` printed full usage/options/multi-file merge help; `--unknown-flag` exited with code `2` and printed help to stderr |
| Prohibited Patterns Diff Scan | `git diff \| grep -n -E '^\+.*(thread::sleep\|std::thread::sleep\|test-support\|#\[ignore\|todo!\(\|unimplemented!\(\|catch_unwind)'` | `0` | Only 1 match in `git diff`: production polling `std::thread::sleep(remaining.min(Duration::from_millis(5)))` in `crates/loganalysis-core/src/adb/device.rs:259`; **0** in `crates/*/tests/` or `#[cfg(test)]`; **0** `test-support`, `#[ignore]`, `todo!()`, `unimplemented!()`, or `catch_unwind` |

### 4.2 Item-by-Item Implementation Cross-Validation

- **`[HIGH-1]`**: `LogAnalysisWorkspace::search_cancel_gen` removed (`crates/loganalysis-gui/src/workspace/mod.rs`); `reset_derived_log_state()` delegates to `reset_derived_controllers_and_tokens()`, invoking `search_ctrl.abort_ongoing_searches()` while preserving `filter_cancel_gen` independence (`[INV-02-A]`). Verified via `git grep -n search_cancel_gen crates/` (0 hits) and `test_derived_state_reset_source_gen_and_facet_auto_sampling`.
- **`[HIGH-2]`**: `crates/loganalysis-core/src/adb/device.rs:245-268` replaces unbounded `child.wait()` in `RecvTimeoutError::Disconnected` with bounded `child.try_wait()` polling against `timeout.saturating_sub(start_time.elapsed())`, invoking `cleanup_aborted_command` and returning `Err(AdbError::Timeout(timeout))`. Verified by both Unix subprocess tests (`elapsed < 2s`).
- **`[HIGH-3]`**: `primary_source_gen` (`workspace/mod.rs:45, 440-505`, `workspace/merged.rs:37-88`, `workspace/adb.rs:198-495`) and `SplitViewController::secondary_source_gen` (`controllers/split_view.rs:15-60`, `workspace/split_view.rs:94-147`) guard all async `open_file`, `open_merged_files`, `start_adb_stream`, `clear_logs`, and `open_secondary_file` transitions and discard stale `Ok`/`Err` completions.
- **`[HIGH-4]`**: `adb.is_paused = false` and `clear_paused_buffer()` enforced in `AdbController::stop_stream()` (`controllers/adb.rs:41-48`), `reset_derived_controllers_and_tokens()` (`workspace/mod.rs:503-504`), `start_adb_stream()` (`workspace/adb.rs:201-202`), `stop_adb_stream()` (`workspace/adb.rs:541`), and both disconnect paths (`workspace/adb.rs:376-378, 420-422`).
- **`[HIGH-5]`**: `crates/loganalysis-gui/src/modals/bookmark_drawer.rs:144-187` stops `MouseButton::Left` propagation on the action container and calls `cx.stop_propagation()` inside `btn-drawer-note` and `btn-drawer-del`, with `clamp_drawer_selected_index()` clamping post-delete selection.
- **`[MEDIUM-1]`**: `crates/loganalysis-core/src/buffer/correlation.rs:5-97` tracks `best: Option<(usize, u64)>` (returning `None` on unstamped logs) and probes outward from `mid` within `[low..=high]`; `FileLogBuffer` (`file_buffer.rs:39, 468-499`) and `StreamLogBuffer` (`stream_buffer.rs:36, 115-121, 276-283`) normalize `MonthDayTime` rollover monotonically.
- **`[MEDIUM-2]`**: `measure_display_width` (`buffer/mod.rs:170-180`) used in `LogSource::get_measured_length`, `FileLogBuffer::from_storage` (`file_buffer.rs:73, 86`), and `execute_filtering` (`filter/engine.rs:297`); `append_streaming_batch_with_matcher` (`engine.rs:486-519`) recalculates or clears `widest_visible_length`/`widest_visible_text`/`widest_visible_source_idx` on trims, and `engine_tests.rs:1420, 1455, 1491` now asserts `check_widest = true`.
- **`[MEDIUM-3]`**: `crates/loganalysis-core/src/adb/logcat.rs:245-288` inspects `hit_newline` and `hit_read_limit` prior to stripping `\n`/`\r`, suppresses orphan `\n` after 64KB-split `\r\n`, and carries incomplete UTF-8 only on `hit_read_limit && !stripped_limit_cr`.
- **`[MEDIUM-4]`**: `crates/loganalysis-core/src/filter/matcher.rs:215-232, 502-546` routes non-ASCII case-insensitive literals through Unicode `RegexBuilder` (`unicode(true)`) / `LiteralUnicodeInsensitive`, uses `unicode(false)` for ASCII CI literals, and replaces `.ok()` automaton drops with `individual_filters` fallback.
- **`[MEDIUM-5]`**: `crates/loganalysis-gui/src/theme.rs:126-195` + `components/log_view.rs:894-904` implement `selection_fg` (`#0f172a` in Light, `#ffffff` in Dark) and `resolve_selected_row_fg` preserving $\ge 4.5:1$ WCAG AA contrast.
- **`[MEDIUM-6]`**: `crates/loganalysis-gui/src/modals/filter_edit_modal.rs:7-55, 418-540` assigns distinct index-based `swatch_element_id`s to `BG_PALETTE_SWATCHES` and `FG_PALETTE_SWATCHES` and removes Korean parentheticals.
- **`[MEDIUM-7]`**: `crates/loganalysis-gui/src/components/text_input.rs:118-295, 711-768` derives placeholder color via `placeholder_color(style.color)` and clears `marked_range` in `move_cursor_state`, `select_to_state`, `select_word_state`, and `select_all_state`, while guarding `prepaint` underline runs against empty/placeholder text.
- **`[MEDIUM-8]`**: `crates/loganalysis-gui/src/workspace/mod.rs:350-397, 509-558` eliminates UI-thread `CompiledMatcher::new` compilation, uses `ensure_cached_stream_filters()` + `resolve_status_line_for_selection()` with last-match precedence across enabled tabs, ignores empty/inactive rules, and removes the stale doc comment.
- **`[MEDIUM-9]`**: `crates/loganalysis-core/src/filter/timeline.rs:14-145` converts `start_visible_row` to `Option<usize>`, scans true `min_micros`/`max_micros`, uses `u128`/`i128` partitioning (`start_micros <= end_micros`), and `components/timeline_sparkline.rs:12-85` disables interaction on empty buckets via `bucket_navigation_row`.
- **`[MEDIUM-10]`**: `crates/loganalysis-core/src/buffer/timestamp.rs:81-94, 180-310` and `merged_buffer.rs:65-238` separate `TimestampKind::{MonthDayTime, Iso8601, TimeOnly}`, validate `sources.len() == source_names.len()`, restrict rollover offsets to `MonthDayTime`, and anchor leading headers immediately before their source's first timestamped line.
- **`[LOW-1]`–`[LOW-8f]`**:
  - `[LOW-1]`: `workspace/search.rs:44-81` (`apply_find_input_query`) preserves boundary spaces on non-whitespace queries while clearing whitespace-only queries.
  - `[LOW-2]`: `timestamp.rs:135-177` + `workspace/selection.rs:133-172` add `compute_delta_micros_typed` and rounding-aware `format_delta_micros` unit promotion.
  - `[LOW-3]`: `export/bookmark.rs:91-100` uses `(Bound::Excluded(current_source_idx), Bound::Unbounded)` and `saturating_add(1)`.
  - `[LOW-4]`: `export/html_report.rs:16-39, 314-345` separates `match_count`, adds `"Rule Matches"` card + `'match'` filter pill, and renders `{total_lines} total lines` while preserving the UTC footer.
  - `[LOW-5]`: `actions.rs`, `lib.rs`, `modals/command_palette.rs`, `workspace/mod.rs`, `workspace/command_palette.rs` register/wire `ResetZoom` (`REGISTERED_ACTION_COUNT == 46`, `Ctrl+0`/`Cmd+0`), fix `(Bottom / Left)`, and fix `Alt+F2`.
  - `[LOW-6]`: `CloseFind` wired (`actions.rs`, `lib.rs`, `workspace/search.rs`, `find_bar.rs`); `active_filter_profile_path` and `FilterTabData::{modified, file_path}` differentiate `SaveFilters` vs `SaveFiltersAs` across the 11 specified filter-mutation methods in `workspace/filters.rs` (note: `workspace/dock.rs:171-186` toggling an existing facet chip inline does not call `mark_filter_tabs_modified()`, though `tab.modified` is not surfaced in UI and direct `SaveFilters` still works via `active_filter_profile_path`).
  - `[LOW-7]`: `components/facets_bar.rs:191` calls `refresh_facet_tags_async(cx)` (dead sync helper removed from `workspace/dock.rs`); `workspace/recent_files.rs` loads and clears recent files asynchronously with `recent_files_gen`.
  - `[LOW-8a]`–`[LOW-8f]`: Verified in `filter/matcher.rs` (`RepeatState` allowing bounded `(?=(?:\d{1,3}\.){3}\d{1,3})` while rejecting nested/variable repeats), `filter/engine.rs` + `filter/heatmap.rs` (`ResolvedStyle.is_matched`), `constants.rs` (`is_ascii_hexdigit`), `buffer/column.rs` (`tag: None` on empty tags), `workspace/filters.rs` (`apply_imported_filter_tabs` empty rejection), and `workspace/adb.rs` (`has_auto_sampled_facets` one-shot guard).
- **Column View Scrollbar Geometry & CLI & Defective Core Tests**:
  - Column View (`components/log_view.rs:666-690, 1377-1418`) renders `render_column_header` (`24px`) + `15px` right corner spacer above `[list_el | scrollbar_track]`.
  - CLI (`cli.rs`, `lib.rs:546-628`) handles `--help`/`-h`, `--version`/`-V`/`-v`, `--`, unknown flags (exit 2), 0 files (ADB scan), 1 file (`open_file`), and 2+ files (`open_merged_files`).
  - Defective core tests fixed: `engine_tests.rs:227-315` uses embedded `AUTOMOTIVE_PROFILE_JSON` with unconditional assertions; `challenger_core_stress_tests.rs:606-648` drops `catch_unwind` and asserts `EpsTimelineData::compute` directly over `[i64::MIN, i64::MIN+100]` and `[i64::MIN+1, i64::MAX-1]`.

### 4.3 Final Quality Certification

- **Overall Quality Rating**: High (all 5 High, 10 Medium, 8+6 Low fixes, Column View geometry alignment, CLI handling, and both defective core tests verified directly against source and test execution; 1 minor non-blocking note on inline facet toggle `modified` flag in `dock.rs:171-186`).
- **System Completeness**: `100%` of `spec-fix-all-bugs.md` implemented and verified.
- **Certification Status**: **`PASS`**

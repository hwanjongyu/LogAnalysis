# 📐 Specification: Comprehensive P1 & P2 Remediation

- **Goal**: Full remediation of all Priority 1 (Architecture & Concurrency) and Priority 2 (Ergonomics & Cleanup) issues identified in `docs/multi_model_review_report.md`.
- **Target Crates**: `crates/loganalysis-core`, `crates/loganalysis-gui`
- **Execution Mode**: Autonomous Goal Execution (`/goal`)

---

## 1. Scope of Work

### Priority 1 (P1): Performance & Architecture Hardening
1. **[P1-1] Decouple `cancel_generation` Tokens**:
   - Separate `cancel_generation: Arc<AtomicU64>` in `workspace.rs` into:
     - `filter_cancel_gen: Arc<AtomicU64>` (for Rayon background filter execution)
     - `search_cancel_gen: Arc<AtomicU64>` (for cyclic search execution)
   - Prevents search execution from erroneously aborting active background filtering passes.
2. **[P1-2] Native `Entity<TextInput>` in Find Bar**:
   - Embed an `Entity<TextInput>` in `find_bar.rs` matching the implementation in `filter_edit_modal.rs`.
   - Wire `TextInputEvent::Submit` to `find_next(cx)` and `TextInputEvent::Cancel` to closing search.
   - Remove fragile simulated character appending and hardcoded US-shift translation tables.
   - Guarantees full Korean/CJK IME composition, arrow keys, and clipboard shortcuts in search.
3. **[P1-3] Bounded Line Reader in `stream_log_lines`**:
   - In `crates/loganalysis-core/src/adb/logcat.rs`, cap line reading at 64 KB (`MAX_LINE_BYTES = 65,536`).
   - Defend against memory exhaustion on corrupt binary stream data without newline delimiters.
4. **[P1-4] Synchronize Search Matcher on Query Changes**:
   - Ensure `update_cached_search_matcher()` is called whenever search text or options change.

### Priority 2 (P2): Ergonomics, Cleanup & Precision
1. **[P2-1] East Asian Full-Width Character Width Measurement**:
   - Add `unicode-width` dependency to `crates/loganalysis-core`.
   - Use `unicode_width::UnicodeWidthStr::width(str)` in `StreamLogBuffer` and `FileLogBuffer` to correctly calculate `max_line_len` in visual monospace columns.
2. **[P2-2] Clean Dead Code**:
   - Remove unused error variant `FilterError::InvalidColor` in `crates/loganalysis-core/src/error.rs`.
   - Clean up unused `tab_index` field in `PreparedFilter`.
3. **[P2-3] Asynchronous Filter Profile Loading**:
   - In `crates/loganalysis-gui/src/workspace.rs`, dispatch `std::fs::read_to_string` to `cx.background_executor().spawn(...)`.
   - Eliminate UI thread disk I/O when importing large JSON profiles.

---

## 2. Verification Criteria
1. All unit and integration tests compile and pass via `cargo test --workspace`.
2. Find Bar operates natively with `Entity<TextInput>` and proper IME composition.
3. Independent cancellation tokens verified via concurrent filter/search stress test.
4. Bounded line reader verified with oversized stream line chunk test.
5. CJK width calculation verified with East Asian characters.
6. QA report generated at `docs/qa-fix-p1-p2.md`.

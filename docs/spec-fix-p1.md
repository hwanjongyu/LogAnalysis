# ⚡ P1 Optimization Specification: Hot-Loop Filtering & Ring Buffer

- **Version:** 1.0.0
- **Scope:** Priority 1 Performance Optimizations (`loganalysis-core`)
- **Status:** Approved for Implementation

---

## 1. Problem Statement

1. **Redundant 2x Filter Evaluation**:
   `execute_filtering` evaluated filters inside `evaluate_and_style_line`, and then immediately executed `prepared.iter().any(|p| p.matches(raw_line))` a second time per line, doubling CPU regex operations.
2. **Rayon False Sharing (Cache Line Bouncing)**:
   All Rayon threads called `fetch_add` on adjacent `AtomicUsize` elements inside a shared `filter_counts` vector, triggering continuous L1/L2 cache line invalidations.
3. **`StreamLogBuffer` $O(N)$ Memory Shifting & Stale Lengths**:
   `StreamLogBuffer` was backed by `Vec<String>`. Draining lines copied 200,000 fat pointers ($4.8\text{MB}$) per trim and failed to recalculate `max_line_len` if the longest line was drained.
4. **Repeated Dynamic String Color Lookups**:
   `COLOR_MAP` linear string searches and hex parsing were executed on every matching line instead of being pre-cached at filter preparation time.
5. **Slow Case-Insensitive Literal Matching**:
   Case-insensitive literal matching used an unvectorized byte-by-byte sliding window.

---

## 2. Technical Architecture & Solutions

### 2.1 Single-Pass Line Evaluation & Pre-cached Colors
- Modify `PreparedFilter` to pre-calculate `resolved_bg_hex: Option<u32>` and `resolved_fg_hex: Option<u32>` during compilation.
- Update `evaluate_and_style_line` signature:
  ```rust
  pub fn evaluate_and_style_line(
      raw_line: &[u8],
      prepared_filters: &[PreparedFilter],
      show_only_filtered: bool,
      filter_counts: Option<&mut [usize]>,
  ) -> (bool, ResolvedStyle, bool) // (is_visible, style, is_matched)
  ```
- Eliminate the second matching loop in `execute_filtering`.

### 2.2 Chunk-Local Counter Reduction (Zero False Sharing)
- Each Rayon chunk allocates a local `vec![0usize; filter_count]` and passes `Some(&mut local_counts)` to `evaluate_and_style_line`.
- Local counts are accumulated via simple non-atomic addition (`local_counts[flat_index] += 1`).
- Chunks merge their counts into `combined_counts` sequentially during chunk reduction.

### 2.3 `VecDeque` Ring Buffer & Dynamic Length Resync
- Switch `StreamLogBuffer.lines` to `std::collections::VecDeque<String>`.
- `drain(0..excess)` executes in $O(1)$ head pointer displacement without pointer shifting.
- If any drained line matches `max_line_len`, trigger a resync scan across the remaining deque items to keep horizontal scroll boundaries accurate.

### 2.4 SIMD-Accelerated Regex Builder for Literals
- In `CompiledMatcher::new`, build case-insensitive literals using `regex::bytes::RegexBuilder` with `case_insensitive(true)`, leveraging AVX2/Teddy SIMD vectorization.
- For `FancyRegex`, check `std::str::from_utf8` first to avoid `String::from_utf8_lossy` heap allocations on valid UTF-8 lines.

---

## 3. Verification Plan

1. Run `cargo test --workspace` and verify all tests pass.
2. Add dedicated tests for:
   - `test_stream_buffer_recalculates_max_len_on_drain`
   - `test_single_pass_filtering_and_match_flag`
3. Update `docs/qa-loganalysis-rust-gpui.md` with P1 benchmarks and verification.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use rayon::prelude::*;
use smallvec::SmallVec;

use crate::buffer::LogSource;
use super::matcher::{MultiFilterMatcher, PreparedFilter};
use super::model::FilterRule;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ResolvedStyle {
    pub bg_hex: Option<u32>,
    pub fg_hex: Option<u32>,
    pub is_matched: bool,
}

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
    #[doc(hidden)]
    pub widest_visible_source_idx: Option<usize>,
}

#[inline]
fn decrement_filter_counts(filter_counts: &mut [usize], matches: &[usize]) {
    for &flat_idx in matches {
        if let Some(count) = filter_counts.get_mut(flat_idx) {
            *count = count.saturating_sub(1);
        }
    }
}

impl FilterResult {
    /// Returns the deterministic hit count for a specific rule by UUID, returning 0 if not matched or inactive.
    pub fn count_for_rule(&self, rule_id: &uuid::Uuid) -> usize {
        self.rule_counts.get(rule_id).copied().unwrap_or(0)
    }
}

/// Resolves visibility and final background/foreground style from a list of matched filter indices.
#[inline]
pub fn resolve_style_from_matches(
    prepared_filters: &[PreparedFilter],
    matched_indices: &[usize],
    show_only_filtered: bool,
) -> (bool, ResolvedStyle, bool) {
    // 2. No matching filters
    if matched_indices.is_empty() {
        let is_visible = !show_only_filtered;
        return (
            is_visible,
            ResolvedStyle {
                bg_hex: None,
                fg_hex: Some(0x808080),
                is_matched: false,
            },
            false,
        );
    }

    // 3. Exclude rule wins if the last matching filter is an exclude rule
    let last_matched_idx = *matched_indices.last().unwrap();
    if prepared_filters[last_matched_idx].rule.exclude {
        return (false, ResolvedStyle::default(), true);
    }

    // 4. Independent reverse BG and FG inheritance (using pre-cached hex colors)
    let mut bg_hex = None;
    let mut fg_hex = None;

    for &idx in matched_indices.iter().rev() {
        let pf = &prepared_filters[idx];
        if pf.rule.exclude {
            // Earlier exclude rule acts as an inheritance barrier: stop walking backwards
            // without erasing partial styles already resolved from later include rules.
            break;
        }

        if bg_hex.is_none() {
            bg_hex = pf.resolved_bg_hex;
        }
        if fg_hex.is_none() {
            fg_hex = pf.resolved_fg_hex;
        }

        if bg_hex.is_some() && fg_hex.is_some() {
            break;
        }
    }

    (
        true,
        ResolvedStyle {
            bg_hex,
            fg_hex,
            is_matched: true,
        },
        true,
    )
}

/// Evaluates a single line using a single-pass `MultiFilterMatcher` (Aho-Corasick SIMD).
/// Returns `(is_visible, style, is_matched, matched_filter_flat_indices)`.
#[inline]
pub fn evaluate_and_style_line_with_matcher_full(
    raw_line: &[u8],
    prepared_filters: &[PreparedFilter],
    matcher: &MultiFilterMatcher,
    show_only_filtered: bool,
    filter_counts: Option<&mut [usize]>,
) -> (bool, ResolvedStyle, bool, SmallVec<[usize; 2]>) {
    if prepared_filters.is_empty() {
        return (
            true,
            ResolvedStyle {
                bg_hex: None,
                fg_hex: Some(0x808080),
                is_matched: false,
            },
            false,
            SmallVec::new(),
        );
    }

    let mut matched_indices = SmallVec::<[usize; 8]>::new();
    matcher.find_matches(raw_line, prepared_filters, &mut matched_indices);

    let mut flat_matches = SmallVec::<[usize; 2]>::new();
    for &idx in &matched_indices {
        let pf = &prepared_filters[idx];
        flat_matches.push(pf.flat_index);
    }

    if let Some(counts) = filter_counts {
        for &flat_idx in &flat_matches {
            if flat_idx < counts.len() {
                counts[flat_idx] += 1;
            }
        }
    }

    let (visible, style, matched) =
        resolve_style_from_matches(prepared_filters, &matched_indices, show_only_filtered);
    (visible, style, matched, flat_matches)
}

/// Evaluates a single line using a single-pass `MultiFilterMatcher` (Aho-Corasick SIMD).
/// Returns `(is_visible, style, is_matched)`.
#[inline]
pub fn evaluate_and_style_line_with_matcher(
    raw_line: &[u8],
    prepared_filters: &[PreparedFilter],
    matcher: &MultiFilterMatcher,
    show_only_filtered: bool,
    filter_counts: Option<&mut [usize]>,
) -> (bool, ResolvedStyle, bool) {
    let (v, s, m, _) = evaluate_and_style_line_with_matcher_full(
        raw_line,
        prepared_filters,
        matcher,
        show_only_filtered,
        filter_counts,
    );
    (v, s, m)
}

/// Evaluates a single line against prepared filters according to LogAnalysis specifications.
/// Returns `(is_visible, style, is_matched)`.
#[inline]
pub fn evaluate_and_style_line(
    raw_line: &[u8],
    prepared_filters: &[PreparedFilter],
    show_only_filtered: bool,
    mut filter_counts: Option<&mut [usize]>,
) -> (bool, ResolvedStyle, bool) {
    // 1. Edge Case: If there are no active filters, display all lines with default styling
    if prepared_filters.is_empty() {
        return (
            true,
            ResolvedStyle {
                bg_hex: None,
                fg_hex: Some(0x808080),
                is_matched: false,
            },
            false,
        );
    }

    let mut matched_indices = SmallVec::<[usize; 8]>::new();
    for (idx, pf) in prepared_filters.iter().enumerate() {
        if pf.matches(raw_line) {
            if let Some(counts) = filter_counts.as_deref_mut() {
                if pf.flat_index < counts.len() {
                    counts[pf.flat_index] += 1; // Pure local non-atomic addition, zero false sharing
                }
            }
            matched_indices.push(idx);
        }
    }

    resolve_style_from_matches(prepared_filters, &matched_indices, show_only_filtered)
}

/// Executes parallel filtering across a log source using Rayon.
pub fn execute_filtering(
    source: &(dyn LogSource + Sync),
    filters: &[FilterRule],
    show_only_filtered: bool,
    cancel_token: Option<&Arc<AtomicU64>>,
    current_generation: u64,
) -> Option<FilterResult> {
    let line_count = source.line_count();
    let filter_count = filters.len();

    // Prepare filter matchers and compile SIMD multi-pattern automaton
    let mut prepared = Vec::new();
    for (idx, rule) in filters.iter().enumerate() {
        if rule.active && !rule.text.is_empty() {
            if let Ok(pf) = PreparedFilter::new(rule.clone(), idx) {
                prepared.push(pf);
            }
        }
    }
    let multi_matcher = MultiFilterMatcher::new(&prepared);

    if line_count == 0 {
        let mut rule_counts = std::collections::HashMap::with_capacity(filters.len());
        for rule in filters {
            rule_counts.insert(rule.id, 0);
        }
        return Some(FilterResult {
            visible_indices: Vec::new(),
            visible_styles: Vec::new(),
            visible_filter_matches: Vec::new(),
            excluded_filter_matches: Vec::new(),
            filter_counts: vec![0; filter_count],
            rule_counts,
            total_matched_lines: 0,
            widest_visible_text: String::new(),
            widest_visible_length: 0,
            widest_visible_source_idx: None,
        });
    }

    // Chunk size for parallel processing (8192 lines per chunk)
    const CHUNK_SIZE: usize = 8192;
    let num_chunks = line_count.div_ceil(CHUNK_SIZE);

    let chunk_results: Option<Vec<_>> = (0..num_chunks)
        .into_par_iter()
        .map(|chunk_idx| {
            if let Some(token) = cancel_token {
                if token.load(Ordering::Relaxed) != current_generation {
                    return None;
                }
            }

            let start = chunk_idx * CHUNK_SIZE;
            let end = (start + CHUNK_SIZE).min(line_count);

            let mut visible_indices = Vec::with_capacity(end - start);
            let mut visible_styles = Vec::with_capacity(end - start);
            let mut visible_filter_matches = Vec::with_capacity(end - start);
            let mut excluded_filter_matches: Vec<(usize, SmallVec<[usize; 2]>)> = Vec::new();
            let mut chunk_matched_count = 0;
            let mut local_max_len = 0;
            let mut local_longest_idx = None;
            let mut local_counts = vec![0usize; filter_count];

            for line_idx in start..end {
                let raw_line = source.get_raw_line(line_idx).unwrap_or(b"");
                let (is_visible, style, is_matched, flat_matches) = evaluate_and_style_line_with_matcher_full(
                    raw_line,
                    &prepared,
                    &multi_matcher,
                    show_only_filtered,
                    Some(&mut local_counts),
                );

                if is_visible {
                    visible_indices.push(line_idx);
                    visible_styles.push(style);
                    visible_filter_matches.push(flat_matches);

                    let len = crate::buffer::measure_display_width(raw_line);
                    if len > local_max_len {
                        local_max_len = len;
                        local_longest_idx = Some(line_idx);
                    }
                } else if !flat_matches.is_empty() {
                    excluded_filter_matches.push((line_idx, flat_matches));
                }

                // Match counting in a single pass without re-evaluating regexes
                if is_matched && is_visible {
                    chunk_matched_count += 1;
                }
            }

            Some((
                visible_indices,
                visible_styles,
                visible_filter_matches,
                excluded_filter_matches,
                chunk_matched_count,
                local_max_len,
                local_longest_idx,
                local_counts,
            ))
        })
        .collect();

    let chunks = chunk_results?;

    // Combine results from chunks
    let total_visible: usize = chunks.iter().map(|c| c.0.len()).sum();
    let total_excluded: usize = chunks.iter().map(|c| c.3.len()).sum();
    let mut combined_indices = Vec::with_capacity(total_visible);
    let mut combined_styles = Vec::with_capacity(total_visible);
    let mut combined_filter_matches = Vec::with_capacity(total_visible);
    let mut combined_excluded_matches = Vec::with_capacity(total_excluded);
    let mut combined_counts = vec![0usize; filter_count];
    let mut total_matched_lines = 0;
    let mut max_len = 0;
    let mut longest_idx = None;

    for (
        indices,
        styles,
        matches,
        excluded_matches,
        matched_count,
        local_max,
        local_idx,
        local_counts,
    ) in chunks
    {
        combined_indices.extend(indices);
        combined_styles.extend(styles);
        combined_filter_matches.extend(matches);
        combined_excluded_matches.extend(excluded_matches);
        total_matched_lines += matched_count;
        for (i, &cnt) in local_counts.iter().enumerate() {
            combined_counts[i] += cnt;
        }
        if local_max > max_len {
            max_len = local_max;
            longest_idx = local_idx;
        }
    }

    let (widest_visible_text, widest_visible_length) = if let Some(idx) = longest_idx {
        let text = source
            .get_line(idx)
            .map(|s| s.into_owned())
            .unwrap_or_default();
        (text, max_len)
    } else {
        (String::new(), 0)
    };

    let mut rule_counts = std::collections::HashMap::with_capacity(filters.len());
    for (rule, &cnt) in filters.iter().zip(combined_counts.iter()) {
        rule_counts.insert(rule.id, cnt);
    }

    Some(FilterResult {
        visible_indices: combined_indices,
        visible_styles: combined_styles,
        visible_filter_matches: combined_filter_matches,
        excluded_filter_matches: combined_excluded_matches,
        filter_counts: combined_counts,
        rule_counts,
        total_matched_lines,
        widest_visible_text,
        widest_visible_length,
        widest_visible_source_idx: longest_idx,
    })
}

/// Nearest-Neighbor binary search anchor preservation algorithm.
/// Given a previously focused source line index `target_idx` and the new list of `visible_indices`,
/// finds the row index in `visible_indices` closest to `target_idx`.
pub fn find_nearest_anchor_row(visible_indices: &[usize], target_idx: usize) -> Option<usize> {
    if visible_indices.is_empty() {
        return None;
    }

    match visible_indices.binary_search(&target_idx) {
        Ok(exact) => Some(exact),
        Err(pos) => {
            if pos == 0 {
                Some(0)
            } else if pos >= visible_indices.len() {
                Some(visible_indices.len() - 1)
            } else {
                let dist_curr = visible_indices[pos] - target_idx;
                let dist_prev = target_idx - visible_indices[pos - 1];
                if dist_curr <= dist_prev {
                    Some(pos)
                } else {
                    Some(pos - 1)
                }
            }
        }
    }
}

/// Incrementally updates a `FilterResult` when a new batch of lines is appended to a streaming log buffer.
/// Evaluates newly added lines against pre-compiled matchers and updates `result` incrementally.
///
/// If `trimmed_count > 0` (head of rolling buffer was trimmed), removes visible lines that fell off
/// and shifts remaining indices down by `trimmed_count`.
/// Note: callers maintaining `FilterResult::rule_counts` should sync `rule_counts` from
/// `filter_counts` (or use [`CompiledStreamFilters::append_streaming_batch`]).
pub fn append_streaming_batch_with_matcher(
    result: &mut FilterResult,
    source: &(dyn LogSource + Sync),
    new_lines_range: std::ops::Range<usize>,
    trimmed_count: usize,
    prepared: &[PreparedFilter],
    multi_matcher: &MultiFilterMatcher,
    show_only_filtered: bool,
) {
    let required_counts = prepared
        .iter()
        .map(|pf| pf.flat_index + 1)
        .max()
        .unwrap_or(0);
    if result.filter_counts.len() < required_counts {
        result.filter_counts.resize(required_counts, 0);
    }

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
                        decrement_filter_counts(&mut result.filter_counts, &matches);
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

        if result.visible_indices.is_empty() {
            result.widest_visible_length = 0;
            result.widest_visible_text.clear();
            result.widest_visible_source_idx = None;
        } else {
            let widest_was_dropped = match result.widest_visible_source_idx {
                Some(w_idx) if w_idx < trimmed_count => {
                    result.widest_visible_source_idx = None;
                    true
                }
                Some(w_idx) => {
                    result.widest_visible_source_idx = Some(w_idx - trimmed_count);
                    false
                }
                None => result.widest_visible_length > 0,
            };

            if widest_was_dropped {
                let mut max_w = 0usize;
                let mut max_idx = None;
                for &vis_src_idx in &result.visible_indices {
                    let w = source.get_measured_length(vis_src_idx);
                    if w > max_w {
                        max_w = w;
                        max_idx = Some(vis_src_idx);
                    }
                }
                result.widest_visible_length = max_w;
                result.widest_visible_source_idx = max_idx;
                result.widest_visible_text = max_idx
                    .and_then(|idx| source.get_line(idx).map(|s| s.into_owned()))
                    .unwrap_or_default();
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
                    decrement_filter_counts(&mut result.filter_counts, &matches);
                }
            }
            for (idx, _) in &mut result.excluded_filter_matches {
                *idx = idx.saturating_sub(trimmed_count);
            }
        }
    }

    if new_lines_range.is_empty() {
        return;
    }

    for line_idx in new_lines_range {
        let Some(raw_line) = source.get_raw_line(line_idx) else {
            continue;
        };
        let (is_visible, style, is_matched, flat_matches) =
            evaluate_and_style_line_with_matcher_full(
                raw_line,
                prepared,
                multi_matcher,
                show_only_filtered,
                Some(&mut result.filter_counts),
            );

        if is_matched && is_visible {
            result.total_matched_lines += 1;
        }

        if is_visible {
            let line_len = source.get_measured_length(line_idx);
            if line_len > result.widest_visible_length {
                result.widest_visible_length = line_len;
                result.widest_visible_source_idx = Some(line_idx);
                if let Some(cow) = source.get_line(line_idx) {
                    result.widest_visible_text = cow.into_owned();
                }
            }
            result.visible_indices.push(line_idx);
            result.visible_styles.push(style);
            result.visible_filter_matches.push(flat_matches);
        } else if !flat_matches.is_empty() {
            result.excluded_filter_matches.push((line_idx, flat_matches));
        }
    }
}

/// Pre-compiled filter set cached across streaming batches to avoid recompiling regexes
/// and rebuilding the Aho-Corasick automaton on every periodic stream flush.
#[derive(Clone)]
pub struct CompiledStreamFilters {
    rules: Vec<FilterRule>,
    prepared: Vec<PreparedFilter>,
    matcher: MultiFilterMatcher,
}

impl std::fmt::Debug for CompiledStreamFilters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledStreamFilters")
            .field("rules_len", &self.rules.len())
            .field("prepared_len", &self.prepared.len())
            .finish()
    }
}

impl CompiledStreamFilters {
    /// Compiles `filters` into `PreparedFilter` entries and a [`MultiFilterMatcher`].
    ///
    /// Indices preserve `0..filters.len()` slots (`flat_index`) so `filter_counts` and
    /// `rule_counts` remain aligned even when inactive or invalid-regex rules are present.
    pub fn new(filters: &[FilterRule]) -> Self {
        let rules = filters.to_vec();
        let mut prepared = Vec::new();
        for (idx, rule) in rules.iter().enumerate() {
            if rule.active && !rule.text.is_empty() {
                if let Ok(pf) = PreparedFilter::new(rule.clone(), idx) {
                    prepared.push(pf);
                }
            }
        }
        let matcher = MultiFilterMatcher::new(&prepared);
        Self {
            rules,
            prepared,
            matcher,
        }
    }

    /// Compiled filter rules currently active.
    pub fn prepared(&self) -> &[PreparedFilter] {
        &self.prepared
    }

    /// Shared SIMD multi-pattern automaton covering literal filters.
    pub fn matcher(&self) -> &MultiFilterMatcher {
        &self.matcher
    }

    /// Complete filter rule slice used to construct this compiled set (defining `flat_index` alignment).
    pub fn rules(&self) -> &[FilterRule] {
        &self.rules
    }

    /// Incrementally updates `result` using the cached compiled matchers and synchronizes `rule_counts`.
    pub fn append_streaming_batch(
        &self,
        result: &mut FilterResult,
        source: &(dyn LogSource + Sync),
        new_lines_range: std::ops::Range<usize>,
        trimmed_count: usize,
        show_only_filtered: bool,
    ) {
        if result.filter_counts.len() != self.rules.len() {
            result.filter_counts.resize(self.rules.len(), 0);
        }

        append_streaming_batch_with_matcher(
            result,
            source,
            new_lines_range,
            trimmed_count,
            &self.prepared,
            &self.matcher,
            show_only_filtered,
        );

        // Maintain [INV-09] rule_counts parity on both trim and append paths
        for (rule, &cnt) in self.rules.iter().zip(result.filter_counts.iter()) {
            result.rule_counts.insert(rule.id, cnt);
        }
    }
}

/// Incrementally updates an existing `FilterResult` without rescanning old unchanged lines.
///
/// If `trimmed_count > 0` (head of rolling buffer was trimmed), removes visible lines that fell off
/// and shifts remaining indices down by `trimmed_count`.
/// Evaluates only the newly added lines (`new_lines_range`) against the active filters,
/// appending matches directly to `visible_indices` and `visible_styles`.
pub fn append_streaming_batch(
    result: &mut FilterResult,
    source: &(dyn LogSource + Sync),
    new_lines_range: std::ops::Range<usize>,
    trimmed_count: usize,
    filters: &[FilterRule],
    show_only_filtered: bool,
) {
    CompiledStreamFilters::new(filters).append_streaming_batch(
        result,
        source,
        new_lines_range,
        trimmed_count,
        show_only_filtered,
    );
}

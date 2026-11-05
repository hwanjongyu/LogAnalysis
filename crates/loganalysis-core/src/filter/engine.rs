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
}

#[derive(Clone, Debug, Default)]
pub struct FilterResult {
    pub visible_indices: Vec<usize>,
    pub visible_styles: Vec<ResolvedStyle>,
    pub filter_counts: Vec<usize>,
    pub rule_counts: std::collections::HashMap<uuid::Uuid, usize>,
    pub total_matched_lines: usize,
    pub widest_visible_text: String,
    pub widest_visible_length: usize,
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
            // Any exclude rule encountered backwards resets styles and stops
            bg_hex = None;
            fg_hex = None;
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

    (true, ResolvedStyle { bg_hex, fg_hex }, true)
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
    if prepared_filters.is_empty() {
        return (
            true,
            ResolvedStyle {
                bg_hex: None,
                fg_hex: Some(0x808080),
            },
            false,
        );
    }

    let mut matched_indices = SmallVec::<[usize; 8]>::new();
    matcher.find_matches(raw_line, prepared_filters, &mut matched_indices);

    if let Some(counts) = filter_counts {
        for &idx in &matched_indices {
            let pf = &prepared_filters[idx];
            if pf.flat_index < counts.len() {
                counts[pf.flat_index] += 1;
            }
        }
    }

    resolve_style_from_matches(prepared_filters, &matched_indices, show_only_filtered)
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
            if let Ok(pf) = PreparedFilter::new(rule.clone(), idx, 0) {
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
            filter_counts: vec![0; filter_count],
            rule_counts,
            total_matched_lines: 0,
            widest_visible_text: String::new(),
            widest_visible_length: 0,
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
            let mut chunk_matched_count = 0;
            let mut local_max_len = 0;
            let mut local_longest_idx = None;
            let mut local_counts = vec![0usize; filter_count];

            for line_idx in start..end {
                let raw_line = source.get_raw_line(line_idx).unwrap_or(b"");
                let (is_visible, style, is_matched) = evaluate_and_style_line_with_matcher(
                    raw_line,
                    &prepared,
                    &multi_matcher,
                    show_only_filtered,
                    Some(&mut local_counts),
                );

                if is_visible {
                    visible_indices.push(line_idx);
                    visible_styles.push(style);

                    let len = raw_line.len();
                    if len > local_max_len {
                        local_max_len = len;
                        local_longest_idx = Some(line_idx);
                    }
                }

                // Match counting in a single pass without re-evaluating regexes
                if is_matched && is_visible {
                    chunk_matched_count += 1;
                }
            }

            Some((
                visible_indices,
                visible_styles,
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
    let mut combined_indices = Vec::with_capacity(total_visible);
    let mut combined_styles = Vec::with_capacity(total_visible);
    let mut combined_counts = vec![0usize; filter_count];
    let mut total_matched_lines = 0;
    let mut max_len = 0;
    let mut longest_idx = None;

    for (indices, styles, matched_count, local_max, local_idx, local_counts) in chunks {
        combined_indices.extend(indices);
        combined_styles.extend(styles);
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
        let len = source.get_measured_length(idx);
        (text, len)
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
        filter_counts: combined_counts,
        rule_counts,
        total_matched_lines,
        widest_visible_text,
        widest_visible_length,
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
    if trimmed_count > 0 && !result.visible_indices.is_empty() {
        let drop_count = result.visible_indices.partition_point(|&idx| idx < trimmed_count);
        if drop_count > 0 {
            result.visible_indices.drain(0..drop_count);
            result.visible_styles.drain(0..drop_count);
        }
        for idx in &mut result.visible_indices {
            *idx = idx.saturating_sub(trimmed_count);
        }
    }

    if new_lines_range.is_empty() {
        return;
    }

    let filter_count = filters.len();
    let mut prepared = Vec::new();
    for (idx, rule) in filters.iter().enumerate() {
        if rule.active && !rule.text.is_empty() {
            if let Ok(pf) = PreparedFilter::new(rule.clone(), idx, 0) {
                prepared.push(pf);
            }
        }
    }
    let multi_matcher = MultiFilterMatcher::new(&prepared);

    if result.filter_counts.len() < filter_count {
        result.filter_counts.resize(filter_count, 0);
    }

    for line_idx in new_lines_range {
        let raw_line = source.get_raw_line(line_idx).unwrap_or(b"");
        let (is_visible, style, is_matched) = evaluate_and_style_line_with_matcher(
            raw_line,
            &prepared,
            &multi_matcher,
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
                if let Some(cow) = source.get_line(line_idx) {
                    result.widest_visible_text = cow.into_owned();
                }
            }
            result.visible_indices.push(line_idx);
            result.visible_styles.push(style);
        }
    }

    for (rule, &cnt) in filters.iter().zip(result.filter_counts.iter()) {
        result.rule_counts.insert(rule.id, cnt);
    }
}

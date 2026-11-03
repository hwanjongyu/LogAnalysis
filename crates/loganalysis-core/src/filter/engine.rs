use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use rayon::prelude::*;
use smallvec::SmallVec;

use crate::buffer::LogSource;
use super::matcher::PreparedFilter;
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
    pub total_matched_lines: usize,
    pub widest_visible_text: String,
    pub widest_visible_length: usize,
}

/// Evaluates a single line against prepared filters according to LogAnalysis specifications.
#[inline]
pub fn evaluate_and_style_line(
    raw_line: &[u8],
    prepared_filters: &[PreparedFilter],
    show_only_filtered: bool,
    filter_counts: Option<&[AtomicUsize]>,
) -> (bool, ResolvedStyle) {
    // 1. Edge Case: If there are no active filters, display all lines with default styling
    if prepared_filters.is_empty() {
        return (
            true,
            ResolvedStyle {
                bg_hex: None,
                fg_hex: Some(0x808080),
            },
        );
    }

    let mut matched_indices = SmallVec::<[usize; 8]>::new();
    for (idx, pf) in prepared_filters.iter().enumerate() {
        if pf.matches(raw_line) {
            if let Some(counts) = filter_counts {
                counts[pf.flat_index].fetch_add(1, Ordering::Relaxed);
            }
            matched_indices.push(idx);
        }
    }

    // 2. No matching filters
    if matched_indices.is_empty() {
        let is_visible = !show_only_filtered;
        return (
            is_visible,
            ResolvedStyle {
                bg_hex: None,
                fg_hex: Some(0x808080),
            },
        );
    }

    // 3. Exclude rule wins if the last matching filter is an exclude rule
    let last_matched_idx = *matched_indices.last().unwrap();
    if prepared_filters[last_matched_idx].rule.exclude {
        return (false, ResolvedStyle::default());
    }

    // 4. Independent reverse BG and FG inheritance
    let mut bg_hex = None;
    let mut fg_hex = None;

    for &idx in matched_indices.iter().rev() {
        let rule = &prepared_filters[idx].rule;
        if rule.exclude {
            // Any exclude rule encountered backwards resets styles and stops
            bg_hex = None;
            fg_hex = None;
            break;
        }

        if bg_hex.is_none() {
            bg_hex = rule.resolved_bg_hex();
        }
        if fg_hex.is_none() {
            fg_hex = rule.resolved_fg_hex();
        }

        if bg_hex.is_some() && fg_hex.is_some() {
            break;
        }
    }

    (true, ResolvedStyle { bg_hex, fg_hex })
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

    // Prepare filter matchers
    let mut prepared = Vec::new();
    for (idx, rule) in filters.iter().enumerate() {
        if rule.active && !rule.text.is_empty() {
            if let Ok(pf) = PreparedFilter::new(rule.clone(), idx, 0) {
                prepared.push(pf);
            }
        }
    }

    let filter_counts: Vec<AtomicUsize> = (0..filter_count).map(|_| AtomicUsize::new(0)).collect();

    if line_count == 0 {
        return Some(FilterResult {
            visible_indices: Vec::new(),
            visible_styles: Vec::new(),
            filter_counts: vec![0; filter_count],
            total_matched_lines: 0,
            widest_visible_text: String::new(),
            widest_visible_length: 0,
        });
    }

    // Chunk size for parallel processing (e.g. 8192 lines per chunk)
    const CHUNK_SIZE: usize = 8192;
    let num_chunks = (line_count + CHUNK_SIZE - 1) / CHUNK_SIZE;

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

            for line_idx in start..end {
                let raw_line = source.get_raw_line(line_idx).unwrap_or(b"");
                let (is_visible, style) = evaluate_and_style_line(
                    raw_line,
                    &prepared,
                    show_only_filtered,
                    Some(&filter_counts),
                );

                if is_visible {
                    visible_indices.push(line_idx);
                    visible_styles.push(style);

                    let len = source.get_measured_length(line_idx);
                    if len > local_max_len {
                        local_max_len = len;
                        local_longest_idx = Some(line_idx);
                    }
                }

                // If non-empty matches and not excluded, increment matched count
                if !prepared.is_empty() {
                    let is_matched = prepared.iter().any(|p| p.matches(raw_line));
                    if is_matched && is_visible {
                        chunk_matched_count += 1;
                    }
                }
            }

            Some((
                visible_indices,
                visible_styles,
                chunk_matched_count,
                local_max_len,
                local_longest_idx,
            ))
        })
        .collect();

    let chunks = chunk_results?;

    // Combine results from chunks
    let total_visible: usize = chunks.iter().map(|c| c.0.len()).sum();
    let mut combined_indices = Vec::with_capacity(total_visible);
    let mut combined_styles = Vec::with_capacity(total_visible);
    let mut total_matched_lines = 0;
    let mut max_len = 0;
    let mut longest_idx = None;

    for (indices, styles, matched_count, local_max, local_idx) in chunks {
        combined_indices.extend(indices);
        combined_styles.extend(styles);
        total_matched_lines += matched_count;
        if local_max > max_len {
            max_len = local_max;
            longest_idx = local_idx;
        }
    }

    let widest_visible_text = longest_idx
        .and_then(|idx| source.get_line(idx).map(|s| s.into_owned()))
        .unwrap_or_default();

    let final_counts: Vec<usize> = filter_counts
        .into_iter()
        .map(|c| c.into_inner())
        .collect();

    Some(FilterResult {
        visible_indices: combined_indices,
        visible_styles: combined_styles,
        filter_counts: final_counts,
        total_matched_lines,
        widest_visible_text,
        widest_visible_length: max_len,
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

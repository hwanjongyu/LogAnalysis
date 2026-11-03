use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use crate::buffer::LogSource;
use crate::error::SearchError;
use crate::filter::matcher::CompiledMatcher;

#[derive(Clone, Debug)]
pub struct SearchQuery {
    pub pattern: String,
    pub case_sensitive: bool,
    pub is_regex: bool,
    pub forward: bool,
}

pub struct SearchResult {
    pub visible_row: usize,
    pub real_idx: usize,
}

/// Performs cyclic search across visible indices.
/// Returns `Some(SearchResult)` on match, or `None` if not found or cancelled.
pub fn search_in_log(
    source: &(dyn LogSource + Sync),
    visible_indices: &[usize],
    query: &SearchQuery,
    start_row: Option<usize>,
    cancel_token: Option<&Arc<AtomicU64>>,
    current_generation: u64,
    progress_callback: Option<&dyn Fn(usize, usize)>,
) -> Result<Option<SearchResult>, SearchError> {
    let total = visible_indices.len();
    if total == 0 || query.pattern.is_empty() {
        return Ok(None);
    }

    let matcher = CompiledMatcher::new(&query.pattern, query.case_sensitive, query.is_regex)
        .map_err(|e| SearchError::InvalidRegex(e.to_string()))?;

    let mut idx = match start_row {
        Some(r) => {
            if r >= total {
                0
            } else {
                r
            }
        }
        None => {
            if query.forward {
                total - 1
            } else {
                0
            }
        }
    };

    let mut visited_count = 0;

    while visited_count < total {
        // Stride cancel check every 1024 lines to minimize atomic overhead
        if (visited_count & 1023) == 0 {
            if let Some(token) = cancel_token {
                if token.load(Ordering::Relaxed) != current_generation {
                    return Ok(None); // Cancelled
                }
            }
        }

        // Branching index wrapping (avoids modulo % CPU penalty)
        if query.forward {
            idx += 1;
            if idx == total {
                idx = 0;
            }
        } else {
            idx = if idx == 0 { total - 1 } else { idx - 1 };
        }

        let real_idx = visible_indices[idx];
        if let Some(raw_line) = source.get_raw_line(real_idx) {
            if matcher.matches(raw_line) {
                return Ok(Some(SearchResult {
                    visible_row: idx,
                    real_idx,
                }));
            }
        }

        visited_count += 1;
        if visited_count % 10_000 == 0 {
            if let Some(cb) = progress_callback {
                cb(visited_count, total);
            }
        }
    }

    Ok(None)
}

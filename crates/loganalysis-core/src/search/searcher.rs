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
                    return Err(SearchError::Cancelled);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::atomic::AtomicUsize;

    struct VecLogSource {
        lines: Vec<Vec<u8>>,
    }

    impl VecLogSource {
        fn from_strs(items: &[&str]) -> Self {
            Self {
                lines: items.iter().map(|s| s.as_bytes().to_vec()).collect(),
            }
        }
    }

    impl LogSource for VecLogSource {
        fn line_count(&self) -> usize {
            self.lines.len()
        }

        fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
            self.lines.get(idx).map(|v| v.as_slice())
        }
    }

    struct CountingCancelSource {
        lines: Vec<Vec<u8>>,
        reads: AtomicUsize,
        cancel_at_read: Option<usize>,
        cancel_token: Option<Arc<AtomicU64>>,
    }

    impl CountingCancelSource {
        fn new(count: usize, cancel_at_read: Option<usize>, cancel_token: Option<Arc<AtomicU64>>) -> Self {
            Self {
                lines: vec![b"normal nonmatching log line".to_vec(); count],
                reads: AtomicUsize::new(0),
                cancel_at_read,
                cancel_token,
            }
        }
    }

    impl LogSource for CountingCancelSource {
        fn line_count(&self) -> usize {
            self.lines.len()
        }

        fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
            let call_num = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
            if let (Some(trigger), Some(token)) = (self.cancel_at_read, &self.cancel_token) {
                if call_num == trigger {
                    token.fetch_add(1, Ordering::SeqCst);
                }
            }
            self.lines.get(idx).map(|v| v.as_slice())
        }
    }

    fn query(pattern: &str, forward: bool) -> SearchQuery {
        SearchQuery {
            pattern: pattern.to_string(),
            case_sensitive: false,
            is_regex: false,
            forward,
        }
    }

    #[test]
    fn test_search_empty_inputs_and_invalid_regex() {
        let src = VecLogSource::from_strs(&["hello world"]);

        // Empty visible_indices
        let empty_vis = search_in_log(&src, &[], &query("hello", true), None, None, 0, None)
            .expect("empty visible_indices should return Ok(None)");
        assert!(empty_vis.is_none());

        // Empty pattern
        let empty_pat = search_in_log(&src, &[0], &query("", true), None, None, 0, None)
            .expect("empty pattern should return Ok(None)");
        assert!(empty_pat.is_none());

        // Invalid regex pattern
        let bad_regex = SearchQuery {
            pattern: "[unclosed".to_string(),
            case_sensitive: false,
            is_regex: true,
            forward: true,
        };
        let err = search_in_log(&src, &[0], &bad_regex, None, None, 0, None)
            .err()
            .expect("invalid regex must return Err");
        assert!(matches!(err, SearchError::InvalidRegex(_)));
    }

    #[test]
    fn test_search_full_scan_miss_and_non_identity_visible_indices() {
        let src = VecLogSource::from_strs(&["zero", "hit one", "two", "hit three", "four"]);
        let vis = [1, 3];

        let miss = search_in_log(&src, &vis, &query("missing", true), None, None, 0, None)
            .expect("miss should be Ok");
        assert!(miss.is_none());

        let hit = search_in_log(&src, &vis, &query("three", true), Some(0), None, 0, None)
            .expect("search ok")
            .expect("match expected");
        assert_eq!(hit.visible_row, 1);
        assert_eq!(hit.real_idx, 3);
    }

    #[test]
    fn test_search_forward_and_backward_wrap_around_and_start_rows() {
        let src = VecLogSource::from_strs(&["alpha", "needle_1", "beta", "needle_2", "gamma"]);
        let vis: Vec<usize> = (0..5).collect();

        // Forward from None starts at row 0 -> finds needle_1 at visible_row 1
        let f_none = search_in_log(&src, &vis, &query("needle", true), None, None, 0, None)
            .unwrap()
            .unwrap();
        assert_eq!(f_none.visible_row, 1);

        // Forward from last row (total - 1) wraps around to 0 -> finds needle_1 at 1
        let f_wrap = search_in_log(&src, &vis, &query("needle", true), Some(4), None, 0, None)
            .unwrap()
            .unwrap();
        assert_eq!(f_wrap.visible_row, 1);

        // Forward from row 3 wraps past 4 -> 0 -> finds needle_1 at 1
        let f_wrap_mid = search_in_log(&src, &vis, &query("needle", true), Some(3), None, 0, None)
            .unwrap()
            .unwrap();
        assert_eq!(f_wrap_mid.visible_row, 1);

        // Backward from None starts at total - 1 -> finds needle_2 at 3
        let b_none = search_in_log(&src, &vis, &query("needle", false), None, None, 0, None)
            .unwrap()
            .unwrap();
        assert_eq!(b_none.visible_row, 3);

        // Backward from row 0 wraps around to total - 1 -> finds needle_2 at 3
        let b_wrap = search_in_log(&src, &vis, &query("needle", false), Some(0), None, 0, None)
            .unwrap()
            .unwrap();
        assert_eq!(b_wrap.visible_row, 3);

        // Out-of-bounds start_row = Some(total + 5) clamps idx to 0:
        // forward advances to row 1 first; backward wraps to row total - 1 first
        let all_match_src = VecLogSource::from_strs(&["hit_0", "hit_1", "hit_2"]);
        let all_vis = [0, 1, 2];

        let oob_fwd =
            search_in_log(&all_match_src, &all_vis, &query("hit", true), Some(8), None, 0, None)
                .unwrap()
                .unwrap();
        assert_eq!(oob_fwd.visible_row, 1);

        let oob_bwd = search_in_log(
            &all_match_src,
            &all_vis,
            &query("hit", false),
            Some(8),
            None,
            0,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(oob_bwd.visible_row, 2);
    }

    #[test]
    fn test_search_progress_callback_every_10000_lines() {
        let src = CountingCancelSource::new(25_000, None, None);
        let vis: Vec<usize> = (0..25_000).collect();
        let reports = RefCell::new(Vec::new());

        let cb = |visited: usize, total: usize| {
            reports.borrow_mut().push((visited, total));
        };

        let res = search_in_log(
            &src,
            &vis,
            &query("never_matches", true),
            None,
            None,
            0,
            Some(&cb),
        )
        .unwrap();
        assert!(res.is_none());
        assert_eq!(*reports.borrow(), vec![(10_000, 25_000), (20_000, 25_000)]);
        assert_eq!(src.reads.load(Ordering::SeqCst), 25_000);
    }

    #[test]
    fn test_search_deterministic_1024_stride_cancellation() {
        // 1. Pre-stale token cancels immediately at visited_count == 0 with 0 reads
        let stale_token = Arc::new(AtomicU64::new(2));
        let pre_src = CountingCancelSource::new(5_000, None, None);
        let pre_vis: Vec<usize> = (0..5_000).collect();
        let err = search_in_log(
            &pre_src,
            &pre_vis,
            &query("never_matches", true),
            None,
            Some(&stale_token),
            1,
            None,
        )
        .err()
        .expect("stale token must cancel");
        assert!(matches!(err, SearchError::Cancelled));
        assert_eq!(pre_src.reads.load(Ordering::SeqCst), 0);

        // 2. Flip token inside get_raw_line at call #500 -> cancels deterministically at stride 1024
        let mid_token = Arc::new(AtomicU64::new(7));
        let mid_src = CountingCancelSource::new(5_000, Some(500), Some(Arc::clone(&mid_token)));
        let err = search_in_log(
            &mid_src,
            &pre_vis,
            &query("never_matches", true),
            None,
            Some(&mid_token),
            7,
            None,
        )
        .err()
        .expect("mid-scan token change must cancel");
        assert!(matches!(err, SearchError::Cancelled));
        assert_eq!(mid_src.reads.load(Ordering::SeqCst), 1024);

        // 3. Flip token inside progress_callback at visited == 10_000 -> cancels at next 1024 stride (10_240)
        let cb_token = Arc::new(AtomicU64::new(3));
        let cb_src = CountingCancelSource::new(25_000, None, None);
        let cb_vis: Vec<usize> = (0..25_000).collect();
        let cb = |visited: usize, _total: usize| {
            if visited == 10_000 {
                cb_token.fetch_add(1, Ordering::SeqCst);
            }
        };
        let err = search_in_log(
            &cb_src,
            &cb_vis,
            &query("never_matches", true),
            None,
            Some(&cb_token),
            3,
            Some(&cb),
        )
        .err()
        .expect("callback cancellation must abort at next stride");
        assert!(matches!(err, SearchError::Cancelled));
        assert_eq!(cb_src.reads.load(Ordering::SeqCst), 10_240);
    }
}

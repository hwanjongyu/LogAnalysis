use crate::buffer::timestamp::extract_timestamp;
use crate::buffer::LogSource;

/// Finds the visible row index in `source` whose timestamp is closest to `target_micros`.
/// Performs an O(log N) binary search over chronological visible log rows.
pub fn find_nearest_timestamp_row(
    source: &dyn LogSource,
    visible_indices: &[usize],
    target_micros: i64,
) -> Option<usize> {
    if visible_indices.is_empty() {
        return None;
    }

    let mut low = 0usize;
    let mut high = visible_indices.len().saturating_sub(1);
    let mut best_row = 0usize;
    let mut best_diff = i64::MAX;

    // Helper closure to extract timestamp with small local linear probe if current line lacks timestamp
    let get_ts = |vis_idx: usize| -> Option<i64> {
        let src_idx = *visible_indices.get(vis_idx)?;
        let raw = source.get_raw_line(src_idx)?;
        if let Some(t) = extract_timestamp(raw) {
            return Some(t.total_micros);
        }
        // Local probe up to 4 lines forward
        for step in 1..=4 {
            if let Some(&next_src) = visible_indices.get(vis_idx + step) {
                if let Some(r) = source.get_raw_line(next_src) {
                    if let Some(t) = extract_timestamp(r) {
                        return Some(t.total_micros);
                    }
                }
            } else {
                break;
            }
        }
        None
    };

    while low <= high {
        let mid = low + (high - low) / 2;
        if let Some(ts) = get_ts(mid) {
            let diff = (ts - target_micros).abs();
            if diff < best_diff {
                best_diff = diff;
                best_row = mid;
            }

            if ts == target_micros {
                return Some(mid);
            } else if ts < target_micros {
                low = mid + 1;
            } else {
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            }
        } else {
            low = mid + 1;
        }
    }

    Some(best_row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    struct MockLogSource {
        lines: Vec<String>,
    }

    impl LogSource for MockLogSource {
        fn line_count(&self) -> usize {
            self.lines.len()
        }

        fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
            self.lines.get(idx).map(|s| s.as_bytes())
        }

        fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
            self.lines.get(idx).map(|s| Cow::Borrowed(s.as_str()))
        }
    }

    #[test]
    fn test_nearest_timestamp_empty_source() {
        let src = MockLogSource { lines: vec![] };
        assert_eq!(find_nearest_timestamp_row(&src, &[], 1000), None);
    }

    #[test]
    fn test_nearest_timestamp_correlation_exact_and_neighbor() {
        let lines = vec![
            "11-06 18:00:00.000 1 1 I Tag: Msg 0".into(),
            "11-06 18:00:10.000 1 1 I Tag: Msg 1".into(),
            "11-06 18:00:20.000 1 1 I Tag: Msg 2".into(),
            "11-06 18:00:30.000 1 1 I Tag: Msg 3".into(),
            "11-06 18:00:40.000 1 1 I Tag: Msg 4".into(),
        ];
        let src = MockLogSource { lines };
        let vis = vec![0, 1, 2, 3, 4];

        // Target: 18:00:20.000 exactly matches row 2
        let t2 = extract_timestamp(src.get_raw_line(2).unwrap()).unwrap();
        assert_eq!(find_nearest_timestamp_row(&src, &vis, t2.total_micros), Some(2));

        // Target: 18:00:18.000 (+8s from 10s, -2s from 20s) -> closest to row 2
        let target_18s = t2.total_micros - 2_000_000;
        assert_eq!(find_nearest_timestamp_row(&src, &vis, target_18s), Some(2));

        // Target: 18:00:11.000 (+1s from 10s, -9s from 20s) -> closest to row 1
        let target_11s = t2.total_micros - 9_000_000;
        assert_eq!(find_nearest_timestamp_row(&src, &vis, target_11s), Some(1));
    }
}

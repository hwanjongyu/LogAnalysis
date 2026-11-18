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

    let get_ts = |vis_idx: usize| -> Option<i64> {
        let src_idx = *visible_indices.get(vis_idx)?;
        source.get_timestamp_micros(src_idx)
    };

    let mut low = 0usize;
    let mut high = visible_indices.len() - 1;
    let mut best: Option<(usize, u64)> = None;

    let mut update_best = |row: usize, ts: i64| {
        let diff = ts.abs_diff(target_micros);
        match best {
            None => best = Some((row, diff)),
            Some((_, best_diff)) if diff < best_diff => best = Some((row, diff)),
            _ => {}
        }
    };

    while low <= high {
        let mid = low + (high - low) / 2;
        if let Some(ts) = get_ts(mid) {
            update_best(mid, ts);
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
            // Probe outward from `mid` within `[low..=high]` to avoid discarding half on unstamped blocks.
            let max_dist = (mid - low).max(high - mid);
            let mut found = None;
            for d in 1..=max_dist {
                if mid >= low + d {
                    let left = mid - d;
                    if let Some(ts) = get_ts(left) {
                        found = Some((left, ts));
                        break;
                    }
                }
                if mid + d <= high {
                    let right = mid + d;
                    if let Some(ts) = get_ts(right) {
                        found = Some((right, ts));
                        break;
                    }
                }
            }

            let Some((anchor_idx, ts)) = found else {
                break;
            };

            update_best(anchor_idx, ts);
            if ts == target_micros {
                return Some(anchor_idx);
            }

            if anchor_idx <= mid {
                if ts < target_micros {
                    low = mid + 1;
                } else {
                    if anchor_idx == 0 {
                        break;
                    }
                    high = anchor_idx - 1;
                }
            } else if ts < target_micros {
                low = anchor_idx + 1;
            } else {
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            }
        }
    }

    best.map(|(row, _)| row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::merged_buffer::MergedLogBuffer;
    use crate::buffer::timestamp::extract_timestamp;
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

    #[test]
    fn test_correlation_find_nearest_with_merged_buffer_and_i64_min() {
        let file_dec = b"12-31 23:59:58.000 S1 Dec 58\n\
                         12-31 23:59:59.000 S1 Dec 59\n";
        let file_jan = b"01-01 00:00:01.000 S2 Jan 01\n\
                         01-01 00:00:02.000 S2 Jan 02\n";

        let mut dec_dir = std::env::temp_dir();
        dec_dir.push(format!("corr_dec_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&dec_dir, file_dec).unwrap();

        let mut jan_dir = std::env::temp_dir();
        jan_dir.push(format!("corr_jan_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&jan_dir, file_jan).unwrap();

        let merged = MergedLogBuffer::open_multiple(&[dec_dir.clone(), jan_dir.clone()]).unwrap();
        let vis = vec![0, 1, 2, 3];

        // Extreme target_micros (i64::MIN and i64::MAX) must not panic with overflow
        let row_min = find_nearest_timestamp_row(&merged, &vis, i64::MIN);
        assert_eq!(row_min, Some(0)); // Closest to earliest line

        let row_max = find_nearest_timestamp_row(&merged, &vis, i64::MAX);
        assert_eq!(row_max, Some(3)); // Closest to latest line

        // Target midnight transition
        let jan_ts = merged.get_line_timestamp_micros(2).unwrap();
        let found_jan = find_nearest_timestamp_row(&merged, &vis, jan_ts);
        assert_eq!(found_jan, Some(2));

        let _ = std::fs::remove_file(dec_dir);
        let _ = std::fs::remove_file(jan_dir);
    }

    #[test]
    fn test_nearest_timestamp_unstamped_and_block_boundaries() {
        // 100% unstamped source returns None (never Some(0))
        let unstamped = MockLogSource {
            lines: vec!["no ts 0".into(), "no ts 1".into(), "no ts 2".into()],
        };
        assert_eq!(find_nearest_timestamp_row(&unstamped, &[0, 1, 2], 12345), None);

        // Large leading unstructured block before timestamps in right half
        let mut leading_block: Vec<String> = (0..10).map(|i| format!("header {i}")).collect();
        leading_block.push("11-06 18:00:00.000 1 1 I Tag: First".into());
        leading_block.push("11-06 18:00:10.000 1 1 I Tag: Second".into());
        let src_lead = MockLogSource { lines: leading_block };
        let vis_lead: Vec<usize> = (0..src_lead.line_count()).collect();
        let t_first = extract_timestamp(src_lead.get_raw_line(10).unwrap()).unwrap();
        assert_eq!(
            find_nearest_timestamp_row(&src_lead, &vis_lead, t_first.total_micros - 1000),
            Some(10)
        );

        // Large trailing unstructured block after timestamps in left half
        let mut trailing_block = vec![
            "11-06 18:00:00.000 1 1 I Tag: First".into(),
            "11-06 18:00:10.000 1 1 I Tag: Second".into(),
        ];
        trailing_block.extend((0..12).map(|i| format!("\tstack frame {i}")));
        let src_trail = MockLogSource { lines: trailing_block };
        let vis_trail: Vec<usize> = (0..src_trail.line_count()).collect();
        let t_second = extract_timestamp(src_trail.get_raw_line(1).unwrap()).unwrap();
        assert_eq!(
            find_nearest_timestamp_row(&src_trail, &vis_trail, t_second.total_micros + 5000),
            Some(1)
        );
    }

    #[test]
    fn test_single_file_and_stream_rollover_monotonic_correlation() {
        use crate::buffer::{FileLogBuffer, StreamLogBuffer};
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "12-31 23:59:58.000 1 1 I Tag: Dec 58").unwrap();
        writeln!(tmp, "12-31 23:59:59.000 1 1 I Tag: Dec 59").unwrap();
        writeln!(tmp, "01-01 00:00:01.000 1 1 I Tag: Jan 01").unwrap();
        writeln!(tmp, "01-01 00:00:02.000 1 1 I Tag: Jan 02").unwrap();
        tmp.flush().unwrap();

        let file_buf = FileLogBuffer::open(tmp.path()).unwrap();
        let vis = vec![0, 1, 2, 3];
        let ts_jan01 = file_buf.get_timestamp_micros(2).unwrap();
        assert!(ts_jan01 > file_buf.get_timestamp_micros(1).unwrap());
        assert_eq!(find_nearest_timestamp_row(&file_buf, &vis, ts_jan01), Some(2));

        let mut stream_buf = StreamLogBuffer::new(2, 2);
        stream_buf.append_chunk(vec![
            "12-31 23:59:58.000 1 1 I Tag: Dec 58".into(),
            "12-31 23:59:59.000 1 1 I Tag: Dec 59".into(),
            "01-01 00:00:01.000 1 1 I Tag: Jan 01".into(),
            "01-01 00:00:02.000 1 1 I Tag: Jan 02".into(),
        ]);
        let dec59_ts = stream_buf.get_timestamp_micros(1).unwrap();
        let pre_trim_jan01 = stream_buf.get_timestamp_micros(2).unwrap();
        assert!(pre_trim_jan01 > dec59_ts);
        assert_eq!(find_nearest_timestamp_row(&stream_buf, &vis, pre_trim_jan01), Some(2));

        // Trim both Dec 31 lines: surviving Jan 01/02 lines keep normalized rollover anchor
        let trimmed = stream_buf.trim_if_needed().expect("should trim 2 Dec lines");
        assert_eq!(trimmed, 2);
        let post_trim_jan01 = stream_buf.get_timestamp_micros(0).unwrap();
        assert_eq!(post_trim_jan01, pre_trim_jan01);
        assert!(post_trim_jan01 > dec59_ts);
        assert_eq!(
            find_nearest_timestamp_row(&stream_buf, &[0, 1], post_trim_jan01),
            Some(0)
        );
        assert_eq!(find_nearest_timestamp_row(&stream_buf, &[], post_trim_jan01), None);
    }
}

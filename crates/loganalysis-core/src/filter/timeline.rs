use crate::buffer::merged_buffer::{ONE_YEAR_MICROS, YEAR_ROLLOVER_THRESHOLD_MICROS};
use crate::buffer::LogSource;
use crate::filter::engine::ResolvedStyle;
use crate::filter::heatmap::{detect_line_severity, LineSeverity};

/// Single temporal aggregation bucket representing events within a time slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct EpsTimelineBucket {
    pub start_micros: i64,
    pub end_micros: i64,
    pub total_count: u32,
    pub error_count: u32,
    pub warn_count: u32,
    pub start_visible_row: Option<usize>,
    pub is_anomaly: bool,
}

impl EpsTimelineBucket {
    /// Computes the Events Per Second (EPS) rate for this bucket.
    #[inline]
    pub fn eps(&self) -> f64 {
        let duration_us = self.end_micros.abs_diff(self.start_micros).max(1);
        (self.total_count as f64) / (duration_us as f64 / 1_000_000.0)
    }
}

/// Comprehensive throughput profile data across the entire log timeline.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EpsTimelineData {
    pub buckets: Vec<EpsTimelineBucket>,
    pub min_timestamp: i64,
    pub max_timestamp: i64,
    pub peak_count: u32,
    pub peak_eps: f64,
    pub avg_eps: f64,
    pub total_duration_secs: f64,
    pub has_timestamps: bool,
}

impl EpsTimelineData {
    /// Computes temporal throughput buckets and anomaly thresholds.
    pub fn compute(
        source: &dyn LogSource,
        visible_indices: &[usize],
        styles: &[ResolvedStyle],
        bucket_count: usize,
    ) -> Self {
        let total_visible = visible_indices.len();
        if total_visible == 0 || bucket_count == 0 {
            return Self::default();
        }

        let normalize_rollover = |micros: i64, anchor: i64| -> i64 {
            if anchor >= YEAR_ROLLOVER_THRESHOLD_MICROS
                && micros >= 0
                && micros < anchor - YEAR_ROLLOVER_THRESHOLD_MICROS
            {
                micros.saturating_add(ONE_YEAR_MICROS)
            } else {
                micros
            }
        };

        // Pass 1: Scan all visible rows to find anchor, true min_micros, and true max_micros
        let mut first_ts: Option<i64> = None;
        let mut min_micros = i64::MAX;
        let mut max_micros = i64::MIN;

        for &src_idx in visible_indices {
            let Some(raw_micros) = source.get_timestamp_micros(src_idx) else {
                continue;
            };
            let anchor = *first_ts.get_or_insert(raw_micros);
            let micros = normalize_rollover(raw_micros, anchor);
            if micros < min_micros {
                min_micros = micros;
            }
            if micros > max_micros {
                max_micros = micros;
            }
        }

        let Some(anchor_ts) = first_ts else {
            return Self::default();
        };

        let total_duration_us: u128 = max_micros.abs_diff(min_micros) as u128;
        if total_duration_us == 0 {
            return Self {
                min_timestamp: min_micros,
                max_timestamp: max_micros,
                has_timestamps: true,
                total_duration_secs: 0.0,
                ..Default::default()
            };
        }

        let actual_buckets = bucket_count.clamp(8, 128);
        let mut buckets = Vec::with_capacity(actual_buckets);
        for i in 0..actual_buckets {
            let b_start_off = ((i as u128) * total_duration_us) / (actual_buckets as u128);
            let b_end_off = (((i + 1) as u128) * total_duration_us) / (actual_buckets as u128);
            let start_micros = ((min_micros as i128) + (b_start_off as i128)) as i64;
            let end_micros = ((min_micros as i128) + (b_end_off as i128)) as i64;
            buckets.push(EpsTimelineBucket {
                start_micros,
                end_micros,
                total_count: 0,
                error_count: 0,
                warn_count: 0,
                start_visible_row: None,
                is_anomaly: false,
            });
        }

        let mut first_seen = vec![false; actual_buckets];
        let mut total_stamped_lines = 0usize;

        // Pass 2: Bin every stamped visible line using exact u128 proportioning
        for (vis_idx, &src_idx) in visible_indices.iter().enumerate() {
            let Some(raw_micros) = source.get_timestamp_micros(src_idx) else {
                continue;
            };

            let micros = normalize_rollover(raw_micros, anchor_ts);
            let offset = micros.abs_diff(min_micros) as u128;
            let b_idx = ((offset * (actual_buckets as u128)) / total_duration_us)
                .min((actual_buckets - 1) as u128) as usize;

            buckets[b_idx].total_count += 1;
            total_stamped_lines += 1;

            if !first_seen[b_idx] {
                first_seen[b_idx] = true;
                buckets[b_idx].start_visible_row = Some(vis_idx);
            }

            let style = styles.get(vis_idx).copied().unwrap_or_default();
            let raw = source.get_raw_line(src_idx);
            match detect_line_severity(raw, &style) {
                LineSeverity::Error => buckets[b_idx].error_count += 1,
                LineSeverity::Warning => buckets[b_idx].warn_count += 1,
                _ => {}
            }
        }

        // 3. Peak metrics & statistical anomaly detection
        let mut peak_count = 0u32;
        let mut peak_eps = 0.0f64;

        for b in &buckets {
            if b.total_count > peak_count {
                peak_count = b.total_count;
            }
            let eps = b.eps();
            if eps > peak_eps {
                peak_eps = eps;
            }
        }

        let total_duration_secs = (total_duration_us as f64) / 1_000_000.0;
        let avg_eps = if total_duration_secs > 0.0 {
            (total_stamped_lines as f64) / total_duration_secs
        } else {
            0.0
        };

        // 4. Anomaly detection: Flag buckets > mean + 3*sigma
        if actual_buckets > 0 && peak_count > 0 {
            let mean = (total_stamped_lines as f64) / (actual_buckets as f64);
            let variance: f64 = buckets
                .iter()
                .map(|b| {
                    let diff = (b.total_count as f64) - mean;
                    diff * diff
                })
                .sum::<f64>()
                / (actual_buckets as f64);

            let std_dev = variance.sqrt();
            let anomaly_threshold = mean + 3.0 * std_dev;

            if std_dev > 2.0 {
                for b in &mut buckets {
                    if (b.total_count as f64) > anomaly_threshold && b.total_count >= 10 {
                        b.is_anomaly = true;
                    }
                }
            }
        }

        Self {
            buckets,
            min_timestamp: min_micros,
            max_timestamp: max_micros,
            peak_count,
            peak_eps,
            avg_eps,
            total_duration_secs,
            has_timestamps: true,
        }
    }
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
    fn test_timeline_empty_and_no_timestamps() {
        let empty_buf = MockLogSource { lines: vec![] };
        let data = EpsTimelineData::compute(&empty_buf, &[], &[], 60);
        assert!(!data.has_timestamps);
        assert_eq!(data.buckets.len(), 0);

        let non_timestamped = MockLogSource {
            lines: vec!["hello world".into(), "second line without time".into()],
        };
        let data2 = EpsTimelineData::compute(&non_timestamped, &[0, 1], &[], 60);
        assert!(!data2.has_timestamps);
        assert_eq!(data2.buckets.len(), 0);
    }

    #[test]
    fn test_timeline_binning_with_mock_timestamps() {
        let lines = vec![
            "11-06 18:00:00.000 1 1 I Tag: Step 1".into(),
            "11-06 18:00:10.000 1 1 W Tag: Step 2".into(),
            "11-06 18:00:20.000 1 1 E Tag: Step 3 Fatal".into(),
            "11-06 18:00:30.000 1 1 I Tag: Step 4".into(),
        ];
        let buf = MockLogSource { lines };
        let vis = vec![0, 1, 2, 3];
        let styles = vec![ResolvedStyle::default(); 4];

        let data = EpsTimelineData::compute(&buf, &vis, &styles, 10);
        assert!(data.has_timestamps);
        assert!(data.total_duration_secs >= 30.0);
        assert_eq!(data.buckets.len(), 10);
        assert!(data.peak_count >= 1);

        // Find bucket containing the Error line
        let has_err_bucket = data.buckets.iter().any(|b| b.error_count > 0);
        assert!(has_err_bucket);
    }

    #[test]
    fn test_timeline_anomaly_detection_sigma() {
        let mut lines = Vec::new();
        // 50 lines with 1 line/sec
        for s in 0..50 {
            lines.push(format!(
                "11-06 18:00:{:02}.000 1 1 I Tag: Normal log line {s}",
                s
            ));
        }
        // Burst of 100 lines at second 25
        for _ in 0..100 {
            lines.push("11-06 18:00:25.500 1 1 E Tag: Burst error!".into());
        }

        let buf = MockLogSource { lines };
        let count = buf.line_count();
        let vis: Vec<usize> = (0..count).collect();
        let styles = vec![ResolvedStyle::default(); count];

        let data = EpsTimelineData::compute(&buf, &vis, &styles, 20);
        assert!(data.has_timestamps);
        assert!(data.peak_count >= 100);

        // Verify anomaly was detected on the burst bucket
        let has_anomaly = data.buckets.iter().any(|b| b.is_anomaly);
        assert!(
            has_anomaly,
            "Anomaly threshold should trigger for 100x burst"
        );
    }

    #[test]
    fn test_timeline_year_rollover_dec31_to_jan01_nonzero_duration_and_accurate_buckets() {
        let lines = vec![
            "12-31 23:59:50.000 1 1 I Tag: Step 1 Pre-rollover".into(),
            "12-31 23:59:55.000 1 1 W Tag: Step 2 Warning before midnight".into(),
            "01-01 00:00:05.000 1 1 E Tag: Step 3 Error after midnight".into(),
            "01-01 00:00:10.000 1 1 I Tag: Step 4 Steady new year".into(),
        ];
        let buf = MockLogSource { lines };
        let count = buf.line_count();
        let vis: Vec<usize> = (0..count).collect();
        let styles = vec![ResolvedStyle::default(); count];

        let data = EpsTimelineData::compute(&buf, &vis, &styles, 10);
        assert!(data.has_timestamps);
        // Duration: 23:59:50 to 00:00:10 is exactly 20 seconds
        assert!(
            (data.total_duration_secs - 20.0).abs() < 0.1,
            "Total duration should be ~20s across year transition, got {}",
            data.total_duration_secs
        );
        assert_eq!(data.buckets.len(), 10);

        // Check error count in the latter half of the buckets
        let has_err_bucket = data.buckets[5..].iter().any(|b| b.error_count > 0);
        assert!(
            has_err_bucket,
            "Error on Jan 01 must appear in latter buckets"
        );

        // Check warn count in the first half of the buckets
        let has_warn_bucket = data.buckets[..5].iter().any(|b| b.warn_count > 0);
        assert!(
            has_warn_bucket,
            "Warn on Dec 31 must appear in early buckets"
        );

        // Total count across all buckets should equal 4
        let total_binned: u32 = data.buckets.iter().map(|b| b.total_count).sum();
        assert_eq!(total_binned, 4);

        // Average EPS should be non-zero and match 4 / 20.0 = 0.2
        assert!((data.avg_eps - 0.2).abs() < 0.05);
    }

    #[test]
    fn test_timeline_clock_skew_tiny_duration_and_empty_bucket_rows() {
        // Boundary clock skew: first line is +5s, second is +1s, third is +10s, fourth is +6s
        let skewed = MockLogSource {
            lines: vec![
                "11-06 18:00:05.000 1 1 I Tag: out of order 5".into(),
                "11-06 18:00:01.000 1 1 I Tag: earliest 1".into(),
                "11-06 18:00:10.000 1 1 I Tag: latest 10".into(),
                "11-06 18:00:06.000 1 1 I Tag: out of order 6".into(),
            ],
        };
        let vis = vec![0, 1, 2, 3];
        let styles = vec![ResolvedStyle::default(); 4];
        let data = EpsTimelineData::compute(&skewed, &vis, &styles, 10);
        let binned: u32 = data.buckets.iter().map(|b| b.total_count).sum();
        assert_eq!(binned, 4);
        assert!(data.buckets.iter().all(|b| b.start_micros <= b.end_micros));
        assert!(data
            .buckets
            .iter()
            .filter(|b| b.total_count == 0)
            .all(|b| b.start_visible_row.is_none()));

        // Tiny duration < bucket_count (5 us across 8 buckets) must never invert start/end
        let tiny = MockLogSource {
            lines: vec![
                "11-06 18:00:00.000 1 1 I Tag: a".into(),
                "11-06 18:00:00.005 1 1 I Tag: b".into(),
            ],
        };
        // 5000 us vs 16 buckets works; let's also test sub-microsecond via custom source below
        let tiny_data = EpsTimelineData::compute(&tiny, &[0, 1], &styles[..2], 16);
        assert!(tiny_data.buckets.iter().all(|b| b.start_micros <= b.end_micros));

        struct NanoSpanSource;
        impl LogSource for NanoSpanSource {
            fn line_count(&self) -> usize {
                2
            }
            fn get_raw_line(&self, _idx: usize) -> Option<&[u8]> {
                Some(b"event")
            }
            fn get_timestamp_micros(&self, idx: usize) -> Option<i64> {
                Some(if idx == 0 { 100 } else { 103 })
            }
        }
        let nano_data = EpsTimelineData::compute(&NanoSpanSource, &[0, 1], &styles[..2], 32);
        assert_eq!(nano_data.buckets.len(), 32);
        assert!(nano_data.buckets.iter().all(|b| b.start_micros <= b.end_micros));
    }
}

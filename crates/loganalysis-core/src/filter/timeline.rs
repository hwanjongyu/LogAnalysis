use crate::buffer::{extract_timestamp, LogSource};
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
    pub start_visible_row: usize,
    pub is_anomaly: bool,
}

impl EpsTimelineBucket {
    /// Computes the Events Per Second (EPS) rate for this bucket.
    #[inline]
    pub fn eps(&self) -> f64 {
        let duration_us = (self.end_micros - self.start_micros).max(1);
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

        // 1. Establish temporal boundaries [min_micros, max_micros]
        let mut min_t = None;
        let mut max_t = None;

        for &src_idx in visible_indices.iter().take(2000) {
            if let Some(raw) = source.get_raw_line(src_idx) {
                if let Some(t) = extract_timestamp(raw) {
                    min_t = Some(t.total_micros);
                    break;
                }
            }
        }
        if min_t.is_none() {
            for &src_idx in visible_indices.iter() {
                if let Some(raw) = source.get_raw_line(src_idx) {
                    if let Some(t) = extract_timestamp(raw) {
                        min_t = Some(t.total_micros);
                        break;
                    }
                }
            }
        }

        let Some(min_micros) = min_t else {
            return Self::default();
        };

        for &src_idx in visible_indices.iter().rev().take(2000) {
            if let Some(raw) = source.get_raw_line(src_idx) {
                if let Some(t) = extract_timestamp(raw) {
                    max_t = Some(t.total_micros);
                    break;
                }
            }
        }
        if max_t.is_none() {
            for &src_idx in visible_indices.iter().rev() {
                if let Some(raw) = source.get_raw_line(src_idx) {
                    if let Some(t) = extract_timestamp(raw) {
                        max_t = Some(t.total_micros);
                        break;
                    }
                }
            }
        }

        let max_micros = max_t.unwrap_or(min_micros);
        let total_duration_us = max_micros - min_micros;
        if total_duration_us <= 0 {
            return Self {
                min_timestamp: min_micros,
                max_timestamp: max_micros,
                has_timestamps: true,
                total_duration_secs: 0.0,
                ..Default::default()
            };
        }

        let actual_buckets = bucket_count.max(8).min(128);
        let bucket_span_us = (total_duration_us + actual_buckets as i64 - 1) / actual_buckets as i64;
        let bucket_span_us = bucket_span_us.max(1);

        let mut buckets = Vec::with_capacity(actual_buckets);
        for i in 0..actual_buckets {
            let start = min_micros + (i as i64) * bucket_span_us;
            let end = (start + bucket_span_us).min(max_micros);
            buckets.push(EpsTimelineBucket {
                start_micros: start,
                end_micros: end,
                total_count: 0,
                error_count: 0,
                warn_count: 0,
                start_visible_row: 0,
                is_anomaly: false,
            });
        }

        let mut first_seen = vec![false; actual_buckets];
        let mut total_stamped_lines = 0usize;

        // 2. Reduce lines into temporal buckets
        for (vis_idx, &src_idx) in visible_indices.iter().enumerate() {
            let Some(raw) = source.get_raw_line(src_idx) else {
                continue;
            };
            let Some(t) = extract_timestamp(raw) else {
                continue;
            };

            let offset = t.total_micros - min_micros;
            if offset >= 0 && offset <= total_duration_us {
                let b_idx = (offset / bucket_span_us) as usize;
                let b_idx = b_idx.min(actual_buckets - 1);

                buckets[b_idx].total_count += 1;
                total_stamped_lines += 1;

                if !first_seen[b_idx] {
                    first_seen[b_idx] = true;
                    buckets[b_idx].start_visible_row = vis_idx;
                }

                let style = styles.get(vis_idx).copied().unwrap_or_default();
                match detect_line_severity(Some(raw), &style) {
                    LineSeverity::Error => buckets[b_idx].error_count += 1,
                    LineSeverity::Warning => buckets[b_idx].warn_count += 1,
                    _ => {}
                }
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
            lines: vec![
                "hello world".into(),
                "second line without time".into(),
            ],
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
            lines.push(format!("11-06 18:00:{:02}.000 1 1 I Tag: Normal log line {s}", s));
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
        assert!(has_anomaly, "Anomaly threshold should trigger for 100x burst");
    }
}

use rayon::prelude::*;

use crate::buffer::LogSource;
use crate::filter::engine::ResolvedStyle;

/// Visual severity category for heatmap overview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineSeverity {
    Error,
    Warning,
    RuleMatch,
    Normal,
}

/// Aggregated severity bucket corresponding to a physical vertical segment of the scrollbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HeatmapBucket {
    pub error_count: u32,
    pub warn_count: u32,
    pub match_count: u32,
    /// The first visible line index that mapped into this bucket
    pub start_line_index: usize,
    /// The last visible line index that mapped into this bucket
    pub end_line_index: usize,
}

impl HeatmapBucket {
    #[inline]
    pub fn dominant_severity(&self) -> Option<LineSeverity> {
        if self.error_count > 0 {
            Some(LineSeverity::Error)
        } else if self.warn_count > 0 {
            Some(LineSeverity::Warning)
        } else if self.match_count > 0 {
            Some(LineSeverity::RuleMatch)
        } else {
            None
        }
    }

    #[inline]
    pub fn total_count(&self) -> u32 {
        self.error_count + self.warn_count + self.match_count
    }
}

/// Complete aggregated overview data ready for constant-time GPU rendering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeatmapData {
    pub buckets: Vec<HeatmapBucket>,
    pub total_errors: usize,
    pub total_warnings: usize,
    pub total_matches: usize,
    pub total_lines: usize,
}

impl HeatmapData {
    /// Computes spatial overview buckets using parallel SIMD classification when beneficial.
    pub fn compute(
        source: &dyn LogSource,
        visible_indices: &[usize],
        styles: &[ResolvedStyle],
        bucket_count: usize,
    ) -> Self {
        let total_lines = visible_indices.len();
        if total_lines == 0 || bucket_count == 0 {
            return Self::default();
        }

        let actual_buckets = bucket_count.min(total_lines).max(1);

        // Precompute line index ranges for each bucket
        let mut template_buckets = vec![HeatmapBucket::default(); actual_buckets];
        for (b_idx, bucket) in template_buckets.iter_mut().enumerate() {
            let start_line = (b_idx * total_lines) / actual_buckets;
            let end_line = (((b_idx + 1) * total_lines) / actual_buckets).saturating_sub(1);
            bucket.start_line_index = start_line;
            bucket.end_line_index = end_line.max(start_line);
        }

        // Parallel reduction for high-throughput multi-core aggregation
        let merged_buckets = if total_lines >= 40_000 {
            visible_indices
                .par_iter()
                .enumerate()
                .fold(
                    || vec![HeatmapBucket::default(); actual_buckets],
                    |mut acc, (vis_idx, &src_idx)| {
                        let b_idx = (vis_idx * actual_buckets) / total_lines;
                        let b_idx = b_idx.min(actual_buckets - 1);

                        let style = styles.get(vis_idx).copied().unwrap_or_default();
                        let raw_line = source.get_raw_line(src_idx);
                        let severity = detect_line_severity(raw_line, &style);

                        match severity {
                            LineSeverity::Error => acc[b_idx].error_count += 1,
                            LineSeverity::Warning => acc[b_idx].warn_count += 1,
                            LineSeverity::RuleMatch => acc[b_idx].match_count += 1,
                            LineSeverity::Normal => {}
                        }
                        acc
                    },
                )
                .reduce(
                    || vec![HeatmapBucket::default(); actual_buckets],
                    |mut acc1, acc2| {
                        for (a1, a2) in acc1.iter_mut().zip(acc2.iter()) {
                            a1.error_count += a2.error_count;
                            a1.warn_count += a2.warn_count;
                            a1.match_count += a2.match_count;
                        }
                        acc1
                    },
                )
        } else {
            let mut acc = vec![HeatmapBucket::default(); actual_buckets];
            for (vis_idx, &src_idx) in visible_indices.iter().enumerate() {
                let b_idx = (vis_idx * actual_buckets) / total_lines;
                let b_idx = b_idx.min(actual_buckets - 1);

                let style = styles.get(vis_idx).copied().unwrap_or_default();
                let raw_line = source.get_raw_line(src_idx);
                let severity = detect_line_severity(raw_line, &style);

                match severity {
                    LineSeverity::Error => acc[b_idx].error_count += 1,
                    LineSeverity::Warning => acc[b_idx].warn_count += 1,
                    LineSeverity::RuleMatch => acc[b_idx].match_count += 1,
                    LineSeverity::Normal => {}
                }
            }
            acc
        };

        // Combine counts with precomputed ranges
        let mut total_errors = 0;
        let mut total_warnings = 0;
        let mut total_matches = 0;

        for (target, computed) in template_buckets.iter_mut().zip(merged_buckets.into_iter()) {
            target.error_count = computed.error_count;
            target.warn_count = computed.warn_count;
            target.match_count = computed.match_count;

            total_errors += target.error_count as usize;
            total_warnings += target.warn_count as usize;
            total_matches += target.match_count as usize;
        }

        Self {
            buckets: template_buckets,
            total_errors,
            total_warnings,
            total_matches,
            total_lines,
        }
    }
}

/// Evaluates a line's visual severity using resolved filter style priorities first,
/// falling back to raw byte keyword inspection.
#[inline]
pub fn detect_line_severity(raw_line: Option<&[u8]>, style: &ResolvedStyle) -> LineSeverity {
    // 1. Check style colors first (user-defined rules have top precedence)
    if let Some(bg) = style.bg_hex {
        if is_error_color(bg) {
            return LineSeverity::Error;
        } else if is_warn_color(bg) {
            return LineSeverity::Warning;
        }
    }
    if let Some(fg) = style.fg_hex {
        if is_error_color(fg) {
            return LineSeverity::Error;
        } else if is_warn_color(fg) {
            return LineSeverity::Warning;
        }
    }

    // 2. High-speed raw byte heuristics (Android logcat & standard logs)
    if let Some(bytes) = raw_line {
        if has_error_keyword(bytes) {
            return LineSeverity::Error;
        }
        if has_warn_keyword(bytes) {
            return LineSeverity::Warning;
        }
    }

    // 3. User-defined rule match without red/orange color
    if style.bg_hex.is_some() || (style.fg_hex.is_some() && style.fg_hex != Some(0x808080)) {
        return LineSeverity::RuleMatch;
    }

    LineSeverity::Normal
}

#[inline]
fn is_error_color(hex: u32) -> bool {
    let r = ((hex >> 16) & 0xff) as u32;
    let g = ((hex >> 8) & 0xff) as u32;
    let b = (hex & 0xff) as u32;
    // Dominant red component
    r >= 170 && g <= 100 && b <= 100
}

#[inline]
fn is_warn_color(hex: u32) -> bool {
    let r = ((hex >> 16) & 0xff) as u32;
    let g = ((hex >> 8) & 0xff) as u32;
    let b = (hex & 0xff) as u32;
    // Dominant amber/orange/yellow: high red, moderate green, low blue
    r >= 180 && g >= 110 && b <= 90
}

#[inline]
fn has_error_keyword(bytes: &[u8]) -> bool {
    memchr::memmem::find(bytes, b" E ").is_some()
        || memchr::memmem::find(bytes, b" F ").is_some()
        || memchr::memmem::find(bytes, b"E/").is_some()
        || memchr::memmem::find(bytes, b"F/").is_some()
        || memchr::memmem::find(bytes, b"ERROR").is_some()
        || memchr::memmem::find(bytes, b"FATAL").is_some()
        || memchr::memmem::find(bytes, b"Exception").is_some()
        || memchr::memmem::find(bytes, b"CRITICAL").is_some()
}

#[inline]
fn has_warn_keyword(bytes: &[u8]) -> bool {
    memchr::memmem::find(bytes, b" W ").is_some()
        || memchr::memmem::find(bytes, b"W/").is_some()
        || memchr::memmem::find(bytes, b"WARN").is_some()
        || memchr::memmem::find(bytes, b"WARNING").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    struct MockLogSource {
        lines: Vec<&'static str>,
    }

    impl LogSource for MockLogSource {
        fn line_count(&self) -> usize {
            self.lines.len()
        }

        fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
            self.lines.get(idx).map(|s| s.as_bytes())
        }

        fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
            self.lines.get(idx).map(|s| Cow::Borrowed(*s))
        }
    }

    #[test]
    fn test_heatmap_empty_buffer() {
        let source = MockLogSource { lines: vec![] };
        let res = HeatmapData::compute(&source, &[], &[], 100);
        assert_eq!(res.total_lines, 0);
        assert_eq!(res.total_errors, 0);
        assert!(res.buckets.is_empty());
    }

    #[test]
    fn test_heatmap_density_binning() {
        let source = MockLogSource {
            lines: vec![
                "01-01 10:00:00.000 1 1 I Tag: Normal info line 1",
                "01-01 10:00:01.000 1 1 E Tag: Critical NullPointerException",
                "01-01 10:00:02.000 1 1 W Tag: Low memory warning",
                "01-01 10:00:03.000 1 1 D Tag: Debug verbose message",
            ],
        };

        let visible_indices = vec![0, 1, 2, 3];
        let styles = vec![ResolvedStyle::default(); 4];

        let heatmap = HeatmapData::compute(&source, &visible_indices, &styles, 2);
        assert_eq!(heatmap.buckets.len(), 2);
        assert_eq!(heatmap.total_errors, 1);
        assert_eq!(heatmap.total_warnings, 1);

        // First bucket covers line 0 & 1 -> Contains 1 error
        assert_eq!(heatmap.buckets[0].error_count, 1);
        assert_eq!(heatmap.buckets[0].dominant_severity(), Some(LineSeverity::Error));

        // Second bucket covers line 2 & 3 -> Contains 1 warning
        assert_eq!(heatmap.buckets[1].warn_count, 1);
        assert_eq!(heatmap.buckets[1].dominant_severity(), Some(LineSeverity::Warning));
    }

    #[test]
    fn test_heatmap_severity_precedence() {
        let bucket = HeatmapBucket {
            error_count: 1,
            warn_count: 5,
            match_count: 10,
            start_line_index: 0,
            end_line_index: 10,
        };
        // Error must strictly dominate over warning and rule match
        assert_eq!(bucket.dominant_severity(), Some(LineSeverity::Error));
    }

    #[test]
    fn test_style_color_detection() {
        // Red color style overrides normal text
        let red_style = ResolvedStyle {
            bg_hex: Some(0xef4444),
            fg_hex: None,
        };
        assert_eq!(
            detect_line_severity(Some(b"regular info message"), &red_style),
            LineSeverity::Error
        );

        // Yellow/orange color style overrides normal text
        let warn_style = ResolvedStyle {
            bg_hex: Some(0xffaa00),
            fg_hex: None,
        };
        assert_eq!(
            detect_line_severity(Some(b"regular info message"), &warn_style),
            LineSeverity::Warning
        );
    }
}

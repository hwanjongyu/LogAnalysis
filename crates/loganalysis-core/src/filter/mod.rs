pub mod contrast;
pub mod engine;
pub mod heatmap;
pub mod matcher;
pub mod model;
pub mod timeline;

pub use contrast::{contrast_ratio, evaluate_contrast, relative_luminance, ContrastLevel};
pub use engine::{
    append_streaming_batch, append_streaming_batch_with_matcher, evaluate_and_style_line,
    evaluate_and_style_line_with_matcher, evaluate_and_style_line_with_matcher_full,
    execute_filtering, find_nearest_anchor_row, CompiledStreamFilters, FilterResult, ResolvedStyle,
};
pub use heatmap::{detect_line_severity, HeatmapBucket, HeatmapData, LineSeverity};
pub use matcher::{CompiledMatcher, MultiFilterMatcher, PreparedFilter};
pub use model::{parse_filters_json, FilterRule, FilterTabData};
pub use timeline::{EpsTimelineBucket, EpsTimelineData};

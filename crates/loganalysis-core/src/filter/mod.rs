pub mod contrast;
pub mod engine;
pub mod matcher;
pub mod model;

pub use contrast::{contrast_ratio, evaluate_contrast, relative_luminance, ContrastLevel};
pub use engine::{
    append_streaming_batch, evaluate_and_style_line, evaluate_and_style_line_with_matcher,
    execute_filtering, find_nearest_anchor_row, FilterResult, ResolvedStyle,
};
pub use matcher::{CompiledMatcher, MultiFilterMatcher, PreparedFilter};
pub use model::{parse_filters_json, FilterRule, FilterTabData};

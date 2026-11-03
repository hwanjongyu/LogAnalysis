pub mod contrast;
pub mod engine;
pub mod matcher;
pub mod model;

pub use contrast::{contrast_ratio, evaluate_contrast, relative_luminance, ContrastLevel};
pub use engine::{
    evaluate_and_style_line, execute_filtering, find_nearest_anchor_row, FilterResult,
    ResolvedStyle,
};
pub use matcher::{CompiledMatcher, PreparedFilter};
pub use model::{parse_filters_json, FilterRule, FilterTabData};

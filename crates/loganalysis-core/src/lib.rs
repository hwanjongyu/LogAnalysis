pub mod adb;
pub mod buffer;
pub mod constants;
pub mod filter;
pub mod search;

pub use adb::{is_valid_device_serial, scan_adb_devices, start_logcat_stream, LogcatStreamHandle};
pub use buffer::{file_buffer::FileLogBuffer, stream_buffer::StreamLogBuffer, LogSource};
pub use constants::{
    parse_color_spec, ASYNC_SEARCH_THRESHOLD, COLOR_MAP, MAX_MONITOR_LINES, MONITOR_TRIM_BATCH,
    TEXT_COLOR_MAP,
};
pub use filter::{
    contrast_ratio, evaluate_and_style_line, evaluate_contrast, execute_filtering,
    find_nearest_anchor_row, parse_filters_json, CompiledMatcher, ContrastLevel,
    FilterResult, FilterRule, FilterTabData, PreparedFilter, ResolvedStyle,
};
pub use search::{search_in_log, SearchQuery, SearchResult};

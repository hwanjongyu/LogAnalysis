pub mod adb;
pub mod buffer;
pub mod constants;
pub mod error;
pub mod export;
pub mod filter;
pub mod search;

pub use adb::{
    is_valid_device_serial, parse_adb_devices_output, resolve_adb_path, scan_adb_device_details,
    scan_adb_devices, start_logcat_stream, stream_log_lines, AdbDeviceInfo, LogcatStreamHandle,
};
pub use buffer::{
    correlation::find_nearest_timestamp_row, file_buffer::FileLogBuffer, stream_buffer::StreamLogBuffer,
    timestamp::compute_delta_micros, timestamp::extract_timestamp, timestamp::format_delta_micros,
    timestamp::LatencyLevel, timestamp::LogTimestamp, ActiveLogSource, LogSource,
};
pub use constants::{
    parse_color_spec, ASYNC_SEARCH_THRESHOLD, COLOR_MAP, MAX_MONITOR_LINES, MONITOR_TRIM_BATCH,
    TEXT_COLOR_MAP,
};
pub use error::{AdbError, FilterError, SearchError};
pub use export::{escape_html, generate_html_report, Bookmark, BookmarkManager, HtmlReportOptions};
pub use uuid;
pub use filter::{
    append_streaming_batch, contrast_ratio, detect_line_severity, evaluate_and_style_line,
    evaluate_and_style_line_with_matcher, evaluate_contrast, execute_filtering,
    find_nearest_anchor_row, parse_filters_json, CompiledMatcher, ContrastLevel, FilterResult,
    FilterRule, FilterTabData, HeatmapBucket, HeatmapData, LineSeverity, MultiFilterMatcher,
    PreparedFilter, ResolvedStyle, EpsTimelineBucket, EpsTimelineData,
};
pub use search::{
    fuzzy_match, fuzzy_match_detailed, search_in_log, FuzzyMatchResult, SearchQuery, SearchResult,
};

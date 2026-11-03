pub mod adb;
pub mod buffer;
pub mod constants;
pub mod error;
pub mod filter;
pub mod search;
pub mod text_edit;

pub use adb::{
    is_valid_device_serial, resolve_adb_path, scan_adb_devices, start_logcat_stream,
    LogcatStreamHandle,
};
pub use buffer::{
    file_buffer::FileLogBuffer, stream_buffer::StreamLogBuffer, ActiveLogSource, LogSource,
};
pub use constants::{
    parse_color_spec, ASYNC_SEARCH_THRESHOLD, COLOR_MAP, MAX_MONITOR_LINES, MONITOR_TRIM_BATCH,
    TEXT_COLOR_MAP,
};
pub use error::{AdbError, FilterError, SearchError};
pub use filter::{
    contrast_ratio, evaluate_and_style_line, evaluate_contrast, execute_filtering,
    find_nearest_anchor_row, parse_filters_json, CompiledMatcher, ContrastLevel,
    FilterResult, FilterRule, FilterTabData, PreparedFilter, ResolvedStyle,
};
pub use search::{search_in_log, SearchQuery, SearchResult};
pub use text_edit::{
    delete_previous_word_at, insert_at_char_index, remove_at_char_index, remove_before_char_index,
};

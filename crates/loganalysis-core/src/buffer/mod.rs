pub mod column;
pub mod correlation;
pub mod file_buffer;
pub mod stream_buffer;
pub mod timestamp;

use std::borrow::Cow;
use std::sync::Arc;

pub use column::{parse_logcat_column_line, ParsedColumnLine};
pub use correlation::find_nearest_timestamp_row;
pub use file_buffer::FileLogBuffer;
pub use stream_buffer::StreamLogBuffer;
pub use timestamp::{
    compute_delta_micros, extract_timestamp, format_delta_micros, LatencyLevel, LogTimestamp,
};

/// Unified log source enum representing either a file-backed mmap buffer or a streaming ring buffer.
pub enum ActiveLogSource {
    File(FileLogBuffer),
    Stream(StreamLogBuffer),
}

impl std::fmt::Debug for ActiveLogSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File(_) => write!(f, "ActiveLogSource::File"),
            Self::Stream(s) => write!(f, "ActiveLogSource::Stream({:?})", s),
        }
    }
}

impl LogSource for ActiveLogSource {
    #[inline]
    fn line_count(&self) -> usize {
        match self {
            Self::File(f) => f.line_count(),
            Self::Stream(s) => s.line_count(),
        }
    }

    #[inline]
    fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
        match self {
            Self::File(f) => f.get_raw_line(idx),
            Self::Stream(s) => s.get_raw_line(idx),
        }
    }

    #[inline]
    fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
        match self {
            Self::File(f) => f.get_line(idx),
            Self::Stream(s) => s.get_line(idx),
        }
    }

    #[inline]
    fn get_line_arc(&self, idx: usize) -> Option<Arc<str>> {
        match self {
            Self::File(f) => f.get_line_arc(idx),
            Self::Stream(s) => s.get_line_arc(idx),
        }
    }

    #[inline]
    fn get_measured_length(&self, idx: usize) -> usize {
        match self {
            Self::File(f) => f.get_measured_length(idx),
            Self::Stream(s) => s.get_measured_length(idx),
        }
    }
}

/// A common trait for log data sources (mmap file or streaming memory buffer)
pub trait LogSource: Send + Sync {
    /// Total number of lines in this log source
    fn line_count(&self) -> usize;

    /// Retrieve the raw line bytes for a 0-based line index (without trailing \r or \n)
    fn get_raw_line(&self, idx: usize) -> Option<&[u8]>;

    /// Retrieve the line as a UTF-8 string with lossy replacement
    fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
        self.get_raw_line(idx).map(|bytes| String::from_utf8_lossy(bytes))
    }

    /// Retrieve the line as an Arc<str>, reusing existing Arc when available to eliminate copying
    fn get_line_arc(&self, idx: usize) -> Option<Arc<str>> {
        self.get_line(idx).map(|s| Arc::from(s.as_ref()))
    }

    /// Retrieve the measured character display column width of the line (excluding trailing whitespace/newlines).
    /// Uses East Asian Unicode width so CJK full-width characters reserve 2 monospace columns.
    fn get_measured_length(&self, idx: usize) -> usize {
        match self.get_raw_line(idx) {
            Some(bytes) => {
                let trimmed = trim_line_end(bytes);
                if let Ok(s) = std::str::from_utf8(trimmed) {
                    unicode_width::UnicodeWidthStr::width(s)
                } else {
                    trimmed.len()
                }
            }
            None => 0,
        }
    }
}

/// Helper function to trim trailing `\r` and `\n` from a byte slice
#[inline]
pub fn trim_line_end(mut slice: &[u8]) -> &[u8] {
    while let Some((&last, rest)) = slice.split_last() {
        if last == b'\n' || last == b'\r' {
            slice = rest;
        } else {
            break;
        }
    }
    slice
}

/// Helper function to trim trailing whitespace including `\r` and `\n` for measurements
#[inline]
pub fn trim_trailing_whitespace(mut slice: &[u8]) -> &[u8] {
    while let Some((&last, rest)) = slice.split_last() {
        if last.is_ascii_whitespace() {
            slice = rest;
        } else {
            break;
        }
    }
    slice
}

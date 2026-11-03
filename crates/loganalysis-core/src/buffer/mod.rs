pub mod file_buffer;
pub mod stream_buffer;

use std::borrow::Cow;

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

    /// Retrieve the measured character length of the line (excluding trailing whitespace/newlines).
    /// Uses UTF-8 character counts to prevent CJK 3-byte expansion distortion.
    fn get_measured_length(&self, idx: usize) -> usize {
        match self.get_raw_line(idx) {
            Some(bytes) => {
                let trimmed = trim_line_end(bytes);
                if let Ok(s) = std::str::from_utf8(trimmed) {
                    s.chars().count()
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

use super::{trim_line_end, LogSource};
use crate::constants::{MAX_MONITOR_LINES, MONITOR_TRIM_BATCH};

/// In-memory rolling log buffer for real-time ADB streaming.
pub struct StreamLogBuffer {
    lines: Vec<String>,
    max_capacity: usize,
    trim_batch: usize,
    max_line_len: usize,
    longest_line_text: String,
}

impl Default for StreamLogBuffer {
    fn default() -> Self {
        Self::new(MAX_MONITOR_LINES, MONITOR_TRIM_BATCH)
    }
}

impl StreamLogBuffer {
    pub fn new(max_capacity: usize, trim_batch: usize) -> Self {
        Self {
            lines: Vec::new(),
            max_capacity,
            trim_batch,
            max_line_len: 0,
            longest_line_text: String::new(),
        }
    }

    pub fn append_chunk(&mut self, chunk: Vec<String>) -> bool {
        if chunk.is_empty() {
            return false;
        }

        for line in &chunk {
            let measured = line.trim_end();
            if measured.len() > self.max_line_len {
                self.max_line_len = measured.len();
                self.longest_line_text = measured.to_string();
            }
        }

        self.lines.extend(chunk);
        true
    }

    /// Trims excess lines if `len - max_capacity >= trim_batch`.
    /// Returns `Some(trimmed_count)` if trimmed, or `None` if no trim occurred.
    pub fn trim_if_needed(&mut self) -> Option<usize> {
        if self.lines.len() <= self.max_capacity {
            return None;
        }

        let excess = self.lines.len() - self.max_capacity;
        if excess < self.trim_batch {
            return None;
        }

        self.lines.drain(0..excess);
        Some(excess)
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.max_line_len = 0;
        self.longest_line_text.clear();
    }

    pub fn max_line_len(&self) -> usize {
        self.max_line_len
    }

    pub fn longest_line_text(&self) -> &str {
        &self.longest_line_text
    }
}

impl LogSource for StreamLogBuffer {
    #[inline]
    fn line_count(&self) -> usize {
        self.lines.len()
    }

    #[inline]
    fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
        self.lines.get(idx).map(|s| trim_line_end(s.as_bytes()))
    }
}

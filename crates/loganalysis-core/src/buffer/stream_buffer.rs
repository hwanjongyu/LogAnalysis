use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::Arc;
use unicode_width::UnicodeWidthStr;

use super::{trim_line_end, LogSource};
use crate::constants::{MAX_MONITOR_LINES, MONITOR_TRIM_BATCH};

/// Helper to strip trailing newline or carriage return from a string slice without copying.
#[inline]
pub fn trim_line_end_str(mut s: &str) -> &str {
    while s.ends_with('\n') || s.ends_with('\r') {
        s = &s[..s.len() - 1];
    }
    s
}

/// High-performance rolling ring buffer for real-time ADB streaming backed by `VecDeque<Arc<str>>`.
/// Avoids O(N) memory copying during buffer trim operations and provides lightweight zero-copy cloning.
#[derive(Clone, Debug)]
pub struct StreamLogBuffer {
    lines: VecDeque<Arc<str>>,
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
            lines: VecDeque::new(),
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
            let width = UnicodeWidthStr::width(measured);
            if width > self.max_line_len {
                self.max_line_len = width;
                self.longest_line_text = measured.to_string();
            }
        }

        self.lines.extend(chunk.into_iter().map(Arc::from));
        true
    }

    /// Trims excess lines if `len - max_capacity >= trim_batch`.
    /// Advances the ring buffer head in O(1) without shifting memory.
    /// Resynchronizes `max_line_len` if the longest line was drained.
    pub fn trim_if_needed(&mut self) -> Option<usize> {
        if self.lines.len() <= self.max_capacity {
            return None;
        }

        let excess = self.lines.len() - self.max_capacity;
        if excess < self.trim_batch {
            return None;
        }

        let had_longest = self
            .lines
            .range(0..excess)
            .any(|l| UnicodeWidthStr::width(l.trim_end()) >= self.max_line_len);

        self.lines.drain(0..excess);

        if had_longest {
            self.recalculate_max_line();
        }

        Some(excess)
    }

    fn recalculate_max_line(&mut self) {
        let mut max_len = 0;
        let mut longest_idx = None;
        for (idx, line) in self.lines.iter().enumerate() {
            let measured = line.trim_end();
            let width = UnicodeWidthStr::width(measured);
            if width > max_len {
                max_len = width;
                longest_idx = Some(idx);
            }
        }
        self.max_line_len = max_len;
        self.longest_line_text = longest_idx
            .map(|i| self.lines[i].trim_end().to_string())
            .unwrap_or_default();
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

    #[inline]
    fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
        self.lines.get(idx).map(|s| Cow::Borrowed(trim_line_end_str(s.as_ref())))
    }

    #[inline]
    fn get_line_arc(&self, idx: usize) -> Option<Arc<str>> {
        self.lines.get(idx).cloned()
    }
}

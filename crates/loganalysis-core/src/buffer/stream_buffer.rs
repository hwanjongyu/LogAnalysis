//! Copy-on-Write paged rolling buffer for live streaming log lines.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::Arc;
use unicode_width::UnicodeWidthStr;

use super::timestamp::{
    extract_timestamp_with_kind, normalize_month_day_rollover, TimestampKind,
};
use super::{trim_line_end, LogSource};
use crate::constants::{MAX_MONITOR_LINES, MONITOR_TRIM_BATCH};

/// Fixed page size (in lines) for [`StreamLogBuffer`], capping CoW tail-page clones at `<= 1024` `Arc<str>` entries.
pub const STREAM_PAGE_SIZE: usize = 1024;

/// Zero-allocation string equivalent of [`trim_line_end`].
pub fn trim_line_end_str(mut s: &str) -> &str {
    while s.ends_with('\n') || s.ends_with('\r') {
        s = &s[..s.len() - 1];
    }
    s
}

/// Paged, Copy-on-Write rolling line buffer for live log streaming (e.g. ADB logcat).
#[derive(Debug, Clone)]
pub struct StreamLogBuffer {
    pages: VecDeque<Arc<Vec<Arc<str>>>>,
    head_offset: usize,
    len: usize,
    max_capacity: usize,
    trim_batch: usize,
    max_line_len: usize,
    longest_line_text: String,
    total_trimmed: usize,
    first_month_day_ts: Option<i64>,
}

impl Default for StreamLogBuffer {
    fn default() -> Self {
        Self::new(MAX_MONITOR_LINES, MONITOR_TRIM_BATCH)
    }
}

impl StreamLogBuffer {
    /// Creates an empty streaming buffer bounded by `max_capacity` and trimming in batches of at least `trim_batch`.
    pub fn new(max_capacity: usize, trim_batch: usize) -> Self {
        Self {
            pages: VecDeque::new(),
            head_offset: 0,
            len: 0,
            max_capacity,
            trim_batch,
            max_line_len: 0,
            longest_line_text: String::new(),
            total_trimmed: 0,
            first_month_day_ts: None,
        }
    }

    /// Number of currently retained live lines.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` when the buffer holds no live lines.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Configured maximum line capacity before trimming is eligible.
    pub fn max_capacity(&self) -> usize {
        self.max_capacity
    }

    /// Configured minimum excess batch size required to trigger a head trim.
    pub fn trim_batch(&self) -> usize {
        self.trim_batch
    }

    /// Number of backing CoW pages currently allocated.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    #[inline]
    fn line_arc_at(&self, idx: usize) -> Option<&Arc<str>> {
        if idx >= self.len {
            return None;
        }
        let abs_idx = self.head_offset + idx;
        let page_idx = abs_idx / STREAM_PAGE_SIZE;
        let in_page = abs_idx % STREAM_PAGE_SIZE;
        self.pages.get(page_idx).and_then(|page| page.get(in_page))
    }

    /// Appends a batch of incoming lines and updates maximum display width metadata.
    ///
    /// Mutates the tail page in-place when uniquely owned (`Arc::get_mut`), falling back
    /// to CoW-cloning only the active tail page (`<= STREAM_PAGE_SIZE` entries) when a
    /// background reader snapshot holds a reference.
    pub fn append_chunk(&mut self, chunk: Vec<String>) -> bool {
        if chunk.is_empty() {
            return false;
        }

        for line in chunk {
            let measured = line.trim_end();
            let width = UnicodeWidthStr::width(measured);
            if width > self.max_line_len {
                self.max_line_len = width;
                self.longest_line_text = measured.to_string();
            }

            if self.first_month_day_ts.is_none() {
                if let Some(parsed) = extract_timestamp_with_kind(trim_line_end(line.as_bytes())) {
                    if parsed.kind == TimestampKind::MonthDayTime {
                        self.first_month_day_ts = Some(parsed.timestamp.total_micros);
                    }
                }
            }

            let arc: Arc<str> = Arc::from(line);
            let need_new_page = self
                .pages
                .back()
                .is_none_or(|last| last.len() >= STREAM_PAGE_SIZE);

            if need_new_page {
                let mut page = Vec::with_capacity(STREAM_PAGE_SIZE);
                page.push(arc);
                self.pages.push_back(Arc::new(page));
            } else if let Some(last_arc) = self.pages.back_mut() {
                if let Some(last_mut) = Arc::get_mut(last_arc) {
                    last_mut.push(arc);
                } else {
                    let mut cloned = Vec::with_capacity(STREAM_PAGE_SIZE);
                    cloned.extend(last_arc.iter().cloned());
                    cloned.push(arc);
                    *last_arc = Arc::new(cloned);
                }
            }

            self.len += 1;
        }

        true
    }

    /// Trims excess leading lines if capacity exceeded `max_capacity` by at least `trim_batch`.
    /// Returns the number of trimmed lines if a trim occurred.
    pub fn trim_if_needed(&mut self) -> Option<usize> {
        if self.len <= self.max_capacity {
            return None;
        }
        let excess = self.len - self.max_capacity;
        if excess < self.trim_batch {
            return None;
        }

        let mut remaining = excess;
        let mut longest_was_trimmed = false;

        while remaining > 0 {
            let Some(front) = self.pages.front() else {
                break;
            };
            let avail = front.len().saturating_sub(self.head_offset);
            let take = remaining.min(avail);

            if !longest_was_trimmed && self.max_line_len > 0 {
                for line in &front[self.head_offset..self.head_offset + take] {
                    if UnicodeWidthStr::width(line.trim_end()) >= self.max_line_len {
                        longest_was_trimmed = true;
                        break;
                    }
                }
            }

            if take == avail {
                self.pages.pop_front();
                self.head_offset = 0;
            } else {
                self.head_offset += take;
            }
            remaining -= take;
        }

        self.len = self.len.saturating_sub(excess);
        self.total_trimmed += excess;

        if self.len == 0 {
            self.max_line_len = 0;
            self.longest_line_text.clear();
        } else if longest_was_trimmed {
            self.recalculate_max_line();
        }

        Some(excess)
    }

    fn recalculate_max_line(&mut self) {
        let mut max_len = 0usize;
        let mut longest_text = String::new();
        for (page_idx, page) in self.pages.iter().enumerate() {
            let start = if page_idx == 0 { self.head_offset } else { 0 };
            for line in &page[start..] {
                let measured = line.trim_end();
                let width = UnicodeWidthStr::width(measured);
                if width > max_len {
                    max_len = width;
                    longest_text = measured.to_string();
                }
            }
        }
        self.max_line_len = max_len;
        self.longest_line_text = longest_text;
    }

    /// Removes all lines and resets trim/width counters.
    pub fn clear(&mut self) {
        self.pages.clear();
        self.head_offset = 0;
        self.len = 0;
        self.max_line_len = 0;
        self.longest_line_text.clear();
        self.total_trimmed = 0;
        self.first_month_day_ts = None;
    }

    /// Total number of lines trimmed from the head over the buffer's lifetime.
    pub fn total_trimmed(&self) -> usize {
        self.total_trimmed
    }

    /// Delta of head lines trimmed since a previously captured `total_trimmed()` count.
    pub fn head_trimmed_since(&self, prev_total_trimmed: usize) -> usize {
        self.total_trimmed.saturating_sub(prev_total_trimmed)
    }

    /// Cached maximum display width (columns) across currently retained lines.
    pub fn max_line_len(&self) -> usize {
        self.max_line_len
    }

    /// Trimmed text of the widest currently retained line.
    pub fn longest_line_text(&self) -> &str {
        &self.longest_line_text
    }
}

impl LogSource for StreamLogBuffer {
    #[inline]
    fn line_count(&self) -> usize {
        self.len
    }

    #[inline]
    fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
        self.line_arc_at(idx).map(|s| trim_line_end(s.as_bytes()))
    }

    #[inline]
    fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
        self.line_arc_at(idx)
            .map(|s| Cow::Borrowed(trim_line_end_str(s.as_ref())))
    }

    #[inline]
    fn get_line_arc(&self, idx: usize) -> Option<Arc<str>> {
        self.line_arc_at(idx).cloned()
    }

    #[inline]
    fn get_timestamp_micros(&self, idx: usize) -> Option<i64> {
        let parsed = self.get_raw_line(idx).and_then(extract_timestamp_with_kind)?;
        let ts = parsed.timestamp.total_micros;
        if parsed.kind == TimestampKind::MonthDayTime {
            if let Some(anchor) = self.first_month_day_ts {
                return Some(normalize_month_day_rollover(ts, anchor));
            }
        }
        Some(ts)
    }
}

/// Inner buffer type wrapped by [`SharedStreamBuffer`].
pub type StreamBufferInner = StreamLogBuffer;

/// Thread-safe handle to a [`StreamLogBuffer`] supporting fast CoW snapshots via `parking_lot::RwLock`.
#[derive(Debug, Clone)]
pub struct SharedStreamBuffer(Arc<parking_lot::RwLock<StreamBufferInner>>);

impl SharedStreamBuffer {
    /// Wraps an existing [`StreamBufferInner`] in a shared lock handle.
    pub fn new(inner: StreamBufferInner) -> Self {
        Self(Arc::new(parking_lot::RwLock::new(inner)))
    }

    /// Creates a new shared buffer with the specified capacity and trim batch size.
    pub fn with_capacity(max_capacity: usize, trim_batch: usize) -> Self {
        Self::new(StreamLogBuffer::new(max_capacity, trim_batch))
    }

    /// Acquires a shared read guard to inspect live lines directly.
    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, StreamBufferInner> {
        self.0.read()
    }

    /// Acquires an exclusive write guard to mutate the underlying buffer.
    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, StreamBufferInner> {
        self.0.write()
    }

    /// Returns an isolated CoW snapshot by cloning only page `Arc`s (`O(len / 1024)`).
    pub fn snapshot(&self) -> StreamLogBuffer {
        self.0.read().clone()
    }

    /// Appends a chunk of incoming lines under the write lock.
    pub fn append_chunk(&self, chunk: Vec<String>) -> bool {
        self.0.write().append_chunk(chunk)
    }

    /// Trims excess leading lines under the write lock when capacity thresholds are exceeded.
    pub fn trim_if_needed(&self) -> Option<usize> {
        self.0.write().trim_if_needed()
    }

    /// Returns the number of live lines currently retained under the read lock.
    pub fn line_count(&self) -> usize {
        self.0.read().len()
    }
}

impl Default for SharedStreamBuffer {
    fn default() -> Self {
        Self::new(StreamLogBuffer::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::thread;

    fn make_lines(range: std::ops::Range<usize>) -> Vec<String> {
        range
            .map(|i| match i {
                10 => "W".repeat(90),   // page 0, trimmed in first partial trim
                1500 => "X".repeat(60), // page 1, trimmed in cross-page trim
                2500 => "Y".repeat(40), // page 2, retained after cross-page trim
                _ => format!("line-{i:04}"),
            })
            .collect()
    }

    #[test]
    fn test_stream_log_buffer_cow_page_boundaries_and_trim() {
        assert_eq!(STREAM_PAGE_SIZE, 1024);

        // Verify trim_batch gating before exceeding max_capacity + trim_batch
        let mut small = StreamLogBuffer::new(10, 5);
        assert!(!small.append_chunk(Vec::new()));
        assert!(small.append_chunk((0..14).map(|i| format!("s-{i}")).collect()));
        assert_eq!(small.trim_if_needed(), None);
        assert!(small.append_chunk(vec!["s-14".to_string()]));
        assert_eq!(small.trim_if_needed(), Some(5));
        assert_eq!(small.len(), 10);
        assert_eq!(small.get_line(0).as_deref(), Some("s-5"));

        // Exercise multi-page CoW snapshots (3,500 lines => 4 pages: 1024*3 + 428)
        let mut buf = StreamLogBuffer::new(3000, 100);
        assert!(buf.append_chunk(make_lines(0..3500)));
        assert_eq!(buf.len(), 3500);
        assert_eq!(buf.page_count(), 4);
        assert_eq!(buf.head_offset, 0);
        assert_eq!(buf.max_line_len(), 90);
        assert_eq!(buf.longest_line_text(), &"W".repeat(90));

        // Taking a CoW snapshot shares tail page 3 Arc; appending mutates via tail-page clone
        let snap_before_mutation = buf.clone();
        assert!(buf.append_chunk(make_lines(3500..3505)));
        assert_eq!(buf.len(), 3505);
        assert_eq!(snap_before_mutation.len(), 3500);
        assert_eq!(snap_before_mutation.get_line(3500), None);
        assert_eq!(buf.get_line(3504).as_deref(), Some("line-3504"));

        // Partial page-0 trim: excess = 505 (< 1024), pops 0 pages and advances head_offset.
        // Index 10 (width 90) falls below head_offset=505 and must be excluded by recalculate_max_line.
        assert_eq!(buf.trim_if_needed(), Some(505));
        assert_eq!(buf.head_offset, 505);
        assert_eq!(buf.page_count(), 4);
        assert_eq!(buf.len(), 3000);
        assert_eq!(buf.total_trimmed(), 505);
        assert_eq!(buf.max_line_len(), 60);
        assert_eq!(buf.longest_line_text(), &"X".repeat(60));
        assert_eq!(buf.get_line(0).as_deref(), Some("line-0505"));
        assert_eq!(buf.get_raw_line(0), Some(b"line-0505".as_slice()));
        assert_eq!(buf.get_line_arc(2999).as_deref(), Some("line-3504"));
        assert_eq!(buf.get_line(3000), None);

        // Pre-mutation snapshot still sees line 10 and untouched length/offset
        assert_eq!(snap_before_mutation.head_offset, 0);
        assert_eq!(snap_before_mutation.max_line_len(), 90);
        assert_eq!(
            snap_before_mutation.get_line(10).as_deref(),
            Some("W".repeat(90).as_str())
        );

        // Cross-page trim: append 1,600 lines (len=4600) => trim 1,600 lines (total trimmed=2,105).
        // 2,105 = 2 * 1024 + 57 => drops pages 0 and 1 completely, leaving head_offset=57 on page 2.
        // Line 1500 (width 60, page 1) was evicted, so max width recalculates to 40 (index 2500).
        assert!(buf.append_chunk(make_lines(3505..5105)));
        assert_eq!(buf.trim_if_needed(), Some(1600));
        assert_eq!(buf.len(), 3000);
        assert_eq!(buf.total_trimmed(), 2105);
        assert_eq!(buf.head_trimmed_since(505), 1600);
        assert_eq!(buf.head_offset, 57);
        assert_eq!(buf.max_line_len(), 40);
        assert_eq!(buf.longest_line_text(), &"Y".repeat(40));
        assert_eq!(buf.get_line(0).as_deref(), Some("line-2105"));
        assert_eq!(buf.get_line(2500 - 2105).as_deref(), Some("Y".repeat(40).as_str()));

        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.page_count(), 0);
        assert_eq!(buf.total_trimmed(), 0);
        assert_eq!(buf.max_line_len(), 0);
        assert_eq!(buf.longest_line_text(), "");
        assert_eq!(buf.get_line(0), None);
    }

    #[test]
    fn test_shared_stream_buffer_concurrent_snapshot_isolation() {
        let shared = SharedStreamBuffer::with_capacity(2048, 100);
        assert!(shared.append_chunk((0..1500).map(|i| format!("line-{i:04}")).collect()));

        // Synchronize 1 writer + 2 concurrent snapshot readers purely with Barriers (zero sleeps).
        let b_round1_snap_taken = Arc::new(Barrier::new(3));
        let b_round1_mutated = Arc::new(Barrier::new(3));
        let b_round2_snap_taken = Arc::new(Barrier::new(3));
        let b_round2_mutated = Arc::new(Barrier::new(3));

        let writer = {
            let shared = shared.clone();
            let b1 = Arc::clone(&b_round1_snap_taken);
            let b2 = Arc::clone(&b_round1_mutated);
            let b3 = Arc::clone(&b_round2_snap_taken);
            let b4 = Arc::clone(&b_round2_mutated);
            thread::spawn(move || {
                b1.wait();
                // Crosses 1024/2048 page boundaries and triggers 452-line head trim.
                assert!(shared.append_chunk((1500..2500).map(|i| format!("line-{i:04}")).collect()));
                assert_eq!(shared.trim_if_needed(), Some(452));
                assert_eq!(shared.line_count(), 2048);
                b2.wait();

                b3.wait();
                // Advances head across another page boundary and trims 1,500 lines (452..1952).
                assert!(shared.append_chunk((2500..4000).map(|i| format!("line-{i:04}")).collect()));
                assert_eq!(shared.trim_if_needed(), Some(1500));
                assert_eq!(shared.line_count(), 2048);
                b4.wait();
            })
        };

        let readers: Vec<_> = (0..2)
            .map(|_| {
                let shared = shared.clone();
                let b1 = Arc::clone(&b_round1_snap_taken);
                let b2 = Arc::clone(&b_round1_mutated);
                let b3 = Arc::clone(&b_round2_snap_taken);
                let b4 = Arc::clone(&b_round2_mutated);
                thread::spawn(move || {
                    let snap1 = shared.snapshot();
                    assert_eq!(snap1.line_count(), 1500);
                    b1.wait();

                    b2.wait();
                    // Round-1 snapshot must retain exact 0..1500 view after writer appended & trimmed.
                    assert_eq!(snap1.line_count(), 1500);
                    assert_eq!(snap1.get_line(0).as_deref(), Some("line-0000"));
                    assert_eq!(snap1.get_line(1499).as_deref(), Some("line-1499"));

                    let snap2 = shared.snapshot();
                    assert_eq!(snap2.line_count(), 2048);
                    assert_eq!(snap2.get_line(0).as_deref(), Some("line-0452"));
                    assert_eq!(snap2.get_line(2047).as_deref(), Some("line-2499"));
                    b3.wait();

                    b4.wait();
                    // Round-2 snapshot must remain isolated after second trim while live buffer shifts.
                    assert_eq!(snap2.line_count(), 2048);
                    assert_eq!(snap2.get_line(0).as_deref(), Some("line-0452"));
                    assert_eq!(snap2.get_line(2047).as_deref(), Some("line-2499"));

                    let snap3 = shared.snapshot();
                    assert_eq!(snap3.line_count(), 2048);
                    assert_eq!(snap3.get_line(0).as_deref(), Some("line-1952"));
                    assert_eq!(snap3.get_line(2047).as_deref(), Some("line-3999"));
                })
            })
            .collect();

        writer.join().expect("writer thread panicked");
        for reader in readers {
            reader.join().expect("reader thread panicked");
        }
    }
}

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use memmap2::Mmap;
use parking_lot::RwLock;

use super::timestamp::{
    extract_timestamp_with_kind, normalize_month_day_rollover, TimestampKind,
};
use super::{measure_display_width, trim_line_end, LogSource};

const LINE_CACHE_SIZE: usize = 1024;
type LineCache = RwLock<Box<[Option<(usize, Arc<str>)>; LINE_CACHE_SIZE]>>;

enum Storage {
    Mmap(Mmap, File),
    Heap(Vec<u8>),
}

impl Storage {
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        match self {
            Storage::Mmap(m, _) => m.as_ref(),
            Storage::Heap(v) => v.as_slice(),
        }
    }
}

/// A high-performance file-backed log buffer using memory-mapped I/O and line offset indexing.
pub struct FileLogBuffer {
    path: PathBuf,
    storage: Storage,
    /// Byte offsets of line starts in storage. Length is `line_count + 1`.
    line_offsets: Vec<usize>,
    max_line_len: usize,
    longest_line_idx: Option<usize>,
    line_cache: LineCache,
    first_month_day_ts: OnceLock<Option<i64>>,
}

impl FileLogBuffer {
    fn from_storage(path_buf: PathBuf, storage: Storage) -> Self {
        let empty_cache = Box::new(std::array::from_fn(|_| None));

        let bytes = storage.as_bytes();
        if bytes.is_empty() {
            return Self {
                path: path_buf,
                storage,
                line_offsets: vec![0],
                max_line_len: 0,
                longest_line_idx: None,
                line_cache: RwLock::new(empty_cache),
                first_month_day_ts: OnceLock::new(),
            };
        }

        let mut line_offsets = Vec::with_capacity((bytes.len() / 40).max(1024));
        line_offsets.push(0);

        let mut max_line_len = 0usize;
        let mut longest_line_idx = None;

        let mut current_line_idx = 0;
        let mut prev_offset = 0;

        for newline_pos in memchr::memchr_iter(b'\n', bytes) {
            let next_offset = newline_pos + 1;
            let width = measure_display_width(&bytes[prev_offset..newline_pos]);
            if width > max_line_len {
                max_line_len = width;
                longest_line_idx = Some(current_line_idx);
            }

            line_offsets.push(next_offset);
            prev_offset = next_offset;
            current_line_idx += 1;
        }

        // Handle possible trailing line without trailing newline
        if prev_offset < bytes.len() {
            let width = measure_display_width(&bytes[prev_offset..]);
            if width > max_line_len {
                max_line_len = width;
                longest_line_idx = Some(current_line_idx);
            }
            line_offsets.push(bytes.len());
        }

        Self {
            path: path_buf,
            storage,
            line_offsets,
            max_line_len,
            longest_line_idx,
            line_cache: RwLock::new(empty_cache),
            first_month_day_ts: OnceLock::new(),
        }
    }

    fn first_month_day_anchor(&self) -> Option<i64> {
        *self.first_month_day_ts.get_or_init(|| {
            for idx in 0..self.line_count() {
                if let Some(raw) = self.get_raw_line(idx) {
                    if let Some(parsed) = extract_timestamp_with_kind(raw) {
                        if parsed.kind == TimestampKind::MonthDayTime {
                            return Some(parsed.timestamp.total_micros);
                        }
                    }
                }
            }
            None
        })
    }

    /// Loads a file from path, mapping it into memory and computing line offsets.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        use std::io::{Read, Seek, SeekFrom};
        let path_buf = path.as_ref().to_path_buf();
        let mut file = File::open(&path_buf)?;
        let metadata = file.metadata()?;
        let file_len = usize::try_from(metadata.len()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "File size exceeds addressable usize range",
            )
        })?;

        if file_len == 0 {
            return Ok(Self::from_storage(path_buf, Storage::Heap(Vec::new())));
        }

        // Safety: We assume the file is opened in read-only mode and not concurrently modified.
        let storage = match unsafe { Mmap::map(&file) } {
            Ok(mmap) => Storage::Mmap(mmap, file),
            Err(_) => {
                // Fallback to heap allocation if mmap is unsupported; rewind existing handle
                file.seek(SeekFrom::Start(0))?;
                let mut bytes = Vec::with_capacity(file_len);
                file.read_to_end(&mut bytes)?;
                Storage::Heap(bytes)
            }
        };

        Ok(Self::from_storage(path_buf, storage))
    }

    /// Loads a file completely into heap-owned memory (`Storage::Heap`), making all subsequent
    /// reads immune to external file truncation (`SIGBUS`). Intended for volatile/rotating logs.
    pub fn open_heap<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        use std::io::Read;
        let path_buf = path.as_ref().to_path_buf();
        let mut file = File::open(&path_buf)?;
        let metadata = file.metadata()?;
        let file_len = usize::try_from(metadata.len()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "File size exceeds addressable usize range",
            )
        })?;
        let mut bytes = Vec::with_capacity(file_len);
        file.read_to_end(&mut bytes)?;
        Ok(Self::from_storage(path_buf, Storage::Heap(bytes)))
    }

    /// Returns `true` if this buffer is backed by heap memory rather than an OS memory mapping.
    #[inline]
    pub fn is_heap_backed(&self) -> bool {
        matches!(self.storage, Storage::Heap(_))
    }

    /// Checks whether an mmap-backed file has shrunk on disk below its mapped byte length.
    /// Always returns `false` for heap-backed buffers and on metadata lookup failure.
    /// MUST NOT be called on the per-line `get_raw_line` hot path.
    pub fn is_truncated_on_disk(&self) -> bool {
        match &self.storage {
            Storage::Heap(_) => false,
            Storage::Mmap(_, file) => file
                .metadata()
                .map(|m| m.len() < self.byte_size() as u64)
                .unwrap_or(false),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn byte_size(&self) -> usize {
        self.storage.as_bytes().len()
    }

    pub fn max_line_len(&self) -> usize {
        self.max_line_len
    }

    pub fn longest_line_text(&self) -> Option<String> {
        self.longest_line_idx.and_then(|idx| self.get_line(idx).map(|c| c.into_owned()))
    }
}

impl LogSource for FileLogBuffer {
    #[inline]
    fn line_count(&self) -> usize {
        if self.line_offsets.len() <= 1 {
            0
        } else {
            self.line_offsets.len() - 1
        }
    }

    #[inline]
    fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
        if idx >= self.line_count() {
            return None;
        }
        let start = self.line_offsets[idx];
        let end = self.line_offsets[idx + 1];
        let bytes = self.storage.as_bytes();
        if start <= end && end <= bytes.len() {
            Some(trim_line_end(&bytes[start..end]))
        } else {
            None
        }
    }

    #[inline]
    fn get_line_arc(&self, idx: usize) -> Option<Arc<str>> {
        if idx >= self.line_count() {
            return None;
        }

        let slot = idx & (LINE_CACHE_SIZE - 1);

        {
            let guard = self.line_cache.read();
            if let Some((cached_idx, ref arc)) = guard[slot] {
                if cached_idx == idx {
                    return Some(Arc::clone(arc));
                }
            }
        }

        let line_cow = self.get_line(idx)?;
        let arc: Arc<str> = Arc::from(line_cow.as_ref());

        {
            let mut guard = self.line_cache.write();
            guard[slot] = Some((idx, Arc::clone(&arc)));
        }

        Some(arc)
    }

    #[inline]
    fn get_timestamp_micros(&self, idx: usize) -> Option<i64> {
        let parsed = self.get_raw_line(idx).and_then(extract_timestamp_with_kind)?;
        let ts = parsed.timestamp.total_micros;
        if parsed.kind == TimestampKind::MonthDayTime {
            if let Some(anchor) = self.first_month_day_anchor() {
                return Some(normalize_month_day_rollover(ts, anchor));
            }
        }
        Some(ts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_log_buffer_open_heap_survives_truncation_to_zero() {
        let mut tmp = NamedTempFile::new().expect("create temp file");
        let content = b"line zero\nline one\nline two\n";
        tmp.write_all(content).expect("write initial lines");
        tmp.flush().expect("flush initial lines");

        let buf = FileLogBuffer::open_heap(tmp.path()).expect("open_heap");
        assert!(buf.is_heap_backed());
        assert!(!buf.is_truncated_on_disk());
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.byte_size(), content.len());
        assert_eq!(buf.get_raw_line(0), Some(&b"line zero"[..]));
        assert_eq!(buf.get_raw_line(1), Some(&b"line one"[..]));
        assert_eq!(buf.get_raw_line(2), Some(&b"line two"[..]));

        // Truncate the backing file on disk to 0 bytes; heap buffer must remain readable
        tmp.as_file().set_len(0).expect("truncate backing file");
        tmp.as_file().sync_all().expect("sync truncated file");

        assert!(buf.is_heap_backed());
        assert!(!buf.is_truncated_on_disk());
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.get_raw_line(0), Some(&b"line zero"[..]));
        assert_eq!(buf.get_line(1).as_deref(), Some("line one"));
        assert_eq!(buf.get_line_arc(2).as_deref(), Some("line two"));
    }

    #[test]
    fn test_file_log_buffer_open_mmap_detects_shrink_and_allows_growth() {
        let mut tmp = NamedTempFile::new().expect("create temp file");
        let content = b"11-06 10:00:00.000 alpha\n11-06 10:00:01.000 beta\n";
        tmp.write_all(content).expect("write initial content");
        tmp.flush().expect("flush initial content");

        let buf = FileLogBuffer::open(tmp.path()).expect("open mmap buffer");
        assert!(!buf.is_heap_backed());
        assert!(!buf.is_truncated_on_disk());
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.byte_size(), content.len());

        // On Unix, external file modification succeeds while mmap is active.
        // (Windows blocks shrinking a file with an open mapped view via ERROR_USER_MAPPED_FILE.)
        #[cfg(unix)]
        {
            use std::fs::OpenOptions;
            let mut append_handle = OpenOptions::new()
                .append(true)
                .open(tmp.path())
                .expect("open for append");
            append_handle
                .write_all(b"11-06 10:00:02.000 gamma\n")
                .expect("append line");
            append_handle.flush().expect("flush append");
            assert!(
                !buf.is_truncated_on_disk(),
                "Growing the file beyond mapped byte_size must keep is_truncated_on_disk() == false"
            );

            tmp.as_file().set_len(5).expect("shrink backing file");
            tmp.as_file().sync_all().expect("sync shrink");
            assert!(
                buf.is_truncated_on_disk(),
                "Shrinking the file below mapped byte_size must set is_truncated_on_disk() == true"
            );
        }
    }

    #[test]
    fn test_file_log_buffer_empty_file_open_and_open_heap() {
        let tmp = NamedTempFile::new().expect("create empty temp file");

        let buf_open = FileLogBuffer::open(tmp.path()).expect("open empty file");
        assert!(buf_open.is_heap_backed());
        assert_eq!(buf_open.line_count(), 0);
        assert_eq!(buf_open.byte_size(), 0);
        assert!(!buf_open.is_truncated_on_disk());
        assert_eq!(buf_open.get_raw_line(0), None);

        let buf_heap = FileLogBuffer::open_heap(tmp.path()).expect("open_heap empty file");
        assert!(buf_heap.is_heap_backed());
        assert_eq!(buf_heap.line_count(), 0);
        assert_eq!(buf_heap.byte_size(), 0);
        assert!(!buf_heap.is_truncated_on_disk());
        assert_eq!(buf_heap.get_raw_line(0), None);
    }

    #[test]
    fn test_file_log_buffer_widest_line_uses_display_width() {
        let mut tmp = NamedTempFile::new().expect("create temp file");
        // Line 0: 10 Hangul chars = 30 UTF-8 bytes, display width 20
        // Line 1: 25 ASCII chars = 25 UTF-8 bytes, display width 25 (visually wider!)
        // Line 2: 5 ASCII chars + 50 trailing spaces = 55 bytes, display width 5
        let cjk_line = "한".repeat(10);
        let ascii_line = "a".repeat(25);
        let padded_line = format!("short{}", " ".repeat(50));
        writeln!(tmp, "{cjk_line}").unwrap();
        writeln!(tmp, "{ascii_line}").unwrap();
        writeln!(tmp, "{padded_line}").unwrap();
        tmp.flush().unwrap();

        let buf = FileLogBuffer::open(tmp.path()).expect("open file");
        assert_eq!(buf.max_line_len(), 25);
        assert_eq!(buf.longest_line_text().as_deref(), Some(ascii_line.as_str()));
    }

    #[cfg(unix)]
    #[test]
    fn test_file_log_buffer_fstat_ignores_path_rotation_and_detects_inode_truncation() {
        use std::fs::OpenOptions;

        let dir = tempfile::tempdir().expect("create temp dir");
        let orig_path = dir.path().join("app.log");
        let rotated_path = dir.path().join("app.log.1");

        std::fs::write(
            &orig_path,
            b"11-06 10:00:00.000 alpha\n11-06 10:00:01.000 beta\n11-06 10:00:02.000 gamma\n",
        )
        .expect("write initial log");

        let buf = FileLogBuffer::open(&orig_path).expect("open mmap buffer");
        assert!(!buf.is_heap_backed());
        assert!(!buf.is_truncated_on_disk());

        // Rotate: rename mapped file and replace original path with a shorter file.
        std::fs::rename(&orig_path, &rotated_path).expect("rename mapped file");
        std::fs::write(&orig_path, b"new\n").expect("write smaller replacement at old path");

        assert!(
            !buf.is_truncated_on_disk(),
            "Replacing path after rotation must not falsely report truncation of mapped inode"
        );

        // Truncate the actual mapped inode via rotated_path.
        let rotated_file = OpenOptions::new()
            .write(true)
            .open(&rotated_path)
            .expect("open rotated inode");
        rotated_file.set_len(5).expect("truncate mapped inode");
        rotated_file.sync_all().expect("sync truncated inode");

        assert!(
            buf.is_truncated_on_disk(),
            "Truncating the mapped inode itself must return true"
        );
    }
}

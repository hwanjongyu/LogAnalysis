use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use memmap2::Mmap;

use super::timestamp::extract_timestamp;
use super::{trim_line_end, trim_trailing_whitespace, LogSource};

const LINE_CACHE_SIZE: usize = 1024;
type LineCache = RwLock<Box<[Option<(usize, Arc<str>)>; LINE_CACHE_SIZE]>>;

enum Storage {
    Mmap(Mmap),
    Heap(Vec<u8>),
}

impl Storage {
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        match self {
            Storage::Mmap(m) => m.as_ref(),
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
}

impl FileLogBuffer {
    fn from_storage(path_buf: PathBuf, storage: Storage) -> Self {
        let empty_cache = vec![None; LINE_CACHE_SIZE]
            .into_boxed_slice()
            .try_into()
            .unwrap();

        let bytes = storage.as_bytes();
        if bytes.is_empty() {
            return Self {
                path: path_buf,
                storage,
                line_offsets: vec![0],
                max_line_len: 0,
                longest_line_idx: None,
                line_cache: RwLock::new(empty_cache),
            };
        }

        let mut line_offsets = Vec::with_capacity((bytes.len() / 40).max(1024));
        line_offsets.push(0);

        let mut max_raw_len = 0;
        let mut longest_line_idx = None;

        let mut current_line_idx = 0;
        let mut prev_offset = 0;

        for newline_pos in memchr::memchr_iter(b'\n', bytes) {
            let next_offset = newline_pos + 1;
            let raw_len = newline_pos.saturating_sub(prev_offset);
            if raw_len > max_raw_len {
                max_raw_len = raw_len;
                longest_line_idx = Some(current_line_idx);
            }

            line_offsets.push(next_offset);
            prev_offset = next_offset;
            current_line_idx += 1;
        }

        // Handle possible trailing line without trailing newline
        if prev_offset < bytes.len() {
            let raw_len = bytes.len() - prev_offset;
            if raw_len > max_raw_len {
                longest_line_idx = Some(current_line_idx);
            }
            line_offsets.push(bytes.len());
        }

        let max_line_len = if let Some(idx) = longest_line_idx {
            let start = line_offsets[idx];
            let end = if idx + 1 < line_offsets.len() {
                line_offsets[idx + 1]
            } else {
                bytes.len()
            };
            let slice = trim_trailing_whitespace(&bytes[start..end]);
            let s = String::from_utf8_lossy(slice);
            unicode_width::UnicodeWidthStr::width(s.as_ref())
        } else {
            0
        };

        Self {
            path: path_buf,
            storage,
            line_offsets,
            max_line_len,
            longest_line_idx,
            line_cache: RwLock::new(empty_cache),
        }
    }

    /// Loads a file from path, mapping it into memory and computing line offsets.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        use std::io::{Read, Seek, SeekFrom};
        let path_buf = path.as_ref().to_path_buf();
        let mut file = File::open(&path_buf)?;
        let metadata = file.metadata()?;
        let file_len = metadata.len() as usize;

        if file_len == 0 {
            return Ok(Self::from_storage(path_buf, Storage::Heap(Vec::new())));
        }

        // Safety: We assume the file is opened in read-only mode and not concurrently modified.
        let storage = match unsafe { Mmap::map(&file) } {
            Ok(mmap) => Storage::Mmap(mmap),
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
        let file_len = file.metadata()?.len() as usize;
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
            Storage::Mmap(_) => std::fs::metadata(&self.path)
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

        if let Ok(guard) = self.line_cache.read() {
            if let Some((cached_idx, ref arc)) = guard[slot] {
                if cached_idx == idx {
                    return Some(Arc::clone(arc));
                }
            }
        }

        let line_cow = self.get_line(idx)?;
        let arc: Arc<str> = Arc::from(line_cow.as_ref());

        if let Ok(mut guard) = self.line_cache.write() {
            guard[slot] = Some((idx, Arc::clone(&arc)));
        }

        Some(arc)
    }

    #[inline]
    fn get_timestamp_micros(&self, idx: usize) -> Option<i64> {
        self.get_raw_line(idx).and_then(extract_timestamp).map(|t| t.total_micros)
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
}

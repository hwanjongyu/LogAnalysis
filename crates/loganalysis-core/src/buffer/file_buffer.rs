use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use memmap2::Mmap;

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
    /// Loads a file from path, mapping it into memory and computing line offsets.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = File::open(&path_buf)?;
        let metadata = file.metadata()?;
        let file_len = metadata.len() as usize;

        let empty_cache = vec![None; LINE_CACHE_SIZE]
            .into_boxed_slice()
            .try_into()
            .unwrap();

        if file_len == 0 {
            return Ok(Self {
                path: path_buf,
                storage: Storage::Heap(Vec::new()),
                line_offsets: vec![0],
                max_line_len: 0,
                longest_line_idx: None,
                line_cache: RwLock::new(empty_cache),
            });
        }

        // Safety: We assume the file is opened in read-only mode and not concurrently modified.
        let storage = match unsafe { Mmap::map(&file) } {
            Ok(mmap) => Storage::Mmap(mmap),
            Err(_) => {
                // Fallback to heap allocation if mmap is unsupported
                use std::io::Read;
                let mut file = File::open(&path_buf)?;
                let mut bytes = Vec::with_capacity(file_len);
                file.read_to_end(&mut bytes)?;
                Storage::Heap(bytes)
            }
        };

        let bytes = storage.as_bytes();
        let mut line_offsets = Vec::with_capacity((file_len / 40).max(1024));
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

        Ok(Self {
            path: path_buf,
            storage,
            line_offsets,
            max_line_len,
            longest_line_idx,
            line_cache: RwLock::new(empty_cache),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
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
}

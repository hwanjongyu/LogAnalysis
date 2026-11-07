use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use super::file_buffer::FileLogBuffer;
use super::timestamp::extract_timestamp;
use super::LogSource;

/// Reference to a single log line within a multi-file merged index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergedLineRef {
    pub source_id: u8,
    pub source_line_idx: usize,
    pub timestamp_micros: Option<i64>,
}

/// A unified chronological multi-file log source.
/// Merges lines from 2 or more files sorted by microsecond timestamp.
pub struct MergedLogBuffer {
    sources: Vec<Arc<FileLogBuffer>>,
    source_names: Vec<String>,
    merged_index: Vec<MergedLineRef>,
    max_line_len: usize,
}

impl MergedLogBuffer {
    /// Opens multiple log files and merges their lines into a single chronological stream.
    pub fn open_multiple(paths: &[PathBuf]) -> std::io::Result<Self> {
        if paths.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "No paths provided for merge",
            ));
        }
        if paths.len() > 255 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Maximum number of merged files is 255",
            ));
        }

        let mut sources = Vec::with_capacity(paths.len());
        let mut source_names = Vec::with_capacity(paths.len());

        for path in paths {
            let buf = FileLogBuffer::open(path)?;
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            sources.push(Arc::new(buf));
            source_names.push(name);
        }

        Self::from_sources(sources, source_names)
    }

    /// Constructs a merged buffer from already opened FileLogBuffers.
    pub fn from_sources(
        sources: Vec<Arc<FileLogBuffer>>,
        source_names: Vec<String>,
    ) -> std::io::Result<Self> {
        if sources.len() > 255 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Maximum number of merged files is 255",
            ));
        }
        let mut total_lines = 0usize;
        for s in &sources {
            total_lines += s.line_count();
        }

        let mut merged_index = Vec::with_capacity(total_lines);
        let mut max_line_len = 0usize;

        for (s_id, src) in sources.iter().enumerate() {
            max_line_len = max_line_len.max(src.max_line_len());
            let line_count = src.line_count();
            let mut last_ts = None;

            for l_idx in 0..line_count {
                let ts = if let Some(raw) = src.get_raw_line(l_idx) {
                    if let Some(t) = extract_timestamp(raw) {
                        last_ts = Some(t.total_micros);
                        Some(t.total_micros)
                    } else {
                        // Inherit last known timestamp from this file so multiline exceptions stay grouped
                        last_ts
                    }
                } else {
                    last_ts
                };

                merged_index.push(MergedLineRef {
                    source_id: s_id as u8,
                    source_line_idx: l_idx,
                    timestamp_micros: ts,
                });
            }
        }

        // Chronological stable sort:
        // Prioritize timestamps; if equal or missing, preserve original file sequence
        merged_index.sort_by(|a, b| match (a.timestamp_micros, b.timestamp_micros) {
            (Some(ta), Some(tb)) => {
                if ta != tb {
                    ta.cmp(&tb)
                } else {
                    (a.source_id, a.source_line_idx).cmp(&(b.source_id, b.source_line_idx))
                }
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => (a.source_id, a.source_line_idx).cmp(&(b.source_id, b.source_line_idx)),
        });

        Ok(Self {
            sources,
            source_names,
            merged_index,
            max_line_len,
        })
    }

    /// Returns the maximum line length across all merged files.
    #[inline]
    pub fn max_line_len(&self) -> usize {
        self.max_line_len
    }

    /// Returns the source ID (0-indexed file index) for a given merged row index.
    #[inline]
    pub fn get_line_source_id(&self, idx: usize) -> Option<u8> {
        self.merged_index.get(idx).map(|r| r.source_id)
    }

    /// Returns the name of the source file for a given merged row index.
    #[inline]
    pub fn get_line_source_name(&self, idx: usize) -> Option<&str> {
        let sid = self.get_line_source_id(idx)?;
        self.source_name(sid)
    }

    /// Returns the (source_id, source_name) pair for a given merged row index.
    #[inline]
    pub fn get_line_source_info(&self, idx: usize) -> Option<(u8, &str)> {
        let sid = self.get_line_source_id(idx)?;
        let name = self.source_name(sid)?;
        Some((sid, name))
    }

    /// Returns the name of the source file for a given source ID.
    #[inline]
    pub fn source_name(&self, source_id: u8) -> Option<&str> {
        self.source_names.get(source_id as usize).map(|s| s.as_str())
    }

    /// Returns all source file names.
    #[inline]
    pub fn source_names(&self) -> &[String] {
        &self.source_names
    }

    /// Returns the number of distinct files merged.
    #[inline]
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
}

impl LogSource for MergedLogBuffer {
    #[inline]
    fn line_count(&self) -> usize {
        self.merged_index.len()
    }

    #[inline]
    fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
        let r = self.merged_index.get(idx)?;
        let src = self.sources.get(r.source_id as usize)?;
        src.get_raw_line(r.source_line_idx)
    }

    #[inline]
    fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
        let r = self.merged_index.get(idx)?;
        let src = self.sources.get(r.source_id as usize)?;
        src.get_line(r.source_line_idx)
    }

    #[inline]
    fn get_line_arc(&self, idx: usize) -> Option<Arc<str>> {
        let r = self.merged_index.get(idx)?;
        let src = self.sources.get(r.source_id as usize)?;
        src.get_line_arc(r.source_line_idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_two_files_interleaved_chronologically() {
        let file_a_data = b"01-01 10:00:00.100 Line A1\n\
                            01-01 10:00:02.000 Line A2\n\
                            01-01 10:00:05.000 Line A3\n";

        let file_b_data = b"01-01 10:00:00.050 Line B1\n\
                            01-01 10:00:01.500 Line B2\n\
                            01-01 10:00:03.000 Line B3\n";

        let mut a_dir = std::env::temp_dir();
        a_dir.push(format!("merge_a_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&a_dir, file_a_data).unwrap();

        let mut b_dir = std::env::temp_dir();
        b_dir.push(format!("merge_b_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&b_dir, file_b_data).unwrap();

        let merged = MergedLogBuffer::open_multiple(&[a_dir.clone(), b_dir.clone()]).unwrap();
        assert_eq!(merged.line_count(), 6);
        assert_eq!(merged.source_count(), 2);

        // Expected chronological order:
        // 0: B1 (10:00:00.050) [source 1]
        // 1: A1 (10:00:00.100) [source 0]
        // 2: B2 (10:00:01.500) [source 1]
        // 3: A2 (10:00:02.000) [source 0]
        // 4: B3 (10:00:03.000) [source 1]
        // 5: A3 (10:00:05.000) [source 0]

        assert_eq!(merged.get_line_source_id(0), Some(1));
        assert!(merged.get_line(0).unwrap().contains("Line B1"));

        assert_eq!(merged.get_line_source_id(1), Some(0));
        assert!(merged.get_line(1).unwrap().contains("Line A1"));

        assert_eq!(merged.get_line_source_id(2), Some(1));
        assert!(merged.get_line(2).unwrap().contains("Line B2"));

        assert_eq!(merged.get_line_source_id(3), Some(0));
        assert!(merged.get_line(3).unwrap().contains("Line A2"));

        assert_eq!(merged.get_line_source_id(4), Some(1));
        assert!(merged.get_line(4).unwrap().contains("Line B3"));

        assert_eq!(merged.get_line_source_id(5), Some(0));
        assert!(merged.get_line(5).unwrap().contains("Line A3"));

        let _ = std::fs::remove_file(a_dir);
        let _ = std::fs::remove_file(b_dir);
    }

    #[test]
    fn test_merge_preserves_stack_trace_continuity() {
        let file_a_data = b"01-01 10:00:01.000 Exception occurred\n\
                            \tat com.example.App.run(App.java:10)\n\
                            \tat com.example.App.main(App.java:5)\n";

        let file_b_data = b"01-01 10:00:00.500 Server idle\n\
                            01-01 10:00:02.000 Server heartbeat\n";

        let mut a_dir = std::env::temp_dir();
        a_dir.push(format!("merge_stack_a_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&a_dir, file_a_data).unwrap();

        let mut b_dir = std::env::temp_dir();
        b_dir.push(format!("merge_stack_b_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&b_dir, file_b_data).unwrap();

        let merged = MergedLogBuffer::open_multiple(&[a_dir.clone(), b_dir.clone()]).unwrap();
        assert_eq!(merged.line_count(), 5);

        // Sequence:
        // 0: B Server idle (10:00:00.500)
        // 1: A Exception (10:00:01.000)
        // 2: A stack trace 1 (inherited 10:00:01.000)
        // 3: A stack trace 2 (inherited 10:00:01.000)
        // 4: B Server heartbeat (10:00:02.000)

        assert!(merged.get_line(0).unwrap().contains("Server idle"));
        assert!(merged.get_line(1).unwrap().contains("Exception occurred"));
        assert!(merged.get_line(2).unwrap().contains("App.run"));
        assert!(merged.get_line(3).unwrap().contains("App.main"));
        assert!(merged.get_line(4).unwrap().contains("Server heartbeat"));

        let _ = std::fs::remove_file(a_dir);
        let _ = std::fs::remove_file(b_dir);
    }

    #[test]
    fn test_merge_rejects_more_than_255_sources() {
        let dummy_paths: Vec<PathBuf> = (0..256)
            .map(|i| PathBuf::from(format!("dummy_{}.log", i)))
            .collect();
        let Err(err) = MergedLogBuffer::open_multiple(&dummy_paths) else {
            panic!("expected error for > 255 paths");
        };
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("255"));
    }

    #[test]
    fn test_from_sources_rejects_more_than_255_sources() {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("test_max_sources_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&tmp, b"test line\n").unwrap();

        let buf = Arc::new(FileLogBuffer::open(&tmp).unwrap());
        let sources = vec![Arc::clone(&buf); 256];
        let names = vec!["test.log".to_string(); 256];

        let Err(err) = MergedLogBuffer::from_sources(sources, names) else {
            panic!("expected error for > 255 sources");
        };
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("255"));

        let _ = std::fs::remove_file(tmp);
    }
}

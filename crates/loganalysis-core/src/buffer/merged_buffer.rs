use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use super::file_buffer::FileLogBuffer;
use super::timestamp::extract_timestamp;
use super::LogSource;

pub use super::timestamp::ONE_YEAR_MICROS;

/// Threshold for year rollover detection (300 days).
/// When timestamps jump backwards by more than 300 days (e.g., Dec 31 -> Jan 01),
/// it indicates an un-year-stamped timestamp rollover rather than clock drift.
pub const YEAR_ROLLOVER_THRESHOLD_MICROS: i64 = 300 * 86_400 * 1_000_000;

/// Threshold for detecting early-year logs (< 60 days, e.g. January/February).
pub const EARLY_YEAR_THRESHOLD_MICROS: i64 = 60 * 86_400 * 1_000_000;

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

        struct SourceTimeInfo {
            first_ts: Option<i64>,
            last_ts: Option<i64>,
        }

        // Pass 1: Scan temporal boundaries for each source to detect ensemble rollover
        let mut source_infos = Vec::with_capacity(sources.len());
        for src in &sources {
            let line_count = src.line_count();
            let mut first_ts = None;
            for l_idx in 0..line_count {
                if let Some(raw) = src.get_raw_line(l_idx) {
                    if let Some(t) = extract_timestamp(raw) {
                        first_ts = Some(t.total_micros);
                        break;
                    }
                }
            }

            let mut last_ts = None;
            for l_idx in (0..line_count).rev() {
                if let Some(raw) = src.get_raw_line(l_idx) {
                    if let Some(t) = extract_timestamp(raw) {
                        last_ts = Some(t.total_micros);
                        break;
                    }
                }
            }

            source_infos.push(SourceTimeInfo { first_ts, last_ts });
        }

        let has_late_year = source_infos.iter().any(|info| {
            info.first_ts.is_some_and(|ts| ts >= YEAR_ROLLOVER_THRESHOLD_MICROS)
                || info.last_ts.is_some_and(|ts| ts >= YEAR_ROLLOVER_THRESHOLD_MICROS)
        });

        let has_early_year = source_infos.iter().any(|info| {
            info.first_ts.is_some_and(|ts| ts <= EARLY_YEAR_THRESHOLD_MICROS)
                || info.last_ts.is_some_and(|ts| ts <= EARLY_YEAR_THRESHOLD_MICROS)
        });

        let ensemble_rollover = has_late_year && has_early_year;

        let mut base_offsets = vec![0i64; sources.len()];
        if ensemble_rollover {
            for (s_id, info) in source_infos.iter().enumerate() {
                if let Some(first) = info.first_ts {
                    if first <= EARLY_YEAR_THRESHOLD_MICROS {
                        base_offsets[s_id] = ONE_YEAR_MICROS;
                    }
                }
            }
        }

        // Pass 2: Populate merged_index with adjusted timestamps
        let mut merged_index = Vec::with_capacity(total_lines);
        let mut max_line_len = 0usize;

        for (s_id, src) in sources.iter().enumerate() {
            max_line_len = max_line_len.max(src.max_line_len());
            let line_count = src.line_count();
            let mut file_year_offset = base_offsets[s_id];
            let mut prev_raw_ts: Option<i64> = None;
            let mut last_ts = None;

            for l_idx in 0..line_count {
                let ts = if let Some(raw) = src.get_raw_line(l_idx) {
                    if let Some(t) = extract_timestamp(raw) {
                        let raw_micros = t.total_micros;
                        if let Some(prev) = prev_raw_ts {
                            // Intra-file year rollover: e.g. 12-31 23:59:59 -> 01-01 00:00:00
                            if raw_micros < prev - YEAR_ROLLOVER_THRESHOLD_MICROS {
                                file_year_offset += ONE_YEAR_MICROS;
                            }
                        }
                        prev_raw_ts = Some(raw_micros);
                        let adjusted = raw_micros + file_year_offset;
                        last_ts = Some(adjusted);
                        Some(adjusted)
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

    /// Returns the adjusted timestamp in microseconds for a given merged row index.
    #[inline]
    pub fn get_line_timestamp_micros(&self, idx: usize) -> Option<i64> {
        self.merged_index.get(idx).and_then(|r| r.timestamp_micros)
    }

    /// Returns the name of the source file for a given source ID.
    #[inline]
    pub fn source_name(&self, source_id: u8) -> Option<&str> {
        self.source_names
            .get(source_id as usize)
            .map(|s| s.as_str())
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

    #[inline]
    fn get_timestamp_micros(&self, idx: usize) -> Option<i64> {
        self.merged_index.get(idx).and_then(|r| r.timestamp_micros)
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

    #[test]
    fn test_intra_file_year_rollover_dec31_to_jan01() {
        let file_data = b"12-31 23:59:59.999 Line Before Midnight\n\
                          01-01 00:00:00.001 Line After Midnight\n";

        let mut dir = std::env::temp_dir();
        dir.push(format!("merge_rollover_intra_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&dir, file_data).unwrap();

        let merged = MergedLogBuffer::open_multiple(std::slice::from_ref(&dir)).unwrap();
        assert_eq!(merged.line_count(), 2);

        // Row 0 should be Dec 31, Row 1 should be Jan 01
        assert!(merged.get_line(0).unwrap().contains("Line Before Midnight"));
        assert!(merged.get_line(1).unwrap().contains("Line After Midnight"));

        let ts0 = merged.get_line_timestamp_micros(0).unwrap();
        let ts1 = merged.get_line_timestamp_micros(1).unwrap();

        // 2 milliseconds elapsed: from 23:59:59.999 to 00:00:00.001
        assert_eq!(ts1 - ts0, 2_000);
        assert!(
            ts1 > ts0,
            "Jan 01 timestamp must be chronologically after Dec 31"
        );

        let _ = std::fs::remove_file(dir);
    }

    #[test]
    fn test_cross_file_year_rollover_source1_dec31_source2_jan01() {
        let file_dec = b"12-31 23:59:58.000 S1 Dec31 58s\n\
                         12-31 23:59:59.500 S1 Dec31 59s\n";

        let file_jan = b"01-01 00:00:00.500 S2 Jan01 00s\n\
                         01-01 00:00:01.000 S2 Jan01 01s\n";

        let mut dec_dir = std::env::temp_dir();
        dec_dir.push(format!("merge_dec_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&dec_dir, file_dec).unwrap();

        let mut jan_dir = std::env::temp_dir();
        jan_dir.push(format!("merge_jan_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&jan_dir, file_jan).unwrap();

        let merged = MergedLogBuffer::open_multiple(&[dec_dir.clone(), jan_dir.clone()]).unwrap();
        assert_eq!(merged.line_count(), 4);

        // Expected chronological sequence:
        // 0: S1 Dec31 58s (Source 0)
        // 1: S1 Dec31 59s (Source 0)
        // 2: S2 Jan01 00s (Source 1)
        // 3: S2 Jan01 01s (Source 1)
        assert_eq!(merged.get_line_source_id(0), Some(0));
        assert!(merged.get_line(0).unwrap().contains("S1 Dec31 58s"));

        assert_eq!(merged.get_line_source_id(1), Some(0));
        assert!(merged.get_line(1).unwrap().contains("S1 Dec31 59s"));

        assert_eq!(merged.get_line_source_id(2), Some(1));
        assert!(merged.get_line(2).unwrap().contains("S2 Jan01 00s"));

        assert_eq!(merged.get_line_source_id(3), Some(1));
        assert!(merged.get_line(3).unwrap().contains("S2 Jan01 01s"));

        let ts0 = merged.get_line_timestamp_micros(0).unwrap();
        let ts1 = merged.get_line_timestamp_micros(1).unwrap();
        let ts2 = merged.get_line_timestamp_micros(2).unwrap();
        let ts3 = merged.get_line_timestamp_micros(3).unwrap();

        assert!(ts0 < ts1);
        assert!(
            ts1 < ts2,
            "Dec 31 23:59:59.500 must precede Jan 01 00:00:00.500"
        );
        assert!(ts2 < ts3);
        // Time between 23:59:59.500 and 00:00:00.500 is exactly 1 second (1,000,000 micros)
        assert_eq!(ts2 - ts1, 1_000_000);

        let _ = std::fs::remove_file(dec_dir);
        let _ = std::fs::remove_file(jan_dir);
    }

    #[test]
    fn test_multiline_exception_on_jan01_retains_adjusted_year_timestamp() {
        let file_dec = b"12-31 23:59:59.000 S1 Shutdown initiated\n";
        let file_jan = b"01-01 00:00:01.000 S2 Exception on boot\n\
                         \tat com.example.Boot.init(Boot.java:20)\n\
                         \tat com.example.Boot.main(Boot.java:5)\n\
                         01-01 00:00:02.000 S2 Retry initiated\n";

        let mut dec_dir = std::env::temp_dir();
        dec_dir.push(format!("merge_multi_dec_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&dec_dir, file_dec).unwrap();

        let mut jan_dir = std::env::temp_dir();
        jan_dir.push(format!("merge_multi_jan_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&jan_dir, file_jan).unwrap();

        let merged = MergedLogBuffer::open_multiple(&[dec_dir.clone(), jan_dir.clone()]).unwrap();
        assert_eq!(merged.line_count(), 5);

        // Expected order:
        // 0: S1 Shutdown (Dec 31)
        // 1: S2 Exception on boot (Jan 01)
        // 2: S2 stack trace line 1 (Jan 01 inherited)
        // 3: S2 stack trace line 2 (Jan 01 inherited)
        // 4: S2 Retry initiated (Jan 01)
        assert!(merged.get_line(0).unwrap().contains("Shutdown initiated"));
        assert!(merged.get_line(1).unwrap().contains("Exception on boot"));
        assert!(merged.get_line(2).unwrap().contains("Boot.init"));
        assert!(merged.get_line(3).unwrap().contains("Boot.main"));
        assert!(merged.get_line(4).unwrap().contains("Retry initiated"));

        let ts0 = merged.get_line_timestamp_micros(0).unwrap();
        let ts1 = merged.get_line_timestamp_micros(1).unwrap();
        let ts2 = merged.get_line_timestamp_micros(2).unwrap();
        let ts3 = merged.get_line_timestamp_micros(3).unwrap();
        let ts4 = merged.get_line_timestamp_micros(4).unwrap();

        assert!(ts0 < ts1);
        assert_eq!(ts1, ts2);
        assert_eq!(ts2, ts3);
        assert!(ts3 < ts4);

        let _ = std::fs::remove_file(dec_dir);
        let _ = std::fs::remove_file(jan_dir);
    }

    #[test]
    fn test_cross_file_interleaved_dec31_and_jan01_across_multiple_sources() {
        // Source A spans Dec 31 -> Jan 01 (intra-file)
        let file_a = b"12-31 23:59:58.000 S_A Dec 58\n\
                       01-01 00:00:01.000 S_A Jan 01\n";

        // Source B also spans Dec 31 -> Jan 01 (intra-file)
        let file_b = b"12-31 23:59:59.000 S_B Dec 59\n\
                       01-01 00:00:00.500 S_B Jan 00.5\n";

        // Source C only has Jan 01 (cross-file rollover against prior Dec)
        let file_c = b"01-01 00:00:02.000 S_C Jan 02\n";

        let mut a_dir = std::env::temp_dir();
        a_dir.push(format!("merge_3src_a_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&a_dir, file_a).unwrap();

        let mut b_dir = std::env::temp_dir();
        b_dir.push(format!("merge_3src_b_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&b_dir, file_b).unwrap();

        let mut c_dir = std::env::temp_dir();
        c_dir.push(format!("merge_3src_c_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&c_dir, file_c).unwrap();

        let merged =
            MergedLogBuffer::open_multiple(&[a_dir.clone(), b_dir.clone(), c_dir.clone()]).unwrap();
        assert_eq!(merged.line_count(), 5);

        // Expected sorted order:
        // 0: S_A Dec 58
        // 1: S_B Dec 59
        // 2: S_B Jan 00.5
        // 3: S_A Jan 01
        // 4: S_C Jan 02
        assert!(merged.get_line(0).unwrap().contains("S_A Dec 58"));
        assert!(merged.get_line(1).unwrap().contains("S_B Dec 59"));
        assert!(merged.get_line(2).unwrap().contains("S_B Jan 00.5"));
        assert!(merged.get_line(3).unwrap().contains("S_A Jan 01"));
        assert!(merged.get_line(4).unwrap().contains("S_C Jan 02"));

        let _ = std::fs::remove_file(a_dir);
        let _ = std::fs::remove_file(b_dir);
        let _ = std::fs::remove_file(c_dir);
    }

    #[test]
    fn test_merge_reverse_order_jan01_and_dec31() {
        let file_jan = b"01-01 00:00:00.500 S_Jan Jan01 00s\n\
                         01-01 00:00:01.000 S_Jan Jan01 01s\n";

        let file_dec = b"12-31 23:59:58.000 S_Dec Dec31 58s\n\
                         12-31 23:59:59.500 S_Dec Dec31 59s\n";

        let mut jan_dir = std::env::temp_dir();
        jan_dir.push(format!("merge_rev_jan_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&jan_dir, file_jan).unwrap();

        let mut dec_dir = std::env::temp_dir();
        dec_dir.push(format!("merge_rev_dec_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&dec_dir, file_dec).unwrap();

        // Pass in reverse order: [jan, dec]
        let merged = MergedLogBuffer::open_multiple(&[jan_dir.clone(), dec_dir.clone()]).unwrap();
        assert_eq!(merged.line_count(), 4);

        // Expected chronological sequence: Dec 31 lines first, Jan 01 lines second
        assert_eq!(merged.get_line_source_id(0), Some(1));
        assert!(merged.get_line(0).unwrap().contains("S_Dec Dec31 58s"));

        assert_eq!(merged.get_line_source_id(1), Some(1));
        assert!(merged.get_line(1).unwrap().contains("S_Dec Dec31 59s"));

        assert_eq!(merged.get_line_source_id(2), Some(0));
        assert!(merged.get_line(2).unwrap().contains("S_Jan Jan01 00s"));

        assert_eq!(merged.get_line_source_id(3), Some(0));
        assert!(merged.get_line(3).unwrap().contains("S_Jan Jan01 01s"));

        let ts1 = merged.get_line_timestamp_micros(1).unwrap();
        let ts2 = merged.get_line_timestamp_micros(2).unwrap();
        assert!(ts1 < ts2);
        // Delta between 23:59:59.500 and 00:00:00.500 is exactly 1 second
        assert_eq!(ts2 - ts1, 1_000_000);

        let _ = std::fs::remove_file(jan_dir);
        let _ = std::fs::remove_file(dec_dir);
    }

    #[test]
    fn test_log_source_get_timestamp_micros_merged_buffer_preserves_year_offset() {
        let file_dec = b"12-31 23:59:59.000 S1 Dec\n";
        let file_jan = b"01-01 00:00:01.000 S2 Jan\n";

        let mut dec_dir = std::env::temp_dir();
        dec_dir.push(format!("merge_ts_trait_dec_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&dec_dir, file_dec).unwrap();

        let mut jan_dir = std::env::temp_dir();
        jan_dir.push(format!("merge_ts_trait_jan_{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&jan_dir, file_jan).unwrap();

        let merged = MergedLogBuffer::open_multiple(&[dec_dir.clone(), jan_dir.clone()]).unwrap();
        let source: &dyn LogSource = &merged;

        let ts0 = source.get_timestamp_micros(0).expect("ts0 should exist");
        let ts1 = source.get_timestamp_micros(1).expect("ts1 should exist");

        // ts1 must have the ONE_YEAR_MICROS offset preserved through the trait
        assert!(ts1 > ts0);
        assert_eq!(ts1 - ts0, 2_000_000); // Exactly 2 seconds

        let _ = std::fs::remove_file(dec_dir);
        let _ = std::fs::remove_file(jan_dir);
    }
}

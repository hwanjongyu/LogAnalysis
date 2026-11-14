//! Adversarial empirical challenge test suite for Milestone 1 Core remediation.
//!
//! Evaluates:
//! 1. Stream ring buffer match counting (`append_streaming_batch`) under gray, null, bg colors,
//!    multiple trim events, and validates total_matched_lines / rule_counts against ground-truth oracle.
//! 2. ADB streaming UTF-8 splitting across 64KB boundary with 3-byte CJK and 4-byte emoji.
//! 3. `LogSource::get_timestamp_micros` on all log source types with edge indices (0, len-1, len, usize::MAX).
//! 4. `correlation.rs` and `timeline.rs` under extreme microsecond timestamps, negative time differences, and i64 extremes.

use crossbeam_channel::bounded;
use loganalysis_core::adb::stream_log_lines;
use loganalysis_core::buffer::file_buffer::FileLogBuffer;
use loganalysis_core::buffer::merged_buffer::MergedLogBuffer;
use loganalysis_core::buffer::stream_buffer::StreamLogBuffer;
use loganalysis_core::buffer::{find_nearest_timestamp_row, ActiveLogSource, LogSource};
use loganalysis_core::filter::{
    append_streaming_batch, execute_filtering, EpsTimelineData, FilterResult, FilterRule,
};
use std::borrow::Cow;
use std::io::{BufReader, Write};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tempfile::NamedTempFile;
use uuid::Uuid;

// ============================================================================
// CHALLENGE 1: Stream ring buffer match counting with color variants & trims
// ============================================================================

struct StreamMockSource {
    lines: Vec<String>,
}

impl StreamMockSource {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }
    fn push(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }
    fn drain(&mut self, count: usize) {
        let n = count.min(self.lines.len());
        self.lines.drain(0..n);
    }
}

impl LogSource for StreamMockSource {
    fn line_count(&self) -> usize {
        self.lines.len()
    }
    fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
        self.lines.get(idx).map(|s| s.as_bytes())
    }
    fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
        self.lines.get(idx).map(|s| Cow::Borrowed(s.as_str()))
    }
}

#[test]
fn test_challenge_1_stream_match_counting_gray_null_bg_colors_and_multi_trim() {
    // 1. Define adversarial filter rules:
    // - rule_gray: explicitly gray text fg_hex == 0x808080
    // - rule_null: no colors configured (null fg, null bg)
    // - rule_bg: blue background, null foreground
    // - rule_overlap: yellow fg, green bg, overlaps with other rules
    // - rule_inactive: active = false (must never count)
    let id_gray = Uuid::new_v4();
    let id_null = Uuid::new_v4();
    let id_bg = Uuid::new_v4();
    let id_overlap = Uuid::new_v4();
    let id_inactive = Uuid::new_v4();

    let filters = vec![
        FilterRule {
            id: id_gray,
            text: "TAG_GRAY".to_string(),
            text_color: "gray".to_string(), // 0x808080
            bg_color: "".to_string(),
            active: true,
            regex: false,
            case_sensitive: true,
            exclude: false,
            description: "Gray Filter".to_string(),
        },
        FilterRule {
            id: id_null,
            text: "TAG_NULL".to_string(),
            text_color: "".to_string(), // None
            bg_color: "".to_string(),   // None
            active: true,
            regex: false,
            case_sensitive: true,
            exclude: false,
            description: "Null Color Filter".to_string(),
        },
        FilterRule {
            id: id_bg,
            text: "TAG_BG".to_string(),
            text_color: "".to_string(),
            bg_color: "blue".to_string(), // Some(0x2196F3)
            active: true,
            regex: false,
            case_sensitive: true,
            exclude: false,
            description: "BG Only Filter".to_string(),
        },
        FilterRule {
            id: id_overlap,
            text: "OVERLAP".to_string(),
            text_color: "yellow".to_string(),
            bg_color: "green".to_string(),
            active: true,
            regex: false,
            case_sensitive: true,
            exclude: false,
            description: "Overlap Filter".to_string(),
        },
        FilterRule {
            id: id_inactive,
            text: "TAG_INACTIVE".to_string(),
            text_color: "red".to_string(),
            bg_color: "".to_string(),
            active: false,
            regex: false,
            case_sensitive: true,
            exclude: false,
            description: "Inactive Filter".to_string(),
        },
    ];

    for show_only_filtered in [false, true] {
        let mut source = StreamMockSource::new();
        let mut result = FilterResult::default();

        // Helper to check parity with full execution oracle
        let check_parity = |step: &str, res: &FilterResult, src: &StreamMockSource| {
            let oracle = execute_filtering(src, &filters, show_only_filtered, None, 0)
                .expect("Oracle execution failed");

            assert_eq!(
                res.total_matched_lines, oracle.total_matched_lines,
                "[{step}] total_matched_lines divergence! incremental={}, oracle={}",
                res.total_matched_lines, oracle.total_matched_lines
            );

            for rule in &filters {
                let inc_cnt = res.count_for_rule(&rule.id);
                let ora_cnt = oracle.count_for_rule(&rule.id);
                assert_eq!(
                    inc_cnt, ora_cnt,
                    "[{step}] count_for_rule({}) divergence! inc={}, ora={}",
                    rule.text, inc_cnt, ora_cnt
                );
            }

            assert_eq!(
                res.visible_indices.len(),
                oracle.visible_indices.len(),
                "[{step}] visible_indices length mismatch"
            );
        };

        // Batch 1: Initial 100 lines
        // 20 TAG_GRAY, 20 TAG_NULL, 20 TAG_BG, 10 OVERLAP with TAG_GRAY, 10 TAG_INACTIVE, 20 UNMATCHED
        for i in 0..20 {
            source.push(&format!("Line {i}: TAG_GRAY message"));
        }
        for i in 20..40 {
            source.push(&format!("Line {i}: TAG_NULL message"));
        }
        for i in 40..60 {
            source.push(&format!("Line {i}: TAG_BG message"));
        }
        for i in 60..70 {
            source.push(&format!("Line {i}: TAG_GRAY OVERLAP message"));
        }
        for i in 70..80 {
            source.push(&format!("Line {i}: TAG_INACTIVE message"));
        }
        for i in 80..100 {
            source.push(&format!("Line {i}: UNMATCHED plain message"));
        }

        append_streaming_batch(
            &mut result,
            &source,
            0..source.line_count(),
            0,
            &filters,
            show_only_filtered,
        );
        check_parity("Batch 1 (Initial)", &result, &source);

        // Batch 2: Append 40 lines and trim 25 lines from head
        let trim_1 = 25;
        source.drain(trim_1);
        let start_len = source.line_count();
        for i in 0..10 {
            source.push(&format!("NewBatch2_{i}: TAG_GRAY msg"));
        }
        for i in 10..20 {
            source.push(&format!("NewBatch2_{i}: TAG_BG OVERLAP msg"));
        }
        for i in 20..40 {
            source.push(&format!("NewBatch2_{i}: plain unmatching line"));
        }
        let end_len = source.line_count();

        append_streaming_batch(
            &mut result,
            &source,
            start_len..end_len,
            trim_1,
            &filters,
            show_only_filtered,
        );
        check_parity("Batch 2 (Trim 25)", &result, &source);

        // Batch 3: Append 30 lines and trim 50 lines from head
        let trim_2 = 50;
        source.drain(trim_2);
        let start_len = source.line_count();
        for i in 0..15 {
            source.push(&format!("NewBatch3_{i}: TAG_NULL msg"));
        }
        for i in 15..30 {
            source.push(&format!("NewBatch3_{i}: TAG_GRAY msg"));
        }
        let end_len = source.line_count();

        append_streaming_batch(
            &mut result,
            &source,
            start_len..end_len,
            trim_2,
            &filters,
            show_only_filtered,
        );
        check_parity("Batch 3 (Trim 50)", &result, &source);

        // Batch 4: Trim more lines than currently in buffer (massive trim)
        let total_current = source.line_count();
        let trim_3 = total_current + 10;
        source.drain(total_current); // buffer is now empty
        source.push("PostMassiveTrim: TAG_BG message");
        source.push("PostMassiveTrim: plain message");

        append_streaming_batch(
            &mut result,
            &source,
            0..2,
            trim_3,
            &filters,
            show_only_filtered,
        );
        check_parity("Batch 4 (Massive Trim)", &result, &source);
    }
}

// ============================================================================
// CHALLENGE 2: ADB streaming UTF-8 split across 64KB chunks
// ============================================================================

#[test]
fn test_challenge_2_adb_utf8_split_cjk_and_emoji_comprehensive() {
    // Test 3-byte CJK chars split at boundary offset 1 and 2
    // '한' = [0xED, 0x95, 0x9C]
    // '漢' = [0xE6, 0xBC, 0xA2]
    // '字' = [0xE5, 0xAD, 0x97]
    //
    // Test 4-byte Emoji chars split at boundary offset 1, 2, and 3
    // '🔥' = [0xF0, 0x9F, 0x94, 0xA5]
    // '🚀' = [0xF0, 0x9F, 0x9A, 0x80]

    let test_cases = vec![
        // (Name, multi-byte sequence, split offset from end of chunk 1)
        ("CJK_한_split_1", vec![0xED, 0x95, 0x9C], 1),
        ("CJK_한_split_2", vec![0xED, 0x95, 0x9C], 2),
        ("CJK_漢_split_1", vec![0xE6, 0xBC, 0xA2], 1),
        ("CJK_漢_split_2", vec![0xE6, 0xBC, 0xA2], 2),
        ("CJK_字_split_1", vec![0xE5, 0xAD, 0x97], 1),
        ("CJK_字_split_2", vec![0xE5, 0xAD, 0x97], 2),
        ("Emoji_Fire_split_1", vec![0xF0, 0x9F, 0x94, 0xA5], 1),
        ("Emoji_Fire_split_2", vec![0xF0, 0x9F, 0x94, 0xA5], 2),
        ("Emoji_Fire_split_3", vec![0xF0, 0x9F, 0x94, 0xA5], 3),
        ("Emoji_Rocket_split_1", vec![0xF0, 0x9F, 0x9A, 0x80], 1),
        ("Emoji_Rocket_split_2", vec![0xF0, 0x9F, 0x9A, 0x80], 2),
        ("Emoji_Rocket_split_3", vec![0xF0, 0x9F, 0x9A, 0x80], 3),
    ];

    for (case_name, char_bytes, split_offset) in test_cases {
        let (tx, rx) = bounded(10);
        let stop_flag = Arc::new(AtomicBool::new(false));

        let mut payload = vec![b'X'; 65536 - split_offset];
        payload.extend_from_slice(&char_bytes[0..split_offset]);
        assert_eq!(payload.len(), 65536);

        // Chunk 2 continues the character bytes, then adds some ASCII and newline
        payload.extend_from_slice(&char_bytes[split_offset..]);
        payload.extend_from_slice(b" RestOfLine\n");

        let reader = BufReader::new(&payload[..]);
        stream_log_lines(reader, tx, stop_flag);

        let mut all_lines = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            all_lines.extend(batch);
        }

        assert_eq!(
            all_lines.len(),
            2,
            "[{case_name}] Expected 2 chunk lines for 64KB split"
        );

        // Chunk 1 MUST NOT contain replacement character \u{FFFD}
        assert!(
            !all_lines[0].contains('\u{FFFD}'),
            "[{case_name}] Chunk 1 contained \\u{{FFFD}} replacement char!"
        );
        assert_eq!(
            all_lines[0].len(),
            65536 - split_offset,
            "[{case_name}] Chunk 1 length mismatch"
        );

        // Chunk 2 MUST start with the reconstituted character
        let expected_char = std::str::from_utf8(&char_bytes).unwrap();
        assert!(
            !all_lines[1].contains('\u{FFFD}'),
            "[{case_name}] Chunk 2 contained \\u{{FFFD}} replacement char!"
        );
        assert!(
            all_lines[1].starts_with(expected_char),
            "[{case_name}] Chunk 2 should start with '{expected_char}', but got: {:?}",
            &all_lines[1][..all_lines[1].len().min(10)]
        );
    }
}

#[test]
fn test_challenge_2_adb_utf8_multiple_consecutive_chunk_boundaries() {
    // Multi-chunk stress test: 4 consecutive 64KB chunks (256KB total)
    // where EVERY 64KB boundary splits a 4-byte emoji '🔥'
    let (tx, rx) = bounded(20);
    let stop_flag = Arc::new(AtomicBool::new(false));

    let emoji = [0xF0, 0x9F, 0x94, 0xA5]; // 🔥
    let mut stream_bytes = Vec::with_capacity(256 * 1024);

    for boundary in 1..=3 {
        let target_len = boundary * 65536;
        let fill = target_len - stream_bytes.len() - 2; // leave 2 bytes before boundary
        stream_bytes.extend(std::iter::repeat(b'Z').take(fill));
        // Put 2 bytes of emoji at end of chunk, 2 bytes at start of next chunk
        stream_bytes.extend_from_slice(&emoji[0..2]);
        assert_eq!(stream_bytes.len(), target_len);
        stream_bytes.extend_from_slice(&emoji[2..4]);
        stream_bytes.extend_from_slice(b" ContinuingChunkData ");
    }
    stream_bytes.push(b'\n');

    let reader = BufReader::new(&stream_bytes[..]);
    stream_log_lines(reader, tx, stop_flag);

    let mut lines = Vec::new();
    while let Ok(batch) = rx.try_recv() {
        lines.extend(batch);
    }

    assert_eq!(lines.len(), 4, "Expected 4 lines for 256KB stream");
    for (idx, line) in lines.iter().enumerate() {
        assert!(
            !line.contains('\u{FFFD}'),
            "Line {idx} contained replacement char \\u{{FFFD}}"
        );
    }
    // Chunks 1, 2, 3 must start with '🔥'
    for idx in 1..4 {
        assert!(
            lines[idx].starts_with('🔥'),
            "Line {idx} should start with '🔥'"
        );
    }
}

// ============================================================================
// CHALLENGE 3: LogSource::get_timestamp_micros boundary tests
// ============================================================================

#[test]
fn test_challenge_3_log_source_edge_indices_on_all_implementations() {
    // 1. SingleLogBuffer / FileLogBuffer
    {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "11-06 18:00:00.000 1 1 I Tag: First line").unwrap();
        writeln!(tmp, "No timestamp on this second line").unwrap();
        writeln!(tmp, "11-06 18:00:10.500 1 1 I Tag: Third line").unwrap();
        tmp.flush().unwrap();

        let file_buf = FileLogBuffer::open(tmp.path()).unwrap();
        let len = file_buf.line_count();
        assert_eq!(len, 3);

        // Test index 0
        let ts0 = file_buf.get_timestamp_micros(0);
        assert!(ts0.is_some() && ts0.unwrap() > 0);

        // Test index 1 (un-timestamped)
        assert_eq!(file_buf.get_timestamp_micros(1), None);

        // Test index len - 1
        let ts_last = file_buf.get_timestamp_micros(len - 1);
        assert!(ts_last.is_some() && ts_last.unwrap() > ts0.unwrap());

        // Test boundary off-by-one: len, len+1, usize::MAX
        assert_eq!(file_buf.get_timestamp_micros(len), None);
        assert_eq!(file_buf.get_timestamp_micros(len + 1), None);
        assert_eq!(file_buf.get_timestamp_micros(usize::MAX), None);
        assert_eq!(file_buf.get_timestamp_micros(usize::MAX / 2), None);

        // Wrap in ActiveLogSource
        let active = ActiveLogSource::File(file_buf);
        assert_eq!(active.get_timestamp_micros(len), None);
        assert_eq!(active.get_timestamp_micros(usize::MAX), None);
        assert!(active.get_timestamp_micros(0).is_some());
    }

    // 2. StreamLogBuffer
    {
        let mut stream_buf = StreamLogBuffer::new(100, 10);
        assert_eq!(stream_buf.line_count(), 0);
        // On empty buffer
        assert_eq!(stream_buf.get_timestamp_micros(0), None);
        assert_eq!(stream_buf.get_timestamp_micros(usize::MAX), None);

        stream_buf.append_chunk(vec![
            "11-06 18:00:00.100 1 1 I Tag: Stream 0".to_string(),
            "Non timestamp stream line".to_string(),
            "11-06 18:00:05.200 1 1 I Tag: Stream 2".to_string(),
        ]);
        let len = stream_buf.line_count();
        assert_eq!(len, 3);

        assert!(stream_buf.get_timestamp_micros(0).is_some());
        assert_eq!(stream_buf.get_timestamp_micros(1), None);
        assert!(stream_buf.get_timestamp_micros(len - 1).is_some());
        assert_eq!(stream_buf.get_timestamp_micros(len), None);
        assert_eq!(stream_buf.get_timestamp_micros(len + 1), None);
        assert_eq!(stream_buf.get_timestamp_micros(usize::MAX), None);

        // Wrap in ActiveLogSource
        let active = ActiveLogSource::Stream(stream_buf);
        assert_eq!(active.get_timestamp_micros(len), None);
        assert_eq!(active.get_timestamp_micros(usize::MAX), None);
    }

    // 3. MergedLogBuffer
    {
        let mut tmp_dec = NamedTempFile::new().unwrap();
        writeln!(tmp_dec, "12-31 23:59:59.000 1 1 I Tag: Dec 31 line").unwrap();
        tmp_dec.flush().unwrap();

        let mut tmp_jan = NamedTempFile::new().unwrap();
        writeln!(tmp_jan, "01-01 00:00:01.000 1 1 I Tag: Jan 01 line").unwrap();
        tmp_jan.flush().unwrap();

        let f_dec = FileLogBuffer::open(tmp_dec.path()).unwrap();
        let f_jan = FileLogBuffer::open(tmp_jan.path()).unwrap();

        // Pass in reverse order: [Jan 01, Dec 31]
        let merged = MergedLogBuffer::from_sources(
            vec![Arc::new(f_jan), Arc::new(f_dec)],
            vec!["jan01.log".into(), "dec31.log".into()],
        )
        .unwrap();

        let len = merged.line_count();
        assert_eq!(len, 2);

        // Row 0 should be Dec 31, Row 1 should be Jan 01 with year rollover offset
        let ts0 = merged.get_timestamp_micros(0).unwrap();
        let ts1 = merged.get_timestamp_micros(1).unwrap();
        assert!(ts1 > ts0, "Jan 01 timestamp must exceed Dec 31 timestamp");
        assert!(
            ts1 - ts0 < 10_000_000,
            "Delta between Dec 31 23:59:59 and Jan 01 00:00:01 should be ~2s"
        );

        // Edge indices: len, len+1, usize::MAX
        assert_eq!(merged.get_timestamp_micros(len), None);
        assert_eq!(merged.get_timestamp_micros(len + 1), None);
        assert_eq!(merged.get_timestamp_micros(usize::MAX), None);
        assert_eq!(merged.get_timestamp_micros(usize::MAX - 1), None);

        // Wrap in ActiveLogSource
        let active = ActiveLogSource::Merged(merged);
        assert_eq!(active.get_timestamp_micros(len), None);
        assert_eq!(active.get_timestamp_micros(usize::MAX), None);
        assert!(active.get_timestamp_micros(0).is_some());
    }
}

// ============================================================================
// CHALLENGE 4: Correlation & Timeline arithmetic overflow stress
// ============================================================================

struct ExtremeTimestampMockSource {
    timestamps: Vec<Option<i64>>,
}

impl LogSource for ExtremeTimestampMockSource {
    fn line_count(&self) -> usize {
        self.timestamps.len()
    }
    fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
        if idx < self.timestamps.len() {
            Some(b"11-06 18:00:00.000 1 1 I Tag: Msg")
        } else {
            None
        }
    }
    fn get_line(&self, idx: usize) -> Option<Cow<'_, str>> {
        self.get_raw_line(idx).map(|b| Cow::Borrowed(std::str::from_utf8(b).unwrap()))
    }
    fn get_timestamp_micros(&self, idx: usize) -> Option<i64> {
        self.timestamps.get(idx).copied().flatten()
    }
}

#[test]
fn test_challenge_4_correlation_find_nearest_extreme_timestamps() {
    let extreme_source = ExtremeTimestampMockSource {
        timestamps: vec![
            Some(i64::MIN + 1),
            Some(-1_000_000_000),
            Some(-500),
            Some(0),
            Some(500),
            Some(1_000_000_000),
            Some(i64::MAX - 1),
        ],
    };
    let visible_indices: Vec<usize> = (0..extreme_source.line_count()).collect();

    // 1. Target is i64::MIN
    let r1 = find_nearest_timestamp_row(&extreme_source, &visible_indices, i64::MIN);
    assert_eq!(r1, Some(0));

    // 2. Target is i64::MAX
    let r2 = find_nearest_timestamp_row(&extreme_source, &visible_indices, i64::MAX);
    assert_eq!(r2, Some(6));

    // 3. Target is 0
    let r3 = find_nearest_timestamp_row(&extreme_source, &visible_indices, 0);
    assert_eq!(r3, Some(3));

    // 4. Target is in negative range
    let r4 = find_nearest_timestamp_row(&extreme_source, &visible_indices, -600);
    assert_eq!(r4, Some(2)); // -500 is closest to -600

    // 5. Target is in large positive range
    let r5 = find_nearest_timestamp_row(&extreme_source, &visible_indices, 900_000_000);
    assert_eq!(r5, Some(5)); // 1_000_000_000 is closest
}

#[test]
fn test_challenge_4_timeline_extreme_and_negative_timestamps() {
    // Test 1: Source with negative microsecond timestamps
    let neg_source = ExtremeTimestampMockSource {
        timestamps: vec![
            Some(-10_000_000), // -10s
            Some(-8_000_000),  // -8s
            Some(-5_000_000),  // -5s
            Some(-1_000_000),  // -1s
            Some(0),           // 0s
        ],
    };
    let vis_indices: Vec<usize> = (0..neg_source.line_count()).collect();
    let vis_styles = vec![loganalysis_core::filter::ResolvedStyle::default(); vis_indices.len()];

    let timeline = EpsTimelineData::compute(&neg_source, &vis_indices, &vis_styles, 32);
    assert!(timeline.has_timestamps);
    assert_eq!(timeline.min_timestamp, -10_000_000);
    assert_eq!(timeline.max_timestamp, 0);
    assert!(timeline.total_duration_secs > 9.9 && timeline.total_duration_secs < 10.1);

    // Test 2: Source with year rollover timestamps
    let rollover_source = ExtremeTimestampMockSource {
        timestamps: vec![
            Some(365 * 86_400 * 1_000_000), // Dec 31
            Some(365 * 86_400 * 1_000_000 + 1_000_000),
            Some(0), // Jan 01 (unadjusted, will trigger rollover)
            Some(5_000_000),
        ],
    };
    let vis_indices: Vec<usize> = (0..rollover_source.line_count()).collect();
    let vis_styles = vec![loganalysis_core::filter::ResolvedStyle::default(); vis_indices.len()];

    let timeline_ro = EpsTimelineData::compute(&rollover_source, &vis_indices, &vis_styles, 32);
    assert!(timeline_ro.has_timestamps);
    assert!(timeline_ro.total_duration_secs > 0.0);
}

#[test]
fn test_challenge_4_timeline_i64_min_behavior() {
    // Specifically test what happens when a source emits i64::MIN or values near i64::MIN
    let extreme_source = ExtremeTimestampMockSource {
        timestamps: vec![
            Some(i64::MIN),
            Some(i64::MIN + 100),
        ],
    };
    let vis_indices: Vec<usize> = (0..extreme_source.line_count()).collect();
    let vis_styles = vec![loganalysis_core::filter::ResolvedStyle::default(); vis_indices.len()];

    // We catch unwind or check if compute handles or panics
    let res = std::panic::catch_unwind(|| {
        EpsTimelineData::compute(&extreme_source, &vis_indices, &vis_styles, 32)
    });

    match res {
        Ok(timeline) => {
            println!("EpsTimelineData::compute survived i64::MIN: has_timestamps={}", timeline.has_timestamps);
        }
        Err(err) => {
            println!("EMPIRICAL FINDING: EpsTimelineData::compute panicked on i64::MIN: {:?}", err);
        }
    }
}

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use loganalysis_core::buffer::merged_buffer::{MergedLogBuffer, ONE_YEAR_MICROS};
use loganalysis_core::buffer::LogSource;
use loganalysis_core::filter::engine::ResolvedStyle;
use loganalysis_core::filter::timeline::EpsTimelineData;
use tempfile::NamedTempFile;

fn create_temp_log_file(content: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("Failed to create named temp file");
    file.write_all(content)
        .expect("Failed to write to temp file");
    file.flush().expect("Failed to flush temp file");
    file
}

// 1. Massive Log Volume Scale Test (60,000 entries across 4 sources)
// Verifies that merging and sorting across Dec 31 -> Jan 01 rollover completes
// without performance bottlenecks, without integer overflows, and with strict
// timestamp monotonicity across all 60,000 lines.
#[test]
fn test_stress_massive_volume_rollover_60k_lines() {
    let total_lines_per_src = 15_000usize;
    let mut f1_data = Vec::with_capacity(total_lines_per_src * 60);
    let mut f2_data = Vec::with_capacity(total_lines_per_src * 60);
    let mut f3_data = Vec::with_capacity(total_lines_per_src * 60);
    let mut f4_data = Vec::with_capacity(total_lines_per_src * 60);

    // Source 1: Dec 31 23:50:00 -> Jan 01 00:10:00 (crosses rollover)
    // 7,500 lines pre-midnight, 7,500 lines post-midnight
    for i in 0..7_500 {
        let sec = (i * 600) / 7_500; // 0..599 seconds -> 23:50:00 to 23:59:59
        let min = 50 + sec / 60;
        let s = sec % 60;
        let ms = (i * 13) % 1000;
        let line = format!("12-31 23:{min:02}:{s:02}.{ms:03} 1001 1001 I S1: Pre {i}\n");
        f1_data.extend_from_slice(line.as_bytes());
    }
    for i in 0..7_500 {
        let sec = (i * 600) / 7_500; // 0..599 seconds -> 00:00:00 to 00:09:59
        let min = sec / 60;
        let s = sec % 60;
        let ms = (i * 17) % 1000;
        let line = format!("01-01 00:{min:02}:{s:02}.{ms:03} 1001 1001 I S1: Post {i}\n");
        f1_data.extend_from_slice(line.as_bytes());
    }

    // Source 2: Dec 31 23:45:00 -> Jan 01 00:15:00 (crosses rollover)
    for i in 0..7_500 {
        let sec = (i * 900) / 7_500; // 23:45:00 to 23:59:59
        let min = 45 + sec / 60;
        let s = sec % 60;
        let ms = (i * 19) % 1000;
        let line = format!("12-31 23:{min:02}:{s:02}.{ms:03} 1002 1002 D S2: Pre {i}\n");
        f2_data.extend_from_slice(line.as_bytes());
    }
    for i in 0..7_500 {
        let sec = (i * 900) / 7_500; // 00:00:00 to 00:14:59
        let min = sec / 60;
        let s = sec % 60;
        let ms = (i * 23) % 1000;
        let line = format!("01-01 00:{min:02}:{s:02}.{ms:03} 1002 1002 D S2: Post {i}\n");
        f2_data.extend_from_slice(line.as_bytes());
    }

    // Source 3: Pre-midnight only (12-31 23:55:00 -> 23:59:59)
    for i in 0..total_lines_per_src {
        let sec = (i * 300) / total_lines_per_src;
        let min = 55 + sec / 60;
        let s = sec % 60;
        let ms = (i * 7) % 1000;
        let line = format!("12-31 23:{min:02}:{s:02}.{ms:03} 1003 1003 W S3: Warn {i}\n");
        f3_data.extend_from_slice(line.as_bytes());
    }

    // Source 4: Post-midnight only (01-01 00:00:01 -> 00:05:00)
    for i in 0..total_lines_per_src {
        let sec = 1 + (i * 299) / total_lines_per_src;
        let min = sec / 60;
        let s = sec % 60;
        let ms = (i * 11) % 1000;
        let line = format!("01-01 00:{min:02}:{s:02}.{ms:03} 1004 1004 E S4: Err {i}\n");
        f4_data.extend_from_slice(line.as_bytes());
    }

    let f1 = create_temp_log_file(&f1_data);
    let f2 = create_temp_log_file(&f2_data);
    let f3 = create_temp_log_file(&f3_data);
    let f4 = create_temp_log_file(&f4_data);

    let paths = vec![
        f1.path().to_path_buf(),
        f2.path().to_path_buf(),
        f3.path().to_path_buf(),
        f4.path().to_path_buf(),
    ];

    let t_start = Instant::now();
    let merged = MergedLogBuffer::open_multiple(&paths).expect("Failed to merge 60k lines");
    let merge_elapsed = t_start.elapsed();

    println!(
        "[EMPIRICAL BENCHMARK] Merging 60,000 lines across 4 sources completed in {:?}",
        merge_elapsed
    );
    assert_eq!(merged.line_count(), 60_000);
    // Latency target: 60k lines merge and sort should easily complete in under 500ms
    assert!(
        merge_elapsed.as_millis() < 500,
        "60k lines merge took too long: {:?}",
        merge_elapsed
    );

    // Verify strict monotonicity across all 60,000 lines
    let mut prev_ts = -1i64;
    let mut rollover_crossings = 0usize;

    for i in 0..merged.line_count() {
        let ts = merged
            .get_line_timestamp_micros(i)
            .unwrap_or_else(|| panic!("Missing timestamp at line {i}"));

        assert!(ts >= 0, "Negative timestamp encountered at row {i}: {ts}");
        assert!(
            ts < 2 * ONE_YEAR_MICROS,
            "Timestamp overflow at row {i}: {ts}"
        );

        if prev_ts >= 0 {
            assert!(
                ts >= prev_ts,
                "Monotonicity violation at row {i}: curr ts {ts} < prev ts {prev_ts}"
            );
            // Count where timestamp jumps across midnight (from < ONE_YEAR_MICROS to >= ONE_YEAR_MICROS)
            if prev_ts < ONE_YEAR_MICROS && ts >= ONE_YEAR_MICROS {
                rollover_crossings += 1;
            }
        }
        prev_ts = ts;
    }

    assert_eq!(
        rollover_crossings, 1,
        "Expected exactly 1 transition point from Dec 31 to Jan 01 in the sorted stream"
    );
}

// 2. Deep Multiline Exception Traces Across Midnight
// Verifies that multiline stack frames (50+ frames with no timestamps) immediately
// before and after the midnight rollover inherit the parent's adjusted timestamp
// and remain strictly contiguous without interleaving other sources.
#[test]
fn test_adversarial_multiline_deep_exception_traces_crossing_midnight() {
    let mut f1_data = Vec::new();
    // Source 1: Dec 31 23:59:59.950 with 40 stack frames
    f1_data
        .extend_from_slice(b"12-31 23:59:59.950 200 200 E AndroidRuntime: FATAL EXCEPTION: main\n");
    for frame in 1..=40 {
        let frame_line =
            format!("\tat com.example.service.Worker.step{frame}(Worker.java:{frame})\n");
        f1_data.extend_from_slice(frame_line.as_bytes());
    }
    f1_data
        .extend_from_slice(b"12-31 23:59:59.999 200 200 I SystemServer: Pre-midnight final log\n");

    let mut f2_data = Vec::new();
    // Source 2: Jan 01 00:00:00.050 with 60 stack frames
    f2_data.extend_from_slice(
        b"01-01 00:00:00.050 300 300 E AndroidRuntime: FATAL EXCEPTION: bg-worker\n",
    );
    for frame in 1..=60 {
        let frame_line = format!("\tat com.example.bg.Task.run{frame}(Task.java:{frame})\n");
        f2_data.extend_from_slice(frame_line.as_bytes());
    }
    f2_data.extend_from_slice(b"01-01 00:00:00.200 300 300 I SystemServer: Post-midnight stable\n");

    // Source 3: Jan 01 00:00:00.050 (SAME MILLISECOND as Source 2) with 30 frames
    let mut f3_data = Vec::new();
    f3_data.extend_from_slice(
        b"01-01 00:00:00.050 400 400 E AndroidRuntime: FATAL EXCEPTION: io-thread\n",
    );
    for frame in 1..=30 {
        let frame_line = format!("\tat com.example.io.Handler.exec{frame}(Handler.java:{frame})\n");
        f3_data.extend_from_slice(frame_line.as_bytes());
    }

    let f1 = create_temp_log_file(&f1_data);
    let f2 = create_temp_log_file(&f2_data);
    let f3 = create_temp_log_file(&f3_data);

    let merged = MergedLogBuffer::open_multiple(&[
        f1.path().to_path_buf(),
        f2.path().to_path_buf(),
        f3.path().to_path_buf(),
    ])
    .expect("merge failed");

    // Line breakdown:
    // S1: 1 (exc) + 40 (frames) + 1 (final) = 42 lines
    // S2: 1 (exc) + 60 (frames) + 1 (stable) = 62 lines
    // S3: 1 (exc) + 30 (frames) = 31 lines
    // Total = 135 lines
    assert_eq!(merged.line_count(), 135);

    // 1. S1 exception header and 40 frames must be rows 0..41
    let s1_exc_ts = merged.get_line_timestamp_micros(0).unwrap();
    assert!(!merged.get_line(0).unwrap().contains("Worker.step"));
    assert!(merged
        .get_line(0)
        .unwrap()
        .contains("FATAL EXCEPTION: main"));

    for r in 1..=40 {
        let frame_ts = merged.get_line_timestamp_micros(r).unwrap();
        assert_eq!(
            frame_ts, s1_exc_ts,
            "S1 frame {r} timestamp must match exception header"
        );
        assert_eq!(merged.get_line_source_id(r), Some(0));
        assert!(
            merged
                .get_line(r)
                .unwrap()
                .contains(&format!("Worker.step{r}")),
            "S1 stack frame order must be preserved at row {r}"
        );
    }

    // Row 41 is S1 pre-midnight final log
    assert_eq!(merged.get_line_source_id(41), Some(0));
    assert!(merged
        .get_line(41)
        .unwrap()
        .contains("Pre-midnight final log"));

    // 2. Rows 42..102 is S2 (1 header + 60 frames)
    // Rows 103..133 is S3 (1 header + 30 frames)
    // Because both have timestamp 00:00:00.050, source_id ordering ensures
    // Source 1 (sid=1) frames remain contiguous, followed by Source 2 (sid=2) frames
    let s2_header_sid = merged.get_line_source_id(42).unwrap();
    assert_eq!(s2_header_sid, 1);
    assert!(merged
        .get_line(42)
        .unwrap()
        .contains("FATAL EXCEPTION: bg-worker"));
    let s2_ts = merged.get_line_timestamp_micros(42).unwrap();
    assert!(s2_ts >= ONE_YEAR_MICROS);

    for r in 43..=102 {
        let frame_idx = r - 42;
        assert_eq!(
            merged.get_line_source_id(r),
            Some(1),
            "S2 frames must remain contiguous at row {r}"
        );
        assert_eq!(
            merged.get_line_timestamp_micros(r).unwrap(),
            s2_ts,
            "S2 frame at row {r} must inherit adjusted year timestamp"
        );
        assert!(merged
            .get_line(r)
            .unwrap()
            .contains(&format!("Task.run{frame_idx}")));
    }

    // Next is S3: 31 lines
    let s3_header_sid = merged.get_line_source_id(103).unwrap();
    assert_eq!(s3_header_sid, 2);
    assert!(merged
        .get_line(103)
        .unwrap()
        .contains("FATAL EXCEPTION: io-thread"));
    let s3_ts = merged.get_line_timestamp_micros(103).unwrap();
    assert_eq!(s3_ts, s2_ts);

    for r in 104..=133 {
        let frame_idx = r - 103;
        assert_eq!(merged.get_line_source_id(r), Some(2));
        assert_eq!(merged.get_line_timestamp_micros(r).unwrap(), s3_ts);
        assert!(merged
            .get_line(r)
            .unwrap()
            .contains(&format!("Handler.exec{frame_idx}")));
    }

    // Row 134 is S2 Post-midnight stable
    assert_eq!(merged.get_line_source_id(134), Some(1));
    assert!(merged
        .get_line(134)
        .unwrap()
        .contains("Post-midnight stable"));
}

// 3. Timeline Rollover Anomaly & Bucket Correctness
// Verifies that EpsTimelineData::compute correctly distributes events across
// 128 buckets spanning Dec 31 to Jan 01, handles sudden bursts after midnight,
// and detects statistical anomalies without panics or negative spans.
#[test]
fn test_adversarial_eps_timeline_128_buckets_burst_and_anomaly() {
    let mut dec_data = Vec::new();
    // 60 seconds before midnight: 10 lines per second (600 lines)
    for s in 0..60 {
        for ms in (0..1000).step_by(100) {
            let line = format!("12-31 23:59:{s:02}.{ms:03} 10 10 I Tag: Dec line\n");
            dec_data.extend_from_slice(line.as_bytes());
        }
    }

    let mut jan_data = Vec::new();
    // 00:00:00 to 00:00:10: Massive crash burst of 2,000 errors at 00:00:05
    for ms in 0..2000 {
        let s = 5;
        let line = format!(
            "01-01 00:00:{s:02}.{:03} 10 10 E CrashTag: Burst anomaly crash {ms}\n",
            ms % 1000
        );
        jan_data.extend_from_slice(line.as_bytes());
    }
    // 00:00:06 to 00:00:59: normal traffic 10 lines per sec (540 lines)
    for s in 6..60 {
        for ms in (0..1000).step_by(100) {
            let line = format!("01-01 00:00:{s:02}.{ms:03} 10 10 I Tag: Jan normal line\n");
            jan_data.extend_from_slice(line.as_bytes());
        }
    }

    let f_dec = create_temp_log_file(&dec_data);
    let f_jan = create_temp_log_file(&jan_data);

    let merged =
        MergedLogBuffer::open_multiple(&[f_dec.path().to_path_buf(), f_jan.path().to_path_buf()])
            .expect("merge failed");

    let count = merged.line_count();
    let visible_indices: Vec<usize> = (0..count).collect();
    let styles = vec![ResolvedStyle::default(); count];

    // Compute timeline with maximum supported buckets (128)
    let timeline = EpsTimelineData::compute(&merged, &visible_indices, &styles, 128);

    assert!(timeline.has_timestamps);
    assert_eq!(timeline.buckets.len(), 128);

    // Duration is 60s (Dec) + 60s (Jan) = ~120s
    println!(
        "[EMPIRICAL TIMELINE] Total duration: {:.2}s, buckets: {}, peak EPS: {:.1}",
        timeline.total_duration_secs,
        timeline.buckets.len(),
        timeline.peak_eps
    );
    assert!(
        (timeline.total_duration_secs - 120.0).abs() < 2.0,
        "Timeline duration must be ~120s across rollover, got {}",
        timeline.total_duration_secs
    );

    // Ensure all buckets have valid non-negative spans
    for (i, b) in timeline.buckets.iter().enumerate() {
        assert!(
            b.end_micros >= b.start_micros,
            "Bucket {i} has negative span: start={}, end={}",
            b.start_micros,
            b.end_micros
        );
    }

    // Verify sum of buckets equals total visible lines
    let sum_events: u32 = timeline.buckets.iter().map(|b| b.total_count).sum();
    assert_eq!(
        sum_events as usize, count,
        "Bucket sum {sum_events} must match total line count {count}"
    );

    // Verify anomaly is detected in the Jan 01 burst bucket
    let anomaly_buckets: Vec<_> = timeline
        .buckets
        .iter()
        .enumerate()
        .filter(|(_, b)| b.is_anomaly)
        .collect();

    assert!(
        !anomaly_buckets.is_empty(),
        "Anomaly detection MUST flag the 2,000-error crash burst"
    );

    // The anomaly must be in the latter half of the buckets (Jan 01 is > bucket 64)
    let (anomaly_idx, anomaly_b) = anomaly_buckets[0];
    assert!(
        anomaly_idx >= 60,
        "Burst occurred at 00:00:05 (approx bucket 69), found at bucket {anomaly_idx}"
    );
    assert!(
        anomaly_b.error_count >= 1000,
        "Anomaly bucket must contain the errors from the crash burst"
    );
}

// 4. Sub-Millisecond Midnight Rollover Boundary Precision
// Verifies that exactly at the midnight boundary, the microsecond delta
// between 23:59:59.999 and 00:00:00.000 is precisely 1,000 µs (1 millisecond).
#[test]
fn test_adversarial_sub_millisecond_midnight_transition_precision() {
    let dec_line = b"12-31 23:59:59.999 1 1 I Tag: Last microsecond of year\n";
    let jan_line_0 = b"01-01 00:00:00.000 1 1 I Tag: First microsecond of year\n";
    let jan_line_1 = b"01-01 00:00:00.001 1 1 I Tag: Second millisecond\n";

    let f1 = create_temp_log_file(dec_line);
    let f2 = create_temp_log_file(jan_line_0);
    let f3 = create_temp_log_file(jan_line_1);

    let merged = MergedLogBuffer::open_multiple(&[
        f1.path().to_path_buf(),
        f2.path().to_path_buf(),
        f3.path().to_path_buf(),
    ])
    .expect("merge failed");

    assert_eq!(merged.line_count(), 3);

    let ts0 = merged.get_line_timestamp_micros(0).unwrap();
    let ts1 = merged.get_line_timestamp_micros(1).unwrap();
    let ts2 = merged.get_line_timestamp_micros(2).unwrap();

    // Delta between 23:59:59.999 and 00:00:00.000 must be EXACTLY 1,000 µs (1 ms)
    let delta01 = ts1 - ts0;
    println!("[EMPIRICAL DELTA] 23:59:59.999 -> 00:00:00.000 delta = {delta01} µs");
    assert_eq!(
        delta01, 1_000,
        "Delta across midnight must be exactly 1,000 microseconds (1 ms)"
    );

    // Delta between 00:00:00.000 and 00:00:00.001 must also be EXACTLY 1,000 µs
    let delta12 = ts2 - ts1;
    assert_eq!(delta12, 1_000);
}

// 5. Interleaved 8-Source Multi-File Rollover Stress Test
// Merges 8 concurrent log files with alternating Dec 31, Jan 01, and cross-over logs,
// verifying that the combined stream maintains 100% timestamp monotonicity.
#[test]
fn test_adversarial_interleaved_8_source_matrix() {
    let mut files = Vec::new();

    // Create 8 files with varied ranges
    for s_id in 0..8 {
        let mut data = Vec::new();
        match s_id % 4 {
            0 => {
                // Dec 31 only
                for sec in 40..60 {
                    let line = format!(
                        "12-31 23:59:{sec:02}.{:03} {s_id} {s_id} I S{s_id}: Dec\n",
                        s_id * 100
                    );
                    data.extend_from_slice(line.as_bytes());
                }
            }
            1 => {
                // Dec 31 -> Jan 01 (Intra-file rollover)
                for sec in 55..60 {
                    let line = format!(
                        "12-31 23:59:{sec:02}.{:03} {s_id} {s_id} I S{s_id}: Dec-Jan\n",
                        s_id * 100
                    );
                    data.extend_from_slice(line.as_bytes());
                }
                for sec in 0..10 {
                    let line = format!(
                        "01-01 00:00:{sec:02}.{:03} {s_id} {s_id} I S{s_id}: Dec-Jan\n",
                        s_id * 100
                    );
                    data.extend_from_slice(line.as_bytes());
                }
            }
            2 => {
                // Jan 01 only
                for sec in 2..20 {
                    let line = format!(
                        "01-01 00:00:{sec:02}.{:03} {s_id} {s_id} I S{s_id}: Jan\n",
                        s_id * 100
                    );
                    data.extend_from_slice(line.as_bytes());
                }
            }
            _ => {
                // Dec 31 only
                for sec in 50..60 {
                    let line = format!(
                        "12-31 23:59:{sec:02}.{:03} {s_id} {s_id} I S{s_id}: Dec Late\n",
                        s_id * 100
                    );
                    data.extend_from_slice(line.as_bytes());
                }
            }
        }
        files.push(create_temp_log_file(&data));
    }

    let paths: Vec<PathBuf> = files.iter().map(|f| f.path().to_path_buf()).collect();
    let merged = MergedLogBuffer::open_multiple(&paths).expect("8-source merge failed");

    assert!(merged.line_count() > 100);

    // Verify strict monotonicity across the entire 8-source merged buffer
    let mut prev_ts = -1i64;
    for i in 0..merged.line_count() {
        let ts = merged.get_line_timestamp_micros(i).unwrap();
        if prev_ts >= 0 {
            assert!(
                ts >= prev_ts,
                "Monotonicity violation in 8-source merge at line {i}: ts {ts} < prev {prev_ts}"
            );
        }
        prev_ts = ts;
    }
}

// 6. Leading Un-timestamped Lines Across Cross-File Rollover
// Verifies that when a Jan 01 file begins with header lines lacking timestamps,
// it still correctly detects the cross-file year rollover once timestamps begin.
#[test]
fn test_adversarial_leading_untimestamped_lines_cross_file_rollover() {
    let f1_dec = b"12-31 23:59:58.000 1 1 I Dec line 1\n\
                  12-31 23:59:59.000 1 1 I Dec line 2\n";

    // File 2 starts with un-timestamped headers/comments
    let f2_jan = b"# Android Logcat Dump Version 2.0\n\
                  # Device: Pixel 8 Pro\n\
                  # Session started: 2024-01-01 00:00:00\n\
                  01-01 00:00:01.000 2 2 I Post-header Jan line 1\n\
                  01-01 00:00:02.000 2 2 I Post-header Jan line 2\n";

    let f1 = create_temp_log_file(f1_dec);
    let f2 = create_temp_log_file(f2_jan);

    let merged =
        MergedLogBuffer::open_multiple(&[f1.path().to_path_buf(), f2.path().to_path_buf()])
            .expect("merge failed");

    assert_eq!(merged.line_count(), 7);

    // Lines 0..1: Dec 31
    let ts0 = merged.get_line_timestamp_micros(0).unwrap();
    let ts1 = merged.get_line_timestamp_micros(1).unwrap();
    assert!(ts0 < ts1);

    // Lines 2..3: Jan 01 timestamped lines (must come AFTER Dec 31 and before un-timestamped headers)
    let ts2 = merged.get_line_timestamp_micros(2).unwrap();
    let ts3 = merged.get_line_timestamp_micros(3).unwrap();
    assert!(
        ts1 < ts2,
        "Dec 31 line must precede Jan 01 line despite leading headers"
    );
    assert!(ts2 < ts3);
    assert!(ts2 >= ONE_YEAR_MICROS);

    // Lines 4..6: Un-timestamped header comments (sorted to the end since timestamp is None)
    assert!(merged.get_line_timestamp_micros(4).is_none());
    assert!(merged.get_line_timestamp_micros(5).is_none());
    assert!(merged.get_line_timestamp_micros(6).is_none());
    assert!(merged.get_line(4).unwrap().contains("Version 2.0"));
    assert!(merged.get_line(5).unwrap().contains("Pixel 8 Pro"));
    assert!(merged.get_line(6).unwrap().contains("Session started"));
}

// 7. Multi-Year Consecutive Rollover (2 Year Boundaries within Single Stream)
// Verifies that if a log stream crosses TWO consecutive year boundaries
// (e.g. Year 0 Dec 31 -> Year 1 Jan 01 -> Year 1 Dec 31 -> Year 2 Jan 01),
// file_year_offset accumulates multiple ONE_YEAR_MICROS increments and remains monotonic.
#[test]
fn test_adversarial_double_year_rollover_intra_file() {
    let content = b"12-31 23:59:59.000 1 1 I Tag: End of Year 0\n\
                    01-01 00:00:01.000 1 1 I Tag: Start of Year 1\n\
                    12-31 23:59:59.000 1 1 I Tag: End of Year 1\n\
                    01-01 00:00:01.000 1 1 I Tag: Start of Year 2\n";

    let f = create_temp_log_file(content);
    let merged = MergedLogBuffer::open_multiple(&[f.path().to_path_buf()]).expect("merge failed");
    assert_eq!(merged.line_count(), 4);

    let ts0 = merged.get_line_timestamp_micros(0).unwrap();
    let ts1 = merged.get_line_timestamp_micros(1).unwrap();
    let ts2 = merged.get_line_timestamp_micros(2).unwrap();
    let ts3 = merged.get_line_timestamp_micros(3).unwrap();

    assert!(ts0 < ts1, "Year 0 Dec 31 must precede Year 1 Jan 01");
    assert!(ts1 < ts2, "Year 1 Jan 01 must precede Year 1 Dec 31");
    assert!(ts2 < ts3, "Year 1 Dec 31 must precede Year 2 Jan 01");

    // ts1 has 1 year offset
    assert!(ts1 >= ONE_YEAR_MICROS);
    // ts3 has 2 years offset
    assert!(ts3 >= 2 * ONE_YEAR_MICROS);

    // Delta between 23:59:59 and 00:00:01 across both rollovers is exactly 2 seconds
    assert_eq!(ts1 - ts0, 2_000_000);
    assert_eq!(ts3 - ts2, 2_000_000);
}

// 8. Extreme Scale Stress Test: 100,000 Lines Across 5 Sources
// Benchmarks merge, timestamp normalization, and sort throughput at 100k scale.
#[test]
fn test_adversarial_massive_100k_lines_stress() {
    let lines_per_file = 20_000usize;
    let mut files = Vec::new();

    for f_idx in 0..5 {
        let mut data = Vec::with_capacity(lines_per_file * 60);
        // Each file spans 23:50:00 to 00:10:00 (10,000 lines pre, 10,000 lines post)
        for i in 0..10_000 {
            let sec = (i * 600) / 10_000;
            let min = 50 + sec / 60;
            let s = sec % 60;
            let ms = (i * 7 + f_idx * 13) % 1000;
            let line = format!(
                "12-31 23:{min:02}:{s:02}.{ms:03} {f_idx} {f_idx} I Tag: S{f_idx} Pre {i}\n"
            );
            data.extend_from_slice(line.as_bytes());
        }
        for i in 0..10_000 {
            let sec = (i * 600) / 10_000;
            let min = sec / 60;
            let s = sec % 60;
            let ms = (i * 11 + f_idx * 17) % 1000;
            let line = format!(
                "01-01 00:{min:02}:{s:02}.{ms:03} {f_idx} {f_idx} I Tag: S{f_idx} Post {i}\n"
            );
            data.extend_from_slice(line.as_bytes());
        }
        files.push(create_temp_log_file(&data));
    }

    let paths: Vec<PathBuf> = files.iter().map(|f| f.path().to_path_buf()).collect();
    let t0 = Instant::now();
    let merged = MergedLogBuffer::open_multiple(&paths).expect("100k merge failed");
    let elapsed = t0.elapsed();

    println!(
        "[EMPIRICAL BENCHMARK 100K] Merging 100,000 lines across 5 sources took {:?}",
        elapsed
    );
    assert_eq!(merged.line_count(), 100_000);
    // Throughput requirement: 100k lines should process in under 1.0 second
    assert!(
        elapsed.as_secs_f64() < 1.0,
        "100k lines took too long: {:?}",
        elapsed
    );

    // Monotonicity verification
    let mut prev = -1i64;
    for i in 0..100_000 {
        let ts = merged.get_line_timestamp_micros(i).unwrap();
        assert!(ts >= prev, "Monotonicity violated at row {i}");
        prev = ts;
    }
}

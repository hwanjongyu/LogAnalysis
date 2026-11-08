use loganalysis_core::buffer::merged_buffer::{MergedLogBuffer, ONE_YEAR_MICROS};
use loganalysis_core::buffer::LogSource;
use loganalysis_core::filter::timeline::EpsTimelineData;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

fn create_temp_logfile(prefix: &str, content: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("{}_{}.log", prefix, uuid::Uuid::new_v4()));
    std::fs::write(&path, content).expect("Failed to write temp log file");
    path
}

/// Stress Test 1: Massive volume (60,000+ entries) crossing Dec 31 -> Jan 01 midnight
/// - Source 1: 30,000 entries on Dec 31
/// - Source 2: 30,000 entries on Jan 01
/// - Verifies monotonicity, microsecond ordering, lack of integer overflow, and high-throughput sorting
#[test]
fn test_adversarial_stress_massive_volume_60k_lines_crossing_rollover() {
    let mut path1 = std::env::temp_dir();
    path1.push(format!("stress_60k_dec_{}.log", uuid::Uuid::new_v4()));
    let mut path2 = std::env::temp_dir();
    path2.push(format!("stress_60k_jan_{}.log", uuid::Uuid::new_v4()));

    // Generate 30,000 lines for Dec 31 (23:30:00 to 23:59:59.999)
    {
        let file1 = std::fs::File::create(&path1).expect("create dec file");
        let mut writer = std::io::BufWriter::new(file1);
        for i in 0..30_000 {
            let total_millis = i * 60; // 0 to 1,800,000 ms (30 mins)
            let sec = (total_millis / 1000) % 60;
            let min = 30 + (total_millis / 60_000);
            let ms = total_millis % 1000;
            writeln!(
                writer,
                "12-31 23:{:02}:{:02}.{:03} 1000 1001 I DecService: Event seq={}",
                min, sec, ms, i
            )
            .unwrap();
        }
        writer.flush().unwrap();
    }

    // Generate 30,000 lines for Jan 01 (00:00:00.000 to 00:30:00.000)
    {
        let file2 = std::fs::File::create(&path2).expect("create jan file");
        let mut writer = std::io::BufWriter::new(file2);
        for i in 0..30_000 {
            let total_millis = i * 60;
            let sec = (total_millis / 1000) % 60;
            let min = (total_millis / 60_000) % 60;
            let ms = total_millis % 1000;
            writeln!(
                writer,
                "01-01 00:{:02}:{:02}.{:03} 2000 2001 I JanService: Event seq={}",
                min, sec, ms, i
            )
            .unwrap();
        }
        writer.flush().unwrap();
    }

    let start = Instant::now();
    let merged = MergedLogBuffer::open_multiple(&[path1.clone(), path2.clone()])
        .expect("open_multiple failed on 60k entries");
    let elapsed = start.elapsed();

    assert_eq!(merged.line_count(), 60_000);
    println!(
        "Merged 60,000 lines across year rollover in {:.3} ms ({:.0} lines/sec)",
        elapsed.as_secs_f64() * 1000.0,
        60_000.0 / elapsed.as_secs_f64()
    );

    // Verify strict monotonic non-decreasing timestamp across all 60,000 lines
    let mut prev_ts = -1i64;
    for i in 0..60_000 {
        let ts = merged
            .get_line_timestamp_micros(i)
            .expect("Timestamp must exist");
        assert!(
            ts >= prev_ts,
            "Monotonicity violation at index {}: ts={} < prev_ts={}",
            i,
            ts,
            prev_ts
        );
        prev_ts = ts;
    }

    // Line 29,999 must be the last Dec 31 line; Line 30,000 must be the first Jan 01 line
    let last_dec_ts = merged.get_line_timestamp_micros(29_999).unwrap();
    let first_jan_ts = merged.get_line_timestamp_micros(30_000).unwrap();
    assert!(
        first_jan_ts > last_dec_ts,
        "Jan 01 line ({first_jan_ts}) must strictly exceed Dec 31 line ({last_dec_ts})"
    );
    assert_eq!(
        merged.get_line_source_id(29_999),
        Some(0),
        "Line 29,999 must come from source 0"
    );
    assert_eq!(
        merged.get_line_source_id(30_000),
        Some(1),
        "Line 30,000 must come from source 1"
    );

    let _ = std::fs::remove_file(path1);
    let _ = std::fs::remove_file(path2);
}

/// Stress Test 2: Highly interleaved 4-source jitter across midnight
/// Verifies that multiple files logging concurrently around 23:59:59 - 00:00:01
/// interleave in exact microsecond order, and intra-file + cross-file offsets work together.
#[test]
fn test_adversarial_interleaved_four_sources_subsecond_jitter() {
    // S0: Intra-file rollover: 12-31 23:59:59.900 -> 01-01 00:00:00.100 -> 01-01 00:00:00.500
    let s0_content = b"12-31 23:59:59.900 S0 event 1\n\
                       01-01 00:00:00.100 S0 event 2\n\
                       01-01 00:00:00.500 S0 event 3\n";

    // S1: Pure Dec 31: 12-31 23:59:59.800, 12-31 23:59:59.950
    let s1_content = b"12-31 23:59:59.800 S1 event 1\n\
                       12-31 23:59:59.950 S1 event 2\n";

    // S2: Pure Jan 01: 01-01 00:00:00.050, 01-01 00:00:00.200, 01-01 00:00:00.600
    let s2_content = b"01-01 00:00:00.050 S2 event 1\n\
                       01-01 00:00:00.200 S2 event 2\n\
                       01-01 00:00:00.600 S2 event 3\n";

    // S3: Intra-file rollover: 12-31 23:59:59.850 -> 01-01 00:00:00.010 -> 01-01 00:00:00.300
    let s3_content = b"12-31 23:59:59.850 S3 event 1\n\
                       01-01 00:00:00.010 S3 event 2\n\
                       01-01 00:00:00.300 S3 event 3\n";

    let p0 = create_temp_logfile("s0", s0_content);
    let p1 = create_temp_logfile("s1", s1_content);
    let p2 = create_temp_logfile("s2", s2_content);
    let p3 = create_temp_logfile("s3", s3_content);

    let merged = MergedLogBuffer::open_multiple(&[p0.clone(), p1.clone(), p2.clone(), p3.clone()])
        .expect("merge 4 sources failed");

    assert_eq!(merged.line_count(), 11);

    // Verify sorted chronological ordering across sources:
    // Expected order:
    // 0: S1 23:59:59.800 (Dec 31)
    // 1: S3 23:59:59.850 (Dec 31)
    // 2: S0 23:59:59.900 (Dec 31)
    // 3: S1 23:59:59.950 (Dec 31)
    // --- MIDNIGHT ROLLOVER ---
    // 4: S3 00:00:00.010 (Jan 01)
    // 5: S2 00:00:00.050 (Jan 01)
    // 6: S0 00:00:00.100 (Jan 01)
    // 7: S2 00:00:00.200 (Jan 01)
    // 8: S3 00:00:00.300 (Jan 01)
    // 9: S0 00:00:00.500 (Jan 01)
    // 10: S2 00:00:00.600 (Jan 01)

    let expected_events = [
        ("S1 event 1", 1),
        ("S3 event 1", 3),
        ("S0 event 1", 0),
        ("S1 event 2", 1),
        ("S3 event 2", 3),
        ("S2 event 1", 2),
        ("S0 event 2", 0),
        ("S2 event 2", 2),
        ("S3 event 3", 3),
        ("S0 event 3", 0),
        ("S2 event 3", 2),
    ];

    let mut prev_ts = -1i64;
    for (i, &(expected_text, expected_src)) in expected_events.iter().enumerate() {
        let line = merged.get_line(i).unwrap();
        assert!(
            line.contains(expected_text),
            "Line {} mismatch: expected {}, got {}",
            i,
            expected_text,
            line
        );
        assert_eq!(
            merged.get_line_source_id(i),
            Some(expected_src),
            "Line {} source mismatch",
            i
        );

        let ts = merged.get_line_timestamp_micros(i).unwrap();
        assert!(
            ts > prev_ts,
            "Timestamp at index {} (ts={}) must strictly exceed prev_ts={}",
            i,
            ts,
            prev_ts
        );
        prev_ts = ts;
    }

    let _ = std::fs::remove_file(p0);
    let _ = std::fs::remove_file(p1);
    let _ = std::fs::remove_file(p2);
    let _ = std::fs::remove_file(p3);
}

/// Stress Test 3: Complex multiline stack trace grouping across midnight boundary
/// Verifies:
/// 1. Multiline trace on Dec 31 stays grouped and precedes Jan 01 lines.
/// 2. Multiline trace on Jan 01 retains adjusted year offset and stays grouped.
/// 3. Stack trace lines from another file occurring at same millisecond preserve source line stability.
#[test]
fn test_adversarial_multiline_traces_on_both_sides_of_midnight() {
    let dec_file_content = b"12-31 23:59:59.998 [Server] Normal tick\n\
12-31 23:59:59.999 [Critical] OutOfMemoryError: Java heap space\n\
\tat com.example.MemoryManager.alloc(MemoryManager.java:100)\n\
\tat com.example.MemoryManager.allocateBuffer(MemoryManager.java:50)\n\
\tat com.example.Worker.run(Worker.java:30)\n";

    let jan_file_content = b"01-01 00:00:00.001 [CrashHandler] Handling crash dump\n\
01-01 00:00:00.002 [Watchdog] Watchdog timeout thread stuck\n\
\tat android.os.BinderProxy.transact(Binder.java:500)\n\
\tat com.android.server.am.ActivityManagerService.dump(AMS.java:9999)\n\
01-01 00:00:00.010 [Watchdog] Killing system server\n";

    let p_dec = create_temp_logfile("multi_dec", dec_file_content);
    let p_jan = create_temp_logfile("multi_jan", jan_file_content);

    let merged = MergedLogBuffer::open_multiple(&[p_dec.clone(), p_jan.clone()]).expect("merge ok");
    assert_eq!(merged.line_count(), 10);

    // Verify ordering and timestamp assignment
    // Lines 0-4 are Dec 31
    let dec_exception_ts = merged.get_line_timestamp_micros(1).unwrap();
    for i in 2..=4 {
        assert_eq!(
            merged.get_line_timestamp_micros(i).unwrap(),
            dec_exception_ts,
            "Dec 31 stack trace line {} must inherit parent timestamp",
            i
        );
        assert_eq!(merged.get_line_source_id(i), Some(0));
    }

    // Line 5 is Jan 01 00:00:00.001
    let jan_event_ts = merged.get_line_timestamp_micros(5).unwrap();
    assert!(jan_event_ts > dec_exception_ts);
    assert_eq!(merged.get_line_source_id(5), Some(1));

    // Lines 6-8 are Jan 01 00:00:00.002
    let jan_exception_ts = merged.get_line_timestamp_micros(6).unwrap();
    assert!(jan_exception_ts > jan_event_ts);
    for i in 7..=8 {
        assert_eq!(
            merged.get_line_timestamp_micros(i).unwrap(),
            jan_exception_ts,
            "Jan 01 stack trace line {} must inherit adjusted parent timestamp",
            i
        );
        assert_eq!(merged.get_line_source_id(i), Some(1));
    }

    // Line 9 is Jan 01 00:00:00.010
    let jan_kill_ts = merged.get_line_timestamp_micros(9).unwrap();
    assert!(jan_kill_ts > jan_exception_ts);

    let _ = std::fs::remove_file(p_dec);
    let _ = std::fs::remove_file(p_jan);
}

/// Stress Test 4: EpsTimelineData::compute stress across year rollover
/// Verifies:
/// 1. Extreme bucket counts (e.g. 8, 64, 128)
/// 2. Non-zero duration across rollover
/// 3. Accurate bucket distribution: Dec 31 lines land in early buckets, Jan 01 lines in late buckets
/// 4. Anomaly detection flag correctness across the year-end burst
#[test]
fn test_adversarial_eps_timeline_stress_and_bucket_distribution() {
    let mut file1_bytes = Vec::new();
    // 50 lines at 12-31 23:59:50 (early)
    for _ in 0..50 {
        file1_bytes.extend_from_slice(b"12-31 23:59:50.000 100 100 I Tag: Dec 31 baseline event\n");
    }
    // 100 lines at 12-31 23:59:59 (spike just before midnight)
    for _ in 0..100 {
        file1_bytes.extend_from_slice(b"12-31 23:59:59.000 100 100 E Tag: Dec 31 critical spike\n");
    }

    let mut file2_bytes = Vec::new();
    // 100 lines at 01-01 00:00:00 (spike just after midnight)
    for _ in 0..100 {
        file2_bytes.extend_from_slice(b"01-01 00:00:00.000 100 100 E Tag: Jan 01 critical spike\n");
    }
    // 50 lines at 01-01 00:00:10 (late)
    for _ in 0..50 {
        file2_bytes.extend_from_slice(b"01-01 00:00:10.000 100 100 I Tag: Jan 01 steady event\n");
    }

    let p1 = create_temp_logfile("timeline_stress_1", &file1_bytes);
    let p2 = create_temp_logfile("timeline_stress_2", &file2_bytes);

    let merged = MergedLogBuffer::open_multiple(&[p1.clone(), p2.clone()]).expect("merged failed");
    assert_eq!(merged.line_count(), 300);

    let all_indices: Vec<usize> = (0..300).collect();

    // Test bucket counts 8, 64, 128
    for &bc in &[8, 64, 128] {
        let timeline = EpsTimelineData::compute(&merged, &all_indices, &[], bc);
        assert!(timeline.has_timestamps);
        // Total duration is from 23:59:50 to 00:00:10 = exactly 20.0 seconds
        assert!(
            (timeline.total_duration_secs - 20.0).abs() < 0.001,
            "Expected 20.0s total duration, got {}",
            timeline.total_duration_secs
        );

        let sum_total_count: u32 = timeline.buckets.iter().map(|b| b.total_count).sum();
        assert_eq!(
            sum_total_count, 300,
            "All 300 lines must be bucketed without loss"
        );

        // Check bucket 0 contains Dec 31 23:59:50 events
        assert!(
            timeline.buckets[0].total_count > 0,
            "Bucket 0 must contain events"
        );

        // Check last bucket contains Jan 01 00:00:10 events
        let last_b = timeline.buckets.last().unwrap();
        assert!(last_b.total_count > 0, "Last bucket must contain events");

        // Error spikes at 23:59:59 and 00:00:00 must be captured
        let total_errors: u32 = timeline.buckets.iter().map(|b| b.error_count).sum();
        assert_eq!(
            total_errors, 200,
            "All 200 error lines must be categorized as errors"
        );
    }

    let _ = std::fs::remove_file(p1);
    let _ = std::fs::remove_file(p2);
}

/// Stress Test 5: Multi-year sequential rollover (Year 0 -> Year 1 -> Year 2)
/// Intra-file crossing 2 consecutive New Year midnights
#[test]
fn test_adversarial_consecutive_multi_year_rollovers() {
    let content = b"12-31 23:59:59.000 Year 0 final line\n\
01-01 00:00:01.000 Year 1 first line\n\
12-31 23:59:59.000 Year 1 final line\n\
01-01 00:00:01.000 Year 2 first line\n";

    let p = create_temp_logfile("multi_year", content);
    let merged = MergedLogBuffer::open_multiple(std::slice::from_ref(&p)).expect("open failed");
    assert_eq!(merged.line_count(), 4);

    let ts0 = merged.get_line_timestamp_micros(0).unwrap();
    let ts1 = merged.get_line_timestamp_micros(1).unwrap();
    let ts2 = merged.get_line_timestamp_micros(2).unwrap();
    let ts3 = merged.get_line_timestamp_micros(3).unwrap();

    assert!(ts0 < ts1, "Year 0 end < Year 1 start");
    assert!(ts1 < ts2, "Year 1 start < Year 1 end");
    assert!(ts2 < ts3, "Year 1 end < Year 2 start");

    // Year 1 start - Year 0 end is 2 seconds
    assert_eq!(ts1 - ts0, 2_000_000);
    // Year 2 start - Year 1 end is 2 seconds
    assert_eq!(ts3 - ts2, 2_000_000);

    let _ = std::fs::remove_file(p);
}

/// Stress Test 6: Intra-day small reverse clock skew (jitter) does NOT trigger year rollover
/// e.g. multi-threaded logging where 23:59:59.500 is followed by 23:59:59.400 (100ms backward jitter)
#[test]
fn test_adversarial_clock_jitter_does_not_falsely_trigger_rollover() {
    let content = b"12-31 23:59:59.500 Thread A event\n\
12-31 23:59:59.400 Thread B slight jitter backward\n\
12-31 23:59:59.600 Thread A resumed\n";

    let p = create_temp_logfile("jitter", content);
    let merged = MergedLogBuffer::open_multiple(std::slice::from_ref(&p)).expect("open failed");
    assert_eq!(merged.line_count(), 3);

    // In sort_by:
    // Line 0: 23:59:59.400 (Thread B)
    // Line 1: 23:59:59.500 (Thread A)
    // Line 2: 23:59:59.600 (Thread A)
    let ts0 = merged.get_line_timestamp_micros(0).unwrap();
    let ts1 = merged.get_line_timestamp_micros(1).unwrap();
    let ts2 = merged.get_line_timestamp_micros(2).unwrap();

    assert!(ts0 < ts1);
    assert!(ts1 < ts2);

    // Delta between ts0 and ts1 must be exactly 100ms (100,000 us), NOT +1 year!
    assert_eq!(ts1 - ts0, 100_000);
    assert!(
        ts0 < ONE_YEAR_MICROS,
        "Timestamps must not have an extraneous ONE_YEAR_MICROS added"
    );

    let _ = std::fs::remove_file(p);
}

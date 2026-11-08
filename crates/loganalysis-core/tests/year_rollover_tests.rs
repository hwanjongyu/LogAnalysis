use loganalysis_core::buffer::merged_buffer::{MergedLogBuffer, ONE_YEAR_MICROS};
use loganalysis_core::buffer::LogSource;
use loganalysis_core::filter::engine::ResolvedStyle;
use loganalysis_core::filter::timeline::EpsTimelineData;
use std::path::PathBuf;

fn create_temp_logfile(prefix: &str, content: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("{}_{}.log", prefix, uuid::Uuid::new_v4()));
    std::fs::write(&path, content).expect("Failed to write temp log file");
    path
}

#[test]
fn test_integration_intra_file_year_rollover_sub_second() {
    let content = b"12-31 23:59:59.999 System shutting down services\n\
                    01-01 00:00:00.001 System boot completed\n";
    let path = create_temp_logfile("intra_rollover", content);

    let merged =
        MergedLogBuffer::open_multiple(std::slice::from_ref(&path)).expect("open_multiple failed");
    assert_eq!(merged.line_count(), 2);

    let line0 = merged.get_line(0).expect("line 0 missing");
    let line1 = merged.get_line(1).expect("line 1 missing");
    assert!(line0.contains("shutting down"));
    assert!(line1.contains("boot completed"));

    let ts0 = merged.get_line_timestamp_micros(0).expect("ts0 missing");
    let ts1 = merged.get_line_timestamp_micros(1).expect("ts1 missing");

    // 2 milliseconds elapsed across midnight
    assert_eq!(ts1 - ts0, 2_000);
    assert!(ts1 > ts0, "Jan 01 must be strictly greater than Dec 31");

    let _ = std::fs::remove_file(path);
}

#[test]
fn test_integration_cross_file_merge_dec31_and_jan01() {
    let file1_dec = b"12-31 23:59:55.000 S1 Service A stopping\n\
                     12-31 23:59:59.000 S1 Service A stopped\n";

    let file2_jan = b"01-01 00:00:01.000 S2 Service B starting\n\
                     01-01 00:00:05.000 S2 Service B started\n";

    let path1 = create_temp_logfile("cross_dec", file1_dec);
    let path2 = create_temp_logfile("cross_jan", file2_jan);

    let merged =
        MergedLogBuffer::open_multiple(&[path1.clone(), path2.clone()]).expect("merge failed");
    assert_eq!(merged.line_count(), 4);

    // Assert strictly chronological ordering
    assert_eq!(merged.get_line_source_id(0), Some(0));
    assert!(merged.get_line(0).unwrap().contains("Service A stopping"));

    assert_eq!(merged.get_line_source_id(1), Some(0));
    assert!(merged.get_line(1).unwrap().contains("Service A stopped"));

    assert_eq!(merged.get_line_source_id(2), Some(1));
    assert!(merged.get_line(2).unwrap().contains("Service B starting"));

    assert_eq!(merged.get_line_source_id(3), Some(1));
    assert!(merged.get_line(3).unwrap().contains("Service B started"));

    let ts0 = merged.get_line_timestamp_micros(0).unwrap();
    let ts1 = merged.get_line_timestamp_micros(1).unwrap();
    let ts2 = merged.get_line_timestamp_micros(2).unwrap();
    let ts3 = merged.get_line_timestamp_micros(3).unwrap();

    assert!(ts0 < ts1);
    assert!(ts1 < ts2, "Dec 31 23:59:59 must be < Jan 01 00:00:01");
    assert!(ts2 < ts3);
    assert!(
        ts2 >= ONE_YEAR_MICROS,
        "Jan 01 timestamp must have year offset added"
    );

    // Delta between 23:59:59 and 00:00:01 is exactly 2 seconds
    assert_eq!(ts2 - ts1, 2_000_000);

    let _ = std::fs::remove_file(path1);
    let _ = std::fs::remove_file(path2);
}

#[test]
fn test_integration_multiline_traces_on_jan01_inherit_adjusted_year() {
    let file1_dec = b"12-31 23:59:59.000 [Watchdog] Heartbeat ok\n";

    let file2_jan = b"01-01 00:00:02.000 [AndroidRuntime] FATAL EXCEPTION: main\n\
                     \tProcess: com.example.app, PID: 12345\n\
                     \tjava.lang.NullPointerException: Attempt to invoke virtual method on null reference\n\
                     \tat com.example.app.MainActivity.onCreate(MainActivity.java:42)\n\
                     \tat android.app.Activity.performCreate(Activity.java:8000)\n\
                     01-01 00:00:03.000 [ActivityManager] Process com.example.app has died\n";

    let path1 = create_temp_logfile("multi_dec", file1_dec);
    let path2 = create_temp_logfile("multi_jan", file2_jan);

    let merged =
        MergedLogBuffer::open_multiple(&[path1.clone(), path2.clone()]).expect("merge failed");
    assert_eq!(merged.line_count(), 7);

    // Expected lines in chronological order:
    // 0: Watchdog Heartbeat ok (Dec 31)
    // 1: FATAL EXCEPTION: main (Jan 01 00:00:02)
    // 2: Process: com.example.app (Jan 01 00:00:02 inherited)
    // 3: NullPointerException (Jan 01 00:00:02 inherited)
    // 4: at MainActivity.onCreate (Jan 01 00:00:02 inherited)
    // 5: at Activity.performCreate (Jan 01 00:00:02 inherited)
    // 6: Process died (Jan 01 00:00:03)

    assert!(merged.get_line(0).unwrap().contains("Heartbeat ok"));
    assert!(merged.get_line(1).unwrap().contains("FATAL EXCEPTION"));
    assert!(merged.get_line(2).unwrap().contains("PID: 12345"));
    assert!(merged.get_line(3).unwrap().contains("NullPointerException"));
    assert!(merged
        .get_line(4)
        .unwrap()
        .contains("MainActivity.onCreate"));
    assert!(merged
        .get_line(5)
        .unwrap()
        .contains("Activity.performCreate"));
    assert!(merged.get_line(6).unwrap().contains("has died"));

    let ts0 = merged.get_line_timestamp_micros(0).unwrap();
    let ts1 = merged.get_line_timestamp_micros(1).unwrap();
    let ts2 = merged.get_line_timestamp_micros(2).unwrap();
    let ts3 = merged.get_line_timestamp_micros(3).unwrap();
    let ts4 = merged.get_line_timestamp_micros(4).unwrap();
    let ts5 = merged.get_line_timestamp_micros(5).unwrap();
    let ts6 = merged.get_line_timestamp_micros(6).unwrap();

    assert!(ts0 < ts1);
    // Stack trace lines must all inherit exact timestamp of the exception head
    assert_eq!(ts1, ts2);
    assert_eq!(ts2, ts3);
    assert_eq!(ts3, ts4);
    assert_eq!(ts4, ts5);
    assert!(ts5 < ts6);

    let _ = std::fs::remove_file(path1);
    let _ = std::fs::remove_file(path2);
}

#[test]
fn test_integration_eps_timeline_compute_with_merged_buffer_across_rollover() {
    let file1_dec = b"12-31 23:59:50.000 100 100 I SystemServer: Pre-midnight step 1\n\
                     12-31 23:59:55.000 100 100 W SystemServer: Pre-midnight warning\n";

    let file2_jan = b"01-01 00:00:05.000 100 100 E SystemServer: Post-midnight error\n\
                     01-01 00:00:10.000 100 100 I SystemServer: Post-midnight steady state\n";

    let path1 = create_temp_logfile("timeline_dec", file1_dec);
    let path2 = create_temp_logfile("timeline_jan", file2_jan);

    let merged =
        MergedLogBuffer::open_multiple(&[path1.clone(), path2.clone()]).expect("merge failed");
    let count = merged.line_count();
    assert_eq!(count, 4);

    let visible_indices: Vec<usize> = (0..count).collect();
    let styles = vec![ResolvedStyle::default(); count];

    let timeline = EpsTimelineData::compute(&merged, &visible_indices, &styles, 10);

    assert!(timeline.has_timestamps);
    // Total duration: 23:59:50 to 00:00:10 = 20.0 seconds
    assert!(
        (timeline.total_duration_secs - 20.0).abs() < 0.1,
        "Timeline duration across rollover must be ~20s, got {}",
        timeline.total_duration_secs
    );
    assert_eq!(timeline.buckets.len(), 10);

    // Sum of counts must match total lines
    let total_events: u32 = timeline.buckets.iter().map(|b| b.total_count).sum();
    assert_eq!(total_events, 4);

    // Early buckets contain Dec 31 warning
    let has_warn = timeline.buckets[..5].iter().any(|b| b.warn_count > 0);
    assert!(has_warn, "Dec 31 warning must be in early buckets");

    // Later buckets contain Jan 01 error
    let has_err = timeline.buckets[5..].iter().any(|b| b.error_count > 0);
    assert!(has_err, "Jan 01 error must be in latter buckets");

    // Average EPS must be positive (~0.2 EPS)
    assert!(timeline.avg_eps > 0.0);
    assert!((timeline.avg_eps - 0.2).abs() < 0.05);

    let _ = std::fs::remove_file(path1);
    let _ = std::fs::remove_file(path2);
}

#[test]
fn test_integration_eps_timeline_anomaly_across_midnight() {
    let mut file1_data = Vec::new();
    // 30 normal lines from 23:59:30 to 23:59:59 (1 line/sec)
    for s in 30..60 {
        let line = format!("12-31 23:59:{:02}.000 1 1 I Tag: Normal log line {s}\n", s);
        file1_data.extend_from_slice(line.as_bytes());
    }

    let mut file2_data = Vec::new();
    // Huge burst of 100 errors at 00:00:05.500
    for _ in 0..100 {
        file2_data.extend_from_slice(b"01-01 00:00:05.500 1 1 E Tag: Critical rollover crash!\n");
    }
    // 30 normal lines from 00:00:06 to 00:00:35 (1 line/sec)
    for s in 6..36 {
        let line = format!(
            "01-01 00:00:{:02}.000 1 1 I Tag: Post-burst log line {s}\n",
            s
        );
        file2_data.extend_from_slice(line.as_bytes());
    }

    let path1 = create_temp_logfile("anomaly_dec", &file1_data);
    let path2 = create_temp_logfile("anomaly_jan", &file2_data);

    let merged =
        MergedLogBuffer::open_multiple(&[path1.clone(), path2.clone()]).expect("merge failed");
    let count = merged.line_count();
    let visible_indices: Vec<usize> = (0..count).collect();
    let styles = vec![ResolvedStyle::default(); count];

    let timeline = EpsTimelineData::compute(&merged, &visible_indices, &styles, 20);

    assert!(timeline.has_timestamps);
    assert!(timeline.total_duration_secs >= 60.0);
    assert!(timeline.peak_count >= 100);

    // Verify anomaly flag is properly triggered on the burst bucket across the rollover
    let anomaly_detected = timeline.buckets.iter().any(|b| b.is_anomaly);
    assert!(
        anomaly_detected,
        "Burst at 00:00:05 must trigger anomaly flag across rollover"
    );

    let _ = std::fs::remove_file(path1);
    let _ = std::fs::remove_file(path2);
}

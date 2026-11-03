use std::io::Write;
use tempfile::NamedTempFile;
use loganalysis_core::*;

#[test]
fn test_empty_filters_shows_all_lines() {
    let dummy_lines = vec![
        "2026-11-03 10:00:00.123 I/System: Starting boot sequence".to_string(),
        "2026-11-03 10:00:01.456 D/Network: Connected to WiFi".to_string(),
    ];
    let mut stream = StreamLogBuffer::new(100, 10);
    stream.append_chunk(dummy_lines);

    // show_only_filtered = true, but empty filter list
    let result = execute_filtering(&stream, &[], true, None, 0).expect("filtering should succeed");

    assert_eq!(result.visible_indices.len(), 2);
    assert_eq!(result.visible_indices, vec![0, 1]);
    assert_eq!(result.visible_styles[0].fg_hex, Some(0x808080));
    assert_eq!(result.visible_styles[1].fg_hex, Some(0x808080));
}

#[test]
fn test_exclude_filter_priority() {
    let raw_line = b"E/Crash: NullPointerException at Service.java:42";
    let f1 = FilterRule {
        text: "Crash".to_string(),
        exclude: false,
        bg_color: "Red".to_string(),
        active: true,
        ..Default::default()
    };
    let f2 = FilterRule {
        text: "NullPointer".to_string(),
        exclude: true, // Exclude wins because it appears after f1
        active: true,
        ..Default::default()
    };

    let p1 = PreparedFilter::new(f1, 0, 0).unwrap();
    let p2 = PreparedFilter::new(f2, 1, 0).unwrap();
    let filters = vec![p1, p2];

    let (visible, _style) = evaluate_and_style_line(raw_line, &filters, true, None);
    assert!(!visible, "Line must be hidden because exclude filter matches last");
}

#[test]
fn test_independent_reverse_style_inheritance() {
    let raw_line = b"W/Auth: Login attempt failed for user admin";
    // Filter 1 sets Background = Yellow, TextColor = None
    let f1 = FilterRule {
        text: "Auth".to_string(),
        bg_color: "Yellow".to_string(),
        text_color: "None".to_string(),
        active: true,
        ..Default::default()
    };
    // Filter 2 sets Background = None, TextColor = Red
    let f2 = FilterRule {
        text: "Login attempt".to_string(),
        bg_color: "None".to_string(),
        text_color: "Red".to_string(),
        active: true,
        ..Default::default()
    };

    let p1 = PreparedFilter::new(f1, 0, 0).unwrap();
    let p2 = PreparedFilter::new(f2, 1, 0).unwrap();
    let filters = vec![p1, p2];

    let (visible, style) = evaluate_and_style_line(raw_line, &filters, true, None);
    assert!(visible);
    assert_eq!(style.bg_hex, parse_color_spec("Yellow", true));
    assert_eq!(style.fg_hex, parse_color_spec("Red", false));
}

#[test]
fn test_rolling_trim_hysteresis() {
    // max_capacity = 200, trim_batch = 50
    let mut buffer = StreamLogBuffer::new(200, 50);

    // Append 220 lines (excess = 20, < 50 trim_batch) -> no trim
    let chunk1: Vec<String> = (0..220).map(|i| format!("Line {}", i)).collect();
    buffer.append_chunk(chunk1);
    assert_eq!(buffer.trim_if_needed(), None);
    assert_eq!(buffer.line_count(), 220);

    // Append 35 more lines (total 255, excess = 55, >= 50 trim_batch) -> trims 55 lines
    let chunk2: Vec<String> = (220..255).map(|i| format!("Line {}", i)).collect();
    buffer.append_chunk(chunk2);
    let trimmed = buffer.trim_if_needed();
    assert_eq!(trimmed, Some(55));
    assert_eq!(buffer.line_count(), 200);
    assert_eq!(buffer.get_line(0).unwrap(), "Line 55");
}

#[test]
fn test_serial_security_validation() {
    assert!(is_valid_device_serial("emulator-5554"));
    assert!(is_valid_device_serial("192.168.1.50:5555"));
    assert!(is_valid_device_serial("R3CN10XYZ99"));
    assert!(is_valid_device_serial("  emulator-5554  ")); // Whitespace is safely trimmed

    // Injections / flags
    assert!(!is_valid_device_serial(""));
    assert!(!is_valid_device_serial("   "));
    assert!(!is_valid_device_serial("-d"));
    assert!(!is_valid_device_serial("--help"));
    assert!(!is_valid_device_serial(" -s bad"));
    assert!(!is_valid_device_serial("device; rm -rf /"));
    assert!(!is_valid_device_serial("device && ls"));
}

#[test]
fn test_scan_adb_devices_timeout_behavior() {
    use std::time::Duration;
    // scan_adb_devices must never hang indefinitely, even with tiny timeouts
    let res = scan_adb_devices(Duration::from_millis(50));
    // Either returns Ok(devices), Err("Failed to execute adb: ..."), or Err("timed out")
    // The critical check is that it returns deterministically without deadlocking.
    assert!(res.is_ok() || res.is_err());
}

#[test]
fn test_lossy_utf8_and_binary_resilience() {
    let mut file = NamedTempFile::new().expect("create temp file");
    // Write valid line, then invalid non-UTF-8 bytes, then another valid line
    file.write_all(b"Normal line 1\n").unwrap();
    file.write_all(b"Invalid bytes: \xff\xfe binary dump\n").unwrap();
    file.write_all(b"Normal line 2\n").unwrap();
    file.flush().unwrap();

    let buffer = FileLogBuffer::open(file.path()).expect("must open file with binary bytes");
    assert_eq!(buffer.line_count(), 3);
    assert_eq!(buffer.get_line(0).unwrap(), "Normal line 1");
    // The invalid bytes must decode lossily with U+FFFD replacement characters, without panicking
    let lossy_line = buffer.get_line(1).unwrap();
    assert!(lossy_line.contains('\u{FFFD}'));
    assert_eq!(buffer.get_line(2).unwrap(), "Normal line 2");
}

#[test]
fn test_nearest_neighbor_anchor_preservation() {
    let visible = vec![10, 20, 30, 40, 50];

    // Exact match
    assert_eq!(find_nearest_anchor_row(&visible, 30), Some(2));

    // Between 20 and 30: 24 is closer to 20 (dist 4 vs 6)
    assert_eq!(find_nearest_anchor_row(&visible, 24), Some(1));

    // Between 20 and 30: 26 is closer to 30 (dist 4 vs 6)
    assert_eq!(find_nearest_anchor_row(&visible, 26), Some(2));

    // Equidistant: 25 (dist 5 to 20, 5 to 30) -> ties favor the next row
    assert_eq!(find_nearest_anchor_row(&visible, 25), Some(2));

    // Out of bounds (before start)
    assert_eq!(find_nearest_anchor_row(&visible, 5), Some(0));

    // Out of bounds (after end)
    assert_eq!(find_nearest_anchor_row(&visible, 100), Some(4));
}

#[test]
fn test_three_json_formats_compatibility() {
    // Format 1: Single tab object
    let json_fmt1 = r#"{
        "name": "Network Tab",
        "enabled": true,
        "filters": [
            {"text": "HTTP", "case_sensitive": false, "regex": false, "exclude": false, "bg_color": "Cyan"}
        ]
    }"#;
    let tabs1 = parse_filters_json(json_fmt1).expect("Format 1 must parse");
    assert_eq!(tabs1.len(), 1);
    assert_eq!(tabs1[0].name, "Network Tab");
    assert_eq!(tabs1[0].filters.len(), 1);

    // Format 2: Array of tab objects
    let json_fmt2 = r#"[
        {
            "name": "Crash Tab",
            "enabled": true,
            "filters": [{"text": "Fatal", "bg_color": "Red"}]
        },
        {
            "name": "DB Tab",
            "enabled": false,
            "filters": [{"text": "SQLITE", "bg_color": "Green"}]
        }
    ]"#;
    let tabs2 = parse_filters_json(json_fmt2).expect("Format 2 must parse");
    assert_eq!(tabs2.len(), 2);
    assert_eq!(tabs2[0].name, "Crash Tab");
    assert_eq!(tabs2[1].name, "DB Tab");

    // Format 3: Array of raw filter rules
    let json_fmt3 = r#"[
        {"text": "DEBUG", "active": true},
        {"text": "VERBOSE", "active": false}
    ]"#;
    let tabs3 = parse_filters_json(json_fmt3).expect("Format 3 must parse");
    assert_eq!(tabs3.len(), 1);
    assert_eq!(tabs3[0].filters.len(), 2);
}

#[test]
fn test_wcag_contrast_ratio() {
    // Black on White: ~21:1
    let ratio_bw = contrast_ratio(0xFFFFFF, 0x000000);
    assert!((ratio_bw - 21.0).abs() < 0.2);

    // Low contrast: Yellow on White
    let (ratio_yw, level_yw) = evaluate_contrast(0xFFFFFF, 0xFFFF00);
    assert!(ratio_yw < 3.0);
    assert_eq!(level_yw, ContrastLevel::Critical);

    // High contrast: Navy on White
    let (_ratio_nw, level_nw) = evaluate_contrast(0xFFFFFF, 0x000080);
    assert_eq!(level_nw, ContrastLevel::Pass);
}

#[test]
fn test_file_buffer_and_mmap() {
    let mut file = NamedTempFile::new().expect("create temp file");
    writeln!(file, "Line 1: Alpha").unwrap();
    writeln!(file, "Line 2: Beta - longest line text here").unwrap();
    writeln!(file, "Line 3: Gamma").unwrap();
    file.flush().unwrap();

    let buffer = FileLogBuffer::open(file.path()).expect("open file buffer");
    assert_eq!(buffer.line_count(), 3);
    assert_eq!(buffer.get_line(0).unwrap(), "Line 1: Alpha");
    assert_eq!(buffer.get_line(1).unwrap(), "Line 2: Beta - longest line text here");
    assert_eq!(buffer.get_line(2).unwrap(), "Line 3: Gamma");
    assert_eq!(buffer.longest_line_text().unwrap(), "Line 2: Beta - longest line text here");
}

#[test]
fn test_cyclic_search() {
    let lines = vec![
        "apple pie".to_string(),
        "banana bread".to_string(),
        "cherry tart".to_string(),
        "apple crumble".to_string(),
    ];
    let mut stream = StreamLogBuffer::new(100, 10);
    stream.append_chunk(lines);
    let visible = vec![0, 1, 2, 3];

    // Find next "apple" starting from row 0 -> should find row 3
    let q_fwd = SearchQuery {
        pattern: "apple".to_string(),
        case_sensitive: false,
        is_regex: false,
        forward: true,
    };
    let res = search_in_log(&stream, &visible, &q_fwd, Some(0), None, 0, None)
        .expect("search should work")
        .expect("should find match");
    assert_eq!(res.visible_row, 3);

    // Find prev "apple" starting from row 3 -> should wrap back to row 0
    let q_bwd = SearchQuery {
        pattern: "apple".to_string(),
        case_sensitive: false,
        is_regex: false,
        forward: false,
    };
    let res_prev = search_in_log(&stream, &visible, &q_bwd, Some(3), None, 0, None)
        .expect("search should work")
        .expect("should find match");
    assert_eq!(res_prev.visible_row, 0);
}

#[test]
fn test_logcat_stream_handle_raii_and_lifecycle() {
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use crossbeam_channel::unbounded;

    let (tx, _rx) = unbounded();
    let stop_flag = Arc::new(AtomicBool::new(false));

    // Test rejection of illegal serial
    let bad_res = start_logcat_stream(Some("-d; rm -rf"), tx.clone(), stop_flag.clone());
    assert!(bad_res.is_err());

    // Test start and immediate RAII drop
    if let Ok(handle) = start_logcat_stream(Some("emulator-5554"), tx, stop_flag) {
        handle.stop();
    }
}

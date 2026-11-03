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

    let (visible, _style, is_matched) = evaluate_and_style_line(raw_line, &filters, true, None);
    assert!(!visible, "Line must be hidden because exclude filter matches last");
    assert!(is_matched, "Rule matched even though it was excluded");
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

    let (visible, style, is_matched) = evaluate_and_style_line(raw_line, &filters, true, None);
    assert!(visible);
    assert!(is_matched);
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
    // scan_adb_devices must never hang indefinitely, even with tiny timeouts.
    // A 1-nanosecond timeout must deterministically yield Err(AdbError::Timeout)
    // or Err(AdbError::ExecutionFailed) if adb is missing.
    let res = scan_adb_devices(Duration::from_nanos(1));
    match res {
        Err(AdbError::Timeout(d)) => assert_eq!(d, Duration::from_nanos(1)),
        Err(AdbError::ExecutionFailed(_)) => { /* adb binary not installed on host */ }
        Err(other) => panic!("Expected Timeout or ExecutionFailed, got: {:?}", other),
        Ok(_) => panic!("1-nanosecond timeout should never succeed!"),
    }
}

#[test]
fn test_active_log_source_dispatch() {
    let mut stream = StreamLogBuffer::new(100, 10);
    stream.append_chunk(vec!["hello stream".to_string(), "world stream".to_string()]);
    let active = ActiveLogSource::Stream(stream);
    assert_eq!(active.line_count(), 2);
    assert_eq!(active.get_line(0).unwrap(), "hello stream");
    assert_eq!(active.get_line(1).unwrap(), "world stream");
    assert_eq!(active.get_measured_length(0), 12);
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

#[test]
fn test_stream_buffer_recalculates_max_len_on_drain() {
    let mut buffer = StreamLogBuffer::new(5, 3);
    // Line 0 is super long (100 chars), lines 1..8 are short (10 chars)
    let giant_line = "A".repeat(100);
    let mut chunk = vec![giant_line];
    for i in 1..8 {
        chunk.push(format!("short line {}", i));
    }
    buffer.append_chunk(chunk);
    assert_eq!(buffer.max_line_len(), 100);

    // Trim occurs (excess = 8 - 5 = 3 >= trim_batch of 3)
    // Draining lines 0..3 removes the giant line at index 0!
    let trimmed = buffer.trim_if_needed();
    assert_eq!(trimmed, Some(3));
    assert_eq!(buffer.line_count(), 5);

    // Max length must resynchronize to the remaining short lines (~12 chars), NOT stay 100!
    assert!(buffer.max_line_len() < 20);
    assert!(buffer.longest_line_text().starts_with("short line"));
}

#[test]
fn test_case_insensitive_simd_literal_matching() {
    let filter = FilterRule {
        text: "error: connection reset".to_string(),
        case_sensitive: false,
        regex: false,
        active: true,
        ..Default::default()
    };
    let pf = PreparedFilter::new(filter, 0, 0).expect("must compile");

    let line1 = b"2026-11-03 ERROR: Connection Reset by peer";
    let line2 = b"2026-11-03 INFO: Normal connection alive";

    assert!(pf.matches(line1));
    assert!(!pf.matches(line2));
}

#[test]
fn test_cjk_measured_length_accuracy() {
    let mut stream = StreamLogBuffer::new(10, 5);
    // Korean log line: "안녕하세요 2026 로그" (13 chars, 25 UTF-8 bytes)
    stream.append_chunk(vec!["안녕하세요 2026 로그".to_string()]);

    let measured = stream.get_measured_length(0);
    // Must be 13 (character count), NOT 25 (byte count)
    assert_eq!(measured, 13);
}

#[test]
fn test_strongly_typed_domain_errors() {
    // 1. Invalid regex produces FilterError::InvalidRegex
    let bad_rule = FilterRule {
        text: "[unclosed_regex".to_string(),
        regex: true,
        active: true,
        ..Default::default()
    };
    match PreparedFilter::new(bad_rule, 0, 0) {
        Err(FilterError::InvalidRegex(_)) => (),
        _ => panic!("Expected FilterError::InvalidRegex"),
    }

    // 2. Invalid device serial produces AdbError::InvalidSerial
    let (tx, _rx) = crossbeam_channel::unbounded();
    let stop_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    match start_logcat_stream(Some("-d; injection"), tx, stop_flag) {
        Err(AdbError::InvalidSerial(_)) => (),
        _ => panic!("Expected AdbError::InvalidSerial"),
    }
}

#[test]
fn test_rayon_multi_chunk_parallel_filtering_reduction() {
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    // CHUNK_SIZE is 8,192. 25,000 lines tests parallel chunk aggregation across 4 distinct chunks.
    let total_lines = 25_000;
    let mut stream = StreamLogBuffer::new(30_000, 1_000);
    let mut chunk = Vec::with_capacity(total_lines);

    let mut expected_fatal_count = 0;
    let mut expected_error_count = 0;

    for i in 0..total_lines {
        if i % 50 == 0 {
            chunk.push(format!("Line {}: Fatal kernel crash", i));
            expected_fatal_count += 1;
        } else if i % 10 == 0 {
            chunk.push(format!("Line {}: Connection Error occurred", i));
            expected_error_count += 1;
        } else {
            chunk.push(format!("Line {}: Normal system log entry", i));
        }
    }
    stream.append_chunk(chunk);

    let filters = vec![
        FilterRule {
            text: "Fatal".to_string(),
            bg_color: "Red".to_string(),
            active: true,
            ..Default::default()
        },
        FilterRule {
            text: "Error".to_string(),
            bg_color: "Yellow".to_string(),
            active: true,
            ..Default::default()
        },
    ];

    let token = Arc::new(AtomicU64::new(1));
    let res = execute_filtering(&stream, &filters, true, Some(&token), 1).expect("filtering completes");

    // All fatal and error lines are matched and visible
    assert_eq!(res.filter_counts[0], expected_fatal_count);
    assert_eq!(res.filter_counts[1], expected_error_count);
    assert_eq!(res.total_matched_lines, expected_fatal_count + expected_error_count);
    assert_eq!(res.visible_indices.len(), expected_fatal_count + expected_error_count);

    // Indices must be strictly monotonically increasing across chunk boundaries
    for w in res.visible_indices.windows(2) {
        assert!(w[0] < w[1], "Indices must be strictly sorted: {} vs {}", w[0], w[1]);
    }
}

#[test]
fn test_cancellation_token_filtering_and_search_abort() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    let total_lines = 20_000;
    let mut stream = StreamLogBuffer::new(25_000, 1_000);
    let chunk: Vec<String> = (0..total_lines).map(|i| format!("Log entry {}", i)).collect();
    stream.append_chunk(chunk);

    let filters = vec![FilterRule {
        text: "entry".to_string(),
        active: true,
        ..Default::default()
    }];

    // 1. Cancel token has already advanced to generation 2
    let token = Arc::new(AtomicU64::new(2));
    // Requesting execution with generation 1 -> must abort immediately and return None
    let res = execute_filtering(&stream, &filters, false, Some(&token), 1);
    assert!(res.is_none(), "Filtering must abort when generation != token");

    // 2. Cancellation during search
    let visible_indices: Vec<usize> = (0..total_lines).collect();
    let query = SearchQuery {
        pattern: "entry".to_string(),
        case_sensitive: false,
        is_regex: false,
        forward: true,
    };
    token.store(99, Ordering::SeqCst);
    // Passing generation 1 with token at 99 -> must abort and return Err(SearchError::Cancelled)
    let search_res = search_in_log(&stream, &visible_indices, &query, None, Some(&token), 1, None);
    assert!(matches!(search_res, Err(SearchError::Cancelled)));
}

#[test]
fn test_zero_width_regex_highlighting_safety() {
    let patterns = vec!["^", "$", r"\b", ".*", "a*", "(?=test)"];
    let sample_text = "This is a test line for zero-width highlighting safety.";

    for pat in patterns {
        if let Ok(matcher) = CompiledMatcher::new(pat, false, true) {
            let ranges = matcher.find_match_ranges(sample_text, 32);
            for r in &ranges {
                // Must strictly filter out zero-width matches (start < end)
                assert!(r.start < r.end, "Range must not be zero-width for pattern '{}': {:?}", pat, r);
                // Must be valid UTF-8 boundaries
                assert!(sample_text.is_char_boundary(r.start));
                assert!(sample_text.is_char_boundary(r.end));
                let _slice = &sample_text[r.clone()];
            }
            // Must be strictly non-overlapping
            for w in ranges.windows(2) {
                assert!(w[0].end <= w[1].start, "Ranges must be non-overlapping: {:?} vs {:?}", w[0], w[1]);
            }
        }
    }
}

#[test]
fn test_literal_and_case_insensitive_highlight_ranges() {
    let sample = "Alpha beta ALPHA Beta alpha";
    
    // Case-insensitive literal
    let matcher_ci = CompiledMatcher::new("alpha", false, false).unwrap();
    let ranges_ci = matcher_ci.find_match_ranges(sample, 32);
    assert_eq!(ranges_ci.len(), 3);
    for r in &ranges_ci {
        assert_eq!(&sample[r.clone()].to_lowercase(), "alpha");
    }

    // Case-sensitive literal
    let matcher_cs = CompiledMatcher::new("alpha", true, false).unwrap();
    let ranges_cs = matcher_cs.find_match_ranges(sample, 32);
    assert_eq!(ranges_cs.len(), 1);
    assert_eq!(&sample[ranges_cs[0].clone()], "alpha");
}

#[test]
fn test_rule_reorder_precedence_inversion_and_anchor_stability() {
    let sample_line = b"Fatal database deadlock encountered";
    let rule_fatal = FilterRule {
        text: "Fatal".to_string(),
        bg_color: "Red".to_string(),
        text_color: "White".to_string(),
        active: true,
        ..Default::default()
    };
    let rule_db = FilterRule {
        text: "database".to_string(),
        bg_color: "Yellow".to_string(),
        text_color: "Black".to_string(),
        active: true,
        ..Default::default()
    };

    // Order 1: Fatal (idx 0), database (idx 1) -> database (later in list) has precedence via reverse inheritance
    let p1 = PreparedFilter::new(rule_fatal.clone(), 0, 0).unwrap();
    let p2 = PreparedFilter::new(rule_db.clone(), 1, 0).unwrap();
    let (_, style1, _) = evaluate_and_style_line(sample_line, &[p1, p2], true, None);
    assert_eq!(style1.bg_hex, parse_color_spec("Yellow", true));

    // Order 2 (Swapped): database (idx 0), Fatal (idx 1) -> Fatal (later in list) now has precedence
    let p1_swapped = PreparedFilter::new(rule_db, 0, 0).unwrap();
    let p2_swapped = PreparedFilter::new(rule_fatal, 1, 0).unwrap();
    let (_, style2, _) = evaluate_and_style_line(sample_line, &[p1_swapped, p2_swapped], true, None);
    assert_eq!(style2.bg_hex, parse_color_spec("Red", true));

    // Anchor stability check
    let visible_indices = vec![0, 10, 20, 30, 40];
    let anchor_target = 20; // Anchor on raw line 20
    let nearest = find_nearest_anchor_row(&visible_indices, anchor_target);
    assert_eq!(nearest, Some(2));
}

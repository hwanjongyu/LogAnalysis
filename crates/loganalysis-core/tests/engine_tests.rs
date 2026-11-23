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

    let p1 = PreparedFilter::new(f1, 0).unwrap();
    let p2 = PreparedFilter::new(f2, 1).unwrap();
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

    let p1 = PreparedFilter::new(f1, 0).unwrap();
    let p2 = PreparedFilter::new(f2, 1).unwrap();
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

const AUTOMOTIVE_PROFILE_JSON: &str = r#"[
  {
    "name": "🚨 Issue Triage",
    "enabled": true,
    "filters": [
      {
        "text": " F ",
        "regex": false,
        "case_sensitive": true,
        "exclude": false,
        "bg_color": "Red",
        "text_color": "White",
        "active": true
      },
      {
        "text": "ACCGYROCALIBRATION",
        "regex": false,
        "case_sensitive": true,
        "exclude": true,
        "bg_color": "None",
        "text_color": "Default",
        "active": true
      }
    ]
  },
  {
    "name": "🚗 Vehicle & Subsystems",
    "enabled": true,
    "filters": [
      {
        "text": "VehicleHal",
        "regex": false,
        "case_sensitive": false,
        "exclude": false,
        "bg_color": "Cyan",
        "text_color": "Black",
        "active": true
      }
    ]
  },
  {
    "name": "⏱️ Performance & Latency",
    "enabled": true,
    "filters": [
      {
        "text": "Choreographer",
        "regex": false,
        "case_sensitive": false,
        "exclude": false,
        "bg_color": "Yellow",
        "text_color": "Black",
        "active": true
      }
    ]
  }
]"#;

#[test]
fn test_automotive_preset_profile_parses() {
    let tabs = parse_filters_json(AUTOMOTIVE_PROFILE_JSON).expect("parse automotive profile");
    assert_eq!(tabs.len(), 3);

    assert_eq!(tabs[0].name, "🚨 Issue Triage");
    assert!(tabs[0].enabled);
    let fatal_rule = tabs[0]
        .filters
        .iter()
        .find(|f| f.text == " F ")
        .expect("Issue Triage must contain ' F ' rule");
    assert!(fatal_rule.active);
    assert!(!fatal_rule.exclude);
    assert_eq!(fatal_rule.bg_color, "Red");
    assert_eq!(fatal_rule.text_color, "White");

    let exclude_rule = tabs[0]
        .filters
        .iter()
        .find(|f| f.exclude && f.text == "ACCGYROCALIBRATION")
        .expect("Issue Triage must contain exclude ACCGYROCALIBRATION rule");
    assert!(exclude_rule.active);

    assert_eq!(tabs[1].name, "🚗 Vehicle & Subsystems");
    assert!(tabs[1].enabled);
    assert!(tabs[1].filters.iter().any(|f| f.text == "VehicleHal"));

    assert_eq!(tabs[2].name, "⏱️ Performance & Latency");
    assert!(tabs[2].enabled);
    assert!(tabs[2].filters.iter().any(|f| f.text == "Choreographer"));
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

    // Test cached get_line_arc pointer equality (zero-copy cache hits)
    let arc1 = buffer.get_line_arc(0).expect("first get_line_arc");
    let arc2 = buffer.get_line_arc(0).expect("second get_line_arc (cache hit)");
    assert_eq!(&*arc1, "Line 1: Alpha");
    assert!(std::sync::Arc::ptr_eq(&arc1, &arc2), "Expected cached Arc pointer equality");

    assert!(buffer.get_line_arc(99).is_none());
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
    let pf = PreparedFilter::new(filter, 0).expect("must compile");

    let line1 = b"2026-11-03 ERROR: Connection Reset by peer";
    let line2 = b"2026-11-03 INFO: Normal connection alive";

    assert!(pf.matches(line1));
    assert!(!pf.matches(line2));
}

#[test]
fn test_cjk_measured_length_accuracy() {
    let mut stream = StreamLogBuffer::new(10, 5);
    // Korean log line: "안녕하세요 2026 로그" (13 chars, 25 UTF-8 bytes, 20 visual monospace columns)
    stream.append_chunk(vec!["안녕하세요 2026 로그".to_string()]);

    let measured = stream.get_measured_length(0);
    // Must be 20 (East Asian monospace column width: 7 fullwidth * 2 + 6 ascii = 20), NOT 25 (byte count)
    assert_eq!(measured, 20);
    assert_eq!(stream.max_line_len(), 20);
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
    match PreparedFilter::new(bad_rule, 0) {
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
    let p1 = PreparedFilter::new(rule_fatal.clone(), 0).unwrap();
    let p2 = PreparedFilter::new(rule_db.clone(), 1).unwrap();
    let (_, style1, _) = evaluate_and_style_line(sample_line, &[p1, p2], true, None);
    assert_eq!(style1.bg_hex, parse_color_spec("Yellow", true));

    // Order 2 (Swapped): database (idx 0), Fatal (idx 1) -> Fatal (later in list) now has precedence
    let p1_swapped = PreparedFilter::new(rule_db, 0).unwrap();
    let p2_swapped = PreparedFilter::new(rule_fatal, 1).unwrap();
    let (_, style2, _) = evaluate_and_style_line(sample_line, &[p1_swapped, p2_swapped], true, None);
    assert_eq!(style2.bg_hex, parse_color_spec("Red", true));

    // Anchor stability check
    let visible_indices = vec![0, 10, 20, 30, 40];
    let anchor_target = 20; // Anchor on raw line 20
    let nearest = find_nearest_anchor_row(&visible_indices, anchor_target);
    assert_eq!(nearest, Some(2));
}

#[test]
fn test_custom_foreground_and_background_styling() {
    let raw_line = b"E/Database: Query execution timed out after 30000ms";
    let rule = FilterRule {
        text: "Database".to_string(),
        bg_color: "Yellow".to_string(),
        text_color: "DarkRed".to_string(),
        active: true,
        ..Default::default()
    };

    let pf = PreparedFilter::new(rule, 0).unwrap();
    let (visible, style, is_matched) = evaluate_and_style_line(raw_line, &[pf], true, None);

    assert!(visible);
    assert!(is_matched);
    assert_eq!(style.bg_hex, parse_color_spec("Yellow", true));
    assert_eq!(style.fg_hex, parse_color_spec("DarkRed", false));
    assert_eq!(style.bg_hex, Some(0xFFFF00));
    assert_eq!(style.fg_hex, Some(0x8B0000));
}

#[test]
fn test_text_only_filter_inherits_theme_bg() {
    let raw_line = b"W/Network: Packet loss detected on interface wlan0";
    let rule = FilterRule {
        text: "Network".to_string(),
        bg_color: "None".to_string(), // Transparent background
        text_color: "Red".to_string(), // Custom text color
        active: true,
        ..Default::default()
    };

    let pf = PreparedFilter::new(rule, 0).unwrap();
    let (visible, style, is_matched) = evaluate_and_style_line(raw_line, &[pf], true, None);

    assert!(visible);
    assert!(is_matched);
    assert_eq!(style.bg_hex, None, "Background must remain None (transparent)");
    assert_eq!(style.fg_hex, parse_color_spec("Red", false));
    assert_eq!(style.fg_hex, Some(0xFF0000));
}

#[test]
fn test_filter_hit_counts_by_uuid_with_disabled_rules_and_multiple_tabs() {
    let raw_lines = vec![
        "2026-11-03 10:00:00 I/Auth: Token refreshed".to_string(),
        "2026-11-03 10:00:01 E/Database: Query failed on table users".to_string(),
        "2026-11-03 10:00:02 E/Database: Syntax error near SELECT".to_string(),
        "2026-11-03 10:00:03 I/Network: TCP handshake completed".to_string(),
        "2026-11-03 10:00:04 E/Database: Connection pool exhausted".to_string(),
    ];
    let mut stream = StreamLogBuffer::new(100, 10);
    stream.append_chunk(raw_lines);

    // Tab 1 rules:
    // Rule 0: inactive rule (should not shift Rule 1's count)
    let rule_inactive = FilterRule {
        text: "Auth".to_string(),
        active: false,
        ..Default::default()
    };
    // Rule 1: active Database rule (3 matches)
    let rule_db = FilterRule {
        text: "Database".to_string(),
        active: true,
        bg_color: "Red".to_string(),
        ..Default::default()
    };

    // Tab 2 rules:
    // Rule 2: active Network rule (1 match)
    let rule_net = FilterRule {
        text: "Network".to_string(),
        active: true,
        bg_color: "Blue".to_string(),
        ..Default::default()
    };

    let rules = vec![rule_inactive.clone(), rule_db.clone(), rule_net.clone()];
    let res = execute_filtering(&stream, &rules, true, None, 0).expect("Filtering should succeed");

    // Verify UUID-keyed lookup
    assert_eq!(res.count_for_rule(&rule_inactive.id), 0, "Inactive rule count must be 0");
    assert_eq!(res.count_for_rule(&rule_db.id), 3, "Database rule count must be exactly 3");
    assert_eq!(res.count_for_rule(&rule_net.id), 1, "Network rule count must be exactly 1");

    // Non-existent UUID must return 0
    let random_id = uuid::Uuid::new_v4();
    assert_eq!(res.count_for_rule(&random_id), 0);
}

#[test]
fn test_scan_adb_devices_nonblocking_timeout_reap() {
    use std::time::Duration;
    // Calling scan_adb_devices with microsecond timeout must immediately return Timeout
    // without deadlocking on child process locks.
    let res = scan_adb_devices(Duration::from_micros(50));
    match res {
        Err(AdbError::Timeout(_)) => {}
        Err(AdbError::ExecutionFailed(_)) => {} // adb binary missing in CI/container
        Err(other) => panic!("Unexpected error: {:?}", other),
        Ok(_) => {} // Finished extremely fast if adb was cached
    }
}

#[test]
fn test_stream_log_lines_immediate_flush_and_stop_flag() {
    use std::io::{BufReader, Cursor};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use crossbeam_channel::bounded;

    let (tx, rx) = bounded(16);
    let stop_flag = Arc::new(AtomicBool::new(false));

    // A single line in input: should be flushed immediately because reader.buffer().is_empty()
    let data = "11-04 18:30:00.123 1000 1001 I SingleLogLine: Hello ADB\n";
    let reader = BufReader::new(Cursor::new(data));

    stream_log_lines(reader, tx, stop_flag.clone());

    let batch = rx.try_recv().expect("Single line must flush immediately");
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0], "11-04 18:30:00.123 1000 1001 I SingleLogLine: Hello ADB");

    // Multiple lines test
    let (tx2, rx2) = bounded(16);
    let multi_data = "Line A\nLine B\nLine C\n";
    let reader2 = BufReader::new(Cursor::new(multi_data));
    stream_log_lines(reader2, tx2, stop_flag.clone());

    let mut total_lines = 0;
    while let Ok(batch) = rx2.try_recv() {
        total_lines += batch.len();
    }
    assert_eq!(total_lines, 3);

    // Stop flag early exit test
    let (tx3, _rx3) = bounded(16);
    let infinite_data = "Line\n".repeat(1000);
    let reader3 = BufReader::new(Cursor::new(infinite_data));
    let stop_now = Arc::new(AtomicBool::new(true));
    stream_log_lines(reader3, tx3, stop_now);
}

#[test]
fn test_stream_log_lines_bounded_line_reader() {
    use std::io::{BufReader, Cursor};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use crossbeam_channel::bounded;

    let (tx, rx) = bounded(16);
    let stop_flag = Arc::new(AtomicBool::new(false));

    // Create a 100 KB line without any newlines
    let giant_line = "A".repeat(100_000);
    let reader = BufReader::new(Cursor::new(giant_line));

    stream_log_lines(reader, tx, stop_flag);

    let mut total_received_chars = 0;
    while let Ok(batch) = rx.try_recv() {
        for line in batch {
            assert!(line.len() <= 65536, "Chunk line must be bounded to 64KB");
            total_received_chars += line.len();
        }
    }
    assert_eq!(total_received_chars, 100_000);
}

#[test]
fn test_stream_log_buffer_in_place_arc_get_mut_and_fallback() {
    use std::sync::Arc;

    let stream = StreamLogBuffer::new(100, 20);
    let mut log_source = Some(Arc::new(ActiveLogSource::Stream(stream)));

    let batch1 = vec!["Line 1".to_string(), "Line 2".to_string()];

    // Case 1: Uniquely owned Arc - must succeed in-place with Arc::get_mut (zero Arc<str> pointer cloning)
    if let Some(src) = &mut log_source {
        let is_unique = Arc::get_mut(src).is_some();
        assert!(is_unique, "Log source Arc must be unique before concurrent tasks");

        if let Some(ActiveLogSource::Stream(ref mut stream)) = Arc::get_mut(src) {
            stream.append_chunk(batch1);
            stream.trim_if_needed();
        }
    }

    assert_eq!(log_source.as_ref().unwrap().line_count(), 2);

    // Case 2: Concurrent background task holds a cloned reference (e.g. background filter task)
    let background_ref = log_source.clone();
    let batch2 = vec!["Line 3".to_string(), "Line 4".to_string()];

    if let Some(src) = &mut log_source {
        // Arc::get_mut must return None because background_ref exists
        assert!(Arc::get_mut(src).is_none(), "Arc::get_mut must be None when shared");

        // Safe fallback clone: mutates independent instance without corrupting background task
        if let ActiveLogSource::Stream(ref stream) = **src {
            let mut new_stream = stream.clone();
            new_stream.append_chunk(batch2);
            new_stream.trim_if_needed();
            *src = Arc::new(ActiveLogSource::Stream(new_stream));
        }
    }

    // Foreground has 4 lines
    assert_eq!(log_source.as_ref().unwrap().line_count(), 4);
    // Background snapshot is completely preserved with 2 lines
    assert_eq!(background_ref.as_ref().unwrap().line_count(), 2);
}

#[test]
fn test_bounded_channel_disconnect_semantics() {
    use crossbeam_channel::{bounded, TryRecvError};

    let (tx, rx) = bounded::<Vec<String>>(256);

    tx.send(vec!["chunk1".to_string()]).unwrap();
    tx.send(vec!["chunk2".to_string()]).unwrap();
    drop(tx); // Simulate ADB process exit / pipe closure

    let mut batch = Vec::new();
    let mut is_disconnected = false;

    loop {
        match rx.try_recv() {
            Ok(chunk) => batch.extend(chunk),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                is_disconnected = true;
                break;
            }
        }
    }

    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0], "chunk1");
    assert_eq!(batch[1], "chunk2");

    // After draining, next try_recv must return Disconnected
    assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    assert!(is_disconnected, "Loop must have detected disconnection after draining");
}

#[test]
fn test_multi_filter_matcher_aho_corasick_consistency() {
    let r1 = FilterRule {
        text: "FATAL".to_string(),
        bg_color: "red".to_string(),
        case_sensitive: true,
        active: true,
        ..Default::default()
    };
    let r2 = FilterRule {
        text: "exception".to_string(),
        bg_color: "orange".to_string(),
        case_sensitive: false, // Case-insensitive
        active: true,
        ..Default::default()
    };
    let r3 = FilterRule {
        text: "FATAL EXCEPTION".to_string(), // Overlapping substring with r1 and r2
        bg_color: "darkred".to_string(),
        case_sensitive: true,
        active: true,
        ..Default::default()
    };
    let r4 = FilterRule {
        text: r"code=\d+".to_string(), // Regex rule
        bg_color: "blue".to_string(),
        regex: true,
        case_sensitive: true,
        active: true,
        ..Default::default()
    };

    let rules = [r1, r2, r3, r4];
    let prepared: Vec<PreparedFilter> = rules
        .iter()
        .enumerate()
        .map(|(i, r)| PreparedFilter::new(r.clone(), i).unwrap())
        .collect();

    let multi_matcher = MultiFilterMatcher::new(&prepared);

    let test_lines = vec![
        b"11-04 12:00:00.000 I Normal: Normal log line with no matches".as_slice(),
        b"11-04 12:00:01.000 E Crash: FATAL EXCEPTION occurred with code=42".as_slice(),
        b"11-04 12:00:02.000 W Warn: Java NullPointerException thrown".as_slice(),
        b"11-04 12:00:03.000 D Debug: Only fatal in lowercase (not matched by r1, but matched by r2 if it were fatal)".as_slice(),
        b"11-04 12:00:04.000 E Tag: error code=999 received".as_slice(),
    ];

    for raw in test_lines {
        let mut counts_seq = vec![0; 4];
        let (vis_seq, style_seq, mat_seq) =
            evaluate_and_style_line(raw, &prepared, true, Some(&mut counts_seq));

        let mut counts_multi = vec![0; 4];
        let (vis_multi, style_multi, mat_multi) = evaluate_and_style_line_with_matcher(
            raw,
            &prepared,
            &multi_matcher,
            true,
            Some(&mut counts_multi),
        );

        assert_eq!(vis_seq, vis_multi, "Visibility mismatch on line: {:?}", String::from_utf8_lossy(raw));
        assert_eq!(style_seq, style_multi, "Style mismatch on line: {:?}", String::from_utf8_lossy(raw));
        assert_eq!(mat_seq, mat_multi, "Matched mismatch on line: {:?}", String::from_utf8_lossy(raw));
        assert_eq!(counts_seq, counts_multi, "Hit counts mismatch on line: {:?}", String::from_utf8_lossy(raw));
    }
}

#[test]
fn test_append_streaming_batch_incremental_vs_full() {
    let r1 = FilterRule {
        text: "ERROR".to_string(),
        bg_color: "red".to_string(),
        case_sensitive: true,
        active: true,
        ..Default::default()
    };
    let r2 = FilterRule {
        text: "noise".to_string(),
        bg_color: "gray".to_string(),
        case_sensitive: false,
        exclude: true, // Exclude rule
        active: true,
        ..Default::default()
    };
    let rules = vec![r1, r2];

    let mut stream = StreamLogBuffer::new(100, 20);

    // Initial batch of 10 lines
    let batch1: Vec<String> = (0..10)
        .map(|i| {
            if i % 3 == 0 {
                format!("Line {}: An ERROR occurred", i)
            } else {
                format!("Line {}: Info message", i)
            }
        })
        .collect();

    stream.append_chunk(batch1);

    // Initial full filter pass
    let mut incremental_res = execute_filtering(&stream, &rules, true, None, 1).unwrap();
    assert_eq!(incremental_res.visible_indices.len(), 4); // i = 0, 3, 6, 9

    // Second batch of 10 lines
    let batch2: Vec<String> = (10..20)
        .map(|i| {
            if i == 12 {
                format!("Line {}: An ERROR but noise excluded", i)
            } else if i % 2 == 0 {
                format!("Line {}: Another ERROR occurred", i)
            } else {
                format!("Line {}: Info message", i)
            }
        })
        .collect();

    let start_idx = stream.line_count();
    stream.append_chunk(batch2);
    let end_idx = stream.line_count();

    // Perform incremental append
    append_streaming_batch(
        &mut incremental_res,
        &stream,
        start_idx..end_idx,
        0,
        &rules,
        true,
    );

    // Compare with full re-execution from scratch
    let full_res = execute_filtering(&stream, &rules, true, None, 2).unwrap();

    assert_eq!(incremental_res.visible_indices, full_res.visible_indices);
    assert_eq!(incremental_res.visible_styles, full_res.visible_styles);
    assert_eq!(incremental_res.filter_counts, full_res.filter_counts);
    assert_eq!(incremental_res.total_matched_lines, full_res.total_matched_lines);
    for rule in &rules {
        assert_eq!(
            incremental_res.count_for_rule(&rule.id),
            full_res.count_for_rule(&rule.id)
        );
    }

    // Now test trimming semantics: simulate buffer trimmed by 5 lines
    let trimmed_count = 5;
    append_streaming_batch(
        &mut incremental_res,
        &stream,
        end_idx..end_idx, // no new lines, only trim
        trimmed_count,
        &rules,
        true,
    );

    // All visible indices should be shifted down by 5, and any index < 5 removed
    for &idx in &incremental_res.visible_indices {
        assert!(idx < stream.line_count());
    }
}

#[test]
fn test_log_source_get_line_arc_zero_copy() {
    use std::sync::Arc;

    let mut stream = StreamLogBuffer::new(50, 10);
    stream.append_chunk(vec!["Test Line 1".to_string(), "Test Line 2".to_string()]);

    let arc1 = stream.get_line_arc(0).expect("Line 0 must exist");
    let arc2 = stream.get_line_arc(0).expect("Line 0 must exist again");

    // Both returned Arcs must point to the identical heap allocation (zero-copy clone)
    assert!(Arc::ptr_eq(&arc1, &arc2));
    assert_eq!(&*arc1, "Test Line 1");
}

#[test]
fn test_append_streaming_batch_trims_adjust_total_matched() {
    let mut stream = StreamLogBuffer::new(100, 10);
    let lines = vec![
        "Match 1".to_string(),
        "Ignore 2".to_string(),
        "Match 3".to_string(),
        "Ignore 4".to_string(),
        "Match 5".to_string(),
    ];
    stream.append_chunk(lines);

    let rule = FilterRule {
        text: "Match".to_string(),
        active: true,
        ..Default::default()
    };
    let rules = vec![rule.clone()];

    // Filter all 5 lines initially with show_only_filtered = true
    let mut res = FilterResult::default();
    append_streaming_batch(&mut res, &stream, 0..5, 0, &rules, true);

    assert_eq!(res.visible_indices, vec![0, 2, 4]);
    assert_eq!(res.total_matched_lines, 3);
    assert_eq!(res.count_for_rule(&rule.id), 3);
    assert_eq!(res.filter_counts[0], 3);

    // Simulate trimming 3 lines from the front (lines 0, 1, 2 trimmed)
    // Matches dropped: line 0 and line 2 (2 matches dropped)
    append_streaming_batch(&mut res, &stream, 5..5, 3, &rules, true);

    // Only line 4 remains, shifted down by 3 -> new index 1
    assert_eq!(res.visible_indices, vec![1]);
    assert_eq!(res.total_matched_lines, 1);
    assert_eq!(res.count_for_rule(&rule.id), 1);
    assert_eq!(res.filter_counts[0], 1);

    // Test with show_only_filtered = false
    let mut res_unfiltered = FilterResult::default();
    append_streaming_batch(&mut res_unfiltered, &stream, 0..5, 0, &rules, false);
    assert_eq!(res_unfiltered.total_matched_lines, 3);
    assert_eq!(res_unfiltered.count_for_rule(&rule.id), 3);
    assert_eq!(res_unfiltered.filter_counts[0], 3);
    assert_eq!(res_unfiltered.visible_indices.len(), 5);

    // Trim 3 lines
    append_streaming_batch(&mut res_unfiltered, &stream, 5..5, 3, &rules, false);
    assert_eq!(res_unfiltered.total_matched_lines, 1);
    assert_eq!(res_unfiltered.count_for_rule(&rule.id), 1);
    assert_eq!(res_unfiltered.filter_counts[0], 1);
    assert_eq!(res_unfiltered.visible_indices.len(), 2);
}

#[test]
fn test_resolve_adb_path_home_auto_detection() {
    let adb_path = resolve_adb_path();
    // Must return a valid non-empty path ending in "adb" (or "adb.exe" on Windows)
    let file_name = adb_path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    assert!(file_name.starts_with("adb"), "ADB binary name must start with adb, got: {:?}", adb_path);

    // Test with temporary mock HOME directory
    let temp_home = tempfile::tempdir().expect("tempdir");
    let mock_platform_tools = temp_home.path().join("Android/Sdk/platform-tools");
    std::fs::create_dir_all(&mock_platform_tools).expect("create mock platform-tools");
    let mock_adb_file = mock_platform_tools.join(if cfg!(windows) { "adb.exe" } else { "adb" });
    std::fs::write(&mock_adb_file, b"#!/bin/sh\nexit 0\n").expect("write mock adb");

    // Preserve real env
    let old_home = std::env::var("HOME").ok();
    let old_android_home = std::env::var("ANDROID_HOME").ok();
    let old_android_sdk_root = std::env::var("ANDROID_SDK_ROOT").ok();

    std::env::remove_var("ANDROID_HOME");
    std::env::remove_var("ANDROID_SDK_ROOT");
    std::env::set_var("HOME", temp_home.path());

    let resolved = resolve_adb_path();
    assert_eq!(resolved, mock_adb_file, "Must resolve mock adb inside user home Android SDK directory");

    // Restore env
    if let Some(h) = old_home {
        std::env::set_var("HOME", h);
    }
    if let Some(a) = old_android_home {
        std::env::set_var("ANDROID_HOME", a);
    }
    if let Some(s) = old_android_sdk_root {
        std::env::set_var("ANDROID_SDK_ROOT", s);
    }
}

#[test]
fn test_parse_adb_devices_output_rich_metadata() {
    let output = "\
List of devices attached
* daemon not running; starting now at tcp:5037
* daemon started successfully
44190DLJH0010R         device usb:1-2.1 product:shiba model:Pixel_8 device:shiba transport_id:1
emulator-5554          device product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64 device:emu64x transport_id:2
192.168.1.50:5555      device product:bramble model:Pixel_4a_5G device:bramble transport_id:3
UNAUTH123456          unauthorized usb:1-1 transport_id:4
OFFLINE99999          offline transport_id:5
";

    let devices = parse_adb_devices_output(output);
    assert_eq!(devices.len(), 5);

    // Pixel 8
    assert_eq!(devices[0].serial, "44190DLJH0010R");
    assert_eq!(devices[0].state, "device");
    assert_eq!(devices[0].model.as_deref(), Some("Pixel_8"));
    assert_eq!(devices[0].short_name(), "Pixel 8");
    assert_eq!(devices[0].display_name(), "Pixel 8 (44190DLJH0010R)");
    assert!(devices[0].is_ready());

    // Emulator
    assert_eq!(devices[1].serial, "emulator-5554");
    assert_eq!(devices[1].short_name(), "sdk gphone64 x86 64");
    assert!(devices[1].is_ready());

    // WiFi device
    assert_eq!(devices[2].serial, "192.168.1.50:5555");
    assert_eq!(devices[2].short_name(), "Pixel 4a 5G");
    assert!(devices[2].is_ready());

    // Unauthorized
    assert_eq!(devices[3].serial, "UNAUTH123456");
    assert_eq!(devices[3].state, "unauthorized");
    assert_eq!(devices[3].display_name(), "UNAUTH123456 [unauthorized]");
    assert!(!devices[3].is_ready());

    // Offline
    assert_eq!(devices[4].serial, "OFFLINE99999");
    assert_eq!(devices[4].state, "offline");
    assert_eq!(devices[4].display_name(), "OFFLINE99999 [offline]");
    assert!(!devices[4].is_ready());
}

#[test]
fn test_append_streaming_batch_gray_filter_trim_counting() {
    let mut stream = StreamLogBuffer::new(100, 10);
    let lines = vec![
        "Match 1".to_string(),
        "Ignore 2".to_string(),
        "Match 3".to_string(),
        "Ignore 4".to_string(),
        "Match 5".to_string(),
    ];
    stream.append_chunk(lines);

    let rule = FilterRule {
        text: "Match".to_string(),
        active: true,
        bg_color: "None".to_string(),
        text_color: "Gray".to_string(), // Gray text (#808080)
        ..Default::default()
    };
    let rules = vec![rule.clone()];

    let mut res_unfiltered = FilterResult::default();
    append_streaming_batch(&mut res_unfiltered, &stream, 0..5, 0, &rules, false);
    assert_eq!(res_unfiltered.total_matched_lines, 3);
    assert_eq!(res_unfiltered.count_for_rule(&rule.id), 3);
    assert_eq!(res_unfiltered.visible_indices.len(), 5);

    // Trim 3 lines: lines 0 and 2 are dropped matches
    append_streaming_batch(&mut res_unfiltered, &stream, 5..5, 3, &rules, false);
    assert_eq!(res_unfiltered.total_matched_lines, 1);
    assert_eq!(res_unfiltered.count_for_rule(&rule.id), 1);
    assert_eq!(res_unfiltered.visible_indices.len(), 2);
}

#[test]
fn test_compiled_stream_filters_parity_with_append_streaming_batch() {
    let r_error = FilterRule {
        text: "ERROR".to_string(),
        bg_color: "Red".to_string(),
        text_color: "White".to_string(),
        active: true,
        ..Default::default()
    };
    let r_inactive = FilterRule {
        text: "FATAL".to_string(),
        bg_color: "Salmon".to_string(),
        active: false,
        ..Default::default()
    };
    let r_invalid_regex = FilterRule {
        text: "[unclosed(".to_string(),
        regex: true,
        active: true,
        ..Default::default()
    };
    let r_warn_regex = FilterRule {
        text: r"WARN\s+\[\d+\]".to_string(),
        regex: true,
        bg_color: "Orange".to_string(),
        active: true,
        ..Default::default()
    };
    let r_exclude = FilterRule {
        text: "healthcheck".to_string(),
        exclude: true,
        active: true,
        ..Default::default()
    };
    let r_auth = FilterRule {
        text: "Auth".to_string(),
        text_color: "Blue".to_string(),
        active: true,
        ..Default::default()
    };

    let rules = vec![
        r_error,
        r_inactive,
        r_invalid_regex,
        r_warn_regex,
        r_exclude,
        r_auth,
    ];
    let compiled = CompiledStreamFilters::new(&rules);
    assert_eq!(compiled.rules().len(), rules.len());
    // Active non-empty valid rules: r_error (0), r_warn_regex (3), r_exclude (4), r_auth (5)
    assert_eq!(compiled.prepared().len(), 4);

    let batch1 = vec![
        "I/Init: normal line".to_string(),
        "E/Core: ERROR connecting to bus".to_string(),
        "W/Net: WARN [42] latency spike with extra padding".to_string(),
        "I/Auth: Auth token refreshed".to_string(),
        "E/Probe: ERROR healthcheck failed".to_string(), // excluded by r_exclude (survives trim-only step 3)
    ];
    let batch2 = vec![
        "D/Idle: unrelated trace line".to_string(),
        "W/Pool: WARN [99] queue depth high".to_string(),
        "E/Auth: ERROR Auth handshake timeout".to_string(),
        "E/Probe: healthcheck Auth skip".to_string(), // excluded by r_exclude despite matching Auth
    ];

    let assert_incremental_eq = |a: &FilterResult, b: &FilterResult, check_widest: bool, label: &str| {
        assert_eq!(a.visible_indices, b.visible_indices, "{label}: visible_indices");
        assert_eq!(a.visible_styles, b.visible_styles, "{label}: visible_styles");
        assert_eq!(
            a.visible_filter_matches, b.visible_filter_matches,
            "{label}: visible_filter_matches"
        );
        assert_eq!(
            a.excluded_filter_matches, b.excluded_filter_matches,
            "{label}: excluded_filter_matches"
        );
        assert_eq!(a.filter_counts, b.filter_counts, "{label}: filter_counts");
        assert_eq!(a.rule_counts, b.rule_counts, "{label}: rule_counts");
        assert_eq!(
            a.total_matched_lines, b.total_matched_lines,
            "{label}: total_matched_lines"
        );
        if check_widest {
            assert_eq!(
                a.widest_visible_text, b.widest_visible_text,
                "{label}: widest_visible_text"
            );
            assert_eq!(
                a.widest_visible_length, b.widest_visible_length,
                "{label}: widest_visible_length"
            );
        }
    };

    for show_only_filtered in [true, false] {
        let mut stream = StreamLogBuffer::new(6, 2);
        stream.append_chunk(batch1.clone());

        let mut compiled_res = FilterResult::default();
        let mut standalone_res = FilterResult::default();

        // 1. Append-only batch
        compiled.append_streaming_batch(&mut compiled_res, &stream, 0..5, 0, show_only_filtered);
        append_streaming_batch(
            &mut standalone_res,
            &stream,
            0..5,
            0,
            &rules,
            show_only_filtered,
        );
        let full_1 = execute_filtering(&stream, &rules, show_only_filtered, None, 0)
            .expect("full filtering phase 1");

        assert_incremental_eq(
            &compiled_res,
            &standalone_res,
            true,
            "batch1 compiled vs standalone",
        );
        assert_incremental_eq(&compiled_res, &full_1, true, "batch1 compiled vs full");

        // 2. Append + trim batch (append 4 lines => 9 lines > cap 6 => trim 3 lines; new live range 2..6)
        stream.append_chunk(batch2.clone());
        let trimmed = stream.trim_if_needed().expect("batch2 should trigger trim");
        assert_eq!(trimmed, 3);
        assert_eq!(stream.len(), 6);

        compiled.append_streaming_batch(
            &mut compiled_res,
            &stream,
            2..6,
            trimmed,
            show_only_filtered,
        );
        append_streaming_batch(
            &mut standalone_res,
            &stream,
            2..6,
            trimmed,
            &rules,
            show_only_filtered,
        );
        let full_2 = execute_filtering(&stream, &rules, show_only_filtered, None, 0)
            .expect("full filtering phase 2");

        assert_incremental_eq(
            &compiled_res,
            &standalone_res,
            true,
            "batch2 compiled vs standalone",
        );
        assert_incremental_eq(&compiled_res, &full_2, true, "batch2 compiled vs full");

        // 3. Trim-only step (evict 1 visible line without appending)
        let mut shrink_stream = StreamLogBuffer::new(5, 1);
        let remaining_lines: Vec<String> = (0..stream.len())
            .filter_map(|i| stream.get_line(i).map(|s| s.into_owned()))
            .collect();
        shrink_stream.append_chunk(remaining_lines);
        let trimmed_only = shrink_stream.trim_if_needed().expect("shrink should trim 1");
        assert_eq!(trimmed_only, 1);

        compiled.append_streaming_batch(
            &mut compiled_res,
            &shrink_stream,
            5..5,
            trimmed_only,
            show_only_filtered,
        );
        append_streaming_batch(
            &mut standalone_res,
            &shrink_stream,
            5..5,
            trimmed_only,
            &rules,
            show_only_filtered,
        );
        let full_3 = execute_filtering(&shrink_stream, &rules, show_only_filtered, None, 0)
            .expect("full filtering phase 3");

        assert_incremental_eq(
            &compiled_res,
            &standalone_res,
            true,
            "trim-only compiled vs standalone",
        );
        assert_incremental_eq(&compiled_res, &full_3, true, "trim-only compiled vs full");

        // 4. Trim-only step dropping the excluded line: full parity across compiled, standalone, and execute_filtering [INV-09]
        let mut drop_excl_stream = StreamLogBuffer::new(4, 1);
        let rem2: Vec<String> = (0..shrink_stream.len())
            .filter_map(|i| shrink_stream.get_line(i).map(|s| s.into_owned()))
            .collect();
        drop_excl_stream.append_chunk(rem2);
        let trimmed_excl = drop_excl_stream
            .trim_if_needed()
            .expect("drop_excl should trim 1");
        assert_eq!(trimmed_excl, 1);

        compiled.append_streaming_batch(
            &mut compiled_res,
            &drop_excl_stream,
            4..4,
            trimmed_excl,
            show_only_filtered,
        );
        append_streaming_batch(
            &mut standalone_res,
            &drop_excl_stream,
            4..4,
            trimmed_excl,
            &rules,
            show_only_filtered,
        );
        let full_4 = execute_filtering(&drop_excl_stream, &rules, show_only_filtered, None, 0)
            .expect("full filtering phase 4");
        assert_incremental_eq(
            &compiled_res,
            &standalone_res,
            true,
            "trim-excluded compiled vs standalone",
        );
        assert_incremental_eq(&compiled_res, &full_4, true, "trim-excluded compiled vs full");
    }
}

#[test]
fn test_compiled_stream_filters_trim_syncs_rule_counts() {
    let r_alpha = FilterRule {
        text: "ALPHA".to_string(),
        active: true,
        ..Default::default()
    };
    let r_beta = FilterRule {
        text: "BETA".to_string(),
        active: true,
        ..Default::default()
    };
    let r_inactive = FilterRule {
        text: "ALPHA".to_string(),
        active: false,
        ..Default::default()
    };
    let rules = vec![r_alpha.clone(), r_inactive.clone(), r_beta.clone()];
    let compiled = CompiledStreamFilters::new(&rules);

    let mut stream = StreamLogBuffer::new(10, 1);
    stream.append_chunk(vec![
        "0 ALPHA only".to_string(),
        "1 ALPHA BETA".to_string(),
        "2 BETA only".to_string(),
        "3 BETA tail".to_string(),
    ]);

    let mut res = FilterResult::default();
    compiled.append_streaming_batch(&mut res, &stream, 0..4, 0, true);
    assert_eq!(res.filter_counts, vec![2, 0, 3]);
    assert_eq!(res.count_for_rule(&r_alpha.id), 2);
    assert_eq!(res.rule_counts.get(&r_alpha.id).copied(), Some(2));
    assert_eq!(res.count_for_rule(&r_inactive.id), 0);
    assert_eq!(res.count_for_rule(&r_beta.id), 3);

    // Partial trim drops first 2 lines: both ALPHA matches removed, 1 BETA match removed
    let mut trimmed_stream = StreamLogBuffer::new(2, 1);
    trimmed_stream.append_chunk(vec![
        "0 ALPHA only".to_string(),
        "1 ALPHA BETA".to_string(),
        "2 BETA only".to_string(),
        "3 BETA tail".to_string(),
    ]);
    assert_eq!(trimmed_stream.trim_if_needed(), Some(2));
    compiled.append_streaming_batch(&mut res, &trimmed_stream, 2..2, 2, true);
    assert_eq!(res.filter_counts, vec![0, 0, 2]);
    assert_eq!(res.count_for_rule(&r_alpha.id), 0);
    assert_eq!(res.rule_counts.get(&r_alpha.id).copied(), Some(0));
    assert_eq!(res.count_for_rule(&r_beta.id), 2);
    assert_eq!(res.rule_counts.get(&r_beta.id).copied(), Some(2));
    assert_eq!(res.visible_indices, vec![0, 1]);

    // Full trim drops remaining 2 lines: all rule_counts synchronized to 0
    trimmed_stream.clear();
    compiled.append_streaming_batch(&mut res, &trimmed_stream, 0..0, 2, true);
    assert_eq!(res.filter_counts, vec![0, 0, 0]);
    assert_eq!(res.count_for_rule(&r_alpha.id), 0);
    assert_eq!(res.count_for_rule(&r_beta.id), 0);
    assert_eq!(res.total_matched_lines, 0);
    assert!(res.visible_indices.is_empty());
}

#[test]
fn test_excluded_line_trim_counter_parity_when_visible_empty() {
    let r_err = FilterRule {
        text: "ERROR".to_string(),
        active: true,
        ..Default::default()
    };
    let r_warn = FilterRule {
        text: "WARN".to_string(),
        active: true,
        ..Default::default()
    };
    let r_drop = FilterRule {
        text: "DROP".to_string(),
        exclude: true,
        active: true,
        ..Default::default()
    };
    let rules = vec![r_err.clone(), r_warn.clone(), r_drop.clone()];
    let compiled = CompiledStreamFilters::new(&rules);

    let initial_lines = vec![
        "ERROR DROP one".to_string(),
        "WARN DROP two".to_string(),
        "ERROR WARN DROP three".to_string(),
        "DROP four".to_string(),
    ];

    let mut stream = StreamLogBuffer::new(10, 1);
    stream.append_chunk(initial_lines.clone());

    let mut res = FilterResult::default();
    compiled.append_streaming_batch(&mut res, &stream, 0..4, 0, true);
    let full_0 = execute_filtering(&stream, &rules, true, None, 0).expect("full 0");

    assert!(res.visible_indices.is_empty());
    assert_eq!(res.total_matched_lines, 0);
    assert_eq!(res.filter_counts, vec![2, 2, 4]);
    assert_eq!(res.count_for_rule(&r_err.id), 2);
    assert_eq!(res.count_for_rule(&r_warn.id), 2);
    assert_eq!(res.count_for_rule(&r_drop.id), 4);
    assert_eq!(res.filter_counts, full_0.filter_counts);
    assert_eq!(res.rule_counts, full_0.rule_counts);
    assert_eq!(res.excluded_filter_matches, full_0.excluded_filter_matches);

    // Partial trim of 2 lines when visible_indices is empty: lines 2 and 3 survive shifted to 0 and 1
    let mut partial_1 = StreamLogBuffer::new(2, 1);
    partial_1.append_chunk(initial_lines.clone());
    assert_eq!(partial_1.trim_if_needed(), Some(2));
    compiled.append_streaming_batch(&mut res, &partial_1, 2..2, 2, true);
    let full_1 = execute_filtering(&partial_1, &rules, true, None, 0).expect("full 1");

    assert!(res.visible_indices.is_empty());
    assert_eq!(res.total_matched_lines, 0);
    assert_eq!(res.filter_counts, vec![1, 1, 2]);
    assert_eq!(res.count_for_rule(&r_err.id), 1);
    assert_eq!(res.count_for_rule(&r_warn.id), 1);
    assert_eq!(res.count_for_rule(&r_drop.id), 2);
    assert_eq!(res.filter_counts, full_1.filter_counts);
    assert_eq!(res.rule_counts, full_1.rule_counts);
    assert_eq!(res.excluded_filter_matches, full_1.excluded_filter_matches);

    // Second partial trim of 1 line: only "DROP four" survives at index 0
    let mut partial_2 = StreamLogBuffer::new(1, 1);
    partial_2.append_chunk(vec![
        "ERROR WARN DROP three".to_string(),
        "DROP four".to_string(),
    ]);
    assert_eq!(partial_2.trim_if_needed(), Some(1));
    compiled.append_streaming_batch(&mut res, &partial_2, 1..1, 1, true);
    let full_2 = execute_filtering(&partial_2, &rules, true, None, 0).expect("full 2");

    assert!(res.visible_indices.is_empty());
    assert_eq!(res.total_matched_lines, 0);
    assert_eq!(res.filter_counts, vec![0, 0, 1]);
    assert_eq!(res.count_for_rule(&r_err.id), 0);
    assert_eq!(res.count_for_rule(&r_warn.id), 0);
    assert_eq!(res.count_for_rule(&r_drop.id), 1);
    assert_eq!(res.filter_counts, full_2.filter_counts);
    assert_eq!(res.rule_counts, full_2.rule_counts);
    assert_eq!(res.excluded_filter_matches, full_2.excluded_filter_matches);

    // Final trim draining all remaining excluded lines to 0
    partial_2.clear();
    compiled.append_streaming_batch(&mut res, &partial_2, 0..0, 1, true);
    let full_3 = execute_filtering(&partial_2, &rules, true, None, 0).expect("full 3");

    assert!(res.visible_indices.is_empty());
    assert_eq!(res.total_matched_lines, 0);
    assert_eq!(res.filter_counts, vec![0, 0, 0]);
    assert_eq!(res.count_for_rule(&r_err.id), 0);
    assert_eq!(res.count_for_rule(&r_warn.id), 0);
    assert_eq!(res.count_for_rule(&r_drop.id), 0);
    assert_eq!(res.rule_counts, full_3.rule_counts);
    assert!(res.excluded_filter_matches.is_empty());
}

#[test]
fn test_excluded_line_partial_trim_and_shift_parity() {
    let r_alpha = FilterRule {
        text: "ALPHA".to_string(),
        active: true,
        ..Default::default()
    };
    let r_beta = FilterRule {
        text: "BETA".to_string(),
        active: true,
        ..Default::default()
    };
    let r_excl = FilterRule {
        text: "EXCLUDE".to_string(),
        exclude: true,
        active: true,
        ..Default::default()
    };
    let rules = vec![r_alpha.clone(), r_beta.clone(), r_excl.clone()];
    let compiled = CompiledStreamFilters::new(&rules);

    for show_only_filtered in [true, false] {
        let mut stream = StreamLogBuffer::new(5, 1);
        let mut compiled_res = FilterResult::default();
        let mut standalone_res = FilterResult::default();

        // Phase 1: Interleaved unmatched, excluded, and visible lines (no trim)
        stream.append_chunk(vec![
            "NOISE zero".to_string(),
            "ALPHA EXCLUDE one".to_string(),
            "ALPHA visible two".to_string(),
            "BETA EXCLUDE three".to_string(),
            "ALPHA BETA EXCLUDE four".to_string(),
        ]);
        compiled.append_streaming_batch(&mut compiled_res, &stream, 0..5, 0, show_only_filtered);
        append_streaming_batch(
            &mut standalone_res,
            &stream,
            0..5,
            0,
            &rules,
            show_only_filtered,
        );
        let full_1 = execute_filtering(&stream, &rules, show_only_filtered, None, 0).expect("full 1");
        assert_eq!(compiled_res.filter_counts, full_1.filter_counts);
        assert_eq!(compiled_res.rule_counts, full_1.rule_counts);
        assert_eq!(compiled_res.excluded_filter_matches, full_1.excluded_filter_matches);
        assert_eq!(compiled_res.visible_indices, full_1.visible_indices);
        assert_eq!(compiled_res.total_matched_lines, full_1.total_matched_lines);
        assert_eq!(standalone_res.filter_counts, full_1.filter_counts);
        assert_eq!(
            standalone_res.excluded_filter_matches,
            full_1.excluded_filter_matches
        );

        // Phase 2: Rolling append + trim dropping idx 0 ("NOISE") and idx 1 ("ALPHA EXCLUDE")
        stream.append_chunk(vec![
            "ALPHA visible five".to_string(),
            "BETA EXCLUDE six".to_string(),
        ]);
        let trimmed_2 = stream.trim_if_needed().expect("phase 2 trim");
        assert_eq!(trimmed_2, 2);
        compiled.append_streaming_batch(&mut compiled_res, &stream, 3..5, trimmed_2, show_only_filtered);
        append_streaming_batch(
            &mut standalone_res,
            &stream,
            3..5,
            trimmed_2,
            &rules,
            show_only_filtered,
        );
        let full_2 = execute_filtering(&stream, &rules, show_only_filtered, None, 0).expect("full 2");
        assert_eq!(compiled_res.filter_counts, full_2.filter_counts);
        assert_eq!(compiled_res.rule_counts, full_2.rule_counts);
        assert_eq!(compiled_res.excluded_filter_matches, full_2.excluded_filter_matches);
        assert_eq!(compiled_res.visible_indices, full_2.visible_indices);
        assert_eq!(compiled_res.total_matched_lines, full_2.total_matched_lines);
        assert_eq!(standalone_res.filter_counts, full_2.filter_counts);
        assert_eq!(
            standalone_res.excluded_filter_matches,
            full_2.excluded_filter_matches
        );

        // Phase 3: Consecutive rolling append + trim dropping shifted visible (0) and excluded (1),
        // shifting "ALPHA BETA EXCLUDE four" from idx 2 -> 0 and "BETA EXCLUDE six" from idx 4 -> 2
        stream.append_chunk(vec![
            "NOISE seven".to_string(),
            "ALPHA EXCLUDE eight".to_string(),
        ]);
        let trimmed_3 = stream.trim_if_needed().expect("phase 3 trim");
        assert_eq!(trimmed_3, 2);
        compiled.append_streaming_batch(&mut compiled_res, &stream, 3..5, trimmed_3, show_only_filtered);
        append_streaming_batch(
            &mut standalone_res,
            &stream,
            3..5,
            trimmed_3,
            &rules,
            show_only_filtered,
        );
        let full_3 = execute_filtering(&stream, &rules, show_only_filtered, None, 0).expect("full 3");
        assert_eq!(compiled_res.filter_counts, full_3.filter_counts);
        assert_eq!(compiled_res.rule_counts, full_3.rule_counts);
        assert_eq!(compiled_res.excluded_filter_matches, full_3.excluded_filter_matches);
        assert_eq!(compiled_res.visible_indices, full_3.visible_indices);
        assert_eq!(compiled_res.total_matched_lines, full_3.total_matched_lines);
        assert_eq!(standalone_res.filter_counts, full_3.filter_counts);
        assert_eq!(
            standalone_res.excluded_filter_matches,
            full_3.excluded_filter_matches
        );

        // Phase 4: Trim-only keeping last 2 lines (one NOISE, one excluded) so visible_indices becomes empty
        let mut tail_stream = StreamLogBuffer::new(2, 1);
        let all_remaining: Vec<String> = (0..stream.len())
            .filter_map(|i| stream.get_line(i).map(|s| s.into_owned()))
            .collect();
        tail_stream.append_chunk(all_remaining);
        let trimmed_4 = tail_stream.trim_if_needed().expect("phase 4 trim");
        assert_eq!(trimmed_4, 3);
        compiled.append_streaming_batch(&mut compiled_res, &tail_stream, 2..2, trimmed_4, show_only_filtered);
        let full_4 =
            execute_filtering(&tail_stream, &rules, show_only_filtered, None, 0).expect("full 4");
        assert_eq!(compiled_res.filter_counts, full_4.filter_counts);
        assert_eq!(compiled_res.rule_counts, full_4.rule_counts);
        assert_eq!(compiled_res.excluded_filter_matches, full_4.excluded_filter_matches);
        assert_eq!(compiled_res.visible_indices, full_4.visible_indices);
        assert_eq!(compiled_res.total_matched_lines, full_4.total_matched_lines);

        // Phase 5: Drain remaining tail to 0
        tail_stream.clear();
        compiled.append_streaming_batch(&mut compiled_res, &tail_stream, 0..0, 2, show_only_filtered);
        let full_5 =
            execute_filtering(&tail_stream, &rules, show_only_filtered, None, 0).expect("full 5");
        assert_eq!(compiled_res.filter_counts, vec![0, 0, 0]);
        assert_eq!(compiled_res.rule_counts, full_5.rule_counts);
        assert!(compiled_res.excluded_filter_matches.is_empty());
        assert!(compiled_res.visible_indices.is_empty());
        assert_eq!(compiled_res.total_matched_lines, 0);
    }
}

#[test]
fn test_execute_filtering_cjk_display_width_and_streaming_trim_shrink() {
    // 1. execute_filtering selects visually widest line (not byte-longest or trailing-whitespace-padded)
    let cjk_24_cols = "한".repeat(12); // 36 bytes, display width 24
    let ascii_26_cols = "a".repeat(26); // 26 bytes, display width 26 (visually widest!)
    let padded_5_cols = format!("short{}", " ".repeat(60)); // 65 bytes, display width 5

    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "{cjk_24_cols}").unwrap();
    writeln!(tmp, "{ascii_26_cols}").unwrap();
    writeln!(tmp, "{padded_5_cols}").unwrap();
    tmp.flush().unwrap();

    let file_buf = FileLogBuffer::open(tmp.path()).unwrap();
    let res = execute_filtering(&file_buf, &[], true, None, 0).unwrap();
    assert_eq!(res.widest_visible_length, 26);
    assert_eq!(res.widest_visible_text, ascii_26_cols);
    assert_eq!(res.widest_visible_source_idx, Some(1));

    // When a CJK line IS visually wider (15 Hangul chars = 45 bytes, width 30 vs 26 ASCII), it wins
    let cjk_30_cols = "한".repeat(15);
    let mut stream_cjk = StreamLogBuffer::new(10, 1);
    stream_cjk.append_chunk(vec![ascii_26_cols.clone(), cjk_30_cols.clone()]);
    let res_cjk = execute_filtering(&stream_cjk, &[], true, None, 0).unwrap();
    assert_eq!(res_cjk.widest_visible_length, 30);
    assert_eq!(res_cjk.widest_visible_text, cjk_30_cols);
    assert_eq!(res_cjk.widest_visible_source_idx, Some(1));

    // 2. append_streaming_batch_with_matcher shrinks widest_visible_length when widest visible row is trimmed
    let wide_40 = "W".repeat(40);
    let med_cjk_20 = "한".repeat(10); // 30 bytes, width 20
    let narrow_8 = "N".repeat(8);
    let small_10 = "S".repeat(10);

    let rules = vec![FilterRule {
        text: "NO_EXCLUDE".to_string(),
        active: false,
        ..Default::default()
    }];
    let prepared: Vec<PreparedFilter> = Vec::new();
    let multi = MultiFilterMatcher::new(&prepared);

    let mut stream = StreamLogBuffer::new(3, 1);
    stream.append_chunk(vec![wide_40.clone(), med_cjk_20.clone(), narrow_8.clone()]);

    let mut inc = FilterResult::default();
    append_streaming_batch_with_matcher(&mut inc, &stream, 0..3, 0, &prepared, &multi, false);
    assert_eq!(inc.widest_visible_length, 40);
    assert_eq!(inc.widest_visible_text, wide_40);
    assert_eq!(inc.widest_visible_source_idx, Some(0));

    // Append small_10 (total 4 > cap 3 -> trims idx 0 = wide_40): widest must shrink from 40 to 20
    stream.append_chunk(vec![small_10.clone()]);
    let trimmed_1 = stream.trim_if_needed().unwrap();
    assert_eq!(trimmed_1, 1);
    append_streaming_batch_with_matcher(&mut inc, &stream, 2..3, trimmed_1, &prepared, &multi, false);
    assert_eq!(inc.widest_visible_length, 20);
    assert_eq!(inc.widest_visible_text, med_cjk_20);
    assert_eq!(inc.widest_visible_source_idx, Some(0));

    let full_after_1 = execute_filtering(&stream, &rules, false, None, 0).unwrap();
    assert_eq!(inc.widest_visible_length, full_after_1.widest_visible_length);
    assert_eq!(inc.widest_visible_text, full_after_1.widest_visible_text);

    // Trim med_cjk_20 (no append): widest shrinks from 20 to 10 (small_10 at surviving idx 1)
    let mut stream_2 = StreamLogBuffer::new(2, 1);
    stream_2.append_chunk(vec![med_cjk_20, narrow_8, small_10.clone()]);
    let trimmed_2 = stream_2.trim_if_needed().unwrap();
    assert_eq!(trimmed_2, 1);
    append_streaming_batch_with_matcher(&mut inc, &stream_2, 2..2, trimmed_2, &prepared, &multi, false);
    assert_eq!(inc.widest_visible_length, 10);
    assert_eq!(inc.widest_visible_text, small_10);
    assert_eq!(inc.widest_visible_source_idx, Some(1));

    // Trim all remaining rows: resets widest to 0 / empty / None
    stream_2.clear();
    append_streaming_batch_with_matcher(&mut inc, &stream_2, 0..0, 2, &prepared, &multi, false);
    assert_eq!(inc.widest_visible_length, 0);
    assert!(inc.widest_visible_text.is_empty());
    assert_eq!(inc.widest_visible_source_idx, None);
}

#[test]
fn test_execute_filtering_and_streaming_non_ascii_case_insensitive_parity() {
    let rules = vec![
        FilterRule {
            text: "café".to_string(),
            case_sensitive: false,
            regex: false,
            bg_color: "Yellow".to_string(),
            active: true,
            ..Default::default()
        },
        FilterRule {
            text: "über".to_string(),
            case_sensitive: false,
            regex: false,
            bg_color: "Cyan".to_string(),
            active: true,
            ..Default::default()
        },
    ];
    let compiled = CompiledStreamFilters::new(&rules);
    let prepared: Vec<PreparedFilter> = rules
        .iter()
        .enumerate()
        .map(|(i, r)| PreparedFilter::new(r.clone(), i).unwrap())
        .collect();

    let lines = vec![
        "ERROR: CAFÉ timeout".to_string(),
        "INFO: normal line".to_string(),
        "WARN: ÜBER connected".to_string(),
    ];
    let mut stream = StreamLogBuffer::new(10, 1);
    stream.append_chunk(lines.clone());

    let full = execute_filtering(&stream, &rules, true, None, 0).unwrap();
    assert_eq!(full.visible_indices, vec![0, 2]);
    assert_eq!(full.total_matched_lines, 2);
    assert_eq!(full.count_for_rule(&rules[0].id), 1);
    assert_eq!(full.count_for_rule(&rules[1].id), 1);

    // Must match line-by-line evaluate_and_style_line identically
    for (idx, line) in lines.iter().enumerate() {
        let single = evaluate_and_style_line(line.as_bytes(), &prepared, true, None);
        if single.0 {
            assert!(full.visible_indices.contains(&idx));
        } else {
            assert!(!full.visible_indices.contains(&idx));
        }
    }

    // Streaming incremental parity
    let mut inc = FilterResult::default();
    compiled.append_streaming_batch(&mut inc, &stream, 0..3, 0, true);
    assert_eq!(inc.visible_indices, full.visible_indices);
    assert_eq!(inc.visible_styles, full.visible_styles);
    assert_eq!(inc.rule_counts, full.rule_counts);
}

#[test]
fn test_include_after_exclude_preserves_partial_style_and_blocks_prior_inheritance() {
    let raw_line = b"E/Auth: Session token expired";

    // Case 1: FG-only subsequent include after an exclude barrier
    let rules_fg_only = vec![
        PreparedFilter::new(
            FilterRule {
                text: "Auth".to_string(),
                bg_color: "Yellow".to_string(),
                text_color: "Blue".to_string(),
                active: true,
                ..Default::default()
            },
            0,
        )
        .unwrap(),
        PreparedFilter::new(
            FilterRule {
                text: "token".to_string(),
                exclude: true,
                active: true,
                ..Default::default()
            },
            1,
        )
        .unwrap(),
        PreparedFilter::new(
            FilterRule {
                text: "expired".to_string(),
                bg_color: "None".to_string(),
                text_color: "Red".to_string(),
                active: true,
                ..Default::default()
            },
            2,
        )
        .unwrap(),
    ];

    let (visible, style, is_matched) =
        evaluate_and_style_line(raw_line, &rules_fg_only, true, None);
    assert!(visible);
    assert!(is_matched);
    assert_eq!(style.fg_hex, parse_color_spec("Red", false));
    assert_eq!(
        style.bg_hex, None,
        "FG-only include after exclude must not inherit Yellow BG across the exclude barrier"
    );

    // Case 2: BG-only subsequent include after an exclude barrier
    let rules_bg_only = vec![
        PreparedFilter::new(
            FilterRule {
                text: "Auth".to_string(),
                bg_color: "Yellow".to_string(),
                text_color: "Blue".to_string(),
                active: true,
                ..Default::default()
            },
            0,
        )
        .unwrap(),
        PreparedFilter::new(
            FilterRule {
                text: "token".to_string(),
                exclude: true,
                active: true,
                ..Default::default()
            },
            1,
        )
        .unwrap(),
        PreparedFilter::new(
            FilterRule {
                text: "expired".to_string(),
                bg_color: "Cyan".to_string(),
                text_color: "None".to_string(),
                active: true,
                ..Default::default()
            },
            2,
        )
        .unwrap(),
    ];

    let (visible, style, is_matched) =
        evaluate_and_style_line(raw_line, &rules_bg_only, true, None);
    assert!(visible);
    assert!(is_matched);
    assert_eq!(style.bg_hex, parse_color_spec("Cyan", true));
    assert_eq!(
        style.fg_hex, None,
        "BG-only include after exclude must not inherit Blue FG across the exclude barrier"
    );
}

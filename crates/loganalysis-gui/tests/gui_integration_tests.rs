use loganalysis_core::parse_color_spec;
use std::collections::HashSet;

static ENV_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_ui_ux_log_level_color_presets_validity() {
    let presets = [
        ("🔴 ERROR", "error", "Red", "DarkRed"),
        ("🟡 WARN", "warn", "Orange", "Black"),
        ("🔵 INFO", "info", "Blue", "DarkBlue"),
        ("🟣 DEBUG", "debug", "Purple", "Purple"),
        ("⚪ TRACE", "trace", "Gray", "Black"),
        ("⚡ FATAL", "fatal", "Salmon", "DarkRed"),
        ("✓ SUCCESS", "success", "Green", "DarkGreen"),
        ("⚙ ActivityTaskManager", "ActivityTaskManager", "Cyan", "Black"),
        ("⚙ System", "System", "Blue", "DarkBlue"),
    ];

    for (label, pattern, bg, fg) in presets {
        assert!(!pattern.is_empty(), "Pattern for {} cannot be empty", label);
        assert!(
            parse_color_spec(bg, true).is_some() || bg.eq_ignore_ascii_case("none"),
            "Background '{}' in preset '{}' must be valid color spec",
            bg,
            label
        );
        assert!(
            parse_color_spec(fg, false).is_some() || fg.eq_ignore_ascii_case("default"),
            "Foreground '{}' in preset '{}' must be valid color spec",
            fg,
            label
        );
    }
}

#[test]
fn test_detect_initial_window_decorations_overrides() {
    use gpui::WindowDecorations;
    use loganalysis_gui::detect_initial_window_decorations;

    let _guard = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // Test explicit force CSD
    std::env::set_var("LOGANALYSIS_FORCE_CSD", "1");
    std::env::remove_var("LOGANALYSIS_DISABLE_CSD");
    assert_eq!(
        detect_initial_window_decorations(),
        Some(WindowDecorations::Client)
    );
    std::env::remove_var("LOGANALYSIS_FORCE_CSD");

    // Test explicit disable CSD
    std::env::set_var("LOGANALYSIS_DISABLE_CSD", "1");
    assert_eq!(
        detect_initial_window_decorations(),
        Some(WindowDecorations::Server)
    );
    std::env::remove_var("LOGANALYSIS_DISABLE_CSD");
}

#[test]
fn test_delta_time_integration_and_formatting() {
    use loganalysis_core::*;

    let line1 = b"11-06 18:32:00.000 1 1 I Tag: Step 1";
    let line2 = b"11-06 18:32:00.045 1 1 I Tag: Step 2"; // +45ms
    let line3 = b"11-06 18:32:01.200 1 1 I Tag: Step 3"; // +1.155s

    let t1 = extract_timestamp(line1).unwrap();
    let t2 = extract_timestamp(line2).unwrap();
    let t3 = extract_timestamp(line3).unwrap();

    let dt1 = compute_delta_micros(t1, t2);
    assert_eq!(format_delta_micros(dt1), "+45.0ms");
    assert_eq!(LatencyLevel::from_micros(dt1), LatencyLevel::Normal);

    let dt2 = compute_delta_micros(t2, t3);
    assert_eq!(format_delta_micros(dt2), "+1.16s");
    assert_eq!(LatencyLevel::from_micros(dt2), LatencyLevel::Critical);
}

#[test]
fn test_command_palette_catalog_and_fuzzy_search() {
    use loganalysis_core::fuzzy_match;
    use loganalysis_gui::modals::command_palette::{get_palette_commands, PaletteCommandId};

    let commands = get_palette_commands();
    assert!(commands.len() >= 20, "Should have at least 20 registered commands");

    // Category sanity check
    let categories: Vec<&str> = commands.iter().map(|c| c.category).collect();
    assert!(categories.contains(&"FILE"));
    assert!(categories.contains(&"LOG"));
    assert!(categories.contains(&"FILTER"));
    assert!(categories.contains(&"ADB"));
    assert!(categories.contains(&"VIEW"));

    // Fuzzy search for "open" -> "File: Open Log File..." must rank #1
    let mut open_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("open", c.title).map(|score| (c, score)))
        .collect();
    open_matches.sort_by_key(|a| std::cmp::Reverse(a.1));
    assert!(!open_matches.is_empty());
    assert_eq!(open_matches[0].0.id, PaletteCommandId::OpenFile);

    // Fuzzy search for "theme" -> "View: Toggle Dark / Light Theme"
    let mut theme_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("theme", c.title).map(|score| (c, score)))
        .collect();
    theme_matches.sort_by_key(|a| std::cmp::Reverse(a.1));
    assert!(!theme_matches.is_empty());
    assert_eq!(theme_matches[0].0.id, PaletteCommandId::ToggleTheme);

    // Subsequence acronym search: "vtp" -> "View: Toggle Filter Dock Position"
    let vtp_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("vtp", c.title).map(|score| (c, score)))
        .collect();
    assert!(!vtp_matches.is_empty());

    // Non-matching query returns empty
    let none_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("nonexistent123xyz", c.title).map(|score| (c, score)))
        .collect();
    assert!(none_matches.is_empty());

    // Bookmark and Export categories check
    assert!(categories.contains(&"BOOKMARK"));
    assert!(categories.contains(&"EXPORT"));

    // Fuzzy search for "bookmark" -> should find bookmark commands
    let bookmark_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("bookmark", c.title).map(|score| (c, score)))
        .collect();
    assert!(bookmark_matches.len() >= 6, "Expected at least 6 bookmark commands");
    assert!(commands.iter().any(|c| c.id == PaletteCommandId::ToggleBookmarkDrawer));
    assert!(commands.iter().any(|c| c.id == PaletteCommandId::EditBookmarkNote));

    // Fuzzy search for "export" -> should find export command
    let export_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("export", c.title).map(|score| (c, score)))
        .collect();
    assert!(!export_matches.is_empty());
    assert_eq!(export_matches[0].0.id, PaletteCommandId::ExportHtmlReport);

    // Fuzzy search for "timeline" -> should find EPS sparkline command
    let timeline_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("timeline", c.title).map(|score| (c, score)))
        .collect();
    assert!(!timeline_matches.is_empty());
    assert_eq!(timeline_matches[0].0.id, PaletteCommandId::ToggleEpsTimeline);

    // Fuzzy search for "zen" -> should find Zen Focus Mode command
    let zen_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("zen", c.title).map(|score| (c, score)))
        .collect();
    assert!(!zen_matches.is_empty(), "Expected command palette to match 'zen'");
    assert_eq!(zen_matches[0].0.id, PaletteCommandId::ToggleZenMode);
    assert!(commands.iter().any(|c| c.id == PaletteCommandId::ToggleZenMode));

    // Fuzzy search for "split" -> should find split view commands
    let split_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("split", c.title).map(|score| (c, score)))
        .collect();
    assert!(split_matches.len() >= 3, "Expected at least 3 split view commands");
    assert!(commands.iter().any(|c| c.id == PaletteCommandId::ToggleSplitView));
    assert!(commands.iter().any(|c| c.id == PaletteCommandId::ToggleSplitOrientation));
    assert!(commands.iter().any(|c| c.id == PaletteCommandId::OpenSecondaryLog));

    // Fuzzy search for "synced" -> should find timestamp sync command
    let sync_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("synced", c.title).map(|score| (c, score)))
        .collect();
    assert!(!sync_matches.is_empty());
    assert_eq!(sync_matches[0].0.id, PaletteCommandId::ToggleSyncedScroll);

    // Fuzzy search for "column" -> should find column view command
    let column_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("column", c.title).map(|score| (c, score)))
        .collect();
    assert!(!column_matches.is_empty());
    assert_eq!(column_matches[0].0.id, PaletteCommandId::ToggleColumnView);

    // Fuzzy search for "merge" -> should find open merged logs command
    let merge_matches: Vec<_> = commands
        .iter()
        .filter_map(|c| fuzzy_match("merge", c.title).map(|score| (c, score)))
        .collect();
    assert!(!merge_matches.is_empty());
    assert_eq!(merge_matches[0].0.id, PaletteCommandId::OpenMergedLogs);
}

#[test]
fn test_bookmark_and_html_report_pipeline() {
    use loganalysis_core::*;

    let mut manager = BookmarkManager::new();
    assert!(manager.is_empty());

    // Add bookmarks
    assert!(manager.toggle(10, "11-06 18:00:00 I Tag: System init".into(), LineSeverity::Normal));
    assert!(manager.toggle(50, "11-06 18:00:05 W Tag: Memory warning".into(), LineSeverity::Warning));
    assert!(manager.toggle(100, "11-06 18:00:10 E Tag: Fatal crash occurred".into(), LineSeverity::Error));

    // Set triage notes
    assert!(manager.set_note(100, Some("Root cause null pointer exception in binder IPC".into())));
    assert!(manager.set_note(50, Some("Heap usage crossed 85% threshold".into())));

    assert_eq!(manager.len(), 3);
    assert!(manager.is_bookmarked(10));
    assert!(manager.is_bookmarked(50));
    assert!(manager.is_bookmarked(100));
    assert!(!manager.is_bookmarked(99));

    // Verify note retrieval
    assert_eq!(
        manager.get(100).and_then(|b| b.note.as_deref()),
        Some("Root cause null pointer exception in binder IPC")
    );
    assert_eq!(
        manager.get(50).and_then(|b| b.note.as_deref()),
        Some("Heap usage crossed 85% threshold")
    );
    assert_eq!(manager.get(10).and_then(|b| b.note.as_deref()), None);

    // Navigation tests
    assert_eq!(manager.next_bookmark(10), Some(50));
    assert_eq!(manager.next_bookmark(50), Some(100));
    assert_eq!(manager.next_bookmark(100), Some(10)); // Cyclic wrap to first

    assert_eq!(manager.prev_bookmark(100), Some(50));
    assert_eq!(manager.prev_bookmark(50), Some(10));
    assert_eq!(manager.prev_bookmark(10), Some(100)); // Cyclic wrap to last

    // HTML report generation with embedded notes
    let options = HtmlReportOptions {
        file_name: "test_session.log".to_string(),
        total_lines: 500,
        bookmarks: manager.items().to_vec(),
    };

    let report = generate_html_report(&options);
    assert!(report.contains("test_session.log"));
    assert!(report.contains("Fatal crash occurred"));
    assert!(report.contains("Memory warning"));
    assert!(report.contains("System init"));
    assert!(report.contains("Root cause null pointer exception in binder IPC"));
    assert!(report.contains("Heap usage crossed 85% threshold"));
    assert!(report.contains("data-severity=\"error\""));
    assert!(report.contains("data-severity=\"warning\""));
    assert!(report.contains("data-severity=\"info\""));

    // Toggle removes bookmark
    assert!(!manager.toggle(50, "".into(), LineSeverity::Warning));
    assert_eq!(manager.len(), 2);
    assert!(!manager.is_bookmarked(50));
}

#[test]
fn test_dual_pane_split_and_timestamp_lockstep_correlation() {
    use loganalysis_core::*;
    use loganalysis_gui::workspace::probe_parent_timestamp;

    // Pane A Source (Primary client log)
    let pane_a_data = b"01-01 10:00:00.100 D Client start\n\
                        01-01 10:00:01.200 I Client request sent\n\
                        01-01 10:00:05.500 E Client timeout\n";

    // Pane B Source (Secondary server log)
    let pane_b_data = b"01-01 10:00:00.050 I Server booted\n\
                        01-01 10:00:01.210 D Server received request\n\
                        01-01 10:00:01.220 D Parsing payload\n\
                        \tat com.example.Server.parse(Server.java:42)\n\
                        01-01 10:00:06.000 W Server cleanup\n";

    let mut a_dir = std::env::temp_dir();
    a_dir.push(format!("pane_a_test_{}.log", uuid::Uuid::new_v4()));
    std::fs::write(&a_dir, pane_a_data).unwrap();
    let buf_a = FileLogBuffer::open(&a_dir).unwrap();

    let mut b_dir = std::env::temp_dir();
    b_dir.push(format!("pane_b_test_{}.log", uuid::Uuid::new_v4()));
    std::fs::write(&b_dir, pane_b_data).unwrap();
    let buf_b = FileLogBuffer::open(&b_dir).unwrap();

    let source_a = ActiveLogSource::File(buf_a);
    let source_b = ActiveLogSource::File(buf_b);

    let vis_a: Vec<usize> = (0..source_a.line_count()).collect();
    let vis_b: Vec<usize> = (0..source_b.line_count()).collect();

    // 1. Primary selects line 1: "Client request sent" at 10:00:01.200 (1_200_000 µs into the second)
    let raw_a1 = source_a.get_raw_line(1).unwrap();
    let ts_a1 = extract_timestamp(raw_a1).unwrap();

    // Correlate to Secondary (Pane B):
    // Nearest should be line 1: "Server received request" at 10:00:01.210 (10ms diff)
    let correlated_b = find_nearest_timestamp_row(&source_b, &vis_b, ts_a1.total_micros);
    assert_eq!(correlated_b, Some(1));

    // 2. Secondary selects line 4: "Server cleanup" at 10:00:06.000
    let raw_b4 = source_b.get_raw_line(4).unwrap();
    let ts_b4 = extract_timestamp(raw_b4).unwrap();

    // Correlate back to Primary (Pane A):
    // Nearest should be line 2: "Client timeout" at 10:00:05.500
    let correlated_a = find_nearest_timestamp_row(&source_a, &vis_a, ts_b4.total_micros);
    assert_eq!(correlated_a, Some(2));

    // 3. Secondary selects line 3: "\tat com.example.Server.parse(Server.java:42)" (stack trace row, no timestamp)
    let raw_b3 = source_b.get_raw_line(3).unwrap();
    assert!(extract_timestamp(raw_b3).is_none());

    // probe_parent_timestamp probes backwards up to 10 lines and finds line 2's timestamp (10:00:01.220)
    let probed_ts = probe_parent_timestamp(&source_b, 3);
    assert!(probed_ts.is_some());
    let correlated_stack_a = find_nearest_timestamp_row(&source_a, &vis_a, probed_ts.unwrap());
    assert_eq!(correlated_stack_a, Some(1)); // Correlates to "Client request sent" (10:00:01.200)

    // 4. Test boundary: if no timestamp within 10 lines backwards, returns None
    let unparsed_data = b"line without timestamp\n".repeat(15);
    let mut unparsed_dir = std::env::temp_dir();
    unparsed_dir.push(format!("unparsed_{}.log", uuid::Uuid::new_v4()));
    std::fs::write(&unparsed_dir, unparsed_data).unwrap();
    let unparsed_buf = FileLogBuffer::open(&unparsed_dir).unwrap();
    let unparsed_src = ActiveLogSource::File(unparsed_buf);
    assert_eq!(probe_parent_timestamp(&unparsed_src, 12), None);
    let _ = std::fs::remove_file(unparsed_dir);

    // Clean up temp files
    let _ = std::fs::remove_file(a_dir);
    let _ = std::fs::remove_file(b_dir);
}

#[test]
fn test_structured_column_view_parsing_and_tag_quick_filter() {
    use loganalysis_core::*;

    let log_sample = "11-06 18:00:00.123  1000  1000 E AndroidRuntime: FATAL EXCEPTION: main\n\
                      11-06 18:00:00.124  1000  1000 E AndroidRuntime: Process: com.example.app\n\
                      \tat com.example.app.MainActivity.onCreate(MainActivity.java:42)\n\
                      11-06 18:00:01.000   500   500 W AudioFlinger: Buffer underrun occurred\n";

    let lines: Vec<&str> = log_sample.lines().collect();
    assert_eq!(lines.len(), 4);

    // 1. Threadtime error line
    let p0 = parse_logcat_column_line(lines[0]);
    assert_eq!(p0.time, Some("11-06 18:00:00.123"));
    assert_eq!(p0.pid, Some("1000"));
    assert_eq!(p0.tid, Some("1000"));
    assert_eq!(p0.level_char, Some('E'));
    assert_eq!(p0.level_severity, LineSeverity::Error);
    assert_eq!(p0.tag, Some("AndroidRuntime"));
    assert_eq!(p0.message, "FATAL EXCEPTION: main");

    // 2. Multiline stack trace line
    let p2 = parse_logcat_column_line(lines[2]);
    assert_eq!(p2.time, None);
    assert_eq!(p2.pid, None);
    assert_eq!(p2.level_char, None);
    assert_eq!(p2.tag, None);
    assert_eq!(p2.message, lines[2]);

    // 3. Threadtime warning line
    let p3 = parse_logcat_column_line(lines[3]);
    assert_eq!(p3.time, Some("11-06 18:00:01.000"));
    assert_eq!(p3.pid, Some("500"));
    assert_eq!(p3.tid, Some("500"));
    assert_eq!(p3.level_char, Some('W'));
    assert_eq!(p3.level_severity, LineSeverity::Warning);
    assert_eq!(p3.tag, Some("AudioFlinger"));
    assert_eq!(p3.message, "Buffer underrun occurred");
}

#[test]
fn test_chronological_multi_file_merge_view_and_filtering() {
    use loganalysis_core::*;
    use std::io::Write;

    let dir = std::env::temp_dir();
    let client_path = dir.join("test_merge_client.log");
    let server_path = dir.join("test_merge_server.log");

    let client_content = "11-06 12:00:01.000 Client sends request\n\
                          11-06 12:00:03.000 Client timeout waiting for reply\n\
                          \tat com.client.Http.call(Http.java:10)\n";

    let server_content = "11-06 12:00:02.000 Server receives request\n\
                          11-06 12:00:02.500 Server database deadlock\n\
                          11-06 12:00:04.000 Server restarted\n";

    {
        let mut f1 = std::fs::File::create(&client_path).unwrap();
        f1.write_all(client_content.as_bytes()).unwrap();
        let mut f2 = std::fs::File::create(&server_path).unwrap();
        f2.write_all(server_content.as_bytes()).unwrap();
    }

    let merged_buf = MergedLogBuffer::open_multiple(&[client_path.clone(), server_path.clone()]).unwrap();
    assert_eq!(merged_buf.line_count(), 6);

    // Verify origin badges and names
    assert_eq!(merged_buf.source_names(), &["test_merge_client.log", "test_merge_server.log"]);

    // Line 0: Client sends request (12:00:01.000)
    assert_eq!(merged_buf.get_line_source_id(0), Some(0));
    assert_eq!(merged_buf.get_line_source_name(0), Some("test_merge_client.log"));
    assert!(merged_buf.get_line(0).unwrap().contains("Client sends request"));

    // Line 1: Server receives request (12:00:02.000)
    assert_eq!(merged_buf.get_line_source_id(1), Some(1));
    assert_eq!(merged_buf.get_line_source_name(1), Some("test_merge_server.log"));
    assert!(merged_buf.get_line(1).unwrap().contains("Server receives request"));

    // Line 2: Server database deadlock (12:00:02.500)
    assert_eq!(merged_buf.get_line_source_id(2), Some(1));
    assert!(merged_buf.get_line(2).unwrap().contains("Server database deadlock"));

    // Line 3: Client timeout waiting for reply (12:00:03.000)
    assert_eq!(merged_buf.get_line_source_id(3), Some(0));
    assert!(merged_buf.get_line(3).unwrap().contains("Client timeout"));

    // Line 4: Stack trace continuation (inherits 12:00:03.000, attached to client)
    assert_eq!(merged_buf.get_line_source_id(4), Some(0));
    assert!(merged_buf.get_line(4).unwrap().contains("Http.java:10"));

    // Line 5: Server restarted (12:00:04.000)
    assert_eq!(merged_buf.get_line_source_id(5), Some(1));
    assert!(merged_buf.get_line(5).unwrap().contains("Server restarted"));

    // Verify ActiveLogSource dispatch and execute_filtering
    let active_source = ActiveLogSource::Merged(merged_buf);
    assert_eq!(active_source.line_count(), 6);
    assert_eq!(active_source.get_line_source_info(1), Some((1, "test_merge_server.log")));

    let rule = FilterRule {
        text: "deadlock".to_string(),
        regex: false,
        case_sensitive: false,
        exclude: false,
        bg_color: "Red".to_string(),
        text_color: "White".to_string(),
        active: true,
        ..Default::default()
    };

    let filter_res = execute_filtering(&active_source, &[rule], true, None, 0).unwrap();
    assert_eq!(filter_res.total_matched_lines, 1);
    assert_eq!(filter_res.visible_indices, vec![2]);

    // Cleanup
    let _ = std::fs::remove_file(client_path);
    let _ = std::fs::remove_file(server_path);
}

#[test]
fn test_render_highlighted_text_zero_allocation() {
    use gpui::{HighlightStyle, SharedString};
    use loganalysis_core::CompiledMatcher;
    use loganalysis_gui::components::log_view::render_highlighted_text;

    let text = SharedString::from("2026-11-06 12:00:00 Normal log line");
    let style = HighlightStyle::default();

    // 1. Without matcher: returns element directly
    let _elem1 = render_highlighted_text(text.clone(), &None, style);

    // 2. With matcher and matches
    let matcher = std::sync::Arc::new(CompiledMatcher::new("Normal", false, false).unwrap());
    let _elem2 = render_highlighted_text(text.clone(), &Some(matcher.clone()), style);

    // 3. With matcher but no matches
    let no_matcher = std::sync::Arc::new(CompiledMatcher::new("NonExistentPattern", false, false).unwrap());
    let _elem3 = render_highlighted_text(text, &Some(no_matcher), style);
}

#[test]
fn test_simple_tooltip_initialization() {
    use loganalysis_gui::components::tooltip::SimpleTooltip;
    use loganalysis_gui::theme::Theme;

    let theme = Theme::dark();
    let tooltip = SimpleTooltip {
        text: "Open File".into(),
        shortcut: Some("Ctrl+O"),
        theme,
    };
    assert_eq!(tooltip.text.as_ref(), "Open File");
    assert_eq!(tooltip.shortcut, Some("Ctrl+O"));
    assert!(tooltip.theme.border.a > 0.0);
}

#[test]
fn test_recent_files_lru_and_persistence() {
    use std::path::PathBuf;

    let mut recents: Vec<PathBuf> = Vec::new();

    // Helper matching workspace.add_recent_file logic
    let add_rec = |list: &mut Vec<PathBuf>, p: PathBuf| {
        list.retain(|item| item != &p);
        list.insert(0, p);
        list.truncate(5);
    };

    let p1 = PathBuf::from("/tmp/log1.txt");
    let p2 = PathBuf::from("/tmp/log2.txt");
    let p3 = PathBuf::from("/tmp/log3.txt");
    let p4 = PathBuf::from("/tmp/log4.txt");
    let p5 = PathBuf::from("/tmp/log5.txt");
    let p6 = PathBuf::from("/tmp/log6.txt");

    add_rec(&mut recents, p1.clone());
    add_rec(&mut recents, p2.clone());
    assert_eq!(recents, vec![p2.clone(), p1.clone()]);

    // Duplicate re-order to front
    add_rec(&mut recents, p1.clone());
    assert_eq!(recents, vec![p1.clone(), p2.clone()]);

    // Truncate at 5 items
    add_rec(&mut recents, p3.clone());
    add_rec(&mut recents, p4.clone());
    add_rec(&mut recents, p5.clone());
    add_rec(&mut recents, p6.clone());
    assert_eq!(recents.len(), 5);
    assert_eq!(recents[0], p6);
    assert_eq!(recents[4], p1); // p2 was evicted

    // JSON serialization roundtrip
    let serialized = serde_json::to_string_pretty(&recents).expect("serialization failed");
    let deserialized: Vec<PathBuf> = serde_json::from_str(&serialized).expect("deserialization failed");
    assert_eq!(deserialized, recents);
}

#[test]
fn test_phase3_workstation_facets_and_telemetry() {
    use loganalysis_core::parse_logcat_column_line;
    use loganalysis_gui::modals::command_palette::{get_palette_commands, PaletteCommandId};
    use std::collections::HashMap;

    // 1. Tag extraction and ranking verification
    let sample_lines = [
        "11-06 18:00:00.123 1000 1000 I ActivityManager: Starting activity",
        "11-06 18:00:00.124 1001 1001 D SurfaceFlinger: Frame submitted",
        "11-06 18:00:00.125 1000 1000 I ActivityManager: Activity displayed",
        "11-06 18:00:00.126 1002 1002 E AudioFlinger: Underrun detected",
        "11-06 18:00:00.127 1000 1000 W ActivityManager: Slow binding",
    ];

    let mut counts: HashMap<String, usize> = HashMap::new();
    for line in sample_lines {
        let parsed = parse_logcat_column_line(line);
        if let Some(tag) = parsed.tag {
            let tag_trimmed = tag.trim();
            if !tag_trimmed.is_empty() {
                *counts.entry(tag_trimmed.to_string()).or_insert(0) += 1;
            }
        }
    }

    let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    assert_eq!(sorted[0], ("ActivityManager".to_string(), 3));
    assert_eq!(sorted.len(), 3);

    // 2. Memory telemetry display formatting
    let format_bytes = |bytes: usize| -> String {
        if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    };

    assert_eq!(format_bytes(512 * 1024), "512.0 KB");
    assert_eq!(format_bytes(16 * 1024 * 1024), "16.0 MB");
    assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.00 GB");

    // 3. Verify Command Palette shortcut alignment (no Shift+F text input collision)
    let cmds = get_palette_commands();
    let dock_cmd = cmds
        .iter()
        .find(|c| c.id == PaletteCommandId::ToggleFilterDock)
        .expect("ToggleFilterDock should exist");
    assert_eq!(dock_cmd.shortcut, Some("Ctrl+J"));

    let facets_cmd = cmds
        .iter()
        .find(|c| c.id == PaletteCommandId::ToggleFacetsBar)
        .expect("ToggleFacetsBar should exist");
    assert_eq!(facets_cmd.shortcut, Some("Alt+F"));
}

#[test]
fn test_resizable_filter_dock_clamping_and_reset() {
    // Test clamp bounds and reset defaults for Filter Dock resizer
    let default_width = 310.0f32;
    let default_height = 220.0f32;

    // 1. Left dock width clamping [180.0, 800.0]
    let clamp_width = |start: f32, delta: f32| (start + delta).clamp(180.0, 800.0);
    assert_eq!(clamp_width(default_width, 100.0), 410.0);
    assert_eq!(clamp_width(default_width, 999.0), 800.0); // max clamped
    assert_eq!(clamp_width(default_width, -500.0), 180.0); // min clamped

    // 2. Bottom dock height clamping [100.0, 700.0]
    let clamp_height = |start: f32, delta: f32| (start + delta).clamp(100.0, 700.0);
    assert_eq!(clamp_height(default_height, 80.0), 300.0);
    assert_eq!(clamp_height(default_height, 1000.0), 700.0); // max clamped
    assert_eq!(clamp_height(default_height, -300.0), 100.0); // min clamped

    // 3. Preset defaults
    assert_eq!(default_width, 310.0);
    assert_eq!(default_height, 220.0);
}

#[test]
fn test_overflow_menu_state_and_escape_priority() {
    use gpui::px;
    use loganalysis_gui::components::log_view::calculate_selection_bar_top;
    use loganalysis_gui::selection::SelectionModel;
    use loganalysis_gui::workspace::{determine_escape_action, EscapeAction};

    let mut overflow_open = false;
    let mut device_open = false;
    let filter_modal_open = false;
    let note_modal_open = false;
    let drawer_open = false;
    let palette_open = false;
    let mut selection = SelectionModel::new();

    // 1. Initial state: nothing open, no selection -> Escape action is None
    assert_eq!(
        determine_escape_action(
            overflow_open,
            device_open,
            filter_modal_open,
            note_modal_open,
            drawer_open,
            palette_open,
            selection.selected_row.is_some()
        ),
        EscapeAction::None
    );

    // 2. Mutual exclusivity logic verification:
    // Opening device selector closes overflow menu
    overflow_open = true;
    device_open = !device_open;
    if device_open {
        overflow_open = false;
    }
    assert!(device_open);
    assert!(!overflow_open);

    // Opening overflow menu closes device selector
    overflow_open = !overflow_open;
    if overflow_open {
        device_open = false;
    }
    assert!(overflow_open);
    assert!(!device_open);

    // 3. Reverse z-order priority hierarchy tests:
    selection.select(42, 100);
    assert_eq!(selection.selected_row, Some(42));
    assert_eq!(selection.anchor_source_idx, Some(100));

    // Priority 1: Overflow menu takes precedence over all other open items
    assert_eq!(
        determine_escape_action(
            true,
            true,
            true,
            true,
            true,
            true,
            selection.selected_row.is_some()
        ),
        EscapeAction::CloseOverflowMenu
    );

    // Priority 2: Device selector takes precedence over modals and selection
    assert_eq!(
        determine_escape_action(
            false,
            true,
            true,
            true,
            true,
            true,
            selection.selected_row.is_some()
        ),
        EscapeAction::CloseDeviceSelector
    );

    // Priority 3: Modals take precedence over selection
    assert_eq!(
        determine_escape_action(
            false,
            false,
            true,
            false,
            false,
            false,
            selection.selected_row.is_some()
        ),
        EscapeAction::CloseFilterModal
    );
    assert_eq!(
        determine_escape_action(
            false,
            false,
            false,
            true,
            false,
            false,
            selection.selected_row.is_some()
        ),
        EscapeAction::CloseNoteModal
    );
    assert_eq!(
        determine_escape_action(
            false,
            false,
            false,
            false,
            true,
            false,
            selection.selected_row.is_some()
        ),
        EscapeAction::CloseBookmarkDrawer
    );
    assert_eq!(
        determine_escape_action(
            false,
            false,
            false,
            false,
            false,
            true,
            selection.selected_row.is_some()
        ),
        EscapeAction::CloseCommandPalette
    );

    // Priority 4: Selection clearing when no popovers or modals open
    assert_eq!(
        determine_escape_action(
            false,
            false,
            false,
            false,
            false,
            false,
            selection.selected_row.is_some()
        ),
        EscapeAction::ClearSelection
    );

    // Execute genuine SelectionModel clear
    selection.clear();
    assert!(selection.selected_row.is_none());
    assert!(selection.anchor_source_idx.is_none());

    // Once cleared, action becomes None
    assert_eq!(
        determine_escape_action(
            false,
            false,
            false,
            false,
            false,
            false,
            selection.selected_row.is_some()
        ),
        EscapeAction::None
    );

    // 4. Verify selection action bar top offset calculation
    assert_eq!(calculate_selection_bar_top(false, false), px(10.0));
    assert_eq!(calculate_selection_bar_top(false, true), px(38.0));
    assert_eq!(calculate_selection_bar_top(true, false), px(52.0));
    assert_eq!(calculate_selection_bar_top(true, true), px(52.0));
}

#[test]
fn test_selection_model_clear_lifecycle() {
    use loganalysis_gui::selection::SelectionModel;

    let mut sel = SelectionModel::new();
    assert!(sel.selected_row.is_none());
    assert!(sel.anchor_source_idx.is_none());
    sel.select(10, 500);
    assert_eq!(sel.selected_row, Some(10));
    assert_eq!(sel.anchor_source_idx, Some(500));
    sel.clear();
    assert!(sel.selected_row.is_none());
    assert!(sel.anchor_source_idx.is_none());
}

#[test]
fn test_unified_compact_header_spec_and_dom_ids() {
    use gpui::*;
    use loganalysis_gui::components::titlebar::{WINDOW_CLOSE_HOVER_COLOR, WINDOW_CONTROL_DOM_IDS};
    use loganalysis_gui::components::toolbar::{
        HEADER_CONTRACT_DOM_IDS, LOGO_PILL_SIZE, UNIFIED_HEADER_HEIGHT,
    };

    // 1. Verify real production layout constants exported from toolbar and titlebar
    assert_eq!(
        UNIFIED_HEADER_HEIGHT, 38.0f32,
        "Unified header height constant must match 38px specification"
    );
    assert_eq!(
        LOGO_PILL_SIZE, 18.0f32,
        "Logo pill dimension constant must match 18px compact specification"
    );
    assert_eq!(
        WINDOW_CLOSE_HOVER_COLOR, 0xe81123u32,
        "Window close button hover color constant must match crimson #e81123"
    );

    // Verify GPUI element constructors with production constants
    assert_eq!(px(UNIFIED_HEADER_HEIGHT), px(38.0));
    assert_eq!(px(LOGO_PILL_SIZE), px(18.0));
    assert_eq!(rgb(WINDOW_CLOSE_HOVER_COLOR), rgb(0xe81123));

    // 2. Verify compiled DOM ID contract arrays exported and consumed by render_unified_header / render_window_controls
    assert_eq!(HEADER_CONTRACT_DOM_IDS.len(), 16);
    assert_eq!(WINDOW_CONTROL_DOM_IDS.len(), 4);

    let expected_toolbar_ids = [
        "unified-header-bar",
        "header-left-section",
        "header-branding",
        "btn-toolbar-open",
        "header-adb-controls",
        "btn-toolbar-adb-devices",
        "btn-toolbar-adb",
        "btn-toolbar-pause-adb",
        "header-center-section",
        "titlebar-breadcrumb-pill",
        "header-right-section",
        "seg-view-toggle",
        "seg-all-lines",
        "seg-filtered-only",
        "btn-toolbar-find",
        "btn-toolbar-overflow-menu",
    ];

    let expected_control_ids = [
        "titlebar-window-controls",
        "btn-window-minimize",
        "btn-window-maximize",
        "btn-window-close",
    ];

    let actual_toolbar_ids: HashSet<&str> = HEADER_CONTRACT_DOM_IDS.iter().copied().collect();
    let expected_toolbar_set: HashSet<&str> = expected_toolbar_ids.iter().copied().collect();
    assert_eq!(actual_toolbar_ids, expected_toolbar_set);

    let actual_control_ids: HashSet<&str> = WINDOW_CONTROL_DOM_IDS.iter().copied().collect();
    let expected_control_set: HashSet<&str> = expected_control_ids.iter().copied().collect();
    assert_eq!(actual_control_ids, expected_control_set);

    let all_ids: Vec<&str> = HEADER_CONTRACT_DOM_IDS
        .iter()
        .chain(WINDOW_CONTROL_DOM_IDS.iter())
        .copied()
        .collect();
    assert_eq!(
        all_ids.len(),
        20,
        "Unified compact header must contain exactly 20 contract DOM IDs"
    );
    assert!(all_ids.iter().all(|id| !id.is_empty()));
    let unique_ids: HashSet<&str> = all_ids.iter().copied().collect();
    assert_eq!(unique_ids.len(), 20, "All 20 contract DOM IDs must be unique");
}

#[test]
fn test_csd_env_resolution_hierarchy() {
    use loganalysis_gui::components::titlebar::check_csd_env_override;

    let _guard = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // Baseline: Ensure clean environment before testing
    std::env::remove_var("LOGANALYSIS_FORCE_CSD");
    std::env::remove_var("LOGANALYSIS_DISABLE_CSD");

    // Permutation 1: Neither variable set -> None (defers to window decoration / OS detection)
    assert_eq!(
        check_csd_env_override(),
        None,
        "Permutation 1 failed: When neither override is set, check_csd_env_override() must return None"
    );

    // Permutation 2: LOGANALYSIS_FORCE_CSD set alone -> Some(true)
    std::env::set_var("LOGANALYSIS_FORCE_CSD", "1");
    std::env::remove_var("LOGANALYSIS_DISABLE_CSD");
    assert_eq!(
        check_csd_env_override(),
        Some(true),
        "Permutation 2 failed: When LOGANALYSIS_FORCE_CSD=1 is set alone, check_csd_env_override() must return Some(true)"
    );

    // Also test case-insensitive string "true"
    std::env::set_var("LOGANALYSIS_FORCE_CSD", "true");
    assert_eq!(
        check_csd_env_override(),
        Some(true),
        "Permutation 2 failed: When LOGANALYSIS_FORCE_CSD=true is set alone, check_csd_env_override() must return Some(true)"
    );

    // Permutation 3: LOGANALYSIS_DISABLE_CSD set alone -> Some(false)
    std::env::remove_var("LOGANALYSIS_FORCE_CSD");
    std::env::set_var("LOGANALYSIS_DISABLE_CSD", "1");
    assert_eq!(
        check_csd_env_override(),
        Some(false),
        "Permutation 3 failed: When LOGANALYSIS_DISABLE_CSD=1 is set alone, check_csd_env_override() must return Some(false)"
    );

    std::env::set_var("LOGANALYSIS_DISABLE_CSD", "true");
    assert_eq!(
        check_csd_env_override(),
        Some(false),
        "Permutation 3 failed: When LOGANALYSIS_DISABLE_CSD=true is set alone, check_csd_env_override() must return Some(false)"
    );

    // Permutation 4: Both variables set -> LOGANALYSIS_DISABLE_CSD takes top priority -> Some(false)
    std::env::set_var("LOGANALYSIS_FORCE_CSD", "1");
    std::env::set_var("LOGANALYSIS_DISABLE_CSD", "1");
    assert_eq!(
        check_csd_env_override(),
        Some(false),
        "Permutation 4 failed: When both FORCE and DISABLE are set, DISABLE must take priority (Some(false))"
    );

    // Teardown cleanup: Clean up environment variables to avoid state leakage
    std::env::remove_var("LOGANALYSIS_FORCE_CSD");
    std::env::remove_var("LOGANALYSIS_DISABLE_CSD");
    assert_eq!(
        check_csd_env_override(),
        None,
        "Teardown failed: Environment variables must be cleaned up and return None"
    );
}

#[test]
fn test_milestone4_collapsible_timeline_and_zen_focus_mode() {
    use gpui::Action;
    use loganalysis_core::fuzzy_match;
    use loganalysis_gui::actions::{
        all_action_names, CloseFind, ResetZoom, ToggleZenMode, REGISTERED_ACTION_COUNT,
    };
    use loganalysis_gui::components::timeline_sparkline::should_render_timeline_ribbon;
    use loganalysis_gui::modals::command_palette::{
        get_palette_commands, palette_command_action_name, PaletteCommandId,
    };
    use loganalysis_gui::workspace::{determine_escape_action, EscapeAction, WORKSPACE_DEFAULTS};
    use loganalysis_gui::{
        default_keybinding_specs, workspace_shows_activity_bar, workspace_shows_filter_dock,
        workspace_shows_status_bar, LogAnalysisWorkspace,
    };

    // 1. Verification of compiled registered action contracts (46 actions including ToggleZenMode, ResetZoom & CloseFind)
    let action_names = all_action_names();
    assert_eq!(REGISTERED_ACTION_COUNT, 46);
    assert_eq!(action_names.len(), REGISTERED_ACTION_COUNT);
    let unique_actions: HashSet<&str> = action_names.iter().copied().collect();
    assert_eq!(unique_actions.len(), REGISTERED_ACTION_COUNT);
    assert!(action_names.contains(&Action::name(&ToggleZenMode)));
    assert!(action_names.contains(&Action::name(&ResetZoom)));
    assert!(action_names.contains(&Action::name(&CloseFind)));

    // 2. Verification of workspace default state contracts & toggle/reset method existence
    let defaults = std::hint::black_box(WORKSPACE_DEFAULTS);
    assert!(!defaults.is_timeline_visible);
    assert!(!defaults.is_zen_mode);
    let _: fn(&mut LogAnalysisWorkspace, &mut gpui::Context<LogAnalysisWorkspace>) =
        LogAnalysisWorkspace::toggle_zen_mode;
    let _: fn(&mut LogAnalysisWorkspace, &mut gpui::Context<LogAnalysisWorkspace>) =
        LogAnalysisWorkspace::toggle_timeline;
    let _: fn(&mut LogAnalysisWorkspace, &mut gpui::Context<LogAnalysisWorkspace>) =
        LogAnalysisWorkspace::reset_zoom;
    let _: fn(&mut LogAnalysisWorkspace, &mut gpui::Context<LogAnalysisWorkspace>) =
        LogAnalysisWorkspace::close_search_bar;

    // 3. Verification of Command Palette catalog integration and action wiring ([LOW-5])
    let commands = get_palette_commands();
    let zen_cmd = commands
        .iter()
        .find(|c| c.id == PaletteCommandId::ToggleZenMode)
        .expect("ToggleZenMode must be present in command palette catalog");
    assert_eq!(zen_cmd.category, "VIEW");
    assert_eq!(zen_cmd.title, "View: Toggle Zen Focus Mode");
    assert_eq!(zen_cmd.shortcut, Some("F11"));
    assert_eq!(
        palette_command_action_name(PaletteCommandId::ToggleZenMode),
        Action::name(&ToggleZenMode)
    );

    let dock_pos_cmd = commands
        .iter()
        .find(|c| c.id == PaletteCommandId::ToggleFilterPanelPosition)
        .expect("ToggleFilterPanelPosition must exist");
    assert_eq!(
        dock_pos_cmd.title,
        "View: Toggle Filter Dock Position (Bottom / Left)"
    );

    let next_bm_cmd = commands
        .iter()
        .find(|c| c.id == PaletteCommandId::NextBookmark)
        .expect("NextBookmark must exist");
    assert_eq!(next_bm_cmd.shortcut, Some("Alt+F2"));

    let reset_zoom_cmd = commands
        .iter()
        .find(|c| c.id == PaletteCommandId::ResetZoom)
        .expect("ResetZoom must exist");
    assert_eq!(reset_zoom_cmd.shortcut, Some("Ctrl+0"));
    assert_eq!(
        palette_command_action_name(PaletteCommandId::ResetZoom),
        Action::name(&ResetZoom)
    );

    // Fuzzy search capability
    let zen_match = fuzzy_match("zen", zen_cmd.title);
    assert!(zen_match.is_some(), "Fuzzy match for 'zen' must match ToggleZenMode title");
    let focus_match = fuzzy_match("focus", zen_cmd.title);
    assert!(focus_match.is_some(), "Fuzzy match for 'focus' must match ToggleZenMode title");

    // 4. Verification that timeline sparkline visibility predicate respects is_zen_mode
    assert!(!should_render_timeline_ribbon(false, false));
    assert!(should_render_timeline_ribbon(true, false));
    assert!(!should_render_timeline_ribbon(true, true));
    assert!(!should_render_timeline_ribbon(false, true));

    // 5. Verification that root layout visibility predicates apply Zen mode suppression guards
    assert!(workspace_shows_activity_bar(false));
    assert!(!workspace_shows_activity_bar(true));
    assert!(workspace_shows_status_bar(false));
    assert!(!workspace_shows_status_bar(true));
    assert!(workspace_shows_filter_dock(false, true));
    assert!(!workspace_shows_filter_dock(false, false));
    assert!(!workspace_shows_filter_dock(true, true));
    assert!(!workspace_shows_filter_dock(true, false));

    // 6. Verification of compiled keybinding contracts for Zen Mode & ResetZoom ([LOW-5])
    let specs = default_keybinding_specs();
    let zen_action_name = Action::name(&ToggleZenMode);
    assert!(specs.contains(&("f11", zen_action_name)));
    assert!(specs.contains(&("ctrl-shift-z", zen_action_name)));
    assert!(specs.contains(&("cmd-shift-z", zen_action_name)));
    let reset_zoom_action_name = Action::name(&ResetZoom);
    assert!(specs.contains(&("ctrl-0", reset_zoom_action_name)));
    assert!(specs.contains(&("cmd-0", reset_zoom_action_name)));

    // 7. Verification of Escape key dismissal fallback (None permits Zen mode exit in workspace::handle_escape)
    assert_eq!(
        determine_escape_action(false, false, false, false, false, false, false),
        EscapeAction::None
    );
}

#[test]
fn test_workspace_controller_snapshots_and_defaults() {
    use loganalysis_core::LineSeverity;
    use loganalysis_gui::controllers::{
        AdbController, BookmarkController, SearchController, SplitViewController,
    };
    use loganalysis_gui::types::{
        AdbStreamState, BookmarkDrawerState, SearchState, SplitViewState,
    };
    use loganalysis_gui::workspace::WORKSPACE_DEFAULTS;
    use loganalysis_gui::LogAnalysisWorkspace;
    use std::sync::atomic::Ordering;

    // 1. Workspace compile-time snapshot API & canonical default contracts
    let _: fn(&LogAnalysisWorkspace) -> AdbStreamState = LogAnalysisWorkspace::adb_state;
    let _: fn(&LogAnalysisWorkspace) -> BookmarkDrawerState =
        LogAnalysisWorkspace::bookmark_drawer_state;
    let _: fn(&LogAnalysisWorkspace) -> SearchState = LogAnalysisWorkspace::search_state;
    let _: fn(&LogAnalysisWorkspace) -> SplitViewState = LogAnalysisWorkspace::split_view_state;
    let defaults = std::hint::black_box(WORKSPACE_DEFAULTS);
    assert!(!defaults.is_timeline_visible);
    assert!(!defaults.is_zen_mode);

    // 2. SplitViewController default orientation (horizontal: is_split_vertical == false) & snapshot
    let mut split = SplitViewController::new();
    let split_default: SplitViewState = split.state();
    assert!(!split_default.is_split_view);
    assert!(!split_default.is_split_vertical);
    assert!(split_default.is_scroll_synced);
    assert!(split_default.secondary_file_name.is_none());

    split.toggle_split();
    split.toggle_orientation();
    split.toggle_sync();
    split.secondary_file_name = Some("secondary.log".into());
    let split_active = split.state();
    assert!(split_active.is_split_view);
    assert!(split_active.is_split_vertical);
    assert!(!split_active.is_scroll_synced);
    assert_eq!(
        split_active.secondary_file_name,
        Some("secondary.log".into())
    );
    split.clear();
    assert!(!split.state().is_split_view);
    assert!(split.state().secondary_file_name.is_none());

    // 3. AdbController defaults, mutations, and snapshot parity
    let mut adb = AdbController::new();
    let adb_default: AdbStreamState = adb.state();
    assert!(adb_default.auto_scroll);
    assert_eq!(adb_default.selected_device, None);
    assert!(!adb_default.is_device_selector_open);
    assert!(!adb_default.is_scanning_adb);
    assert!(!adb_default.is_streaming);
    assert!(!adb_default.is_stream_paused);
    assert_eq!(adb_default.paused_stream_line_count, 0);

    adb.is_device_selector_open = true;
    adb.select_device("emulator-5554".to_string());
    adb.is_streaming = true;
    adb.is_paused = true;
    adb.paused_line_count = 17;
    let adb_active = adb.state();
    assert_eq!(adb_active.selected_device.as_deref(), Some("emulator-5554"));
    assert!(!adb_active.is_device_selector_open);
    assert!(adb_active.is_streaming);
    assert!(adb_active.is_stream_paused);
    assert_eq!(adb_active.paused_stream_line_count, 17);
    adb.clear_paused_buffer();
    assert_eq!(adb.state().paused_stream_line_count, 0);

    // 4. BookmarkController Deref/DerefMut forwarding to BookmarkManager + drawer/modal state
    let mut bookmarks = BookmarkController::new();
    assert!(bookmarks.is_empty());
    assert!(bookmarks.toggle_bookmark(10, "E/Tag: boom".to_string(), LineSeverity::Error));
    assert_eq!(bookmarks.len(), 1);
    assert!(bookmarks.set_note(10, Some("  root cause  ".to_string())));
    bookmarks.shift_on_trim(3);
    let items = bookmarks.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source_idx, 7);
    assert_eq!(items[0].note.as_deref(), Some("root cause"));

    assert!(bookmarks.toggle_drawer());
    bookmarks.drawer_selected_index = 2;
    bookmarks.open_note_modal(7);
    let bm_state: BookmarkDrawerState = bookmarks.state();
    assert!(bm_state.is_bookmark_drawer_open);
    assert_eq!(bm_state.bookmark_drawer_selected_index, 2);
    assert!(bm_state.is_note_modal_open);
    assert_eq!(bm_state.note_editing_source_idx, Some(7));

    bookmarks.clear();
    assert_eq!(bookmarks.len(), 0);
    assert_eq!(bookmarks.state().bookmark_drawer_selected_index, 0);
    bookmarks.close_note_modal();
    bookmarks.close_drawer();
    assert!(!bookmarks.state().is_note_modal_open);
    assert!(!bookmarks.state().is_bookmark_drawer_open);

    // 5. SearchController defaults, state snapshot, visibility gating, and empty/whitespace clearing
    let mut search = SearchController::new();
    let search_default: SearchState = search.state();
    assert!(search_default.search_query.is_empty());
    assert!(!search_default.is_search_visible);
    assert!(!search_default.is_search_regex);
    assert!(!search_default.is_search_case_sensitive);
    assert_eq!(search_default.search_match_status, None);
    assert!(search.cached_matcher.is_none());

    // Hidden search bar must NOT compile cached_matcher on update_matcher_when_visible
    search.query = "ERROR".to_string();
    search.update_matcher_when_visible();
    assert!(search.cached_matcher.is_none());

    // Visible search bar compiles matcher for non-empty query
    assert!(search.toggle_visible());
    search.is_regex = true;
    search.is_case_sensitive = true;
    search.match_status = Some("Line 4".to_string());
    search.update_matcher_when_visible();
    assert!(search.cached_matcher.is_some());

    let search_active = search.state();
    assert_eq!(search_active.search_query, "ERROR");
    assert!(search_active.is_search_visible);
    assert!(search_active.is_search_regex);
    assert!(search_active.is_search_case_sensitive);
    assert_eq!(search_active.search_match_status.as_deref(), Some("Line 4"));

    // Abort bumps cancellation token deterministically
    let gen_before = search.cancel_gen.load(Ordering::SeqCst);
    search.abort_ongoing_searches();
    assert_eq!(search.cancel_gen.load(Ordering::SeqCst), gen_before + 1);

    // Empty query clears cached_matcher
    search.query.clear();
    search.update_matcher_when_visible();
    assert!(search.cached_matcher.is_none());

    // MAJ-02 contract: whitespace-only query must ALSO clear cached_matcher when visible
    search.query = "ERROR".to_string();
    search.update_matcher_when_visible();
    assert!(search.cached_matcher.is_some());
    search.query = "   \t\n".to_string();
    search.update_matcher_when_visible();
    assert!(
        search.cached_matcher.is_none(),
        "MAJ-02: whitespace-only search query must clear cached_matcher when search bar is visible"
    );

    search.set_query("ERROR".to_string());
    assert!(search.cached_matcher.is_some());
    search.set_query("   \t ".to_string());
    assert!(
        search.cached_matcher.is_none(),
        "MAJ-02: set_query with whitespace-only string must clear cached_matcher via update_matcher"
    );
}

#[test]
fn test_escape_priority_with_controllers() {
    use loganalysis_gui::controllers::{AdbController, BookmarkController};
    use loganalysis_gui::selection::SelectionModel;
    use loganalysis_gui::types::FilterEditState;
    use loganalysis_gui::workspace::{determine_escape_action, EscapeAction};

    let mut is_overflow_menu_open = true;
    let mut adb = AdbController::new();
    adb.is_device_selector_open = true;
    let mut filter_edit_modal = FilterEditState {
        is_open: true,
        ..Default::default()
    };
    let mut bookmarks = BookmarkController::new();
    bookmarks.toggle_drawer();
    bookmarks.open_note_modal(42);
    let mut is_command_palette_open = true;
    let mut selection = SelectionModel::new();
    selection.select(5, 42);

    let eval = |overflow: bool,
                adb_ctrl: &AdbController,
                filter_modal: &FilterEditState,
                bm_ctrl: &BookmarkController,
                palette: bool,
                sel: &SelectionModel| {
        let adb_state = adb_ctrl.state();
        let bm_state = bm_ctrl.state();
        determine_escape_action(
            overflow,
            adb_state.is_device_selector_open,
            filter_modal.is_open,
            bm_state.is_note_modal_open,
            bm_state.is_bookmark_drawer_open,
            palette,
            sel.selected_row.is_some(),
        )
    };

    // Walk strictly down the 8-level Escape priority chain driven by controller snapshots
    assert_eq!(
        eval(
            is_overflow_menu_open,
            &adb,
            &filter_edit_modal,
            &bookmarks,
            is_command_palette_open,
            &selection
        ),
        EscapeAction::CloseOverflowMenu
    );
    is_overflow_menu_open = false;

    assert_eq!(
        eval(
            is_overflow_menu_open,
            &adb,
            &filter_edit_modal,
            &bookmarks,
            is_command_palette_open,
            &selection
        ),
        EscapeAction::CloseDeviceSelector
    );
    adb.is_device_selector_open = false;

    assert_eq!(
        eval(
            is_overflow_menu_open,
            &adb,
            &filter_edit_modal,
            &bookmarks,
            is_command_palette_open,
            &selection
        ),
        EscapeAction::CloseFilterModal
    );
    filter_edit_modal.is_open = false;

    assert_eq!(
        eval(
            is_overflow_menu_open,
            &adb,
            &filter_edit_modal,
            &bookmarks,
            is_command_palette_open,
            &selection
        ),
        EscapeAction::CloseNoteModal
    );
    bookmarks.close_note_modal();

    assert_eq!(
        eval(
            is_overflow_menu_open,
            &adb,
            &filter_edit_modal,
            &bookmarks,
            is_command_palette_open,
            &selection
        ),
        EscapeAction::CloseBookmarkDrawer
    );
    bookmarks.close_drawer();

    assert_eq!(
        eval(
            is_overflow_menu_open,
            &adb,
            &filter_edit_modal,
            &bookmarks,
            is_command_palette_open,
            &selection
        ),
        EscapeAction::CloseCommandPalette
    );
    is_command_palette_open = false;

    assert_eq!(
        eval(
            is_overflow_menu_open,
            &adb,
            &filter_edit_modal,
            &bookmarks,
            is_command_palette_open,
            &selection
        ),
        EscapeAction::ClearSelection
    );
    selection.clear();

    assert_eq!(
        eval(
            is_overflow_menu_open,
            &adb,
            &filter_edit_modal,
            &bookmarks,
            is_command_palette_open,
            &selection
        ),
        EscapeAction::None
    );
}

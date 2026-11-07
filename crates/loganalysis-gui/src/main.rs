#![recursion_limit = "512"]
#![windows_subsystem = "windows"]

mod actions;
mod cli;
mod components;
mod dialogs;
mod logging;
mod modals;
mod selection;
mod theme;
mod types;
mod workspace;

use gpui::prelude::FluentBuilder;
use gpui::*;

use actions::*;
use cli::handle_cli_version_flag;
use components::filter_dock::render_filter_dock;
use components::log_view::render_log_viewport;
use components::resize_border::render_window_resize_borders;
use components::status_bar::render_status_bar;
use components::timeline_sparkline::render_timeline_sparkline;
use components::titlebar::render_titlebar;
use components::toolbar::render_toolbar;
use modals::bookmark_drawer::render_bookmark_drawer;
use modals::command_palette::render_command_palette;
use modals::filter_edit_modal::render_filter_edit_modal;
use modals::note_edit_modal::render_note_edit_modal;
use types::{FilterPanelPosition, InputFocusTarget};
pub use workspace::LogAnalysisWorkspace;

impl Render for LogAnalysisWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_pattern_focused = self.modal_pattern_input.read(cx).focus_handle.is_focused(window);
        let note_input_focused = self.note_input.read(cx).focus_handle.is_focused(window);
        if self.filter_edit_modal.is_open {
            if self.input_focus == InputFocusTarget::ModalPattern && !modal_pattern_focused {
                window.focus(&self.modal_pattern_input.read(cx).focus_handle);
            }
        } else if self.is_note_modal_open {
            if self.input_focus == InputFocusTarget::BookmarkNote && !note_input_focused {
                window.focus(&self.note_input.read(cx).focus_handle);
            }
        } else if !self.focus_handle.is_focused(window) {
            window.focus(&self.focus_handle);
        }

        let theme = self.theme;
        let is_bottom_dock = self.filter_panel_position == FilterPanelPosition::Bottom;
        let is_csd = components::titlebar::should_render_csd(window);
        let is_floating = !window.is_maximized();

        let titlebar_el = render_titlebar(self, window, cx);
        let toolbar_el = render_toolbar(self, cx);
        let timeline_ribbon_el = render_timeline_sparkline(self, cx);
        let filter_panel_el = render_filter_dock(self, cx);
        let log_viewport_el = render_log_viewport(self, cx);
        let status_bar_el = render_status_bar(self, cx);
        let modal_el = render_filter_edit_modal(self, cx);
        let palette_el = render_command_palette(self, cx);
        let bookmark_drawer_el = render_bookmark_drawer(self, cx);
        let note_modal_el = render_note_edit_modal(self, cx);
        let resize_borders = render_window_resize_borders(window, cx);

        let mut body = div()
            .flex_1()
            .w_full()
            .flex()
            .overflow_hidden();

        if !is_bottom_dock {
            body = body.flex_row().child(filter_panel_el).child(log_viewport_el);
        } else {
            body = body.flex_col().child(log_viewport_el).child(filter_panel_el);
        }

        div()
            .id("workspace-container")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key == "escape" && this.is_device_selector_open {
                    this.is_device_selector_open = false;
                    cx.notify();
                    return;
                }
                this.handle_key_down(event, cx);
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _window, cx| {
                if let Some(path) = paths.paths().first() {
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext.eq_ignore_ascii_case("json") {
                        this.load_filters_from_path(path, cx);
                    } else {
                        this.open_file(path, cx);
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &OpenFile, window, cx| {
                this.prompt_open_file(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFilteredOnly, _window, cx| {
                this.toggle_filtered_only(cx);
            }))
            .on_action(cx.listener(|this, _: &Find, window, cx| {
                this.toggle_search_bar(cx);
                if this.is_search_visible {
                    window.focus(&this.find_input.read(cx).focus_handle);
                }
            }))
            .on_action(cx.listener(|this, _: &FindNext, _window, cx| {
                this.search(true, cx);
            }))
            .on_action(cx.listener(|this, _: &FindPrev, _window, cx| {
                this.search(false, cx);
            }))
            .on_action(cx.listener(|this, _: &CopySelection, _window, cx| {
                this.copy_selection(cx);
            }))
            .on_action(cx.listener(|this, _: &AddFilter, window, cx| {
                this.open_add_filter_modal(cx);
                window.focus(&this.modal_pattern_input.read(cx).focus_handle);
            }))
            .on_action(cx.listener(|this, _: &SaveFilters, window, cx| {
                this.prompt_save_filters(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &SaveFiltersAs, window, cx| {
                this.prompt_save_filters(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &LoadFilters, window, cx| {
                this.prompt_load_filters(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &ClearLogs, _window, cx| {
                if this.input_focus != InputFocusTarget::None {
                    return;
                }
                this.clear_logs(cx);
            }))
            .on_action(cx.listener(|this, _: &AddFilterTab, _window, cx| {
                if this.input_focus != InputFocusTarget::None {
                    return;
                }
                this.add_filter_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &DeleteFilterTab, _window, cx| {
                if this.input_focus != InputFocusTarget::None {
                    return;
                }
                this.delete_current_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &RenameFilterTab, _window, cx| {
                if this.input_focus != InputFocusTarget::None {
                    return;
                }
                this.rename_current_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleTheme, _window, cx| {
                this.toggle_theme(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFullLineDisplay, _window, cx| {
                this.toggle_full_line_display(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleAutoScroll, _window, cx| {
                this.toggle_auto_scroll(cx);
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _window, cx| {
                this.zoom(1.0, cx);
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _window, cx| {
                this.zoom(-1.0, cx);
            }))
            .on_action(cx.listener(|this, _: &TogglePauseStream, _window, cx| {
                if this.input_focus != InputFocusTarget::None {
                    return;
                }
                this.toggle_pause_stream(cx);
            }))
            .on_action(cx.listener(|this, _: &AddIncludeFromSelection, _window, cx| {
                this.add_quick_filter_from_selection(false, cx);
            }))
            .on_action(cx.listener(|this, _: &AddExcludeFromSelection, _window, cx| {
                this.add_quick_filter_from_selection(true, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFilterPanelPosition, _window, cx| {
                this.toggle_filter_panel_position(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleCommandPalette, window, cx| {
                this.toggle_command_palette(cx);
                if this.is_command_palette_open {
                    window.focus(&this.palette_input.read(cx).focus_handle);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleBookmark, _window, cx| {
                this.toggle_bookmark_on_selected(cx);
            }))
            .on_action(cx.listener(|this, _: &NextBookmark, _window, cx| {
                this.jump_next_bookmark(cx);
            }))
            .on_action(cx.listener(|this, _: &PrevBookmark, _window, cx| {
                this.jump_prev_bookmark(cx);
            }))
            .on_action(cx.listener(|this, _: &ClearBookmarks, _window, cx| {
                this.clear_all_bookmarks(cx);
            }))
            .on_action(cx.listener(|this, _: &ExportHtmlReport, window, cx| {
                this.prompt_export_html_report(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleBookmarkDrawer, _window, cx| {
                this.toggle_bookmark_drawer(cx);
            }))
            .on_action(cx.listener(|this, _: &EditBookmarkNote, window, cx| {
                this.open_note_modal_for_selected(cx);
                if this.is_note_modal_open {
                    window.focus(&this.note_input.read(cx).focus_handle);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleEpsTimeline, _window, cx| {
                this.toggle_eps_timeline(cx);
            }))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_dark)
            .text_color(theme.text_primary)
            .when(is_csd && is_floating, |s| s.border_1().border_color(theme.border))
            .child(titlebar_el)
            .child(toolbar_el)
            .children(timeline_ribbon_el)
            .child(body)
            .child(status_bar_el)
            .children(modal_el)
            .children(palette_el)
            .children(bookmark_drawer_el)
            .children(note_modal_el)
            .children(resize_borders)
    }
}

#[cfg(target_os = "linux")]
fn ensure_desktop_integration() {
    let Ok(home) = std::env::var("HOME") else { return; };
    let home_path = std::path::PathBuf::from(home);
    let apps_dir = home_path.join(".local/share/applications");
    let icons_dir = home_path.join(".local/share/icons/hicolor/256x256/apps");
    let desktop_file = apps_dir.join("loganalysis.desktop");
    let icon_file = icons_dir.join("loganalysis.png");

    let _ = std::fs::create_dir_all(&apps_dir);
    let _ = std::fs::create_dir_all(&icons_dir);

    // 1. Write icon if missing
    const ICON_BYTES: &[u8] = include_bytes!("../../../assets/loganalysis.png");
    if !icon_file.exists() {
        let _ = std::fs::write(&icon_file, ICON_BYTES);
    }

    // 2. Resolve executable path (AppImage or binary)
    let exec_path = std::env::var("APPIMAGE")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok());

    if let Some(exec) = exec_path {
        let content = format!(
            "[Desktop Entry]\n\
            Name=LogAnalysis\n\
            Comment=GPU-Accelerated High Performance Log Analysis Tool\n\
            Exec=\"{}\" %F\n\
            Icon={}\n\
            Terminal=false\n\
            Type=Application\n\
            Categories=Development;Utility;System;\n\
            MimeType=text/plain;application/json;\n\
            StartupWMClass=loganalysis\n",
            exec.display(),
            icon_file.display()
        );
        let _ = std::fs::write(&desktop_file, content);

        let _ = std::process::Command::new("update-desktop-database")
            .arg(&apps_dir)
            .status();
        let _ = std::process::Command::new("gtk-update-icon-cache")
            .args(["-f", "-t"])
            .arg(home_path.join(".local/share/icons/hicolor"))
            .status();
    }
}

/// Detects initial window decoration mode based on compositor session and explicit overrides.
/// On Linux Wayland (e.g. GNOME Mutter), Client-Side Decorations (CSD) must be explicitly requested
/// because the compositor does not provide server-side window decorations.
pub fn detect_initial_window_decorations() -> Option<WindowDecorations> {
    if std::env::var("LOGANALYSIS_DISABLE_CSD").is_ok() {
        return Some(WindowDecorations::Server);
    }
    if std::env::var("LOGANALYSIS_FORCE_CSD").is_ok() {
        return Some(WindowDecorations::Client);
    }

    #[cfg(target_os = "linux")]
    {
        let is_wayland = std::env::var_os("WAYLAND_DISPLAY")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
            || std::env::var("XDG_SESSION_TYPE")
                .map(|v| v.eq_ignore_ascii_case("wayland"))
                .unwrap_or(false);

        if is_wayland {
            Some(WindowDecorations::Client)
        } else {
            None
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn main() {
    // Intercept version flag early before desktop integration or GPU window creation
    if handle_cli_version_flag(std::env::args().skip(1)) {
        return;
    }

    logging::init_crash_handler();

    #[cfg(target_os = "linux")]
    ensure_desktop_integration();

    // Guard against upstream Zed/GPUI bug on Linux X11:
    // When CJK/Korean IMEs (e.g. ibus, fcitx) send PreeditDraw compound text events,
    // zed-xim panics with 'Encoding Error: UnsupportedEncoding' at client.rs:237.
    // Unsetting XMODIFIERS safely bypasses zed-xim in favor of GPUI's native XKB input handler.
    if std::env::var_os("LOGANALYSIS_KEEP_XMODIFIERS").is_none()
        && std::env::var_os("XMODIFIERS").is_some()
    {
        logging::log_event(
            "INFO",
            "Sanitized XMODIFIERS to prevent upstream zed-xim PreeditDraw UnsupportedEncoding panic on X11",
        );
        std::env::remove_var("XMODIFIERS");
    }

    logging::log_event("INFO", "LogAnalysis starting up");

    let cli_path = std::env::args().nth(1).map(std::path::PathBuf::from);

    Application::new().run(move |cx: &mut App| {
        // Register Global Key Bindings
        cx.bind_keys([
            KeyBinding::new("ctrl-o", OpenFile, None),
            KeyBinding::new("ctrl-h", ToggleFilteredOnly, None),
            KeyBinding::new("ctrl-f", Find, None),
            KeyBinding::new("f3", FindNext, None),
            KeyBinding::new("shift-f3", FindPrev, None),
            KeyBinding::new("ctrl-c", CopySelection, None),
            KeyBinding::new("ctrl-shift-f", AddFilter, None),
            KeyBinding::new("ctrl-s", SaveFilters, None),
            KeyBinding::new("ctrl-shift-s", SaveFiltersAs, None),
            KeyBinding::new("ctrl-shift-o", LoadFilters, None),
            KeyBinding::new("ctrl-k", ClearLogs, None),
            KeyBinding::new("ctrl-=", ZoomIn, None),
            KeyBinding::new("ctrl-+", ZoomIn, None),
            KeyBinding::new("ctrl--", ZoomOut, None),
            KeyBinding::new("ctrl-space", TogglePauseStream, None),
            KeyBinding::new("ctrl-t", AddFilterTab, None),
            KeyBinding::new("ctrl-d", DeleteFilterTab, None),
            KeyBinding::new("ctrl-r", RenameFilterTab, None),
            KeyBinding::new("ctrl-shift-l", ToggleFilterPanelPosition, None),
            KeyBinding::new("ctrl-p", ToggleCommandPalette, None),
            KeyBinding::new("cmd-p", ToggleCommandPalette, None),
            KeyBinding::new("ctrl-shift-p", ToggleCommandPalette, None),
            KeyBinding::new("cmd-shift-p", ToggleCommandPalette, None),
            KeyBinding::new("f2", ToggleBookmark, None),
            KeyBinding::new("shift-f2", PrevBookmark, None),
            KeyBinding::new("ctrl-f2", ToggleBookmark, None),
            KeyBinding::new("alt-f2", NextBookmark, None),
            KeyBinding::new("ctrl-b", ToggleBookmarkDrawer, None),
            KeyBinding::new("cmd-b", ToggleBookmarkDrawer, None),
            KeyBinding::new("alt-enter", EditBookmarkNote, None),
            KeyBinding::new("ctrl-shift-t", ToggleEpsTimeline, None),
            KeyBinding::new("cmd-shift-t", ToggleEpsTimeline, None),
        ]);

        // Register Scoped TextInput Key Bindings (Cut, Copy, Paste, Arrows, Selection, etc.)
        components::text_input::register_keybindings(cx);

        let window_decorations = detect_initial_window_decorations();

        let bounds = Bounds::centered(None, size(px(1200.0), px(850.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("loganalysis".into()),
            titlebar: Some(TitlebarOptions {
                title: Some("LogAnalysis (High Performance)".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            window_decorations,
            ..Default::default()
        };

        let initial_path = cli_path.clone();
        if let Err(e) = cx.open_window(options, move |window, cx| {
            let entity = cx.new(LogAnalysisWorkspace::new);
            entity.update(cx, |workspace, _cx| {
                window.focus(&workspace.focus_handle);
            });
            if let Some(path) = initial_path {
                entity.update(cx, |workspace, cx| {
                    workspace.open_file(&path, cx);
                });
            } else {
                // Auto-scan ADB devices on startup in background when no initial file argument is provided
                entity.update(cx, |workspace, cx| {
                    workspace.refresh_adb_devices(cx);
                });
            }
            entity
        }) {
            logging::log_event(
                "ERROR",
                &format!("Failed to open LogAnalysis window: {:?}", e),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use loganalysis_core::parse_color_spec;

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
        use super::*;

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
        use crate::modals::command_palette::{get_palette_commands, PaletteCommandId};
        use loganalysis_core::fuzzy_match;

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
        open_matches.sort_by(|a, b| b.1.cmp(&a.1));
        assert!(!open_matches.is_empty());
        assert_eq!(open_matches[0].0.id, PaletteCommandId::OpenFile);

        // Fuzzy search for "theme" -> "View: Toggle Dark / Light Theme"
        let mut theme_matches: Vec<_> = commands
            .iter()
            .filter_map(|c| fuzzy_match("theme", c.title).map(|score| (c, score)))
            .collect();
        theme_matches.sort_by(|a, b| b.1.cmp(&a.1));
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
}

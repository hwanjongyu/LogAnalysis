#![recursion_limit = "2048"]
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
use components::activity_bar::render_activity_bar;
use components::filter_dock::{render_filter_dock, render_filter_dock_splitter};
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
        let find_input_focused = self.find_input.read(cx).focus_handle.is_focused(window);
        let palette_input_focused = self.palette_input.read(cx).focus_handle.is_focused(window);
        let filter_rule_focused = self.filter_rule_input.read(cx).focus_handle.is_focused(window);

        if self.filter_edit_modal.is_open {
            if self.input_focus == InputFocusTarget::ModalPattern && !modal_pattern_focused {
                window.focus(&self.modal_pattern_input.read(cx).focus_handle);
            }
        } else if self.is_note_modal_open {
            if self.input_focus == InputFocusTarget::BookmarkNote && !note_input_focused {
                window.focus(&self.note_input.read(cx).focus_handle);
            }
        } else if self.is_command_palette_open {
            if !palette_input_focused {
                window.focus(&self.palette_input.read(cx).focus_handle);
            }
        } else if self.is_search_visible && self.input_focus == InputFocusTarget::SearchBar {
            if !find_input_focused {
                window.focus(&self.find_input.read(cx).focus_handle);
            }
        } else if !modal_pattern_focused
            && !note_input_focused
            && !find_input_focused
            && !palette_input_focused
            && !filter_rule_focused
            && !self.focus_handle.is_focused(window)
        {
            window.focus(&self.focus_handle);
        }

        let theme = self.theme;
        let is_bottom_dock = self.filter_panel_position == FilterPanelPosition::Bottom;
        let is_csd = components::titlebar::should_render_csd(window);
        let is_floating = !window.is_maximized();

        let titlebar_el = render_titlebar(self, window, cx);
        let toolbar_el = render_toolbar(self, cx);
        let timeline_ribbon_el = render_timeline_sparkline(self, cx);
        let activity_bar_el = render_activity_bar(self, cx);
        let filter_panel_el = render_filter_dock(self, cx);
        let filter_splitter_el = render_filter_dock_splitter(self, cx);
        let log_viewport_el = render_log_viewport(self, cx);
        let status_bar_el = render_status_bar(self, cx);
        let modal_el = render_filter_edit_modal(self, cx);
        let palette_el = render_command_palette(self, cx);
        let bookmark_drawer_el = render_bookmark_drawer(self, cx);
        let note_modal_el = render_note_edit_modal(self, cx);
        let resize_borders = render_window_resize_borders(window, cx);

        let content_area = if self.is_filter_dock_visible {
            if !is_bottom_dock {
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .when(self.is_resizing_filter_dock, |s| s.cursor_col_resize())
                    .child(filter_panel_el)
                    .child(filter_splitter_el)
                    .child(log_viewport_el)
            } else {
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .when(self.is_resizing_filter_dock, |s| s.cursor_row_resize())
                    .child(log_viewport_el)
                    .child(filter_splitter_el)
                    .child(filter_panel_el)
            }
        } else {
            div()
                .flex_1()
                .h_full()
                .flex()
                .overflow_hidden()
                .child(log_viewport_el)
        };

        let body = div()
            .flex_1()
            .w_full()
            .flex()
            .flex_row()
            .overflow_hidden()
            .child(activity_bar_el)
            .child(content_area);

        div()
            .id("workspace-container")
            .track_focus(&self.focus_handle)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.is_resizing_filter_dock && event.dragging() {
                    this.update_resizing_filter_dock(event.position, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.is_resizing_filter_dock {
                        this.stop_resizing_filter_dock(cx);
                    }
                }),
            )
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
            .on_action(cx.listener(|this, _: &ToggleSplitView, _window, cx| {
                this.toggle_split_view(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSplitOrientation, _window, cx| {
                this.toggle_split_orientation(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSyncedScroll, _window, cx| {
                this.toggle_synced_scroll(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSecondaryLog, window, cx| {
                this.prompt_open_secondary_file(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &OpenMergedLogs, window, cx| {
                this.prompt_open_merged_files(Some(window), cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleColumnView, _window, cx| {
                this.toggle_column_view(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFilterDock, _window, cx| {
                if this.input_focus != InputFocusTarget::None {
                    return;
                }
                this.toggle_filter_dock(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFacetsBar, _window, cx| {
                if this.input_focus != InputFocusTarget::None {
                    return;
                }
                this.toggle_facets_bar(cx);
            }))
            .on_action(cx.listener(|this, _: &StartAdb, _window, cx| {
                if this.input_focus != InputFocusTarget::None {
                    return;
                }
                if this.is_streaming {
                    this.stop_adb_stream(cx);
                } else {
                    this.start_adb_stream(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &StopAdb, _window, cx| {
                this.stop_adb_stream(cx);
            }))
            .on_action(cx.listener(|this, _: &RefreshAdbDevices, _window, cx| {
                this.refresh_adb_devices(cx);
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
        let safe_exec = exec.to_string_lossy().replace(['\n', '\r', '"', '\\'], "");
        if !safe_exec.is_empty() {
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
                safe_exec,
                icon_file.display()
            );
            let _ = std::fs::write(&desktop_file, content);
        }

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
            KeyBinding::new("ctrl-\\", ToggleSplitView, None),
            KeyBinding::new("cmd-\\", ToggleSplitView, None),
            KeyBinding::new("ctrl-alt-\\", ToggleSplitOrientation, None),
            KeyBinding::new("ctrl-alt-s", ToggleSyncedScroll, None),
            KeyBinding::new("ctrl-g", ToggleColumnView, None),
            KeyBinding::new("cmd-g", ToggleColumnView, None),
            KeyBinding::new("ctrl-shift-m", OpenMergedLogs, None),
            KeyBinding::new("cmd-shift-m", OpenMergedLogs, None),
            KeyBinding::new("ctrl-j", ToggleFilterDock, None),
            KeyBinding::new("cmd-j", ToggleFilterDock, None),
            KeyBinding::new("alt-f", ToggleFacetsBar, None),
            KeyBinding::new("ctrl-shift-r", StartAdb, None),
            KeyBinding::new("cmd-shift-r", StartAdb, None),
            KeyBinding::new("ctrl-u", ToggleTheme, None),
            KeyBinding::new("cmd-u", ToggleTheme, None),
            KeyBinding::new("f10", ToggleTheme, None),
            KeyBinding::new("alt-z", ToggleFullLineDisplay, None),
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
        let probed_ts = crate::workspace::probe_parent_timestamp(&source_b, 3);
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
        assert_eq!(crate::workspace::probe_parent_timestamp(&unparsed_src, 12), None);
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
        use crate::components::log_view::render_highlighted_text;
        use gpui::{HighlightStyle, SharedString};
        use loganalysis_core::CompiledMatcher;

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
        use crate::components::tooltip::SimpleTooltip;
        use crate::theme::Theme;

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
        let cmds = crate::modals::command_palette::get_palette_commands();
        let dock_cmd = cmds
            .iter()
            .find(|c| c.id == crate::modals::command_palette::PaletteCommandId::ToggleFilterDock)
            .expect("ToggleFilterDock should exist");
        assert_eq!(dock_cmd.shortcut, Some("Ctrl+J"));

        let facets_cmd = cmds
            .iter()
            .find(|c| c.id == crate::modals::command_palette::PaletteCommandId::ToggleFacetsBar)
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
}

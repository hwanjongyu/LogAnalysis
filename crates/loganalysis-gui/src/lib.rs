#![recursion_limit = "2048"]

pub mod actions;
pub mod cli;
pub mod components;
pub mod controllers;
pub mod dialogs;
pub mod logging;
pub mod modals;
pub mod selection;
pub mod theme;
pub mod types;
pub mod workspace;

use gpui::prelude::FluentBuilder;
use gpui::*;

use actions::*;
use cli::handle_cli_version_flag;
use components::activity_bar::render_activity_bar;
use components::filter_dock::{render_filter_dock, render_filter_dock_splitter};
use components::log_view::render_log_viewport;
use components::resize_border::render_window_resize_borders;
use components::status_bar::render_status_bar;
use components::timeline_sparkline::{render_timeline_sparkline, should_render_timeline_ribbon};
use components::toolbar::render_unified_header;
use modals::bookmark_drawer::render_bookmark_drawer;
use modals::command_palette::render_command_palette;
use modals::filter_edit_modal::render_filter_edit_modal;
use modals::note_edit_modal::render_note_edit_modal;
use types::{FilterPanelPosition, InputFocusTarget};
pub use workspace::LogAnalysisWorkspace;

/// Returns whether the left Activity Bar is visible for the given Zen mode state.
pub fn workspace_shows_activity_bar(is_zen_mode: bool) -> bool {
    !is_zen_mode
}

/// Returns whether the bottom Status Bar is visible for the given Zen mode state.
pub fn workspace_shows_status_bar(is_zen_mode: bool) -> bool {
    !is_zen_mode
}

/// Returns whether the Filter Dock is visible for the given Zen mode and dock visibility flags.
pub fn workspace_shows_filter_dock(is_zen_mode: bool, is_filter_dock_visible: bool) -> bool {
    !is_zen_mode && is_filter_dock_visible
}

macro_rules! define_default_keybindings {
    ($(($key:literal, $action:path)),* $(,)?) => {
        /// Returns keystroke to canonical action name contract entries used for global shortcuts.
        pub fn default_keybinding_specs() -> &'static [(&'static str, &'static str)] {
            static SPECS: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
                std::sync::OnceLock::new();
            SPECS
                .get_or_init(|| vec![$(($key, gpui::Action::name(&$action))),*])
                .as_slice()
        }

        /// Registers all default global GPUI key bindings.
        pub fn register_global_keybindings(cx: &mut App) {
            cx.bind_keys([
                $(KeyBinding::new($key, $action, None)),*
            ]);
        }
    };
}

define_default_keybindings!(
    ("ctrl-o", OpenFile),
    ("ctrl-h", ToggleFilteredOnly),
    ("ctrl-f", Find),
    ("f3", FindNext),
    ("shift-f3", FindPrev),
    ("ctrl-c", CopySelection),
    ("ctrl-shift-f", AddFilter),
    ("ctrl-s", SaveFilters),
    ("ctrl-shift-s", SaveFiltersAs),
    ("ctrl-shift-o", LoadFilters),
    ("ctrl-k", ClearLogs),
    ("ctrl-=", ZoomIn),
    ("ctrl-+", ZoomIn),
    ("ctrl--", ZoomOut),
    ("ctrl-space", TogglePauseStream),
    ("ctrl-t", AddFilterTab),
    ("ctrl-d", DeleteFilterTab),
    ("ctrl-r", RenameFilterTab),
    ("ctrl-shift-l", ToggleFilterPanelPosition),
    ("ctrl-p", ToggleCommandPalette),
    ("cmd-p", ToggleCommandPalette),
    ("ctrl-shift-p", ToggleCommandPalette),
    ("cmd-shift-p", ToggleCommandPalette),
    ("f2", ToggleBookmark),
    ("shift-f2", PrevBookmark),
    ("ctrl-f2", ToggleBookmark),
    ("alt-f2", NextBookmark),
    ("ctrl-b", ToggleBookmarkDrawer),
    ("cmd-b", ToggleBookmarkDrawer),
    ("alt-enter", EditBookmarkNote),
    ("ctrl-shift-t", ToggleEpsTimeline),
    ("cmd-shift-t", ToggleEpsTimeline),
    ("f11", ToggleZenMode),
    ("ctrl-shift-z", ToggleZenMode),
    ("cmd-shift-z", ToggleZenMode),
    ("ctrl-\\", ToggleSplitView),
    ("cmd-\\", ToggleSplitView),
    ("ctrl-alt-\\", ToggleSplitOrientation),
    ("ctrl-alt-s", ToggleSyncedScroll),
    ("ctrl-g", ToggleColumnView),
    ("cmd-g", ToggleColumnView),
    ("ctrl-shift-m", OpenMergedLogs),
    ("cmd-shift-m", OpenMergedLogs),
    ("ctrl-j", ToggleFilterDock),
    ("cmd-j", ToggleFilterDock),
    ("alt-f", ToggleFacetsBar),
    ("ctrl-shift-r", StartAdb),
    ("cmd-shift-r", StartAdb),
    ("ctrl-u", ToggleTheme),
    ("cmd-u", ToggleTheme),
    ("f10", ToggleTheme),
    ("alt-z", ToggleFullLineDisplay),
);

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
        } else if self.bookmarks.is_note_modal_open {
            if self.input_focus == InputFocusTarget::BookmarkNote && !note_input_focused {
                window.focus(&self.note_input.read(cx).focus_handle);
            }
        } else if self.is_command_palette_open {
            if !palette_input_focused {
                window.focus(&self.palette_input.read(cx).focus_handle);
            }
        } else if self.search_ctrl.is_visible && self.input_focus == InputFocusTarget::SearchBar {
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

        let unified_header_el = render_unified_header(self, window, cx);
        let timeline_ribbon_el =
            if should_render_timeline_ribbon(self.is_timeline_visible, self.is_zen_mode) {
                render_timeline_sparkline(self, cx)
            } else {
                None
            };
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

        let content_area =
            if workspace_shows_filter_dock(self.is_zen_mode, self.is_filter_dock_visible) {
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
            .when(workspace_shows_activity_bar(self.is_zen_mode), |s| {
                s.child(activity_bar_el)
            })
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
                if event.keystroke.key == "escape" && this.handle_escape(cx) {
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
                if this.search_ctrl.is_visible {
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
                if this.bookmarks.is_note_modal_open {
                    window.focus(&this.note_input.read(cx).focus_handle);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleEpsTimeline, _window, cx| {
                this.toggle_eps_timeline(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleZenMode, _window, cx| {
                this.toggle_zen_mode(cx);
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
                if this.adb.is_streaming {
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
            .child(unified_header_el)
            .children(timeline_ribbon_el)
            .child(body)
            .when(workspace_shows_status_bar(self.is_zen_mode), |s| {
                s.child(status_bar_el)
            })
            .children(modal_el)
            .children(palette_el)
            .children(bookmark_drawer_el)
            .children(note_modal_el)
            .children(resize_borders)
    }
}

/// Ensures desktop entry and application icon are installed on Linux desktops.
#[cfg(target_os = "linux")]
pub fn ensure_desktop_integration() {
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
    if let Some(forced) = components::titlebar::check_csd_env_override() {
        return if forced {
            Some(WindowDecorations::Client)
        } else {
            Some(WindowDecorations::Server)
        };
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

/// Launches the LogAnalysis GPUI desktop application.
pub fn run_app() {
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
        register_global_keybindings(cx);

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
    logging::flush_logs();
}

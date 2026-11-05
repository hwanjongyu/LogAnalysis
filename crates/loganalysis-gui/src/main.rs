#![recursion_limit = "512"]

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

use gpui::*;

use actions::*;
use cli::handle_cli_version_flag;
use components::filter_dock::render_filter_dock;
use components::log_view::render_log_viewport;
use components::status_bar::render_status_bar;
use components::titlebar::render_titlebar;
use components::toolbar::render_toolbar;
use modals::filter_edit_modal::render_filter_edit_modal;
use types::{FilterPanelPosition, InputFocusTarget};
pub use workspace::LogAnalysisWorkspace;

impl Render for LogAnalysisWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_pattern_focused = self.modal_pattern_input.read(cx).focus_handle.is_focused(window);
        if self.filter_edit_modal.is_open {
            if self.input_focus == InputFocusTarget::ModalPattern && !modal_pattern_focused {
                window.focus(&self.modal_pattern_input.read(cx).focus_handle);
            }
        } else if !self.focus_handle.is_focused(window) {
            window.focus(&self.focus_handle);
        }

        let theme = self.theme;
        let is_bottom_dock = self.filter_panel_position == FilterPanelPosition::Bottom;

        let titlebar_el = render_titlebar(self, window, cx);
        let toolbar_el = render_toolbar(self, cx);
        let filter_panel_el = render_filter_dock(self, cx);
        let log_viewport_el = render_log_viewport(self, cx);
        let status_bar_el = render_status_bar(self, cx);
        let modal_el = render_filter_edit_modal(self, cx);

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
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_dark)
            .text_color(theme.text_primary)
            .child(titlebar_el)
            .child(toolbar_el)
            .child(body)
            .child(status_bar_el)
            .children(modal_el)
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
        ]);

        // Register Scoped TextInput Key Bindings (Cut, Copy, Paste, Arrows, Selection, etc.)
        components::text_input::register_keybindings(cx);

        let window_decorations = if std::env::var("LOGANALYSIS_FORCE_CSD").is_ok() {
            Some(WindowDecorations::Client)
        } else {
            None
        };

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

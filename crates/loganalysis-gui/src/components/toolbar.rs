use gpui::prelude::FluentBuilder;
use gpui::*;
use loganalysis_core::LogSource;

use crate::components::titlebar::should_render_csd;
use crate::components::tooltip::render_tooltip;
use crate::theme::Theme;
use crate::types::FilterPanelPosition;
use crate::workspace::LogAnalysisWorkspace;

#[allow(dead_code)]
pub fn render_kbd_badge(shortcut: &'static str, theme: Theme) -> impl IntoElement {
    div()
        .px(px(4.0))
        .py(px(0.5))
        .rounded_xs()
        .bg(theme.kbd_bg)
        .border_1()
        .border_color(theme.kbd_border)
        .text_size(px(9.0))
        .font_weight(FontWeight::SEMIBOLD)
        .font_family("monospace")
        .text_color(theme.kbd_fg)
        .child(shortcut)
}

#[allow(dead_code)]
pub fn render_kbd_badge_on_accent(shortcut: &'static str) -> impl IntoElement {
    div()
        .px(px(4.0))
        .py(px(0.5))
        .rounded_xs()
        .bg(rgba(0xffffff2e))
        .text_size(px(9.0))
        .font_weight(FontWeight::SEMIBOLD)
        .font_family("monospace")
        .text_color(rgb(0xffffff))
        .child(shortcut)
}

/// Target height in logical pixels for the unified compact header bar.
pub const UNIFIED_HEADER_HEIGHT: f32 = 38.0;

/// Target dimensions in logical pixels for the app branding logo pill.
pub const LOGO_PILL_SIZE: f32 = 18.0;

/// Renders the unified 38px compact header bar integrating branding, log sources,
/// centralized draggable breadcrumb status pill, view toggles, find, overflow menu,
/// and CSD window controls.
#[allow(clippy::needless_pass_by_ref_mut)]
pub fn render_unified_header(
    workspace: &LogAnalysisWorkspace,
    window: &mut Window,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let is_csd = should_render_csd(window);

    div()
        .id("unified-header-bar")
        .h(px(UNIFIED_HEADER_HEIGHT))
        .w_full()
        .bg(theme.bg_header)
        .border_b_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .justify_between()
        .pl(px(10.0))
        .when(!is_csd, |s| s.pr(px(10.0)))
        .when(is_csd, |s| s.pr(px(0.0)))
        // =====================================================================
        // Left Section: Branding + Open Button + Live ADB Controls
        // =====================================================================
        .child(
            div()
                .id("header-left-section")
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_2()
                // App Branding: 18x18px logo pill "L" + "LogAnalysis" (12px semibold)
                .child(
                    div()
                        .id("header-branding")
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .pr(px(2.0))
                        .cursor_default()
                        .on_mouse_down(MouseButton::Left, cx.listener(|_, event: &MouseDownEvent, window, _| {
                            if event.click_count == 2 {
                                window.zoom_window();
                            } else if event.click_count == 1 {
                                window.start_window_move();
                            }
                        }))
                        .child(
                            div()
                                .w(px(LOGO_PILL_SIZE))
                                .h(px(LOGO_PILL_SIZE))
                                .rounded_sm()
                                .bg(theme.accent)
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xffffff))
                                .child("L"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_primary)
                                .child("LogAnalysis"),
                        ),
                )
                // File Open Button (btn-toolbar-open, Ctrl+O)
                .child(
                    div()
                        .id("btn-toolbar-open")
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded_sm()
                        .bg(theme.accent)
                        .hover(move |s| s.bg(theme.accent_muted).text_color(theme.accent))
                        .tooltip(render_tooltip("Open log file from disk", Some("Ctrl+O"), theme))
                        .text_color(rgb(0xffffff))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.prompt_open_file(Some(window), cx);
                        }))
                        .child("Open"),
                )
                // Live ADB Control Group (Device selector + Pause + Live ADB toggle)
                .child(
                    div()
                        .id("header-adb-controls")
                        .flex()
                        .items_center()
                        .gap_1p5()
                        // Pause Stream Button (when streaming)
                        .children(if workspace.is_streaming {
                            Some(
                                div()
                                    .id("btn-toolbar-pause-adb")
                                    .px(px(7.0))
                                    .py(px(3.0))
                                    .rounded_sm()
                                    .bg(if workspace.is_stream_paused {
                                        theme.warning_amber
                                    } else {
                                        theme.bg_panel
                                    })
                                    .text_color(if workspace.is_stream_paused {
                                        rgb(0x000000)
                                    } else {
                                        theme.text_primary
                                    })
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .tooltip(render_tooltip(
                                        if workspace.is_stream_paused { "Resume streaming logcat" } else { "Pause streaming logcat" },
                                        Some("Ctrl+Space"),
                                        theme,
                                    ))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.toggle_pause_stream(cx);
                                    }))
                                    .child(if workspace.is_stream_paused {
                                        "▶ Resume"
                                    } else {
                                        "⏸ Pause"
                                    }),
                            )
                        } else {
                            None
                        })
                        // ADB Device Selector with Dropdown Popover
                        .child(
                            div()
                                .relative()
                                .child(
                                    div()
                                        .id("btn-toolbar-adb-devices")
                                        .px(px(7.0))
                                        .py(px(3.0))
                                        .rounded_sm()
                                        .bg(if workspace.is_device_selector_open {
                                            theme.btn_hover
                                        } else {
                                            theme.bg_panel
                                        })
                                        .border_1()
                                        .border_color(if workspace.is_device_selector_open {
                                            theme.accent
                                        } else {
                                            theme.border_subtle
                                        })
                                        .hover(move |s| s.bg(theme.btn_hover))
                                        .tooltip(render_tooltip("Select connected Android device or emulator", None, theme))
                                        .text_size(px(11.0))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .gap_1p5()
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            this.toggle_device_selector(cx);
                                        }))
                                        .child(
                                            div()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .rounded_full()
                                                .bg(
                                                    if let Some(info) = workspace.get_selected_device_info() {
                                                        if info.is_ready() {
                                                            theme.success_green
                                                        } else {
                                                            theme.warning_amber
                                                        }
                                                    } else if !workspace.adb_devices.is_empty() {
                                                        theme.success_green
                                                    } else {
                                                        theme.text_muted
                                                    },
                                                ),
                                        )
                                        .child(
                                            div()
                                                .max_w(px(180.0))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .child(
                                                    if workspace.is_scanning_adb {
                                                        "📱 Scanning...".to_string()
                                                    } else if let Some(info) = workspace.get_selected_device_info() {
                                                        format!("📱 {} ▾", info.short_name())
                                                    } else if let Some(dev) = &workspace.selected_device {
                                                        format!("📱 {} ▾", dev)
                                                    } else if !workspace.adb_devices.is_empty() {
                                                        format!("📱 ({} found) ▾", workspace.adb_devices.len())
                                                    } else {
                                                        "📱 No Device ▾".to_string()
                                                    },
                                                ),
                                        ),
                                    )
                                    .children(if workspace.is_device_selector_open {
                                        Some(render_device_dropdown(workspace, cx))
                                    } else {
                                        None
                                    }),
                        )
                        // Start / Stop ADB Stream Button
                        .child(
                            div()
                                .id("btn-toolbar-adb")
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded_sm()
                                .bg(if workspace.is_streaming {
                                    theme.error_red
                                } else {
                                    theme.success_green
                                })
                                .hover(move |s| {
                                    if workspace.is_streaming {
                                        s.bg(rgb(0xdc2626))
                                    } else {
                                        s.bg(rgb(0x16a34a))
                                    }
                                })
                                .tooltip(render_tooltip(
                                    if workspace.is_streaming { "Stop live ADB logcat stream" } else { "Start live ADB logcat stream" },
                                    Some("Ctrl+Shift+R"),
                                    theme,
                                ))
                                .text_color(rgb(0xffffff))
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    if this.is_streaming {
                                        this.stop_adb_stream(cx);
                                    } else {
                                        this.start_adb_stream(cx);
                                    }
                                }))
                                .child(if workspace.is_streaming {
                                    "⏹ Stop ADB"
                                } else {
                                    "⚡ Live ADB"
                                }),
                        ),
                ),
        )
        // =====================================================================
        // Center Section: Draggable Central Breadcrumb Pill
        // =====================================================================
        .child(
            div()
                .id("header-center-section")
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, cx.listener(|_, event: &MouseDownEvent, window, _| {
                    if event.click_count == 2 {
                        window.zoom_window();
                    } else if event.click_count == 1 {
                        window.start_window_move();
                    }
                }))
                .child(
                    div()
                        .id("titlebar-breadcrumb-pill")
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px(px(10.0))
                        .py(px(2.5))
                        .rounded_full()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .text_size(px(11.0))
                        .child(if let Some(name) = &workspace.active_file_name {
                            let lines = workspace.log_source.as_ref().map(|s| s.line_count()).unwrap_or(0);
                            let matched = workspace.filter_result.as_ref().map(|r| r.total_matched_lines).unwrap_or(0);
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(div().text_size(px(12.0)).child("📄"))
                                .child(
                                    div()
                                        .max_w(px(220.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text_primary)
                                        .child(name.clone()),
                                )
                                .child(div().text_color(theme.text_muted).child("•"))
                                .child(
                                    div()
                                        .font_family("monospace")
                                        .text_color(theme.text_muted)
                                        .child(format!("{} lines", lines)),
                                )
                                .when(matched > 0, |parent| {
                                    parent
                                        .child(div().text_color(theme.text_muted).child("•"))
                                        .child(
                                            div()
                                                .px(px(5.0))
                                                .py(px(0.5))
                                                .rounded_xs()
                                                .bg(theme.accent_muted)
                                                .font_family("monospace")
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(theme.accent)
                                                .text_size(px(10.0))
                                                .child(format!("{} matches", matched)),
                                        )
                                })
                                .into_any_element()
                        } else if workspace.is_streaming {
                            let dev_name = workspace
                                .get_selected_device_info()
                                .map(|i| i.short_name())
                                .or_else(|| workspace.selected_device.clone())
                                .unwrap_or_else(|| "Android Device".to_string());
                            let lines = workspace.log_source.as_ref().map(|s| s.line_count()).unwrap_or(0);
                            let status = if workspace.is_stream_paused { "Paused" } else { "Live" };
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(
                                    div()
                                        .w(px(6.5))
                                        .h(px(6.5))
                                        .rounded_full()
                                        .bg(if workspace.is_stream_paused {
                                            theme.warning_amber
                                        } else {
                                            theme.success_green
                                        }),
                                )
                                .child(
                                    div()
                                        .max_w(px(180.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text_primary)
                                        .child(format!("📱 {}", dev_name)),
                                )
                                .child(div().text_color(theme.text_muted).child("•"))
                                .child(
                                    div()
                                        .font_family("monospace")
                                        .text_color(if workspace.is_stream_paused {
                                            theme.warning_amber
                                        } else {
                                            theme.success_green
                                        })
                                        .child(format!("{} • {} lines", status, lines)),
                                )
                                .into_any_element()
                        } else {
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(div().text_size(px(11.0)).child("⚡"))
                                .child(
                                    div()
                                        .text_color(theme.text_muted)
                                        .child("LogAnalysis — Ready"),
                                )
                                .into_any_element()
                        }),
                ),
        )
        // =====================================================================
        // Right Section: View Toggles + Find + Overflow Menu + Window Controls
        // =====================================================================
        .child(
            div()
                .id("header-right-section")
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_2()
                .h_full()
                // Segmented Control: All Lines vs Filtered Only (Ctrl+H)
                .child(
                    div()
                        .id("seg-view-toggle")
                        .flex()
                        .items_center()
                        .p(px(2.0))
                        .rounded_md()
                        .bg(theme.bg_panel)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .id("seg-all-lines")
                                .px(px(8.0))
                                .py(px(2.5))
                                .rounded_sm()
                                .bg(if !workspace.show_only_filtered {
                                    theme.accent
                                } else {
                                    rgba(0x00000000)
                                })
                                .hover(move |s| {
                                    if workspace.show_only_filtered {
                                        s.bg(theme.btn_hover)
                                    } else {
                                        s
                                    }
                                })
                                .tooltip(render_tooltip("Display all ingested log lines", Some("Ctrl+H"), theme))
                                .text_color(if !workspace.show_only_filtered {
                                    rgb(0xffffff)
                                } else {
                                    theme.text_muted
                                })
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    if this.show_only_filtered {
                                        this.toggle_filtered_only(cx);
                                    }
                                }))
                                .child("All Lines"),
                        )
                        .child(
                            div()
                                .id("seg-filtered-only")
                                .px(px(8.0))
                                .py(px(2.5))
                                .rounded_sm()
                                .bg(if workspace.show_only_filtered {
                                    theme.accent
                                } else {
                                    rgba(0x00000000)
                                })
                                .hover(move |s| {
                                    if !workspace.show_only_filtered {
                                        s.bg(theme.btn_hover)
                                    } else {
                                        s
                                    }
                                })
                                .tooltip(render_tooltip("Display only lines matching active filters", Some("Ctrl+H"), theme))
                                .text_color(if workspace.show_only_filtered {
                                    rgb(0xffffff)
                                } else {
                                    theme.text_muted
                                })
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    if !this.show_only_filtered {
                                        this.toggle_filtered_only(cx);
                                    }
                                }))
                                .child("Filtered Only"),
                        ),
                )
                // Find Toggle Button (btn-toolbar-find, Ctrl+F)
                .child(
                    div()
                        .id("btn-toolbar-find")
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded_md()
                        .border_1()
                        .border_color(if workspace.is_search_visible {
                            theme.accent
                        } else {
                            theme.border_subtle
                        })
                        .bg(if workspace.is_search_visible {
                            theme.accent_muted
                        } else {
                            theme.bg_panel
                        })
                        .hover(move |s| {
                            if !workspace.is_search_visible {
                                s.bg(theme.btn_hover)
                            } else {
                                s
                            }
                        })
                        .tooltip(render_tooltip("Search text in log buffer", Some("Ctrl+F"), theme))
                        .text_color(if workspace.is_search_visible {
                            theme.accent
                        } else {
                            theme.text_primary
                        })
                        .text_size(px(11.0))
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.toggle_search_bar(cx);
                        }))
                        .child("🔍 Find"),
                )
                // Overflow Menu Button (btn-toolbar-overflow-menu, ⋯)
                .child(
                    div()
                        .relative()
                        .child(
                            div()
                                .id("btn-toolbar-overflow-menu")
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded_md()
                                .bg(if workspace.is_overflow_menu_open {
                                    theme.btn_hover
                                } else {
                                    theme.bg_panel
                                })
                                .border_1()
                                .border_color(if workspace.is_overflow_menu_open {
                                    theme.accent
                                } else {
                                    theme.border_subtle
                                })
                                .hover(move |s| s.bg(theme.btn_hover))
                                .tooltip(render_tooltip("More options and actions", None, theme))
                                .text_size(px(13.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(if workspace.is_overflow_menu_open {
                                    theme.accent
                                } else {
                                    theme.text_primary
                                })
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_center()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.toggle_overflow_menu(cx);
                                }))
                                .child("⋯"),
                        )
                        .children(render_overflow_menu(workspace, cx)),
                )
                // CSD Window Controls (Min, Max, Close) when should_render_csd is true
                .children(crate::components::titlebar::render_window_controls(window, theme, cx)),
        )
}

/// Backwards compatibility wrapper for render_toolbar
#[allow(dead_code)]
pub fn render_toolbar(
    workspace: &LogAnalysisWorkspace,
    _cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    div()
        .id("toolbar-deprecated")
        .h(px(0.0))
        .w_full()
        .bg(theme.bg_header)
}

/// Renders the floating Overflow Menu popover for consolidated secondary toolbar actions.
pub fn render_overflow_menu(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    if !workspace.is_overflow_menu_open {
        return None;
    }
    let theme = workspace.theme;

    Some(
        div()
            .id("popover-toolbar-overflow-menu")
            .occlude()
            .absolute()
            .top(px(32.0))
            .right(px(0.0))
            .w(px(220.0))
            .bg(theme.bg_panel)
            .border_1()
            .border_color(theme.border)
            .rounded_lg()
            .shadow_xl()
            .p(px(4.0))
            .flex()
            .flex_col()
            .gap_1()
            // Item 1: Load Profile... (Ctrl+Shift+O)
            .child(
                div()
                    .id("menu-item-load-profile")
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded_md()
                    .hover(move |s| s.bg(theme.btn_hover))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_overflow_menu(cx);
                        this.prompt_load_filters(Some(window), cx);
                    }))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child("Load Profile..."),
                    )
                    .child(render_kbd_badge("Ctrl+Shift+O", theme)),
            )
            // Item 2: Save Profile... (Ctrl+S)
            .child(
                div()
                    .id("menu-item-save-profile")
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded_md()
                    .hover(move |s| s.bg(theme.btn_hover))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_overflow_menu(cx);
                        this.prompt_save_filters(Some(window), cx);
                    }))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child("Save Profile..."),
                    )
                    .child(render_kbd_badge("Ctrl+S", theme)),
            )
            // Item 3: Clear Logs (Ctrl+K)
            .child(
                div()
                    .id("menu-item-clear-logs")
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded_md()
                    .hover(move |s| s.bg(theme.btn_hover))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.close_overflow_menu(cx);
                        this.clear_logs(cx);
                    }))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child("Clear Logs"),
                    )
                    .child(render_kbd_badge("Ctrl+K", theme)),
            )
            // Divider
            .child(
                div()
                    .h(px(1.0))
                    .w_full()
                    .bg(theme.border_subtle)
                    .my(px(2.0)),
            )
            // Item 4: Dock Position: Bottom / Left (Ctrl+Shift+L)
            .child(
                div()
                    .id("menu-item-toggle-dock")
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded_md()
                    .hover(move |s| s.bg(theme.btn_hover))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.close_overflow_menu(cx);
                        this.toggle_filter_panel_position(cx);
                    }))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(
                                if workspace.filter_panel_position == FilterPanelPosition::Bottom {
                                    "Dock: Bottom"
                                } else {
                                    "Dock: Left"
                                },
                            ),
                    )
                    .child(render_kbd_badge("Ctrl+Shift+L", theme)),
            )
            // Item 5: Toggle Horizontal Scroll (Alt+Z)
            .child(
                div()
                    .id("menu-item-toggle-hscroll")
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded_md()
                    .hover(move |s| s.bg(theme.btn_hover))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.close_overflow_menu(cx);
                        this.toggle_full_line_display(cx);
                    }))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(
                                if workspace.full_line_display {
                                    "H-Scroll: On"
                                } else {
                                    "H-Scroll: Off"
                                },
                            ),
                    )
                    .child(render_kbd_badge("Alt+Z", theme)),
            )
            // Item 6: Toggle Theme (Ctrl+U)
            .child(
                div()
                    .id("menu-item-toggle-theme")
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded_md()
                    .hover(move |s| s.bg(theme.btn_hover))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.close_overflow_menu(cx);
                        this.toggle_theme(cx);
                    }))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(
                                if workspace.is_dark_theme {
                                    "Theme: Light"
                                } else {
                                    "Theme: Dark"
                                },
                            ),
                    )
                    .child(render_kbd_badge("Ctrl+U", theme)),
            ),
    )
}

/// Renders the floating ADB device selector popover menu
fn render_device_dropdown(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let selected_serial = workspace.selected_device.clone();

    div()
        .id("dropdown-adb-devices")
        .absolute()
        .top(px(30.0))
        .right(px(0.0))
        .w(px(290.0))
        .bg(theme.bg_panel)
        .border_1()
        .border_color(theme.border)
        .rounded_lg()
        .shadow_lg()
        .p(px(6.0))
        .flex()
        .flex_col()
        .gap_1()
        // Header
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(6.0))
                .py(px(4.0))
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text_muted)
                        .child(format!(
                            "CONNECTED DEVICES ({})",
                            workspace.adb_device_details.len()
                        )),
                )
                .child(
                    div()
                        .id("btn-dropdown-rescan")
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded_sm()
                        .bg(theme.bg_card)
                        .hover(move |s| s.bg(theme.btn_hover))
                        .text_size(px(10.0))
                        .text_color(theme.accent)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.refresh_adb_devices(cx);
                        }))
                        .child("🔄 Rescan"),
                ),
        )
        // Device Items List
        .children(if workspace.adb_device_details.is_empty() {
            vec![
                div()
                    .px(px(8.0))
                    .py(px(12.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(if workspace.is_scanning_adb {
                        "Scanning for devices..."
                    } else {
                        "No Android devices detected"
                    })
                    .child(
                        div()
                            .text_size(px(9.5))
                            .text_color(theme.text_muted)
                            .child("Check USB cable & Developer Options"),
                    )
                    .into_any_element(),
            ]
        } else {
            workspace
                .adb_device_details
                .iter()
                .map(|dev| {
                    let serial = dev.serial.clone();
                    let is_selected = selected_serial.as_ref() == Some(&serial);
                    let is_ready = dev.is_ready();

                    div()
                        .id(ElementId::Name(format!("device-item-{}", serial).into()))
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded_md()
                        .bg(if is_selected {
                            theme.bg_card
                        } else {
                            theme.bg_panel
                        })
                        .border_1()
                        .border_color(if is_selected {
                            theme.border
                        } else {
                            rgba(0x00000000)
                        })
                        .hover(move |s| s.bg(theme.btn_hover))
                        .cursor_pointer()
                        .on_click(cx.listener({
                            let s = serial.clone();
                            move |this, _, _window, cx| {
                                this.select_adb_device(s.clone(), cx);
                            }
                        }))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .w(px(6.5))
                                        .h(px(6.5))
                                        .rounded_full()
                                        .bg(if is_ready {
                                            theme.success_green
                                        } else {
                                            theme.warning_amber
                                        }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_size(px(11.5))
                                                .font_weight(if is_selected {
                                                    FontWeight::BOLD
                                                } else {
                                                    FontWeight::MEDIUM
                                                })
                                                .text_color(theme.text_primary)
                                                .child(dev.short_name()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(9.5))
                                                .font_family("monospace")
                                                .text_color(theme.text_muted)
                                                .child(format!("{} • {}", dev.serial, dev.state)),
                                        ),
                                ),
                        )
                        .child(if is_selected {
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.accent)
                                .font_weight(FontWeight::BOLD)
                                .child("✓")
                        } else {
                            div()
                        })
                        .into_any_element()
                })
                .collect()
        })
}

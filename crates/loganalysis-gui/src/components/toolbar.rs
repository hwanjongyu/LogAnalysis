use gpui::prelude::FluentBuilder;
use gpui::*;
use loganalysis_core::LogSource;

use crate::components::tooltip::render_tooltip;
use crate::theme::Theme;
use crate::types::FilterPanelPosition;
use crate::workspace::LogAnalysisWorkspace;

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

pub fn render_toolbar(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;

    div()
        .h(px(44.0))
        .w_full()
        .bg(theme.bg_header)
        .border_b_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .justify_between()
        .px(px(14.0))
        // Left Section: Brand + File Actions + Active File Pill
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                // Modernized App Brand Pill
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .pr(px(6.0))
                        .child(
                            div()
                                .w(px(9.0))
                                .h(px(9.0))
                                .rounded_sm()
                                .bg(theme.accent)
                                .shadow_sm(),
                        )
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(13.0))
                                .text_color(theme.text_primary)
                                .child("LogAnalysis"),
                        )
                        .child(
                            div()
                                .px(px(4.0))
                                .py(px(0.5))
                                .rounded_xs()
                                .bg(theme.bg_badge)
                                .text_size(px(8.5))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.accent)
                                .child("STUDIO"),
                        ),
                )
                // File Action Group
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .p(px(2.0))
                        .rounded_md()
                        .bg(theme.bg_panel)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .id("btn-toolbar-open")
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded_sm()
                                .bg(theme.accent)
                                .hover(move |s| s.bg(theme.accent_muted).text_color(theme.accent))
                                .tooltip(render_tooltip("Open log file from disk", Some("Ctrl+O"), theme))
                                .text_color(rgb(0xffffff))
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prompt_open_file(Some(window), cx);
                                }))
                                .child("Open")
                                .child(render_kbd_badge_on_accent("Ctrl+O")),
                        )
                        .child(
                            div()
                                .id("btn-toolbar-load-filters")
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded_sm()
                                .hover(move |s| s.bg(theme.btn_hover))
                                .tooltip(render_tooltip("Load saved filter preset profile", Some("Ctrl+Shift+O"), theme))
                                .text_size(px(11.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prompt_load_filters(Some(window), cx);
                                }))
                                .child("Load Profile"),
                        )
                        .child(
                            div()
                                .id("btn-toolbar-save-filters")
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded_sm()
                                .hover(move |s| s.bg(theme.btn_hover))
                                .tooltip(render_tooltip("Save current filter rules to file", Some("Ctrl+S"), theme))
                                .text_size(px(11.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prompt_save_filters(Some(window), cx);
                                }))
                                .child("Save Profile"),
                        )
                        .child(
                            div()
                                .id("btn-toolbar-clear-logs")
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded_sm()
                                .hover(move |s| s.bg(theme.btn_hover))
                                .tooltip(render_tooltip("Clear current log buffer", Some("Ctrl+K"), theme))
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.clear_logs(cx);
                                }))
                                .child("Clear")
                                .child(render_kbd_badge("Ctrl+K", theme)),
                        ),
                )
                // Contextual Breadcrumb Trail
                .child(
                    div()
                        .px(px(10.0))
                        .py(px(3.5))
                        .rounded_full()
                        .bg(theme.bg_badge)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .flex()
                        .items_center()
                        .gap_1p5()
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
                                .child(div().text_color(theme.text_muted).child("›"))
                                .child(
                                    div()
                                        .font_family("monospace")
                                        .text_color(theme.text_muted)
                                        .child(format!("{} lines", lines)),
                                )
                                .when(matched > 0, |parent| {
                                    parent
                                        .child(div().text_color(theme.text_muted).child("›"))
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
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(div().text_size(px(12.0)).child("📱"))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text_primary)
                                        .child(dev_name),
                                )
                                .child(div().text_color(theme.text_muted).child("›"))
                                .child(
                                    div()
                                        .font_family("monospace")
                                        .text_color(theme.success_green)
                                        .child(format!("Live • {} lines", lines)),
                                )
                                .into_any_element()
                        } else {
                            div()
                                .text_color(theme.text_muted)
                                .child("No active log")
                                .into_any_element()
                        }),
                ),
        )
        // Center Section: Segmented View Toggles & Selection Actions
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                // Segmented Control: All vs Filtered Only
                .child(
                    div()
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
                                .py(px(3.0))
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
                                .py(px(3.0))
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
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    if !this.show_only_filtered {
                                        this.toggle_filtered_only(cx);
                                    }
                                }))
                                .child("Filtered Only")
                                .child(render_kbd_badge("Ctrl+H", theme)),
                        ),
                )
                // View Options Pill Group: Full Width & Auto-Scroll
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .p(px(2.0))
                        .rounded_md()
                        .bg(theme.bg_panel)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .id("btn-toolbar-full-line")
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded_sm()
                                .bg(if workspace.full_line_display {
                                    theme.accent_muted
                                } else {
                                    rgba(0x00000000)
                                })
                                .hover(move |s| s.bg(theme.btn_hover))
                                .tooltip(render_tooltip("Toggle horizontal scrolling vs clipping", Some("Alt+Z"), theme))
                                .text_color(if workspace.full_line_display {
                                    theme.accent
                                } else {
                                    theme.text_muted
                                })
                                .text_size(px(11.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.toggle_full_line_display(cx);
                                }))
                                .child(if workspace.full_line_display {
                                    "↔ H-Scroll"
                                } else {
                                    "⇥ Clip"
                                }),
                        )
                        .child(
                            div()
                                .id("btn-toolbar-auto-scroll")
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded_sm()
                                .bg(if workspace.auto_scroll {
                                    theme.accent_muted
                                } else {
                                    rgba(0x00000000)
                                })
                                .hover(move |s| s.bg(theme.btn_hover))
                                .tooltip(render_tooltip("Lock view to tail (auto-scroll)", Some("End"), theme))
                                .text_color(if workspace.auto_scroll {
                                    theme.accent
                                } else {
                                    theme.text_muted
                                })
                                .text_size(px(11.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.toggle_auto_scroll(cx);
                                }))
                                .child("⬇ Tail"),
                        )
                        // Filter Dock Position Toggle (Left vs Bottom)
                        .child(
                            div()
                                .id("btn-toolbar-filter-dock")
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded_sm()
                                .bg(
                                    if workspace.filter_panel_position
                                        == FilterPanelPosition::Bottom
                                    {
                                        theme.accent_muted
                                    } else {
                                        rgba(0x00000000)
                                    },
                                )
                                .hover(move |s| s.bg(theme.btn_hover))
                                .tooltip(render_tooltip("Toggle filter dock between Left and Bottom", Some("Ctrl+Shift+L"), theme))
                                .text_color(
                                    if workspace.filter_panel_position
                                        == FilterPanelPosition::Bottom
                                    {
                                        theme.accent
                                    } else {
                                        theme.text_muted
                                    },
                                )
                                .text_size(px(11.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.toggle_filter_panel_position(cx);
                                }))
                                .child(
                                    if workspace.filter_panel_position
                                        == FilterPanelPosition::Bottom
                                    {
                                        "⬒ Bottom"
                                    } else {
                                        "◨ Left"
                                    },
                                ),
                        ),
                )
                // Selection Quick Actions
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .p(px(2.0))
                        .rounded_md()
                        .bg(theme.bg_panel)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .child(
                            div()
                                .id("btn-toolbar-copy-line")
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded_sm()
                                .hover(move |s| s.bg(theme.btn_hover))
                                .tooltip(render_tooltip("Copy selected log line to clipboard", Some("Ctrl+C"), theme))
                                .text_size(px(11.0))
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.copy_selection(cx);
                                }))
                                .child("Copy")
                                .child(render_kbd_badge("Ctrl+C", theme)),
                        )
                        .child(
                            div()
                                .id("btn-toolbar-quick-filter")
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded_sm()
                                .hover(move |s| s.bg(theme.btn_hover))
                                .tooltip(render_tooltip("Create instant filter rule from current selection", None, theme))
                                .text_size(px(11.0))
                                .text_color(theme.accent)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.add_quick_filter_from_selection(false, cx);
                                }))
                                .child("+ Filter Sel"),
                        ),
                ),
        )
        // Right Section: Search, Theme Toggle, and ADB Live Stream Pill
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                // Find Toggle Button
                .child(
                    div()
                        .id("btn-toolbar-find")
                        .px(px(10.0))
                        .py(px(4.0))
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
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.toggle_search_bar(cx);
                        }))
                        .child("🔍 Find")
                        .child(render_kbd_badge("Ctrl+F", theme)),
                )
                // Theme Switcher
                .child(
                    div()
                        .id("btn-toolbar-theme-toggle")
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded_md()
                        .bg(theme.bg_panel)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .hover(move |s| s.bg(theme.btn_hover))
                        .tooltip(render_tooltip("Toggle Dark / Light theme", Some("Ctrl+U"), theme))
                        .text_size(px(11.0))
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.toggle_theme(cx);
                        }))
                        .child(if workspace.is_dark_theme {
                            "☀ Light"
                        } else {
                            "🌙 Dark"
                        }),
                )
                // Pause Stream Button (when streaming)
                .children(if workspace.is_streaming {
                    Some(
                        div()
                            .id("btn-toolbar-pause-adb")
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded_md()
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
                // ADB Device Selector with Floating Dropdown Menu
                .child(
                    div()
                        .relative()
                        .child(
                            div()
                                .id("btn-toolbar-adb-devices")
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded_md()
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
                                        .w(px(6.5))
                                        .h(px(6.5))
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
                                    div().child(
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
                // Start / Stop ADB Stream Pill
                .child(
                    div()
                        .id("btn-toolbar-adb")
                        .px(px(10.0))
                        .py(px(4.0))
                        .rounded_md()
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
                            Some("Ctrl+R"),
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

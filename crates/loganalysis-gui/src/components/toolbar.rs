use gpui::*;

use crate::types::FilterPanelPosition;
use crate::workspace::LogAnalysisWorkspace;

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
                // App Brand Pill
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .pr(px(6.0))
                        .child(
                            div()
                                .w(px(8.0))
                                .h(px(8.0))
                                .rounded_full()
                                .bg(theme.accent),
                        )
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(13.0))
                                .child("LogAnalysis"),
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
                                .text_color(rgb(0xffffff))
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prompt_open_file(Some(window), cx);
                                }))
                                .child("Open"),
                        )
                        .child(
                            div()
                                .id("btn-toolbar-load-filters")
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded_sm()
                                .hover(move |s| s.bg(theme.btn_hover))
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
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.clear_logs(cx);
                                }))
                                .child("Clear"),
                        ),
                )
                // Active File Pill Badge
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded_full()
                        .bg(theme.bg_badge)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .text_size(px(11.0))
                        .font_family("monospace")
                        .text_color(if workspace.active_file_name.is_some() {
                            theme.text_primary
                        } else {
                            theme.text_muted
                        })
                        .child(if let Some(name) = &workspace.active_file_name {
                            format!("📄 {}", name)
                        } else {
                            "No active log".into()
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
                                        "⬒ Bottom Dock"
                                    } else {
                                        "◨ Left Dock"
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
                                .text_size(px(11.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.copy_selection(cx);
                                }))
                                .child("Copy Line"),
                        )
                        .child(
                            div()
                                .id("btn-toolbar-quick-filter")
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded_sm()
                                .hover(move |s| s.bg(theme.btn_hover))
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
                // ADB Device Selector / Scan Button
                .child(
                    div()
                        .id("btn-toolbar-adb-devices")
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded_md()
                        .bg(theme.bg_panel)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .hover(move |s| s.bg(theme.btn_hover))
                        .text_size(px(11.0))
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.refresh_adb_devices(cx);
                        }))
                        .child(if let Some(dev) = &workspace.selected_device {
                            format!("📱 {}", dev)
                        } else if !workspace.adb_devices.is_empty() {
                            format!("📱 ({} found)", workspace.adb_devices.len())
                        } else {
                            "📱 Scan ADB".to_string()
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

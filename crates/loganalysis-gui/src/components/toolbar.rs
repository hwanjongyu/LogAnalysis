use gpui::*;
use loganalysis_core::LogSource;

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
                                .hover(move |s| s.bg(theme.accent_muted).text_color(theme.accent))
                                .text_color(rgb(0xffffff))
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prompt_open_file(Some(window), cx);
                                }))
                                .child("Open (Ctrl+O)"),
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
                                .child("Clear (Ctrl+K)"),
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
                            let lines = workspace.log_source.as_ref().map(|s| s.line_count()).unwrap_or(0);
                            format!("📄 {} ({} lines)", name, lines)
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
                                .hover(move |s| {
                                    if workspace.show_only_filtered {
                                        s.bg(theme.btn_hover)
                                    } else {
                                        s
                                    }
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
                                .hover(move |s| {
                                    if !workspace.show_only_filtered {
                                        s.bg(theme.btn_hover)
                                    } else {
                                        s
                                    }
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
                                .child("Filtered Only (Ctrl+H)"),
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
                                        "⬒ Bottom (Ctrl+Shift+L)"
                                    } else {
                                        "◨ Left (Ctrl+Shift+L)"
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
                                .child("Copy (Ctrl+C)"),
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
                        .hover(move |s| {
                            if !workspace.is_search_visible {
                                s.bg(theme.btn_hover)
                            } else {
                                s
                            }
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
                        .child("🔍 Find (Ctrl+F)"),
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

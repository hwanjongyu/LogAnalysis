use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::workspace::LogAnalysisWorkspace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteCommandId {
    OpenFile,
    LoadFilters,
    SaveFilters,
    SaveFiltersAs,
    Find,
    ClearLogs,
    ToggleFilteredOnly,
    AddFilterRule,
    AddFilterTab,
    DeleteFilterTab,
    RenameFilterTab,
    StartAdbStream,
    StopAdbStream,
    TogglePauseStream,
    ToggleAutoScroll,
    RescanAdbDevices,
    ToggleFilterPanelPosition,
    ToggleFilterDock,
    ToggleFacetsBar,
    ToggleFullLineDisplay,
    ToggleTheme,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    ToggleBookmark,
    NextBookmark,
    PrevBookmark,
    ClearBookmarks,
    ExportHtmlReport,
    ToggleBookmarkDrawer,
    EditBookmarkNote,
    ToggleEpsTimeline,
    ToggleSplitView,
    ToggleSplitOrientation,
    ToggleSyncedScroll,
    OpenSecondaryLog,
    ToggleColumnView,
    OpenMergedLogs,
}

#[derive(Clone, Debug)]
pub struct PaletteCommand {
    pub id: PaletteCommandId,
    pub title: &'static str,
    pub category: &'static str,
    pub shortcut: Option<&'static str>,
}

pub fn get_palette_commands() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            id: PaletteCommandId::OpenFile,
            title: "File: Open Log File...",
            category: "FILE",
            shortcut: Some("Ctrl+O"),
        },
        PaletteCommand {
            id: PaletteCommandId::LoadFilters,
            title: "File: Load Filters Profile...",
            category: "FILE",
            shortcut: Some("Ctrl+Shift+O"),
        },
        PaletteCommand {
            id: PaletteCommandId::SaveFilters,
            title: "File: Save Filters Profile",
            category: "FILE",
            shortcut: Some("Ctrl+S"),
        },
        PaletteCommand {
            id: PaletteCommandId::SaveFiltersAs,
            title: "File: Save Filters Profile As...",
            category: "FILE",
            shortcut: Some("Ctrl+Shift+S"),
        },
        PaletteCommand {
            id: PaletteCommandId::Find,
            title: "Log: Find / Search in Log",
            category: "LOG",
            shortcut: Some("Ctrl+F"),
        },
        PaletteCommand {
            id: PaletteCommandId::ClearLogs,
            title: "Log: Clear Loaded Logs",
            category: "LOG",
            shortcut: Some("Ctrl+K"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleFilteredOnly,
            title: "Filter: Toggle Show Only Filtered Lines",
            category: "FILTER",
            shortcut: Some("Ctrl+H"),
        },
        PaletteCommand {
            id: PaletteCommandId::AddFilterRule,
            title: "Filter: Add New Filter Rule...",
            category: "FILTER",
            shortcut: Some("Ctrl+Shift+F"),
        },
        PaletteCommand {
            id: PaletteCommandId::AddFilterTab,
            title: "Filter: Add New Filter Tab",
            category: "FILTER",
            shortcut: Some("Ctrl+T"),
        },
        PaletteCommand {
            id: PaletteCommandId::DeleteFilterTab,
            title: "Filter: Delete Current Filter Tab",
            category: "FILTER",
            shortcut: Some("Ctrl+D"),
        },
        PaletteCommand {
            id: PaletteCommandId::RenameFilterTab,
            title: "Filter: Rename Current Filter Tab",
            category: "FILTER",
            shortcut: Some("Ctrl+R"),
        },
        PaletteCommand {
            id: PaletteCommandId::StartAdbStream,
            title: "ADB: Start Live Logcat Stream",
            category: "ADB",
            shortcut: Some("Ctrl+Shift+R"),
        },
        PaletteCommand {
            id: PaletteCommandId::StopAdbStream,
            title: "ADB: Stop Live Logcat Stream",
            category: "ADB",
            shortcut: None,
        },
        PaletteCommand {
            id: PaletteCommandId::TogglePauseStream,
            title: "ADB: Toggle Pause / Resume Stream",
            category: "ADB",
            shortcut: Some("Ctrl+Space"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleAutoScroll,
            title: "ADB: Toggle Auto-Scroll to Bottom",
            category: "ADB",
            shortcut: None,
        },
        PaletteCommand {
            id: PaletteCommandId::RescanAdbDevices,
            title: "ADB: Rescan Connected Devices",
            category: "ADB",
            shortcut: None,
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleFilterPanelPosition,
            title: "View: Toggle Filter Dock Position (Bottom / Right)",
            category: "VIEW",
            shortcut: Some("Ctrl+Shift+L"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleFilterDock,
            title: "View: Toggle Filter Rules Dock Visibility",
            category: "VIEW",
            shortcut: Some("Ctrl+J"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleFacetsBar,
            title: "View: Toggle Forensic Facets Bar",
            category: "VIEW",
            shortcut: Some("Alt+F"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleFullLineDisplay,
            title: "View: Toggle Full Line Wrap Mode",
            category: "VIEW",
            shortcut: Some("Alt+Z"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleTheme,
            title: "View: Toggle Dark / Light Theme",
            category: "VIEW",
            shortcut: Some("Ctrl+U"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleEpsTimeline,
            title: "View: Toggle Throughput & EPS Timeline Sparkline",
            category: "VIEW",
            shortcut: Some("Ctrl+Shift+T"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleSplitView,
            title: "View: Toggle Dual Split View (Side-by-Side)",
            category: "VIEW",
            shortcut: Some("Ctrl+\\"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleSplitOrientation,
            title: "View: Toggle Split Orientation (Vertical / Horizontal)",
            category: "VIEW",
            shortcut: Some("Ctrl+Alt+\\"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleSyncedScroll,
            title: "View: Toggle Synchronized Timestamp Scrolling",
            category: "VIEW",
            shortcut: Some("Ctrl+Alt+S"),
        },
        PaletteCommand {
            id: PaletteCommandId::OpenSecondaryLog,
            title: "File: Open Log in Secondary Split Pane...",
            category: "FILE",
            shortcut: None,
        },
        PaletteCommand {
            id: PaletteCommandId::OpenMergedLogs,
            title: "File: Open and Merge Multiple Log Files (Chronological)...",
            category: "FILE",
            shortcut: Some("Ctrl+Shift+M"),
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleColumnView,
            title: "View: Toggle Structured Column Grid View",
            category: "VIEW",
            shortcut: Some("Ctrl+G"),
        },
        PaletteCommand {
            id: PaletteCommandId::ZoomIn,
            title: "View: Zoom In (+)",
            category: "VIEW",
            shortcut: Some("Ctrl+="),
        },
        PaletteCommand {
            id: PaletteCommandId::ZoomOut,
            title: "View: Zoom Out (-)",
            category: "VIEW",
            shortcut: Some("Ctrl+-"),
        },
        PaletteCommand {
            id: PaletteCommandId::ResetZoom,
            title: "View: Reset Zoom (Default 13px)",
            category: "VIEW",
            shortcut: None,
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleBookmark,
            title: "Bookmark: Toggle Bookmark on Selected Line",
            category: "BOOKMARK",
            shortcut: Some("F2"),
        },
        PaletteCommand {
            id: PaletteCommandId::NextBookmark,
            title: "Bookmark: Jump to Next Bookmark",
            category: "BOOKMARK",
            shortcut: Some("F2"),
        },
        PaletteCommand {
            id: PaletteCommandId::PrevBookmark,
            title: "Bookmark: Jump to Previous Bookmark",
            category: "BOOKMARK",
            shortcut: Some("Shift+F2"),
        },
        PaletteCommand {
            id: PaletteCommandId::ClearBookmarks,
            title: "Bookmark: Clear All Bookmarks",
            category: "BOOKMARK",
            shortcut: None,
        },
        PaletteCommand {
            id: PaletteCommandId::ToggleBookmarkDrawer,
            title: "Bookmark: Toggle Bookmarks Drawer",
            category: "BOOKMARK",
            shortcut: Some("Ctrl+B"),
        },
        PaletteCommand {
            id: PaletteCommandId::EditBookmarkNote,
            title: "Bookmark: Edit Note on Selected Line",
            category: "BOOKMARK",
            shortcut: Some("Alt+Enter"),
        },
        PaletteCommand {
            id: PaletteCommandId::ExportHtmlReport,
            title: "Export: Generate & Export HTML Incident Report...",
            category: "EXPORT",
            shortcut: None,
        },
    ]
}

/// Renders the floating Command Palette modal.
pub fn render_command_palette(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    if !workspace.is_command_palette_open {
        return None;
    }

    let theme = workspace.theme;
    let query = workspace.palette_input.read(cx).text().to_string();
    let commands = workspace.get_filtered_palette_commands(&query);
    let selected_idx = workspace.palette_selected_index.min(commands.len().saturating_sub(1));

    let items_el = if commands.is_empty() {
        div()
            .w_full()
            .p_4()
            .bg(theme.bg_card)
            .text_sm()
            .text_color(theme.text_muted)
            .child("No matching commands found.")
            .into_any_element()
    } else {
        div()
            .id("palette-items-list")
            .w_full()
            .flex_1()
            .flex()
            .flex_col()
            .max_h(px(380.0))
            .bg(theme.bg_card)
            .overflow_y_scroll()
            .children(commands.iter().enumerate().map(|(idx, cmd)| {
                let is_selected = idx == selected_idx;
                let cmd_id = cmd.id;

                let category_bg = match cmd.category {
                    "FILE" => rgba(0x3b82f625),
                    "ADB" => rgba(0x10b98125),
                    "FILTER" => rgba(0xf59e0b25),
                    "VIEW" => rgba(0x8b5cf625),
                    _ => rgba(0x6b728025),
                };

                let category_fg = match cmd.category {
                    "FILE" => rgb(0x3b82f6),
                    "ADB" => rgb(0x10b981),
                    "FILTER" => rgb(0xf59e0b),
                    "VIEW" => rgb(0x8b5cf6),
                    _ => rgb(0x9ca3af),
                };

                div()
                    .id(ElementId::NamedInteger("palette-item".into(), idx as u64))
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .cursor_pointer()
                    .when(is_selected, |s| {
                        s.bg(rgba(0x3b82f622))
                            .border_l_2()
                            .border_color(theme.accent)
                    })
                    .when(!is_selected, |s| {
                        s.hover(|h| h.bg(rgba(0xffffff08)))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.execute_palette_command(cmd_id, Some(window), cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2p5()
                            // Category Badge
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_xs()
                                    .bg(category_bg)
                                    .text_color(category_fg)
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(cmd.category),
                            )
                            // Command Title
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(if is_selected {
                                        theme.text_primary
                                    } else {
                                        theme.text_muted
                                    })
                                    .child(cmd.title),
                            ),
                    )
                    // Shortcut Badge
                    .child(if let Some(shortcut) = cmd.shortcut {
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_xs()
                            .bg(theme.bg_dark)
                            .border_1()
                            .border_color(theme.border)
                            .font_family("monospace")
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(shortcut)
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    })
            }))
            .into_any_element()
    };

    Some(
        div()
            .id("modal-overlay-command-palette")
            .occlude()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000088))
            .flex()
            .flex_col()
            .items_center()
            .pt(px(72.0))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _window, cx| {
                    this.close_command_palette(cx);
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.close_command_palette(cx);
                } else if event.keystroke.key == "up" {
                    if this.palette_selected_index > 0 {
                        this.palette_selected_index -= 1;
                        cx.notify();
                    }
                } else if event.keystroke.key == "down" {
                    let query = this.palette_input.read(cx).text().to_string();
                    let matches = this.get_filtered_palette_commands(&query);
                    if this.palette_selected_index + 1 < matches.len() {
                        this.palette_selected_index += 1;
                        cx.notify();
                    }
                } else if event.keystroke.key == "enter" {
                    this.submit_command_palette(Some(window), cx);
                }
            }))
            .child(
                div()
                    .id("command-palette-card")
                    .w(px(600.0))
                    .max_h(px(480.0))
                    .bg(theme.bg_card)
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .shadow_xl()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(1.0))
                            .w_full()
                            .bg(theme.border_highlight),
                    )
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == "up" {
                            if this.palette_selected_index > 0 {
                                this.palette_selected_index -= 1;
                                cx.notify();
                            }
                        } else if event.keystroke.key == "down" {
                            let query = this.palette_input.read(cx).text().to_string();
                            let matches = this.get_filtered_palette_commands(&query);
                            if this.palette_selected_index + 1 < matches.len() {
                                this.palette_selected_index += 1;
                                cx.notify();
                            }
                        } else if event.keystroke.key == "enter" {
                            this.submit_command_palette(Some(window), cx);
                        }
                    }))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    // Top Input Row
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_2p5()
                            .bg(theme.bg_card)
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(theme.accent)
                                    .child(">"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .child(workspace.palette_input.clone()),
                            ),
                    )
                    // Commands List
                    .child(items_el)
                    // Bottom Keyboard Hint Footer
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_3()
                            .py_1p5()
                            .bg(theme.bg_header)
                            .border_t_1()
                            .border_color(theme.border)
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child("↑↓ navigate")
                                    .child("↵ select")
                                    .child("esc dismiss"),
                            )
                            .child(format!("{} commands", commands.len())),
                    ),
            ),
    )
}

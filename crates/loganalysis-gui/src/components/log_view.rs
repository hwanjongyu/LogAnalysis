use gpui::prelude::FluentBuilder;
use gpui::*;
use loganalysis_core::{parse_logcat_column_line, LineSeverity, LogSource};

use crate::components::find_bar::render_find_bar;
use crate::workspace::LogAnalysisWorkspace;

pub fn render_log_viewport(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;

    if !workspace.is_split_view {
        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.bg_dark)
            .overflow_hidden()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.is_dragging_scrollbar && event.dragging() {
                    this.scroll_to_ratio_from_mouse_y(event.position.y, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.is_dragging_scrollbar {
                        this.is_dragging_scrollbar = false;
                        cx.notify();
                    }
                }),
            )
            // Interactive Find Bar
            .children(render_find_bar(workspace, cx))
            // Single primary viewport content
            .child(render_pane_content(workspace, true, false, cx))
    } else {
        let is_vertical = workspace.is_split_vertical;
        let mut split_container = div()
            .flex_1()
            .h_full()
            .flex()
            .overflow_hidden();

        if is_vertical {
            split_container = split_container
                .flex_row()
                .child(render_pane_container(workspace, true, cx))
                .child(render_vertical_splitter(workspace, cx))
                .child(render_pane_container(workspace, false, cx));
        } else {
            split_container = split_container
                .flex_col()
                .child(render_pane_container(workspace, true, cx))
                .child(render_horizontal_splitter(workspace, cx))
                .child(render_pane_container(workspace, false, cx));
        }

        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.bg_dark)
            .overflow_hidden()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.is_dragging_scrollbar && event.dragging() {
                    this.scroll_to_ratio_from_mouse_y(event.position.y, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.is_dragging_scrollbar {
                        this.is_dragging_scrollbar = false;
                        cx.notify();
                    }
                }),
            )
            // Interactive Find Bar
            .children(render_find_bar(workspace, cx))
            // Dual-Pane Split Container
            .child(split_container)
    }
}

/// Renders a pane container with top header bar and virtualized log viewport
fn render_pane_container(
    workspace: &LogAnalysisWorkspace,
    is_primary: bool,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let title = if is_primary {
        let name = workspace
            .active_file_name
            .as_ref()
            .map(|s| s.as_ref())
            .unwrap_or(if workspace.is_streaming { "ADB Stream" } else { "Active Log" });
        format!("Pane A: {}", name)
    } else {
        let name = workspace
            .secondary_file_name
            .as_ref()
            .map(|s| s.as_ref())
            .unwrap_or("Secondary Log");
        format!("Pane B: {}", name)
    };

    let line_count = if is_primary {
        workspace
            .filter_result
            .as_ref()
            .map(|r| r.visible_indices.len())
            .unwrap_or(0)
    } else {
        workspace
            .secondary_filter_result
            .as_ref()
            .map(|r| r.visible_indices.len())
            .unwrap_or(0)
    };

    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        // Sub-pane Header
        .child(
            div()
                .h(px(25.0))
                .w_full()
                .bg(theme.bg_header)
                .border_b_1()
                .border_color(theme.border_subtle)
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px(px(8.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded_sm()
                                .bg(if is_primary { theme.accent_muted } else { theme.bg_badge })
                                .text_size(px(10.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(if is_primary { theme.accent } else { theme.text_muted })
                                .child(if is_primary { "PANE A" } else { "PANE B" }),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_primary)
                                .child(title),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(10.5))
                                .font_family("monospace")
                                .text_color(theme.text_muted)
                                .child(format!("{line_count} lines")),
                        )
                        .child(if !is_primary {
                            div()
                                .id("btn-secondary-pick-file")
                                .px(px(6.0))
                                .py(px(1.5))
                                .rounded_sm()
                                .bg(theme.bg_card)
                                .hover(move |s| s.bg(theme.btn_hover))
                                .text_size(px(10.0))
                                .text_color(theme.accent)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prompt_open_secondary_file(Some(window), cx);
                                }))
                                .child("📂 Open...")
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        }),
                ),
        )
        .child(render_pane_content(workspace, is_primary, true, cx))
}

/// Renders the vertical splitter divider bar
fn render_vertical_splitter(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let is_synced = workspace.is_scroll_synced;

    div()
        .id("split-divider-v")
        .w(px(32.0))
        .h_full()
        .bg(theme.bg_panel)
        .border_l_1()
        .border_r_1()
        .border_color(theme.border)
        .flex()
        .flex_col()
        .items_center()
        .justify_between()
        .py(px(8.0))
        .child(
            // Top: Orientation Toggle
            div()
                .id("btn-split-orient-v")
                .p(px(4.0))
                .rounded_md()
                .cursor_pointer()
                .text_size(px(12.0))
                .text_color(theme.text_muted)
                .hover(move |s| s.bg(theme.btn_hover).text_color(theme.text_primary))
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.toggle_split_orientation(cx);
                }))
                .child("⊟"),
        )
        .child(
            // Center: Synced Lockstep Scroll Badge
            div()
                .id("btn-split-sync-v")
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .p(px(4.0))
                .rounded_md()
                .cursor_pointer()
                .bg(if is_synced {
                    theme.accent_muted
                } else {
                    theme.bg_badge
                })
                .text_color(if is_synced {
                    theme.accent
                } else {
                    theme.text_muted
                })
                .border_1()
                .border_color(if is_synced {
                    theme.accent
                } else {
                    theme.border_subtle
                })
                .hover(move |s| s.opacity(0.85))
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.toggle_synced_scroll(cx);
                }))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::BOLD)
                        .child(if is_synced { "🔗" } else { "⛓️" }),
                )
                .child(
                    div()
                        .text_size(px(8.5))
                        .font_weight(FontWeight::BOLD)
                        .child(if is_synced { "SYNC" } else { "OFF" }),
                ),
        )
        .child(
            // Bottom: Close Split View
            div()
                .id("btn-split-close-v")
                .p(px(4.0))
                .rounded_md()
                .cursor_pointer()
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .hover(move |s| s.bg(theme.btn_hover).text_color(theme.error_red))
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.toggle_split_view(cx);
                }))
                .child("✕"),
        )
}

/// Renders the horizontal splitter divider bar
fn render_horizontal_splitter(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let is_synced = workspace.is_scroll_synced;

    div()
        .id("split-divider-h")
        .h(px(26.0))
        .w_full()
        .bg(theme.bg_panel)
        .border_t_1()
        .border_b_1()
        .border_color(theme.border)
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(8.0))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_muted)
                .child("Split View (Stacked)"),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id("btn-split-sync-h")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded_md()
                        .cursor_pointer()
                        .bg(if is_synced {
                            theme.accent_muted
                        } else {
                            theme.bg_badge
                        })
                        .text_color(if is_synced {
                            theme.accent
                        } else {
                            theme.text_muted
                        })
                        .border_1()
                        .border_color(if is_synced {
                            theme.accent
                        } else {
                            theme.border_subtle
                        })
                        .hover(move |s| s.opacity(0.85))
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.toggle_synced_scroll(cx);
                        }))
                        .child(if is_synced { "🔗 Synced" } else { "⛓️ Unlinked" }),
                )
                .child(
                    div()
                        .id("btn-split-orient-h")
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded_md()
                        .cursor_pointer()
                        .bg(theme.bg_badge)
                        .text_color(theme.text_muted)
                        .hover(move |s| s.bg(theme.btn_hover).text_color(theme.text_primary))
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.toggle_split_orientation(cx);
                        }))
                        .child("↕ Side-by-Side"),
                ),
        )
        .child(
            div()
                .id("btn-split-close-h")
                .px(px(6.0))
                .py(px(2.0))
                .rounded_md()
                .cursor_pointer()
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .hover(move |s| s.bg(theme.btn_hover).text_color(theme.error_red))
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.toggle_split_view(cx);
                }))
                .child("✕ Close"),
        )
}

/// Renders either the uniform list or the appropriate empty state for a pane
fn render_pane_content(
    workspace: &LogAnalysisWorkspace,
    is_primary: bool,
    _in_split_mode: bool,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let _theme = workspace.theme;
    let has_source = if is_primary {
        workspace.log_source.is_some()
    } else {
        workspace.secondary_log_source.is_some()
    };

    if !has_source {
        if is_primary {
            render_onboarding_empty_state(workspace, cx).into_any_element()
        } else {
            render_secondary_empty_state(workspace, cx).into_any_element()
        }
    } else {
        render_log_list(workspace, is_primary, cx).into_any_element()
    }
}

/// Helper to render text with search match highlighting
fn render_highlighted_text(
    text_content: &str,
    search_matcher: &Option<std::sync::Arc<loganalysis_core::CompiledMatcher>>,
    highlight_style: HighlightStyle,
) -> AnyElement {
    if let Some(ref matcher) = search_matcher {
        let ranges = matcher.find_match_ranges(text_content, 32);
        if !ranges.is_empty() {
            let highlights = ranges.into_iter().map(|r| (r, highlight_style));
            StyledText::new(text_content.to_string())
                .with_highlights(highlights)
                .into_any_element()
        } else {
            text_content.to_string().into_any_element()
        }
    } else {
        text_content.to_string().into_any_element()
    }
}

/// Column Header Bar for Structured Column Grid View
fn render_column_header(theme: crate::theme::Theme) -> impl IntoElement {
    div()
        .h(px(24.0))
        .w_full()
        .bg(theme.bg_header)
        .border_b_1()
        .border_color(theme.border_subtle)
        .flex()
        .flex_row()
        .items_center()
        .text_size(px(10.5))
        .font_weight(FontWeight::BOLD)
        .text_color(theme.text_muted)
        // Line Gutter Header (64px)
        .child(
            div()
                .w(px(64.0))
                .h_full()
                .flex_shrink_0()
                .px(px(4.0))
                .border_r_1()
                .border_color(theme.border_subtle)
                .flex()
                .items_center()
                .justify_end()
                .child("Line"),
        )
        // Time Header (135px)
        .child(
            div()
                .w(px(135.0))
                .h_full()
                .flex_shrink_0()
                .px(px(6.0))
                .border_r_1()
                .border_color(theme.border_subtle)
                .flex()
                .items_center()
                .child("Time"),
        )
        // Level Header (36px)
        .child(
            div()
                .w(px(36.0))
                .h_full()
                .flex_shrink_0()
                .border_r_1()
                .border_color(theme.border_subtle)
                .flex()
                .items_center()
                .justify_center()
                .child("Lvl"),
        )
        // PID/TID Header (85px)
        .child(
            div()
                .w(px(85.0))
                .h_full()
                .flex_shrink_0()
                .px(px(6.0))
                .border_r_1()
                .border_color(theme.border_subtle)
                .flex()
                .items_center()
                .child("PID / TID"),
        )
        // Tag Header (130px)
        .child(
            div()
                .w(px(130.0))
                .h_full()
                .flex_shrink_0()
                .px(px(6.0))
                .border_r_1()
                .border_color(theme.border_subtle)
                .flex()
                .items_center()
                .child("Tag"),
        )
        // Message Header (flex-1)
        .child(
            div()
                .flex_1()
                .h_full()
                .px(px(8.0))
                .flex()
                .items_center()
                .child("Message"),
        )
}

/// Renders the virtualized uniform list for either pane
fn render_log_list(
    workspace: &LogAnalysisWorkspace,
    is_primary: bool,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let is_column_view = workspace.is_column_view;
    let font_size = workspace.font_size;
    let row_height = font_size * 1.55;
    let weak_self = cx.weak_entity();

    let row_count = if is_primary {
        workspace
            .filter_result
            .as_ref()
            .map(|r| r.visible_indices.len())
            .unwrap_or(0)
    } else {
        workspace
            .secondary_filter_result
            .as_ref()
            .map(|r| r.visible_indices.len())
            .unwrap_or(0)
    };

    let highlight_fg = if workspace.is_dark_theme {
        rgb(0xffffff)
    } else {
        rgb(0x000000)
    };

    let highlight_style = HighlightStyle {
        background_color: Some(theme.highlight_search.into()),
        color: Some(highlight_fg.into()),
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    };

    let weak_handle = weak_self.clone();
    let list_id = if is_primary { "log-view" } else { "secondary-log-view" };

    let mut list = uniform_list(list_id, row_count, move |range, _window, cx| {
        let ws_opt = weak_handle.upgrade();
        let ws = ws_opt.as_ref().map(|entity| entity.read(cx));

        let filter_res = if is_primary {
            ws.and_then(|w| w.filter_result.as_deref())
        } else {
            ws.and_then(|w| w.secondary_filter_result.as_deref())
        };

        let source = if is_primary {
            ws.and_then(|w| w.log_source.as_deref())
        } else {
            ws.and_then(|w| w.secondary_log_source.as_deref())
        };

        let selected = if is_primary {
            ws.and_then(|w| w.selection.selected_row)
        } else {
            ws.and_then(|w| w.secondary_selection.selected_row)
        };

        let search_matcher = ws.and_then(|w| w.cached_search_matcher.clone());
        let row_theme = ws.map(|w| w.theme).unwrap_or(theme);
        let is_col_view = ws.map(|w| w.is_column_view).unwrap_or(false);

        range
            .map(|idx| {
                let (real_idx, text): (usize, SharedString) =
                    if let (Some(res), Some(src)) = (filter_res, source) {
                        let r_idx = res.visible_indices.get(idx).copied().unwrap_or(0);
                        let t: SharedString = src
                            .get_line_arc(r_idx)
                            .map(SharedString::new)
                            .unwrap_or_default();
                        (r_idx, t)
                    } else {
                        (0, SharedString::default())
                    };

                let is_bookmarked = if is_primary {
                    ws.map(|w| w.bookmarks.is_bookmarked(real_idx)).unwrap_or(false)
                } else {
                    false
                };

                let (has_custom_bg, bg_color, fg_color) = if let Some(res) = filter_res {
                    if let Some(style) = res.visible_styles.get(idx) {
                        (
                            style.bg_hex.is_some(),
                            style.bg_hex.map(rgb).unwrap_or(row_theme.bg_dark),
                            style.fg_hex.map(rgb).unwrap_or(row_theme.text_primary),
                        )
                    } else {
                        (false, row_theme.bg_dark, row_theme.text_primary)
                    }
                } else {
                    (false, row_theme.bg_dark, row_theme.text_primary)
                };

                let is_selected = selected == Some(idx);
                let final_bg = if is_selected {
                    row_theme.selection_bg
                } else {
                    bg_color
                };
                let final_fg = if is_selected && !has_custom_bg {
                    rgb(0xffffff)
                } else {
                    fg_color
                };
                let entity_handle = weak_handle.clone();
                let row_id = if is_primary { "log-row" } else { "secondary-log-row" };

                let gutter_el = div()
                    .w(px(64.0))
                    .h_full()
                    .flex_shrink_0()
                    .bg(if is_selected {
                        theme.accent_muted
                    } else if is_bookmarked {
                        theme.accent_muted
                    } else {
                        theme.bg_gutter
                    })
                    .border_l_2()
                    .border_r_1()
                    .border_color(if is_selected {
                        theme.accent
                    } else if is_bookmarked {
                        rgb(0x3b82f6)
                    } else {
                        theme.border_subtle
                    })
                    .text_color(if is_selected {
                        theme.accent
                    } else if is_bookmarked {
                        rgb(0x3b82f6)
                    } else {
                        theme.text_gutter
                    })
                    .font_weight(if is_selected || is_bookmarked {
                        FontWeight::BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(4.0))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .child(if is_bookmarked { "🔖" } else { "" }),
                    )
                    .child(div().child(format!("{}", real_idx + 1)));

                let mut row_el = div()
                    .id(ElementId::NamedInteger(row_id.into(), idx as u64))
                    .h(row_height)
                    .min_h(row_height)
                    .max_h(row_height)
                    .w_full()
                    .bg(final_bg)
                    .hover(move |s| {
                        if !is_selected && !has_custom_bg {
                            s.bg(theme.row_hover)
                        } else {
                            s
                        }
                    })
                    .text_color(final_fg)
                    .font_family("monospace")
                    .text_size(font_size)
                    .line_height(row_height)
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .overflow_hidden()
                    .cursor_pointer()
                    .on_click(move |_, _window, cx| {
                        let _ = entity_handle.update(cx, |this, cx| {
                            if is_primary {
                                this.select_log_row(idx, real_idx, cx);
                            } else {
                                this.select_secondary_log_row(idx, real_idx, cx);
                            }
                        });
                    })
                    .child(gutter_el);

                if is_col_view {
                    let parsed = parse_logcat_column_line(text.as_ref());
                    if let Some(time_str) = parsed.time {
                        let (lvl_bg, lvl_fg) = match parsed.level_char {
                            Some('E') | Some('F') => (row_theme.error_red, rgb(0xffffff)),
                            Some('W') => (row_theme.warning_amber, rgb(0x000000)),
                            Some('I') => (row_theme.success_green, rgb(0x000000)),
                            Some('D') => (row_theme.accent, rgb(0xffffff)),
                            _ => (row_theme.bg_badge, row_theme.text_muted),
                        };
                        let level_char_str = parsed
                            .level_char
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "-".into());
                        let pid_tid_str =
                            format!("{}/{}", parsed.pid.unwrap_or("-"), parsed.tid.unwrap_or("-"));
                        let tag_str = parsed.tag.unwrap_or("-");
                        let msg_text = parsed.message;

                        row_el = row_el
                            .child(
                                div()
                                    .w(px(135.0))
                                    .h_full()
                                    .flex_shrink_0()
                                    .px(px(6.0))
                                    .border_r_1()
                                    .border_color(row_theme.border_subtle)
                                    .flex()
                                    .items_center()
                                    .text_size(px(11.0))
                                    .text_color(row_theme.text_muted)
                                    .child(time_str.to_string()),
                            )
                            .child(
                                div()
                                    .w(px(36.0))
                                    .h_full()
                                    .flex_shrink_0()
                                    .border_r_1()
                                    .border_color(row_theme.border_subtle)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        div()
                                            .px(px(4.0))
                                            .py(px(1.0))
                                            .rounded_xs()
                                            .bg(lvl_bg)
                                            .text_color(lvl_fg)
                                            .text_size(px(10.0))
                                            .font_weight(FontWeight::BOLD)
                                            .child(level_char_str),
                                    ),
                            )
                            .child(
                                div()
                                    .w(px(85.0))
                                    .h_full()
                                    .flex_shrink_0()
                                    .px(px(6.0))
                                    .border_r_1()
                                    .border_color(row_theme.border_subtle)
                                    .flex()
                                    .items_center()
                                    .text_size(px(11.0))
                                    .font_family("monospace")
                                    .text_color(row_theme.text_muted)
                                    .child(pid_tid_str),
                            )
                            .child(
                                div()
                                    .w(px(130.0))
                                    .h_full()
                                    .flex_shrink_0()
                                    .px(px(6.0))
                                    .border_r_1()
                                    .border_color(row_theme.border_subtle)
                                    .flex()
                                    .items_center()
                                    .text_size(font_size)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(row_theme.accent)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(tag_str.to_string()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .px(px(8.0))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(render_highlighted_text(
                                        msg_text,
                                        &search_matcher,
                                        highlight_style,
                                    )),
                            );
                    } else {
                        // Unstructured multiline stack trace fallback
                        row_el = row_el.child(
                            div()
                                .flex_1()
                                .px(px(12.0))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(render_highlighted_text(
                                    text.as_ref(),
                                    &search_matcher,
                                    highlight_style,
                                )),
                        );
                    }
                } else {
                    // Raw monospace line
                    row_el = row_el.child(
                        div()
                            .flex_1()
                            .px(px(10.0))
                            .whitespace_nowrap()
                            .child(render_highlighted_text(
                                text.as_ref(),
                                &search_matcher,
                                highlight_style,
                            )),
                    );
                }

                row_el
            })
            .collect::<Vec<_>>()
    });

    if workspace.full_line_display {
        list = list.with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained);
    }

    let scroll_handle = if is_primary {
        workspace.scroll_handle.clone()
    } else {
        workspace.secondary_scroll_handle.clone()
    };

    let list_el = list.track_scroll(scroll_handle.clone()).flex_1();

    // Compute scrollbar thumb position
    let (thumb_top, thumb_height, track_height) = {
        let scroll_state = scroll_handle.0.borrow();
        let bounds = scroll_state.base_handle.bounds();
        let track_height: f32 = bounds.size.height.into();
        let scroll_y: f32 = (-scroll_state.base_handle.offset().y).into();
        drop(scroll_state);

        let font_size_f32: f32 = font_size.into();
        let row_height_f32 = font_size_f32 * 1.55;
        let total_content_height = (row_count as f32) * row_height_f32;

        if track_height > 0.0 && total_content_height > track_height {
            let visible_ratio = (track_height / total_content_height).clamp(0.03, 1.0);
            let h = (track_height * visible_ratio).max(24.0);
            let max_scroll = (total_content_height - track_height).max(1.0);
            let ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);
            let top = ratio * (track_height - h);
            (top, h, track_height)
        } else {
            (0.0, track_height, track_height)
        }
    };

    let heatmap_ticks = if is_primary {
        if let Some(heatmap) = &workspace.heatmap_data {
            let total_buckets = heatmap.buckets.len() as f32;
            if track_height > 0.0 && total_buckets > 0.0 {
                let bucket_step = track_height / total_buckets;
                heatmap
                    .buckets
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, bucket)| {
                        let severity = bucket.dominant_severity()?;
                        let (color, opacity) = match severity {
                            LineSeverity::Error => (rgb(0xef4444), 0.95),
                            LineSeverity::Warning => (rgb(0xf59e0b), 0.90),
                            LineSeverity::RuleMatch => (rgb(0x3b82f6), 0.75),
                            LineSeverity::Normal => (rgb(0x000000), 0.0),
                        };
                        let top_pos = (idx as f32) * bucket_step;
                        let tick_h = bucket_step.max(2.5);

                        Some(
                            div()
                                .absolute()
                                .left(px(1.5))
                                .right(px(1.5))
                                .top(px(top_pos))
                                .h(px(tick_h))
                                .rounded_xs()
                                .bg(color)
                                .opacity(opacity),
                        )
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let is_dragging = is_primary && workspace.is_dragging_scrollbar;
    let scrollbar_thumb_color = if is_dragging {
        theme.accent
    } else if workspace.is_dark_theme {
        rgba(0xffffff38)
    } else {
        rgba(0x00000038)
    };

    let track_id = if is_primary { "log-scrollbar-track" } else { "secondary-scrollbar-track" };
    let thumb_id = if is_primary { "log-scrollbar-thumb" } else { "secondary-scrollbar-thumb" };

    let mut scrollbar_track = div()
        .id(track_id)
        .relative()
        .w(px(15.0))
        .h_full()
        .bg(theme.bg_gutter)
        .border_l_1()
        .border_color(theme.border_subtle)
        .cursor_pointer();

    if is_primary {
        scrollbar_track = scrollbar_track
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    this.is_dragging_scrollbar = true;
                    this.scroll_to_ratio_from_mouse_y(event.position.y, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.is_dragging_scrollbar && event.dragging() {
                    this.scroll_to_ratio_from_mouse_y(event.position.y, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.is_dragging_scrollbar = false;
                    cx.notify();
                }),
            );
    } else {
        scrollbar_track = scrollbar_track.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                this.scroll_secondary_to_ratio_from_mouse_y(event.position.y, cx);
            }),
        );
    }

    scrollbar_track = scrollbar_track
        .children(heatmap_ticks)
        .child(
            div()
                .id(thumb_id)
                .absolute()
                .left(px(2.0))
                .right(px(2.0))
                .top(px(thumb_top))
                .h(px(thumb_height))
                .rounded_full()
                .bg(scrollbar_thumb_color)
                .hover(move |s| s.bg(theme.accent)),
        );

    let main_viewport = div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .when(is_column_view, |s| s.child(render_column_header(theme)))
        .child(list_el);

    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_row()
        .overflow_hidden()
        .child(main_viewport)
        .child(scrollbar_track)
}

/// Secondary pane empty state when no file is loaded in Pane B
fn render_secondary_empty_state(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;

    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .bg(theme.bg_dark)
        .child(
            div()
                .w(px(380.0))
                .p(px(20.0))
                .rounded_xl()
                .bg(theme.bg_panel)
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .child("📂 Secondary Split Pane"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text_muted)
                        .text_align(TextAlign::Center)
                        .child("Load a reference log file or previous session capture to compare side-by-side with microsecond timestamp correlation."),
                )
                .child(
                    div()
                        .id("btn-secondary-empty-open")
                        .px(px(16.0))
                        .py(px(8.0))
                        .rounded_md()
                        .bg(theme.accent)
                        .hover(move |s| s.bg(theme.btn_hover))
                        .text_color(rgb(0xffffff))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.prompt_open_secondary_file(Some(window), cx);
                        }))
                        .child("📂 Choose Log File..."),
                ),
        )
}

/// Primary pane onboarding layout (Local File vs Live Android ADB)
fn render_onboarding_empty_state(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let selected_dev_info = workspace.get_selected_device_info();
    let has_devices = !workspace.adb_device_details.is_empty();
    let is_scanning = workspace.is_scanning_adb;

    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .child(
            div()
                .w(px(740.0))
                .flex()
                .flex_col()
                .items_center()
                .gap_6()
                // Title Header
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(22.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.text_primary)
                                .child("LogAnalysis Studio"),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(theme.text_muted)
                                .child("GPU-Accelerated High-Performance Exploration for Android & System Logs"),
                        ),
                )
                // Dual-Pillar Ingestion Grid
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .gap_4()
                        // Left Pillar: Local File Ingestion
                        .child(
                            div()
                                .flex_1()
                                .p(px(22.0))
                                .rounded_xl()
                                .bg(theme.bg_panel)
                                .border_1()
                                .border_color(theme.border)
                                .flex()
                                .flex_col()
                                .justify_between()
                                .gap_4()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_size(px(15.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme.text_primary)
                                                .child("📂 Local Log File"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.5))
                                                .text_color(theme.text_muted)
                                                .child("Drop any .log, .txt, or filter profile .json file directly into this window."),
                                        )
                                        .child(
                                            div()
                                                .mt(px(4.0))
                                                .p(px(12.0))
                                                .rounded_md()
                                                .border_1()
                                                .border_dashed()
                                                .border_color(theme.border_subtle)
                                                .bg(theme.bg_card)
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(11.0))
                                                .text_color(theme.text_muted)
                                                .child("Drop file here to open instantly"),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("btn-empty-state-open")
                                        .w_full()
                                        .py(px(8.0))
                                        .rounded_md()
                                        .bg(theme.accent)
                                        .hover(move |s| s.bg(theme.btn_hover))
                                        .text_color(rgb(0xffffff))
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.prompt_open_file(Some(window), cx);
                                        }))
                                        .child("📂 Open File (Ctrl+O)"),
                                ),
                        )
                        // Right Pillar: Live Android ADB Stream
                        .child(
                            div()
                                .flex_1()
                                .p(px(22.0))
                                .rounded_xl()
                                .bg(theme.bg_panel)
                                .border_1()
                                .border_color(theme.border)
                                .flex()
                                .flex_col()
                                .justify_between()
                                .gap_4()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_2()
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    div()
                                                        .text_size(px(15.0))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(theme.text_primary)
                                                        .child("📱 Live Android ADB"),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn-welcome-rescan-adb")
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
                                        .child(
                                            if is_scanning {
                                                div()
                                                    .p(px(10.0))
                                                    .rounded_md()
                                                    .bg(theme.bg_card)
                                                    .text_size(px(11.0))
                                                    .text_color(theme.text_muted)
                                                    .child("Scanning for connected devices...")
                                            } else if has_devices {
                                                let dev_info = selected_dev_info;
                                                let dev_name = dev_info.map(|d| d.short_name()).unwrap_or_else(|| "Connected Device".to_string());
                                                let dev_serial = dev_info.map(|d| d.serial.clone()).unwrap_or_default();
                                                let is_ready = dev_info.map(|d| d.is_ready()).unwrap_or(false);

                                                div()
                                                    .p(px(10.0))
                                                    .rounded_md()
                                                    .bg(theme.bg_card)
                                                    .border_1()
                                                    .border_color(theme.border_subtle)
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1p5()
                                                    .child(
                                                        div()
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
                                                                            .w(px(7.0))
                                                                            .h(px(7.0))
                                                                            .rounded_full()
                                                                            .bg(if is_ready { theme.success_green } else { theme.warning_amber }),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .text_size(px(12.0))
                                                                            .font_weight(FontWeight::BOLD)
                                                                            .text_color(theme.text_primary)
                                                                            .child(dev_name),
                                                                    ),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_size(px(10.0))
                                                                    .text_color(if is_ready { theme.success_green } else { theme.warning_amber })
                                                                    .child(if is_ready { "Ready" } else { "Unauthorized" }),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .font_family("monospace")
                                                            .text_color(theme.text_muted)
                                                            .child(format!("Serial: {} • {} found", dev_serial, workspace.adb_device_details.len())),
                                                    )
                                            } else {
                                                div()
                                                    .p(px(10.0))
                                                    .rounded_md()
                                                    .bg(theme.bg_card)
                                                    .border_1()
                                                    .border_color(theme.border_subtle)
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .text_size(px(11.5))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(theme.warning_amber)
                                                            .child("No Android devices detected"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .text_color(theme.text_muted)
                                                            .child("Enable USB Debugging in Developer Options & connect USB"),
                                                    )
                                            },
                                        ),
                                )
                                .child(
                                    div()
                                        .id("btn-empty-state-adb")
                                        .w_full()
                                        .py(px(8.0))
                                        .rounded_md()
                                        .bg(if has_devices { theme.success_green } else { theme.bg_card })
                                        .hover(move |s| {
                                            if has_devices {
                                                s.bg(rgb(0x16a34a))
                                            } else {
                                                s.bg(theme.btn_hover)
                                            }
                                        })
                                        .text_color(if has_devices { rgb(0xffffff) } else { theme.text_muted })
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            if !this.adb_device_details.is_empty() {
                                                this.start_adb_stream(cx);
                                            } else {
                                                this.refresh_adb_devices(cx);
                                            }
                                        }))
                                        .child(
                                            if is_scanning {
                                                "📱 Scanning ADB...".to_string()
                                            } else if let Some(info) = selected_dev_info {
                                                format!("⚡ Stream from {}", info.short_name())
                                            } else if has_devices {
                                                "⚡ Start Live ADB Stream".to_string()
                                            } else {
                                                "🔄 Scan for Devices".to_string()
                                            },
                                        ),
                                ),
                        ),
                )
                // Shortcut Cheat Sheet
                .child(
                    div()
                        .w_full()
                        .pt(px(12.0))
                        .border_t_1()
                        .border_color(theme.border_subtle)
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1p5()
                        .text_size(px(11.0))
                        .font_family("monospace")
                        .text_color(theme.text_muted)
                        .child("Ctrl+F: Search  •  Ctrl+\\: Split View  •  Ctrl+Alt+S: Sync Scroll  •  Ctrl+Space: Pause"),
                ),
        )
}

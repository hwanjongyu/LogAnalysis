use gpui::prelude::FluentBuilder;
use gpui::*;
use loganalysis_core::{parse_logcat_column_line, LineSeverity, LogSource};

use crate::components::find_bar::render_find_bar;
use crate::components::tooltip::render_tooltip;
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
            .relative()
            .flex()
            .flex_col()
            .bg(theme.bg_dark)
            .overflow_hidden()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.is_dragging_scrollbar && event.dragging() {
                    this.scroll_to_ratio_from_mouse_y(event.position.y, cx);
                } else if this.is_dragging_secondary_scrollbar && event.dragging() {
                    this.scroll_secondary_to_ratio_from_mouse_y(event.position.y, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    let mut changed = false;
                    if this.is_dragging_scrollbar {
                        this.is_dragging_scrollbar = false;
                        changed = true;
                    }
                    if this.is_dragging_secondary_scrollbar {
                        this.is_dragging_secondary_scrollbar = false;
                        changed = true;
                    }
                    if changed {
                        cx.notify();
                    }
                }),
            )
            // Forensic Facets Bar
            .children(crate::components::facets_bar::render_facets_bar(workspace, cx))
            // Single primary viewport content
            .child(render_pane_content(workspace, true, false, cx))
            // Modern Floating Find Widget (over log content without reflow)
            .children(render_find_bar(workspace, cx))
            // Contextual Floating Selection Mini-Bar
            .children(render_selection_action_bar(workspace, cx))
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
            .relative()
            .flex()
            .flex_col()
            .bg(theme.bg_dark)
            .overflow_hidden()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.is_dragging_scrollbar && event.dragging() {
                    this.scroll_to_ratio_from_mouse_y(event.position.y, cx);
                } else if this.is_dragging_secondary_scrollbar && event.dragging() {
                    this.scroll_secondary_to_ratio_from_mouse_y(event.position.y, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    let mut changed = false;
                    if this.is_dragging_scrollbar {
                        this.is_dragging_scrollbar = false;
                        changed = true;
                    }
                    if this.is_dragging_secondary_scrollbar {
                        this.is_dragging_secondary_scrollbar = false;
                        changed = true;
                    }
                    if changed {
                        cx.notify();
                    }
                }),
            )
            // Forensic Facets Bar
            .children(crate::components::facets_bar::render_facets_bar(workspace, cx))
            // Dual-Pane Split Container
            .child(split_container)
            // Modern Floating Find Widget (over split container without reflow)
            .children(render_find_bar(workspace, cx))
            // Contextual Floating Selection Mini-Bar
            .children(render_selection_action_bar(workspace, cx))
    }
}

/// Calculates the dynamic vertical position of the floating selection action bar
/// to prevent collision with the Find widget (top: 8px) and Forensic Facets bar (h: 28px).
pub fn calculate_selection_bar_top(is_search_visible: bool, is_facets_visible: bool) -> Pixels {
    if is_search_visible {
        px(52.0)
    } else if is_facets_visible {
        px(38.0)
    } else {
        px(10.0)
    }
}

/// Renders the contextual floating selection mini-bar when a log row is selected
pub fn render_selection_action_bar(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    let sel_row = workspace.selection.selected_row?;
    let theme = workspace.theme;

    // Smart vertical positioning to avoid collisions with Find bar and Facets bar
    let is_facets_visible = workspace.is_facets_visible && workspace.log_source.is_some();
    let top_pos = calculate_selection_bar_top(workspace.is_search_visible, is_facets_visible);

    // Extract human-friendly line index label (1-based)
    let line_label = if let Some(res) = &workspace.filter_result {
        if let Some(&r_idx) = res.visible_indices.get(sel_row) {
            format!("Line {}", r_idx + 1)
        } else {
            format!("Row {}", sel_row + 1)
        }
    } else {
        format!("Row {}", sel_row + 1)
    };

    Some(
        div()
            .id("floating-selection-action-bar")
            .occlude()
            .absolute()
            .top(top_pos)
            .right(px(16.0))
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border)
            .rounded_lg()
            .shadow_lg()
            .px(px(6.0))
            .py(px(3.0))
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            // Selected Line Badge
            .child(
                div()
                    .px(px(6.0))
                    .py(px(1.5))
                    .rounded_xs()
                    .bg(theme.accent_muted)
                    .text_size(px(10.0))
                    .font_weight(FontWeight::BOLD)
                    .font_family("monospace")
                    .text_color(theme.accent)
                    .child(line_label),
            )
            // Vertical Divider
            .child(
                div()
                    .w(px(1.0))
                    .h(px(14.0))
                    .bg(theme.border_subtle),
            )
            // Button: [📋 Copy]
            .child(
                div()
                    .id("btn-selection-copy")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .px(px(7.0))
                    .py(px(2.5))
                    .rounded_sm()
                    .hover(move |s| s.bg(theme.btn_hover))
                    .cursor_pointer()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .tooltip(render_tooltip("Copy selected log line to clipboard", Some("Ctrl+C"), theme))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.copy_selection(cx);
                    }))
                    .child("📋 Copy"),
            )
            // Button: [+🔍 Filter Sel]
            .child(
                div()
                    .id("btn-selection-quick-filter")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .px(px(7.0))
                    .py(px(2.5))
                    .rounded_sm()
                    .hover(move |s| s.bg(theme.btn_hover))
                    .cursor_pointer()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.accent)
                    .tooltip(render_tooltip("Create instant filter rule from current selection", None, theme))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.add_quick_filter_from_selection(false, cx);
                    }))
                    .child("+🔍 Filter Sel"),
            )
            // Vertical Divider
            .child(
                div()
                    .w(px(1.0))
                    .h(px(14.0))
                    .bg(theme.border_subtle),
            )
            // Button: [✕ Clear]
            .child(
                div()
                    .id("btn-selection-clear")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .px(px(6.0))
                    .py(px(2.5))
                    .rounded_sm()
                    .hover(move |s| s.bg(theme.btn_hover).text_color(theme.error_red))
                    .cursor_pointer()
                    .text_size(px(10.5))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_muted)
                    .tooltip(render_tooltip("Clear selection", Some("Esc"), theme))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.clear_selection(cx);
                    }))
                    .child("✕ Clear"),
            ),
    )
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

    let is_active_pane = if !workspace.is_split_view {
        true
    } else if is_primary {
        !workspace.active_split_pane_is_secondary
    } else {
        workspace.active_split_pane_is_secondary
    };

    let (badge_bg, badge_fg) = if is_active_pane {
        (theme.accent_muted, theme.accent)
    } else {
        (theme.bg_badge, theme.text_muted)
    };

    let header_border_color = if workspace.is_split_view && is_active_pane {
        theme.accent
    } else {
        theme.border_subtle
    };

    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                if this.is_split_view {
                    let new_sec = !is_primary;
                    if this.active_split_pane_is_secondary != new_sec {
                        this.active_split_pane_is_secondary = new_sec;
                        cx.notify();
                    }
                }
            }),
        )
        // Sub-pane Header
        .child(
            div()
                .h(px(25.0))
                .w_full()
                .bg(theme.bg_header)
                .border_b_1()
                .border_color(header_border_color)
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
                                .bg(badge_bg)
                                .text_size(px(10.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(badge_fg)
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

/// Helper to render text with search match highlighting with zero unnecessary string allocations
pub(crate) fn render_highlighted_text(
    text_content: SharedString,
    search_matcher: &Option<std::sync::Arc<loganalysis_core::CompiledMatcher>>,
    highlight_style: HighlightStyle,
) -> AnyElement {
    if let Some(ref matcher) = search_matcher {
        let ranges = matcher.find_match_ranges(text_content.as_ref(), 32);
        if !ranges.is_empty() {
            let highlights = ranges.into_iter().map(|r| (r, highlight_style));
            StyledText::new(text_content)
                .with_highlights(highlights)
                .into_any_element()
        } else {
            text_content.into_any_element()
        }
    } else {
        text_content.into_any_element()
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

    let is_dark_theme = workspace.is_dark_theme;
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
                    .bg(if is_selected || is_bookmarked {
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
                    .px(px(6.0))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .child(if is_bookmarked { "🔖" } else { "" }),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(11.0))
                            .child(format!("{}", real_idx + 1)),
                    );

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

                if let Some((s_id, s_name)) = source.and_then(|s| s.get_line_source_info(real_idx)) {
                    let badge_colors = [
                        (rgb(0x1e3a8a), rgb(0x93c5fd)), // File 0: Blue
                        (rgb(0x581c87), rgb(0xd8b4fe)), // File 1: Purple
                        (rgb(0x064e3b), rgb(0x6ee7b7)), // File 2: Emerald
                        (rgb(0x7c2d12), rgb(0xfdba74)), // File 3: Amber
                    ];
                    let (badge_bg, badge_fg) = badge_colors[(s_id as usize) % badge_colors.len()];
                    row_el = row_el.child(
                        div()
                            .h_full()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .px(px(3.0))
                            .child(
                                div()
                                    .px(px(4.0))
                                    .py(px(1.0))
                                    .rounded_xs()
                                    .bg(badge_bg)
                                    .text_color(badge_fg)
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(SharedString::new(s_name)),
                            ),
                    );
                }

                if is_col_view {
                    let parsed = parse_logcat_column_line(text.as_ref());
                    if let Some(time_str) = parsed.time {
                        let (lvl_bg, lvl_fg) = crate::theme::severity_badge_colors(
                            parsed.level_char,
                            is_dark_theme,
                        );
                        let level_char_str: SharedString = match parsed.level_char {
                            Some('E') | Some('e') => "E".into(),
                            Some('W') | Some('w') => "W".into(),
                            Some('I') | Some('i') => "I".into(),
                            Some('D') | Some('d') => "D".into(),
                            Some('V') | Some('v') => "V".into(),
                            Some('F') | Some('f') => "F".into(),
                            Some(c) => SharedString::new(c.to_string()),
                            None => "-".into(),
                        };
                        let pid_tid_str: SharedString = match (parsed.pid, parsed.tid) {
                            (Some(p), Some(t)) => format!("{}/{}", p, t).into(),
                            (Some(p), None) => format!("{}/-", p).into(),
                            (None, Some(t)) => format!("-/{}" , t).into(),
                            (None, None) => "-/-".into(),
                        };
                        let tag_str = parsed.tag.unwrap_or("-");
                        let tag_color = crate::theme::hash_tag_color(tag_str, is_dark_theme);
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
                                    .child(SharedString::new(time_str)),
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
                                            .w(px(22.0))
                                            .h(px(17.0))
                                            .rounded_xs()
                                            .bg(lvl_bg)
                                            .text_color(lvl_fg)
                                            .text_size(px(10.0))
                                            .font_weight(FontWeight::BOLD)
                                            .flex()
                                            .items_center()
                                            .justify_center()
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
                                    .text_color(tag_color)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(SharedString::new(tag_str)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .px(px(8.0))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(render_highlighted_text(
                                        SharedString::new(msg_text),
                                        &search_matcher,
                                        highlight_style,
                                    )),
                            );
                    } else {
                        // Unstructured multiline stack trace fallback
                        let trimmed = text.as_ref().trim_start();
                        let is_stack_trace = trimmed.starts_with("at ")
                            || trimmed.starts_with("Caused by:")
                            || trimmed.starts_with("...");
                        row_el = row_el.child(
                            div()
                                .flex_1()
                                .when(is_stack_trace, |s| s.pl(px(24.0)).border_l_2().border_color(row_theme.border_subtle))
                                .when(!is_stack_trace, |s| s.px(px(12.0)))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(render_highlighted_text(
                                    text.clone(),
                                    &search_matcher,
                                    highlight_style,
                                )),
                        );
                    }
                } else {
                    // Raw monospace line with stack trace indentation
                    let trimmed = text.as_ref().trim_start();
                    let is_stack_trace = trimmed.starts_with("at ")
                        || trimmed.starts_with("Caused by:")
                        || trimmed.starts_with("...");
                    row_el = row_el.child(
                        div()
                            .flex_1()
                            .when(is_stack_trace, |s| s.pl(px(22.0)).border_l_2().border_color(theme.border_subtle))
                            .when(!is_stack_trace, |s| s.px(px(10.0)))
                            .whitespace_nowrap()
                            .child(render_highlighted_text(
                                text.clone(),
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
                let mut ticks = Vec::new();
                let mut current_run: Option<(usize, usize, LineSeverity)> = None; // (start_idx, count, severity)

                for (idx, bucket) in heatmap.buckets.iter().enumerate() {
                    let severity = bucket.dominant_severity().filter(|&s| s != LineSeverity::Normal);
                    match (current_run, severity) {
                        (Some((start, count, run_sev)), Some(sev)) if run_sev == sev => {
                            current_run = Some((start, count + 1, run_sev));
                        }
                        (Some((start, count, run_sev)), other) => {
                            let (color, opacity) = match run_sev {
                                LineSeverity::Error => (rgb(0xef4444), 0.95),
                                LineSeverity::Warning => (rgb(0xf59e0b), 0.90),
                                LineSeverity::RuleMatch => (rgb(0x3b82f6), 0.75),
                                LineSeverity::Normal => (rgb(0x000000), 0.0),
                            };
                            let top_pos = (start as f32) * bucket_step;
                            let tick_h = ((count as f32) * bucket_step).max(2.5);
                            ticks.push(
                                div()
                                    .absolute()
                                    .left(px(1.5))
                                    .right(px(1.5))
                                    .top(px(top_pos))
                                    .h(px(tick_h))
                                    .rounded_xs()
                                    .bg(color)
                                    .opacity(opacity),
                            );
                            current_run = other.map(|sev| (idx, 1, sev));
                        }
                        (None, Some(sev)) => {
                            current_run = Some((idx, 1, sev));
                        }
                        (None, None) => {}
                    }
                }

                if let Some((start, count, run_sev)) = current_run {
                    let (color, opacity) = match run_sev {
                        LineSeverity::Error => (rgb(0xef4444), 0.95),
                        LineSeverity::Warning => (rgb(0xf59e0b), 0.90),
                        LineSeverity::RuleMatch => (rgb(0x3b82f6), 0.75),
                        LineSeverity::Normal => (rgb(0x000000), 0.0),
                    };
                    let top_pos = (start as f32) * bucket_step;
                    let tick_h = ((count as f32) * bucket_step).max(2.5);
                    ticks.push(
                        div()
                            .absolute()
                            .left(px(1.5))
                            .right(px(1.5))
                            .top(px(top_pos))
                            .h(px(tick_h))
                            .rounded_xs()
                            .bg(color)
                            .opacity(opacity),
                    );
                }

                ticks
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let is_dragging = if is_primary {
        workspace.is_dragging_scrollbar
    } else {
        workspace.is_dragging_secondary_scrollbar
    };
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
        scrollbar_track = scrollbar_track
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    this.is_dragging_secondary_scrollbar = true;
                    this.scroll_secondary_to_ratio_from_mouse_y(event.position.y, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.is_dragging_secondary_scrollbar && event.dragging() {
                    this.scroll_secondary_to_ratio_from_mouse_y(event.position.y, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.is_dragging_secondary_scrollbar = false;
                    cx.notify();
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
                .w(px(400.0))
                .rounded_xl()
                .bg(theme.bg_panel)
                .border_1()
                .border_color(theme.border)
                .shadow_lg()
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(px(1.0))
                        .w_full()
                        .bg(theme.border_highlight),
                )
                .child(
                    div()
                        .p(px(22.0))
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
                ),
        )
}

/// Helper to render a keyboard shortcut chip with label
fn render_kbd_chip(shortcut: &'static str, label: &'static str, theme: crate::theme::Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(crate::components::toolbar::render_kbd_badge(shortcut, theme))
        .child(
            div()
                .text_size(px(10.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_muted)
                .child(label),
        )
}

/// Primary pane onboarding layout (3 Pillars + Recent Workspaces + Cheat Sheet)
fn render_onboarding_empty_state(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let selected_dev_info = workspace.get_selected_device_info();
    let has_devices = !workspace.adb_device_details.is_empty();
    let is_scanning = workspace.is_scanning_adb;

    div()
        .id("onboarding-scroll-container")
        .flex_1()
        .h_full()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .child(
            div()
                .w(px(860.0))
                .flex()
                .flex_col()
                .items_center()
                .gap_5()
                // Title Header
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .w(px(12.0))
                                        .h(px(12.0))
                                        .rounded_sm()
                                        .bg(theme.accent)
                                        .shadow_sm(),
                                )
                                .child(
                                    div()
                                        .text_size(px(22.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme.text_primary)
                                        .child("LogAnalysis Studio"),
                                )
                                .child(
                                    div()
                                        .px(px(5.0))
                                        .py(px(1.0))
                                        .rounded_xs()
                                        .bg(theme.bg_badge)
                                        .text_size(px(9.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme.accent)
                                        .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(theme.text_muted)
                                .child("GPU-Accelerated High-Performance Exploration for Android & System Logs"),
                        ),
                )
                // 3-Pillar Ingestion Grid
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .gap_3p5()
                        // Pillar 1: Local File Ingestion
                        .child(
                            div()
                                .flex_1()
                                .rounded_xl()
                                .bg(theme.bg_panel)
                                .border_1()
                                .border_color(theme.border)
                                .shadow_md()
                                .overflow_hidden()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .h(px(1.0))
                                        .w_full()
                                        .bg(theme.border_highlight),
                                )
                                .child(
                                    div()
                                        .p(px(18.0))
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .justify_between()
                                        .gap_3()
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_1p5()
                                                .child(
                                                    div()
                                                        .text_size(px(14.5))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(theme.text_primary)
                                                        .child("📂 Local Log File"),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(theme.text_muted)
                                                        .child("Inspect local dumps, logcat, and crash logs."),
                                                )
                                                .child(
                                                    div()
                                                        .id("drop-zone-local-file")
                                                        .mt(px(4.0))
                                                        .p(px(14.0))
                                                        .rounded_md()
                                                        .border_1()
                                                        .border_dashed()
                                                        .border_color(theme.border_subtle)
                                                        .bg(theme.bg_card)
                                                        .hover(|s| s.bg(theme.btn_hover).border_color(theme.accent))
                                                        .cursor_pointer()
                                                        .flex()
                                                        .flex_col()
                                                        .items_center()
                                                        .justify_center()
                                                        .gap_1()
                                                        .on_click(cx.listener(|this, _, window, cx| {
                                                            this.prompt_open_file(Some(window), cx);
                                                        }))
                                                        .child(div().text_size(px(18.0)).child("📂"))
                                                        .child(
                                                            div()
                                                                .text_size(px(10.5))
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .text_color(theme.text_primary)
                                                                .child("Drop file or click here"),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("btn-empty-state-open")
                                                .w_full()
                                                .py(px(7.5))
                                                .rounded_md()
                                                .bg(theme.accent)
                                                .hover(move |s| s.bg(theme.btn_hover))
                                                .text_color(rgb(0xffffff))
                                                .text_size(px(11.5))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .gap_1p5()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.prompt_open_file(Some(window), cx);
                                                }))
                                                .child("Open File")
                                                .child(crate::components::toolbar::render_kbd_badge_on_accent("Ctrl+O")),
                                        ),
                                ),
                        )
                        // Pillar 2: Multi-File Interleaved Merge
                        .child(
                            div()
                                .flex_1()
                                .rounded_xl()
                                .bg(theme.bg_panel)
                                .border_1()
                                .border_color(theme.border)
                                .shadow_md()
                                .overflow_hidden()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .h(px(1.0))
                                        .w_full()
                                        .bg(theme.border_highlight),
                                )
                                .child(
                                    div()
                                        .p(px(18.0))
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .justify_between()
                                        .gap_3()
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_1p5()
                                                .child(
                                                    div()
                                                        .text_size(px(14.5))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(theme.text_primary)
                                                        .child("🔀 Multi-File Merge"),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(theme.text_muted)
                                                        .child("Interleave multiple logs chronologically."),
                                                )
                                                .child(
                                                    div()
                                                        .mt(px(4.0))
                                                        .p(px(12.0))
                                                        .rounded_md()
                                                        .border_1()
                                                        .border_color(theme.border_subtle)
                                                        .bg(theme.bg_card)
                                                        .flex()
                                                        .flex_col()
                                                        .justify_center()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .text_size(px(10.5))
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .text_color(theme.text_muted)
                                                                .child("⚡ k-way Priority Heap"),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(px(10.5))
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .text_color(theme.text_muted)
                                                                .child("⏱️ Microsecond Interleaving"),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(px(10.5))
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .text_color(theme.text_muted)
                                                                .child("🔗 Stack Trace Continuity"),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("btn-empty-state-merge")
                                                .w_full()
                                                .py(px(7.5))
                                                .rounded_md()
                                                .bg(theme.accent)
                                                .hover(move |s| s.bg(theme.btn_hover))
                                                .text_color(rgb(0xffffff))
                                                .text_size(px(11.5))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .gap_1p5()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.prompt_open_merged_files(Some(window), cx);
                                                }))
                                                .child("Merge Files")
                                                .child(crate::components::toolbar::render_kbd_badge_on_accent("Ctrl+Shift+M")),
                                        ),
                                ),
                        )
                        // Pillar 3: Live Android ADB Stream
                        .child(
                            div()
                                .flex_1()
                                .rounded_xl()
                                .bg(theme.bg_panel)
                                .border_1()
                                .border_color(theme.border)
                                .shadow_md()
                                .overflow_hidden()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .h(px(1.0))
                                        .w_full()
                                        .bg(theme.border_highlight),
                                )
                                .child(
                                    div()
                                        .p(px(18.0))
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .justify_between()
                                        .gap_3()
                                        .child(
                                            div()
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
                                                                .text_size(px(14.5))
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
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(theme.text_muted)
                                                        .child("Stream live logcat with process isolation."),
                                                )
                                                .child(
                                                    if is_scanning {
                                                        div()
                                                            .mt(px(4.0))
                                                            .p(px(12.0))
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
                                                            .mt(px(4.0))
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
                                                                                    .text_size(px(11.5))
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
                                                                    .text_size(px(9.5))
                                                                    .font_family("monospace")
                                                                    .text_color(theme.text_muted)
                                                                    .child(format!("Serial: {} • {} found", dev_serial, workspace.adb_device_details.len())),
                                                            )
                                                    } else {
                                                        div()
                                                            .mt(px(4.0))
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
                                                                    .text_size(px(11.0))
                                                                    .font_weight(FontWeight::MEDIUM)
                                                                    .text_color(theme.warning_amber)
                                                                    .child("No Android devices detected"),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_size(px(9.5))
                                                                    .text_color(theme.text_muted)
                                                                    .child("Enable USB Debugging in Developer Options"),
                                                            )
                                                    },
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("btn-empty-state-adb")
                                                .w_full()
                                                .py(px(7.5))
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
                                                .text_size(px(11.5))
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
                        ),
                )
                // Recent Workspaces Shelf (when recent_files is not empty)
                .children(if !workspace.recent_files.is_empty() {
                    Some(
                        div()
                            .w_full()
                            .rounded_xl()
                            .bg(theme.bg_panel)
                            .border_1()
                            .border_color(theme.border)
                            .shadow_sm()
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(1.0))
                                    .w_full()
                                    .bg(theme.border_highlight),
                            )
                            .child(
                                div()
                                    .p(px(14.0))
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
                                                    .flex()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .text_size(px(11.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(theme.text_muted)
                                                    .child("🕒 RECENT WORKSPACES"),
                                            )
                                            .child(
                                                div()
                                                    .id("btn-clear-recent-files")
                                                    .cursor_pointer()
                                                    .text_size(px(10.0))
                                                    .text_color(theme.text_muted)
                                                    .hover(|s| s.text_color(theme.error_red))
                                                    .on_click(cx.listener(|this, _, _window, cx| {
                                                        this.clear_recent_files();
                                                        cx.notify();
                                                    }))
                                                    .child("Clear history"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .children(workspace.recent_files.iter().map(|path_buf| {
                                                let path_clone = path_buf.clone();
                                                let file_name = path_buf
                                                    .file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy()
                                                    .to_string();
                                                let path_display = path_buf.to_string_lossy().to_string();

                                                div()
                                                    .id(ElementId::Name(format!("recent-item-{}", path_display).into()))
                                                    .px(px(10.0))
                                                    .py(px(6.0))
                                                    .rounded_md()
                                                    .bg(theme.bg_card)
                                                    .border_1()
                                                    .border_color(theme.border_subtle)
                                                    .hover(|s| s.bg(theme.btn_hover).border_color(theme.accent))
                                                    .cursor_pointer()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                                        this.open_file(&path_clone, cx);
                                                    }))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_2()
                                                            .child(div().text_size(px(13.0)).child("📄"))
                                                            .child(
                                                                div()
                                                                    .font_weight(FontWeight::SEMIBOLD)
                                                                    .text_size(px(11.5))
                                                                    .text_color(theme.text_primary)
                                                                    .child(file_name),
                                                            )
                                                            .child(
                                                                div()
                                                                    .font_family("monospace")
                                                                    .text_size(px(10.0))
                                                                    .text_color(theme.text_muted)
                                                                    .child(path_display),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(10.5))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(theme.accent)
                                                            .child("Open ➔"),
                                                    )
                                                    .into_any_element()
                                            })),
                                    ),
                            ),
                    )
                } else {
                    None
                })
                // Interactive Keyboard Shortcut Cheat Sheet Bar
                .child(
                    div()
                        .w_full()
                        .pt(px(14.0))
                        .border_t_1()
                        .border_color(theme.border_subtle)
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .child(render_kbd_chip("Ctrl+O", "Open", theme))
                        .child(render_kbd_chip("Ctrl+P", "Palette", theme))
                        .child(render_kbd_chip("Ctrl+F", "Find", theme))
                        .child(render_kbd_chip("Ctrl+H", "Filter Mode", theme))
                        .child(render_kbd_chip("Ctrl+J", "Toggle Dock", theme))
                        .child(render_kbd_chip("Ctrl+\\", "Dual Split", theme))
                        .child(render_kbd_chip("Ctrl+Shift+M", "Merge", theme))
                        .child(render_kbd_chip("Ctrl+B", "Bookmarks", theme))
                        .child(render_kbd_chip("Ctrl+Shift+R", "Live ADB", theme)),
                ),
        )
}

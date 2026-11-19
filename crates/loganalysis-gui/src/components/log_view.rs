use gpui::prelude::FluentBuilder;
use gpui::*;
use loganalysis_core::{parse_column_line_auto, LineSeverity, LogSource};

use crate::components::find_bar::render_find_bar;
use crate::components::tooltip::render_tooltip;
use crate::workspace::LogAnalysisWorkspace;

/// Multiplier applied to font size to compute uniform log row height in logical pixels.
pub const LOG_ROW_HEIGHT_MULTIPLIER: f32 = 1.55;

/// Computes the rendered log row height in logical pixels for a given `font_size`.
#[inline]
pub fn log_row_height_px(font_size: f32) -> f32 {
    font_size * LOG_ROW_HEIGHT_MULTIPLIER
}

pub fn render_log_viewport(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;

    let main_content = if !workspace.split_view.is_split_view {
        render_pane_content(workspace, true, false, cx).into_any_element()
    } else {
        let is_vertical = workspace.split_view.is_split_vertical;
        let mut split_container = div()
            .flex_1()
            .w_full()
            .min_h(px(0.0))
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

        split_container.into_any_element()
    };

    let main_content_wrapper = div()
        .flex_1()
        .w_full()
        .min_h(px(0.0))
        .relative()
        .flex()
        .flex_col()
        .overflow_hidden()
        // Single primary viewport or Dual-Pane Split Container
        .child(main_content)
        // Modern Floating Find Widget (over log content without reflow)
        .children(render_find_bar(workspace, cx))
        // Contextual Floating Selection Mini-Bar
        .children(render_selection_action_bar(workspace, cx));

    div()
        .flex_1()
        .min_h(px(0.0))
        .relative()
        .flex()
        .flex_col()
        .bg(theme.bg_dark)
        .overflow_hidden()
        // Forensic Facets Bar
        .children(crate::components::facets_bar::render_facets_bar(workspace, cx))
        // Main log viewport + floating bars wrapper
        .child(main_content_wrapper)
        // Collapsible Line Detail Inspector
        .when(workspace.is_line_detail_open, |s| {
            s.child(render_line_detail_splitter(workspace, cx))
                .child(render_line_detail_panel(workspace, cx))
        })
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
    let active_sel = workspace.resolve_active_selected_line()?;
    let theme = workspace.theme;

    // Smart vertical positioning to avoid collisions with Find bar (main_content_wrapper sits below facets bar)
    let top_pos = calculate_selection_bar_top(workspace.search_ctrl.is_visible, false);

    // Extract human-friendly line index label (1-based)
    let line_label = if workspace.split_view.is_split_view && active_sel.is_secondary_pane {
        format!("Pane B • Line {}", active_sel.source_idx + 1)
    } else {
        format!("Line {}", active_sel.source_idx + 1)
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
            // Button: [📄 Detail]
            .child(
                div()
                    .id("btn-selection-detail")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .px(px(7.0))
                    .py(px(2.5))
                    .rounded_sm()
                    .when(workspace.is_line_detail_open, |s| {
                        s.bg(theme.accent_muted).text_color(theme.accent)
                    })
                    .when(!workspace.is_line_detail_open, |s| {
                        s.text_color(theme.text_primary)
                    })
                    .hover(move |s| s.bg(theme.btn_hover))
                    .cursor_pointer()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .tooltip(render_tooltip(
                        "Toggle wrapped Line Detail inspector",
                        Some("Ctrl+Shift+D"),
                        theme,
                    ))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.toggle_line_detail(cx);
                    }))
                    .child("📄 Detail"),
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
            .unwrap_or(if workspace.adb.is_streaming { "ADB Stream" } else { "Active Log" });
        format!("Pane A: {}", name)
    } else {
        let name = workspace
            .split_view
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
            .split_view
            .secondary_filter_result
            .as_ref()
            .map(|r| r.visible_indices.len())
            .unwrap_or(0)
    };

    let is_active_pane = if !workspace.split_view.is_split_view {
        true
    } else if is_primary {
        !workspace.split_view.active_pane_is_secondary
    } else {
        workspace.split_view.active_pane_is_secondary
    };

    let (badge_bg, badge_fg) = if is_active_pane {
        (theme.accent_muted, theme.accent)
    } else {
        (theme.bg_badge, theme.text_muted)
    };

    let header_border_color = if workspace.split_view.is_split_view && is_active_pane {
        theme.accent
    } else {
        theme.border_subtle
    };

    div()
        .flex_1()
        .min_h(px(0.0))
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .overflow_hidden()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                if this.split_view.is_split_view {
                    let new_sec = !is_primary;
                    if this.split_view.active_pane_is_secondary != new_sec {
                        this.split_view.active_pane_is_secondary = new_sec;
                        this.reset_line_detail_scroll_if_line_changed();
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
                .flex_shrink_0()
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
    let is_synced = workspace.split_view.is_scroll_synced;

    div()
        .id("split-divider-v")
        .w(px(32.0))
        .h_full()
        .flex_shrink_0()
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
    let is_synced = workspace.split_view.is_scroll_synced;

    div()
        .id("split-divider-h")
        .h(px(26.0))
        .w_full()
        .flex_shrink_0()
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
        workspace.split_view.secondary_log_source.is_some()
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

/// Computes match highlight byte ranges (capped at 32 per line) for `text`.
pub fn compute_highlight_ranges(
    text: &str,
    search_matcher: &Option<std::sync::Arc<loganalysis_core::CompiledMatcher>>,
) -> Vec<std::ops::Range<usize>> {
    match search_matcher {
        Some(matcher) => matcher.find_match_ranges(text, 32),
        None => Vec::new(),
    }
}

/// Helper to render text with search match highlighting with zero unnecessary string allocations
pub fn render_highlighted_text(
    text_content: SharedString,
    search_matcher: &Option<std::sync::Arc<loganalysis_core::CompiledMatcher>>,
    highlight_style: HighlightStyle,
) -> AnyElement {
    let ranges = compute_highlight_ranges(text_content.as_ref(), search_matcher);
    if !ranges.is_empty() {
        let highlights = ranges.into_iter().map(|r| (r, highlight_style));
        StyledText::new(text_content)
            .with_highlights(highlights)
            .into_any_element()
    } else {
        text_content.into_any_element()
    }
}

fn render_unstructured_line_content(
    text: SharedString,
    search_matcher: &Option<std::sync::Arc<loganalysis_core::CompiledMatcher>>,
    highlight_style: HighlightStyle,
    border_color: Rgba,
    stack_indent_px: f32,
    normal_px: f32,
    clip_overflow: bool,
) -> Div {
    let trimmed = text.as_ref().trim_start();
    let is_stack_trace = trimmed.starts_with("at ")
        || trimmed.starts_with("Caused by:")
        || trimmed.starts_with("...");
    div()
        .flex_1()
        .when(is_stack_trace, |s| {
            s.pl(px(stack_indent_px))
                .border_l_2()
                .border_color(border_color)
        })
        .when(!is_stack_trace, |s| s.px(px(normal_px)))
        .whitespace_nowrap()
        .when(clip_overflow, |s| s.overflow_hidden())
        .child(render_highlighted_text(text, search_matcher, highlight_style))
}

fn push_heatmap_tick(
    ticks: &mut Vec<Div>,
    start: usize,
    count: usize,
    run_sev: LineSeverity,
    bucket_step: f32,
) {
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

fn build_heatmap_ticks(
    heatmap: Option<&loganalysis_core::HeatmapData>,
    track_height: f32,
) -> Vec<Div> {
    let Some(heatmap) = heatmap else {
        return Vec::new();
    };
    let total_buckets = heatmap.buckets.len() as f32;
    if track_height <= 0.0 || total_buckets <= 0.0 {
        return Vec::new();
    }

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
                push_heatmap_tick(&mut ticks, start, count, run_sev, bucket_step);
                current_run = other.map(|sev| (idx, 1, sev));
            }
            (None, Some(sev)) => {
                current_run = Some((idx, 1, sev));
            }
            (None, None) => {}
        }
    }

    if let Some((start, count, run_sev)) = current_run {
        push_heatmap_tick(&mut ticks, start, count, run_sev, bucket_step);
    }

    ticks
}

/// Fixed height in pixels of the Column View header row and its right corner spacer.
pub const COLUMN_HEADER_HEIGHT_PX: f32 = 24.0;

/// Fixed width in pixels of the custom heatmap scrollbar track and matching column header corner spacer.
pub const SCROLLBAR_TRACK_WIDTH_PX: f32 = 15.0;

/// Fixed height in logical pixels of the bottom horizontal scrollbar track when H-scroll overflows.
pub const H_SCROLLBAR_TRACK_HEIGHT_PX: f32 = 12.0;

/// Minimum horizontal scrollbar thumb width in logical pixels for comfortable grabbing.
pub const H_SCROLLBAR_MIN_THUMB_WIDTH_PX: f32 = 28.0;

/// Sub-pixel hysteresis threshold to prevent showing horizontal scrollbar on 0.0x rounding noise.
pub const H_SCROLLBAR_OVERFLOW_EPSILON_PX: f32 = 0.5;

/// Maximum byte length rendered inside the Line Detail inspector body to avoid layout stalls on multi-MB lines.
pub const LINE_DETAIL_RENDER_MAX_BYTES: usize = 65_536;

/// Maximum raw line byte length scanned for embedded JSON payloads in Line Detail.
pub const LINE_DETAIL_JSON_PARSE_MAX_BYTES: usize = 262_144;

/// Suffix appended when a log line is truncated for display inside the Line Detail inspector.
pub const LINE_DETAIL_TRUNCATION_SUFFIX: &str =
    "…\n[Truncated for display — 📋 Copy copies full line]";

/// Computed thumb geometry for a horizontal scrollbar track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizontalScrollbarMetrics {
    pub thumb_left: f32,
    pub thumb_width: f32,
    pub max_scroll_x: f32,
}

/// Snapshot of horizontal scroll dimensions observed during `render` vs post-`prepaint`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HScrollRenderSnapshot {
    pub has_h_scrollbar: bool,
    pub content_width: f32,
    pub viewport_width: f32,
    pub clamped_scroll_x: f32,
}

/// Formatted payload produced by [`format_line_detail_text`] for the Line Detail inspector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineDetailFormatted {
    pub display_text: String,
    pub has_json: bool,
    pub is_truncated_for_render: bool,
}

/// Resolves the visible row index of the widest line in `filter_result` for `UniformList::with_width_from_item`.
/// Uses O(log N) binary search over monotonically sorted `visible_indices`, falling back to `Some(0)`
/// when `widest_visible_source_idx` is absent or temporarily unmatched, and `None` when empty.
pub fn resolve_widest_visible_row_idx(
    filter_result: Option<&loganalysis_core::FilterResult>,
) -> Option<usize> {
    let res = filter_result?;
    if res.visible_indices.is_empty() {
        return None;
    }
    res.widest_visible_source_idx
        .and_then(|src_idx| res.visible_indices.binary_search(&src_idx).ok())
        .or(Some(0))
}

/// Clamps raw GPUI horizontal scroll offset (always <= 0.0) into `[-max_scroll_x, 0.0]`,
/// guarding non-finite inputs (`NaN`, `Infinity`) and normalizing `-0.0` to `0.0`.
#[inline]
pub fn clamp_horizontal_scroll_x(scroll_x: f32, max_scroll_x: f32) -> f32 {
    if !scroll_x.is_finite() || !max_scroll_x.is_finite() || max_scroll_x <= 0.0 {
        return 0.0;
    }
    let clamped = scroll_x.clamp(-max_scroll_x, 0.0);
    if clamped == 0.0 {
        0.0
    } else {
        clamped
    }
}

/// Computes horizontal scrollbar thumb geometry. Returns `None` when content fits in viewport.
pub fn compute_horizontal_scrollbar_metrics(
    viewport_width: f32,
    content_width: f32,
    raw_scroll_x: f32,
) -> Option<HorizontalScrollbarMetrics> {
    if !viewport_width.is_finite()
        || !content_width.is_finite()
        || viewport_width <= 0.0
        || content_width <= viewport_width + H_SCROLLBAR_OVERFLOW_EPSILON_PX
    {
        return None;
    }
    let max_scroll_x = (content_width - viewport_width).max(0.0);
    let visible_ratio = (viewport_width / content_width).clamp(0.04, 1.0);
    let thumb_width = (viewport_width * visible_ratio)
        .max(H_SCROLLBAR_MIN_THUMB_WIDTH_PX)
        .min(viewport_width);
    let clamped_x = clamp_horizontal_scroll_x(raw_scroll_x, max_scroll_x);
    let ratio = if max_scroll_x > 0.0 {
        ((-clamped_x) / max_scroll_x).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb_left = ratio * (viewport_width - thumb_width).max(0.0);
    Some(HorizontalScrollbarMetrics {
        thumb_left,
        thumb_width,
        max_scroll_x,
    })
}

/// Converts a pointer X coordinate into a clamped negative `scroll_offset.x`.
/// When `grab_offset_in_thumb` is `Some(offset)`, preserves the pointer's relative grab point
/// inside the thumb; when `None`, centers the thumb at the clicked track position.
pub fn scroll_x_from_mouse_x(
    mouse_x: f32,
    track_left: f32,
    track_width: f32,
    thumb_width: f32,
    grab_offset_in_thumb: Option<f32>,
    max_scroll_x: f32,
) -> f32 {
    if !mouse_x.is_finite()
        || !track_left.is_finite()
        || !track_width.is_finite()
        || !thumb_width.is_finite()
        || !max_scroll_x.is_finite()
        || track_width <= thumb_width
        || max_scroll_x <= 0.0
    {
        return 0.0;
    }
    let scrollable_track = track_width - thumb_width;
    let rel_x = (mouse_x - track_left).clamp(0.0, track_width);
    let anchor = grab_offset_in_thumb.unwrap_or(thumb_width * 0.5);
    let target_thumb_left = (rel_x - anchor).clamp(0.0, scrollable_track);
    let ratio = (target_thumb_left / scrollable_track).clamp(0.0, 1.0);
    clamp_horizontal_scroll_x(-(ratio * max_scroll_x), max_scroll_x)
}

/// Reads the current horizontal scroll state snapshot from a [`UniformListScrollHandle`].
pub fn read_h_scroll_render_snapshot(
    handle: &UniformListScrollHandle,
    full_line_display: bool,
) -> HScrollRenderSnapshot {
    let state = handle.0.borrow();
    let viewport_width: f32 = state
        .last_item_size
        .map(|s| s.item.width.into())
        .unwrap_or_else(|| state.base_handle.bounds().size.width.into());
    if !full_line_display {
        return HScrollRenderSnapshot {
            has_h_scrollbar: false,
            content_width: viewport_width,
            viewport_width,
            clamped_scroll_x: 0.0,
        };
    }
    let content_width: f32 = state
        .last_item_size
        .map(|s| s.contents.width.into())
        .unwrap_or(viewport_width);
    let max_scroll_x = (content_width - viewport_width).max(0.0);
    let raw_scroll_x: f32 = state.base_handle.offset().x.into();
    let clamped_scroll_x = clamp_horizontal_scroll_x(raw_scroll_x, max_scroll_x);
    let has_h_scrollbar =
        viewport_width > 0.0 && content_width > viewport_width + H_SCROLLBAR_OVERFLOW_EPSILON_PX;
    HScrollRenderSnapshot {
        has_h_scrollbar,
        content_width,
        viewport_width,
        clamped_scroll_x,
    }
}

/// Returns `true` when two horizontal scroll snapshots differ beyond sub-pixel hysteresis (`0.5px`).
#[inline]
pub fn h_scroll_snapshots_differ(a: HScrollRenderSnapshot, b: HScrollRenderSnapshot) -> bool {
    a.has_h_scrollbar != b.has_h_scrollbar
        || (a.content_width - b.content_width).abs() > H_SCROLLBAR_OVERFLOW_EPSILON_PX
        || (a.viewport_width - b.viewport_width).abs() > H_SCROLLBAR_OVERFLOW_EPSILON_PX
        || (a.clamped_scroll_x - b.clamped_scroll_x).abs() > H_SCROLLBAR_OVERFLOW_EPSILON_PX
}

/// Returns `true` for non-trivial JSON payloads (`Object` with >= 1 key, or `Array` that
/// contains nested objects/arrays or >= 2 elements), avoiding false positives on `[1234]` / `[]` / `{}`.
pub fn is_meaningful_json_value(val: &serde_json::Value) -> bool {
    match val {
        serde_json::Value::Object(map) => !map.is_empty(),
        serde_json::Value::Array(arr) => {
            !arr.is_empty()
                && (arr.len() >= 2 || arr.iter().any(|v| v.is_object() || v.is_array()))
        }
        _ => false,
    }
}

/// Formats a raw log line for the Line Detail inspector:
/// - Strips trailing `\r`/`\n` (e.g. from `StreamLogBuffer::get_line_arc`)
/// - Skips JSON scanning when input exceeds `LINE_DETAIL_JSON_PARSE_MAX_BYTES` (`256 KiB`)
/// - Scans up to 32 `{` / `[` candidates using `serde_json::Deserializer::from_str(...).into_iter::<Value>()`
///   so trailing log suffixes (e.g. `" (12ms)"`) are preserved cleanly
/// - Safely caps `display_text.len() <= max_render_bytes` on a valid UTF-8 char boundary
pub fn format_line_detail_text(
    raw: &str,
    pretty_json: bool,
    max_render_bytes: usize,
) -> LineDetailFormatted {
    let clean_raw = raw.trim_end_matches(['\r', '\n']);

    let mut found_json: Option<(usize, usize, serde_json::Value)> = None;
    if clean_raw.len() <= LINE_DETAIL_JSON_PARSE_MAX_BYTES {
        for (idx, _) in clean_raw.match_indices(['{', '[']).take(32) {
            let mut stream = serde_json::Deserializer::from_str(&clean_raw[idx..])
                .into_iter::<serde_json::Value>();
            if let Some(Ok(val)) = stream.next() {
                if is_meaningful_json_value(&val) {
                    let consumed = stream.byte_offset();
                    found_json = Some((idx, consumed, val));
                    break;
                }
            }
        }
    }

    let has_json = found_json.is_some();
    let formatted = if pretty_json {
        if let Some((idx, consumed, val)) = found_json {
            if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                let prefix = clean_raw[..idx].trim_end();
                let suffix = clean_raw[idx + consumed..].trim();
                let mut out =
                    String::with_capacity(prefix.len() + pretty.len() + suffix.len() + 4);
                if !prefix.is_empty() {
                    out.push_str(prefix);
                    out.push('\n');
                }
                out.push_str(&pretty);
                if !suffix.is_empty() {
                    out.push('\n');
                    out.push_str(suffix);
                }
                out
            } else {
                clean_raw.to_string()
            }
        } else {
            clean_raw.to_string()
        }
    } else {
        clean_raw.to_string()
    };

    let (display_text, is_truncated_for_render) = if formatted.len() > max_render_bytes {
        if max_render_bytes >= LINE_DETAIL_TRUNCATION_SUFFIX.len() {
            let budget = max_render_bytes - LINE_DETAIL_TRUNCATION_SUFFIX.len();
            let cut = formatted.floor_char_boundary(budget);
            let mut truncated =
                String::with_capacity(cut + LINE_DETAIL_TRUNCATION_SUFFIX.len());
            truncated.push_str(&formatted[..cut]);
            truncated.push_str(LINE_DETAIL_TRUNCATION_SUFFIX);
            (truncated, true)
        } else {
            let cut = formatted.floor_char_boundary(max_render_bytes);
            (formatted[..cut].to_string(), true)
        }
    } else {
        (formatted, false)
    };

    LineDetailFormatted {
        display_text,
        has_json,
        is_truncated_for_render,
    }
}

/// Computes the rendered `(list_height_px, scrollbar_track_height_px)` pair for a pane of `viewport_height`.
/// Because the column header sits above the `[list_el | scrollbar_track]` row, both elements always share identical height.
#[inline]
pub fn aligned_list_and_scrollbar_height(viewport_height: f32, is_column_view: bool) -> (f32, f32) {
    let header_offset = if is_column_view {
        COLUMN_HEADER_HEIGHT_PX
    } else {
        0.0
    };
    let h = (viewport_height - header_offset).max(0.0);
    (h, h)
}

/// Computes the rendered `(list_height_px, scrollbar_track_height_px)` pair for a pane of `viewport_height`,
/// accounting for both the optional Column View header and the optional bottom horizontal scrollbar track.
#[inline]
pub fn aligned_list_and_scrollbar_height_with_hscroll(
    viewport_height: f32,
    is_column_view: bool,
    has_h_scrollbar: bool,
) -> (f32, f32) {
    let header_offset = if is_column_view {
        COLUMN_HEADER_HEIGHT_PX
    } else {
        0.0
    };
    let hscroll_offset = if has_h_scrollbar {
        H_SCROLLBAR_TRACK_HEIGHT_PX
    } else {
        0.0
    };
    let h = (viewport_height - header_offset - hscroll_offset).max(0.0);
    (h, h)
}

/// Renders the horizontal resize splitter above the collapsible Line Detail panel.
fn render_line_detail_splitter(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let is_resizing = workspace.is_resizing_line_detail;

    div()
        .id("line-detail-splitter")
        .w_full()
        .h(px(4.0))
        .flex_shrink_0()
        .cursor_row_resize()
        .bg(if is_resizing {
            theme.accent
        } else {
            theme.border
        })
        .hover(move |s| s.bg(theme.accent))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                if event.click_count == 2 {
                    this.reset_line_detail_height(cx);
                } else {
                    this.start_resizing_line_detail(event.position, cx);
                }
            }),
        )
}

/// Renders the collapsible bottom Line Detail inspector with word-wrapped monospace text and JSON pretty-printing.
fn render_line_detail_panel(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let active_opt = workspace.resolve_active_selected_line();

    let formatted_opt = active_opt.as_ref().map(|active| {
        if let Some((cached_key, cached_val)) = &workspace.cached_line_detail {
            if cached_key.is_secondary == active.is_secondary_pane
                && cached_key.abs_source_idx == active.abs_source_idx
                && cached_key.pretty_json == workspace.is_line_detail_json_pretty
                && cached_key.line_len == active.line_text.len()
            {
                return cached_val.clone();
            }
        }
        format_line_detail_text(
            active.line_text.as_ref(),
            workspace.is_line_detail_json_pretty,
            LINE_DETAIL_RENDER_MAX_BYTES,
        )
    });

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

    let has_json = formatted_opt.as_ref().is_some_and(|f| f.has_json);
    let is_json_pretty = workspace.is_line_detail_json_pretty;

    let header_el = div()
        .id("line-detail-header")
        .w_full()
        .h(px(28.0))
        .flex_shrink_0()
        .bg(theme.bg_header)
        .border_b_1()
        .border_color(theme.border_subtle)
        .px(px(10.0))
        .flex()
        .items_center()
        .justify_between()
        // Left metadata badges
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .overflow_hidden()
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text_primary)
                        .child("📄 Line Detail"),
                )
                .when_some(active_opt.as_ref(), |s, active| {
                    let char_count = active.line_text.as_ref().chars().count();
                    let is_truncated = formatted_opt
                        .as_ref()
                        .is_some_and(|f| f.is_truncated_for_render);
                    let line_badge = format!("Line {}", active.source_idx + 1);
                    let chars_badge = format!("{char_count} chars");

                    s.when(workspace.split_view.is_split_view, |s| {
                        s.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded_xs()
                                .bg(theme.bg_badge)
                                .text_size(px(9.5))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.text_muted)
                                .child(if active.is_secondary_pane {
                                    "PANE B"
                                } else {
                                    "PANE A"
                                }),
                        )
                    })
                    .child(
                        div()
                            .px(px(5.0))
                            .py(px(1.0))
                            .rounded_xs()
                            .bg(theme.accent_muted)
                            .font_family("monospace")
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.accent)
                            .child(line_badge),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .child(chars_badge),
                    )
                    .when(is_truncated, |s| {
                        s.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded_xs()
                                .bg(theme.bg_badge)
                                .text_size(px(9.5))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.warning_amber)
                                .child("truncated"),
                        )
                    })
                    .when_some(active.source_badge.as_ref(), |s, (s_id, s_name)| {
                        let badge_colors = [
                            (rgb(0x1e3a8a), rgb(0x93c5fd)),
                            (rgb(0x581c87), rgb(0xd8b4fe)),
                            (rgb(0x064e3b), rgb(0x6ee7b7)),
                            (rgb(0x7c2d12), rgb(0xfdba74)),
                        ];
                        let (badge_bg, badge_fg) = badge_colors[(*s_id as usize) % badge_colors.len()];
                        s.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded_xs()
                                .bg(badge_bg)
                                .text_color(badge_fg)
                                .text_size(px(9.5))
                                .font_weight(FontWeight::BOLD)
                                .child(s_name.clone()),
                        )
                    })
                    .when(active.source_badge.is_none(), |s| {
                        s.when_some(active.parsed_time.as_ref(), |s, time_str| {
                            s.child(
                                div()
                                    .font_family("monospace")
                                    .text_size(px(10.0))
                                    .text_color(theme.text_muted)
                                    .child(time_str.clone()),
                            )
                        })
                    })
                }),
        )
        // Right action buttons
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .flex_shrink_0()
                .when(has_json, |s| {
                    s.child(
                        div()
                            .id("btn-line-detail-json")
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded_sm()
                            .cursor_pointer()
                            .font_family("monospace")
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .when(is_json_pretty, |b| {
                                b.bg(theme.accent_muted).text_color(theme.accent)
                            })
                            .when(!is_json_pretty, |b| {
                                b.bg(theme.bg_card).text_color(theme.text_muted)
                            })
                            .hover(move |b| b.bg(theme.btn_hover))
                            .tooltip(render_tooltip("Toggle JSON pretty-printing", None, theme))
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.toggle_line_detail_json_pretty(cx);
                            }))
                            .child("{ } JSON"),
                    )
                })
                .when(active_opt.is_some(), |s| {
                    s.child(
                        div()
                            .id("btn-line-detail-copy")
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(theme.bg_card)
                            .hover(move |b| b.bg(theme.btn_hover))
                            .text_size(px(10.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .tooltip(render_tooltip(
                                "Copy full raw line to clipboard",
                                Some("Ctrl+C"),
                                theme,
                            ))
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.copy_selection(cx);
                            }))
                            .child("📋 Copy"),
                    )
                })
                .child(
                    div()
                        .id("btn-line-detail-close")
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded_sm()
                        .cursor_pointer()
                        .hover(move |b| b.bg(theme.btn_hover).text_color(theme.error_red))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text_muted)
                        .tooltip(render_tooltip("Close Line Detail inspector", Some("Esc"), theme))
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.close_line_detail(cx);
                        }))
                        .child("✕"),
                ),
        );

    let body_el = if let Some(formatted) = formatted_opt {
        div()
            .id("line-detail-scroll-body")
            .flex_1()
            .w_full()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&workspace.line_detail_scroll_handle)
            .bg(theme.bg_viewport)
            .p(px(10.0))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .whitespace_normal()
                    .font_family("monospace")
                    .text_size(workspace.font_size)
                    .line_height(px(f32::from(workspace.font_size) * 1.45))
                    .text_color(theme.text_primary)
                    .child(render_highlighted_text(
                        SharedString::from(formatted.display_text),
                        &workspace.search_ctrl.cached_matcher,
                        highlight_style,
                    )),
            )
            .into_any_element()
    } else {
        div()
            .id("line-detail-scroll-body")
            .flex_1()
            .w_full()
            .min_h(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.bg_viewport)
            .p(px(10.0))
            .text_size(px(11.5))
            .text_color(theme.text_muted)
            .child("Select any log line to inspect its full wrapped contents")
            .into_any_element()
    };

    div()
        .id("line-detail-panel")
        .w_full()
        .h(px(workspace.line_detail_height))
        .max_h(relative(0.55))
        .flex_shrink_0()
        .bg(theme.bg_panel)
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(header_el)
        .child(body_el)
}

/// Column Header Bar for Structured Column Grid View
fn render_column_header(theme: crate::theme::Theme, scroll_x: f32) -> impl IntoElement {
    div()
        .h_full()
        .min_w_full()
        .when(scroll_x < 0.0, |s| s.ml(px(scroll_x)))
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
    let row_height = px(log_row_height_px(font_size.into()));
    let weak_self = cx.weak_entity();

    let row_count = if is_primary {
        workspace
            .filter_result
            .as_ref()
            .map(|r| r.visible_indices.len())
            .unwrap_or(0)
    } else {
        workspace
            .split_view
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
    let full_line_display_fallback = workspace.full_line_display;
    let weak_handle = weak_self.clone();
    let list_id = if is_primary { "log-view" } else { "secondary-log-view" };

    let mut list = uniform_list(list_id, row_count, move |range, _window, cx| {
        let ws_opt = weak_handle.upgrade();
        let ws = ws_opt.as_ref().map(|entity| entity.read(cx));

        let filter_res = if is_primary {
            ws.and_then(|w| w.filter_result.as_deref())
        } else {
            ws.and_then(|w| w.split_view.secondary_filter_result.as_deref())
        };

        let source = if is_primary {
            ws.and_then(|w| w.log_source.as_deref())
        } else {
            ws.and_then(|w| w.split_view.secondary_log_source.as_deref())
        };

        let selected = if is_primary {
            ws.and_then(|w| w.selection.selected_row)
        } else {
            ws.and_then(|w| w.secondary_selection.selected_row)
        };

        let search_matcher = ws.and_then(|w| w.search_ctrl.cached_matcher.clone());
        let row_theme = ws.map(|w| w.theme).unwrap_or(theme);
        let is_col_view = ws.map(|w| w.is_column_view).unwrap_or(false);
        let full_line_display = ws
            .map(|w| w.full_line_display)
            .unwrap_or(full_line_display_fallback);

        range
            .map(|idx| {
                let (real_idx, text): (Option<usize>, SharedString) =
                    if let (Some(res), Some(src)) = (filter_res, source) {
                        if let Some(r_idx) = crate::selection::resolve_selected_source_idx(
                            &res.visible_indices,
                            Some(idx),
                        ) {
                            let t: SharedString = src
                                .get_line_arc(r_idx)
                                .map(SharedString::new)
                                .unwrap_or_default();
                            (Some(r_idx), t)
                        } else {
                            (None, SharedString::default())
                        }
                    } else {
                        (None, SharedString::default())
                    };

                let is_bookmarked = if is_primary {
                    real_idx
                        .and_then(|r| ws.map(|w| w.bookmarks.is_bookmarked(r)))
                        .unwrap_or(false)
                } else {
                    false
                };

                let (bg_color, style_opt) = if let Some(res) = filter_res {
                    if let Some(style) = res.visible_styles.get(idx) {
                        (
                            style.bg_hex.map(rgb).unwrap_or(row_theme.bg_dark),
                            Some(*style),
                        )
                    } else {
                        (row_theme.bg_dark, None)
                    }
                } else {
                    (row_theme.bg_dark, None)
                };

                let is_selected = selected == Some(idx);
                let has_custom_bg = style_opt.is_some_and(|s| s.bg_hex.is_some());
                let final_bg = if is_selected {
                    row_theme.selection_bg
                } else {
                    bg_color
                };
                let final_fg = match style_opt {
                    Some(style) => crate::theme::resolve_selected_row_fg(
                        is_selected,
                        style.is_matched,
                        style.fg_hex,
                        row_theme,
                    ),
                    None => {
                        crate::theme::resolve_selected_row_fg(is_selected, false, None, row_theme)
                    }
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
                            .child(real_idx.map(|r| format!("{}", r + 1)).unwrap_or_default()),
                    );

                let mut row_el = div()
                    .id(ElementId::NamedInteger(row_id.into(), idx as u64))
                    .h(row_height)
                    .min_h(row_height)
                    .max_h(row_height)
                    .when(!full_line_display, |s| s.w_full().overflow_hidden())
                    .when(full_line_display, |s| s.min_w_full())
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
                    .cursor_pointer()
                    .on_click(move |event: &ClickEvent, _window, cx| {
                        if let Some(real_idx) = real_idx {
                            let _ = entity_handle.update(cx, |this, cx| {
                                if is_primary {
                                    this.select_log_row(idx, real_idx, cx);
                                } else {
                                    this.select_secondary_log_row(idx, real_idx, cx);
                                }
                                if event.standard_click() && event.click_count() == 2 {
                                    this.open_line_detail(cx);
                                }
                            });
                        }
                    })
                    .child(gutter_el);

                if let Some((s_id, s_name)) =
                    real_idx.and_then(|r| source.and_then(|s| s.get_line_source_info(r)))
                {
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
                    let parsed = parse_column_line_auto(text.as_ref());
                    if parsed.time.is_some() || parsed.level_char.is_some() {
                        let time_str = parsed.time.unwrap_or("-");
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
                                    .when(!full_line_display, |s| s.overflow_hidden())
                                    .child(render_highlighted_text(
                                        SharedString::new(msg_text),
                                        &search_matcher,
                                        highlight_style,
                                    )),
                            );
                    } else {
                        // Unstructured multiline stack trace fallback
                        row_el = row_el.child(render_unstructured_line_content(
                            text.clone(),
                            &search_matcher,
                            highlight_style,
                            row_theme.border_subtle,
                            24.0,
                            12.0,
                            !full_line_display,
                        ));
                    }
                } else {
                    // Raw monospace line with stack trace indentation
                    row_el = row_el.child(render_unstructured_line_content(
                        text.clone(),
                        &search_matcher,
                        highlight_style,
                        row_theme.border_subtle,
                        22.0,
                        10.0,
                        false,
                    ));
                }

                row_el
            })
            .collect::<Vec<_>>()
    });

    if workspace.full_line_display {
        let widest_filter = if is_primary {
            workspace.filter_result.as_deref()
        } else {
            workspace.split_view.secondary_filter_result.as_deref()
        };
        let widest_row_idx = resolve_widest_visible_row_idx(widest_filter);
        list = list
            .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
            .with_width_from_item(widest_row_idx);
    }

    list = list.on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, window, cx| {
        if !this.full_line_display || !event.modifiers.shift {
            return;
        }
        let pixel_delta = event.delta.pixel_delta(window.line_height());
        if pixel_delta.x.is_zero() && !pixel_delta.y.is_zero() {
            let handle = if is_primary {
                &this.scroll_handle
            } else {
                &this.secondary_scroll_handle
            };
            let state = handle.0.borrow();
            let max_offset = state.base_handle.max_offset();
            let max_x: f32 = max_offset.width.into();
            let max_y: f32 = max_offset.height.into();
            let cur = state.base_handle.offset();
            let restored_y = (f32::from(cur.y) - f32::from(pixel_delta.y)).clamp(-max_y, 0.0);
            let next_x = clamp_horizontal_scroll_x(
                f32::from(cur.x) + f32::from(pixel_delta.y),
                max_x,
            );
            state.base_handle.set_offset(point(px(next_x), px(restored_y)));
            drop(state);
            cx.stop_propagation();
            cx.notify();
        }
    }));

    let scroll_handle = if is_primary {
        workspace.scroll_handle.clone()
    } else {
        workspace.secondary_scroll_handle.clone()
    };

    let list_el = list.track_scroll(scroll_handle.clone()).flex_1().h_full();

    // Compute scrollbar thumb position
    let (thumb_top, thumb_height, track_height) = {
        let scroll_state = scroll_handle.0.borrow();
        let bounds = scroll_state.base_handle.bounds();
        let track_height: f32 = bounds.size.height.into();
        let scroll_y: f32 = (-scroll_state.base_handle.offset().y).into();
        drop(scroll_state);

        let font_size_f32: f32 = font_size.into();
        let row_height_f32 = log_row_height_px(font_size_f32);
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
        build_heatmap_ticks(workspace.heatmap_data.as_deref(), track_height)
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
        .w(px(SCROLLBAR_TRACK_WIDTH_PX))
        .h_full()
        .bg(theme.bg_gutter)
        .border_l_1()
        .border_color(theme.border_subtle)
        .cursor_pointer();

    if is_primary {
        scrollbar_track = scrollbar_track.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                this.is_dragging_scrollbar = true;
                this.scroll_to_ratio_from_mouse_y(event.position.y, cx);
                cx.notify();
            }),
        );
    } else {
        scrollbar_track = scrollbar_track.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                this.is_dragging_secondary_scrollbar = true;
                this.scroll_secondary_to_ratio_from_mouse_y(event.position.y, cx);
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

    let rendered_snapshot =
        read_h_scroll_render_snapshot(&scroll_handle, workspace.full_line_display);
    let scroll_handle_for_prepaint = scroll_handle.clone();
    let weak_for_prepaint = weak_self.clone();
    let full_line_for_prepaint = workspace.full_line_display;

    let list_and_scrollbar = div()
        .flex_1()
        .w_full()
        .min_h(px(0.0))
        .flex()
        .flex_row()
        .overflow_hidden()
        .on_children_prepainted(move |_, window, _cx| {
            let latest = read_h_scroll_render_snapshot(
                &scroll_handle_for_prepaint,
                full_line_for_prepaint,
            );
            if h_scroll_snapshots_differ(rendered_snapshot, latest) {
                let weak = weak_for_prepaint.clone();
                window.on_next_frame(move |_, cx| {
                    let _ = weak.update(cx, |_, cx| cx.notify());
                });
            }
        })
        .child(list_el)
        .child(scrollbar_track);

    let h_scrollbar_row = if rendered_snapshot.has_h_scrollbar {
        compute_horizontal_scrollbar_metrics(
            rendered_snapshot.viewport_width,
            rendered_snapshot.content_width,
            rendered_snapshot.clamped_scroll_x,
        )
        .map(|metrics| {
            let h_track_id = if is_primary {
                "log-h-scrollbar-track"
            } else {
                "secondary-h-scrollbar-track"
            };
            let h_thumb_id = if is_primary {
                "log-h-scrollbar-thumb"
            } else {
                "secondary-h-scrollbar-thumb"
            };
            let is_h_dragging = if is_primary {
                workspace.is_dragging_h_scrollbar
            } else {
                workspace.is_dragging_secondary_h_scrollbar
            };
            let h_thumb_color = if is_h_dragging {
                theme.accent
            } else if workspace.is_dark_theme {
                rgba(0xffffff38)
            } else {
                rgba(0x00000038)
            };
            let track_bounds_cell = if is_primary {
                workspace.h_scrollbar_track_bounds.clone()
            } else {
                workspace.secondary_h_scrollbar_track_bounds.clone()
            };
            let track_bounds_for_prepaint = track_bounds_cell.clone();
            let track_bounds_for_click = track_bounds_cell;
            let handle_for_click = scroll_handle.clone();
            let thumb_left = metrics.thumb_left;
            let thumb_width = metrics.thumb_width;

            div()
                .w_full()
                .h(px(H_SCROLLBAR_TRACK_HEIGHT_PX))
                .flex()
                .flex_row()
                .flex_shrink_0()
                .on_children_prepainted(move |children_bounds, _window, _cx| {
                    if let Some(&first_bounds) = children_bounds.first() {
                        track_bounds_for_prepaint.set(first_bounds);
                    }
                })
                .child(
                    div()
                        .id(h_track_id)
                        .flex_1()
                        .h_full()
                        .relative()
                        .bg(theme.bg_gutter)
                        .border_t_1()
                        .border_color(theme.border_subtle)
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                let captured = track_bounds_for_click.get();
                                let effective = if captured.size.width > px(0.0) {
                                    captured
                                } else {
                                    handle_for_click.0.borrow().base_handle.bounds()
                                };
                                let track_left: f32 = effective.origin.x.into();
                                let rel_x = f32::from(event.position.x) - track_left;
                                let grab_offset = if rel_x >= thumb_left
                                    && rel_x <= thumb_left + thumb_width
                                {
                                    rel_x - thumb_left
                                } else {
                                    thumb_width * 0.5
                                };
                                this.h_scrollbar_drag_grab_offset = Some(grab_offset);
                                if is_primary {
                                    this.is_dragging_h_scrollbar = true;
                                } else {
                                    this.is_dragging_secondary_h_scrollbar = true;
                                }
                                this.scroll_h_to_mouse_x(is_primary, event.position.x, cx);
                                cx.notify();
                            }),
                        )
                        .child(
                            div()
                                .id(h_thumb_id)
                                .absolute()
                                .top(px(2.0))
                                .bottom(px(2.0))
                                .left(px(metrics.thumb_left))
                                .w(px(metrics.thumb_width))
                                .rounded_full()
                                .bg(h_thumb_color)
                                .hover(move |s| s.bg(theme.accent)),
                        ),
                )
                .child(
                    div()
                        .w(px(SCROLLBAR_TRACK_WIDTH_PX))
                        .h_full()
                        .flex_shrink_0()
                        .bg(theme.bg_gutter)
                        .border_t_1()
                        .border_l_1()
                        .border_color(theme.border_subtle),
                )
        })
    } else {
        None
    };

    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .overflow_hidden()
        .when(is_column_view, |s| {
            s.child(
                div()
                    .w_full()
                    .h(px(COLUMN_HEADER_HEIGHT_PX))
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .bg(theme.bg_header)
                            .border_b_1()
                            .border_color(theme.border_subtle)
                            .overflow_hidden()
                            .child(render_column_header(
                                theme,
                                rendered_snapshot.clamped_scroll_x,
                            )),
                    )
                    .child(
                        div()
                            .w(px(SCROLLBAR_TRACK_WIDTH_PX))
                            .h_full()
                            .flex_shrink_0()
                            .bg(theme.bg_header)
                            .border_b_1()
                            .border_l_1()
                            .border_color(theme.border_subtle),
                    ),
            )
        })
        .child(list_and_scrollbar)
        .children(h_scrollbar_row)
}

/// Secondary pane empty state when no file is loaded in Pane B
fn render_secondary_empty_state(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;

    div()
        .flex_1()
        .w_full()
        .min_h(px(0.0))
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
    let has_devices = !workspace.adb.device_details.is_empty();
    let is_scanning = workspace.adb.is_scanning;

    div()
        .id("onboarding-scroll-container")
        .flex_1()
        .w_full()
        .min_h(px(0.0))
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
                                                                .child("⚡ Chronological Stable Sort"),
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
                                                                    .child(format!("Serial: {} • {} found", dev_serial, workspace.adb.device_details.len())),
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
                                                    if !this.adb.device_details.is_empty() {
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
                                                        this.clear_recent_files_async(cx);
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

#[cfg(test)]
mod tests {
    use super::{
        aligned_list_and_scrollbar_height, aligned_list_and_scrollbar_height_with_hscroll,
        build_heatmap_ticks, clamp_horizontal_scroll_x, compute_highlight_ranges,
        compute_horizontal_scrollbar_metrics, format_line_detail_text, h_scroll_snapshots_differ,
        is_meaningful_json_value, log_row_height_px, read_h_scroll_render_snapshot,
        resolve_widest_visible_row_idx, scroll_x_from_mouse_x, HScrollRenderSnapshot,
        HorizontalScrollbarMetrics, COLUMN_HEADER_HEIGHT_PX, H_SCROLLBAR_MIN_THUMB_WIDTH_PX,
        H_SCROLLBAR_TRACK_HEIGHT_PX, LINE_DETAIL_JSON_PARSE_MAX_BYTES,
        LINE_DETAIL_TRUNCATION_SUFFIX, LOG_ROW_HEIGHT_MULTIPLIER, SCROLLBAR_TRACK_WIDTH_PX,
    };
    use gpui::{point, px, UniformListScrollHandle};
    use loganalysis_core::{
        execute_filtering, ActiveLogSource, CompiledMatcher, CompiledStreamFilters, FilterResult,
        FilterRule, HeatmapBucket, HeatmapData, LogSource, StreamLogBuffer,
    };
    use std::sync::Arc;

    #[test]
    fn test_column_view_scrollbar_alignment_constants_and_heights() {
        assert_eq!(COLUMN_HEADER_HEIGHT_PX, 24.0);
        assert_eq!(SCROLLBAR_TRACK_WIDTH_PX, 15.0);
        assert_eq!(aligned_list_and_scrollbar_height(600.0, false), (600.0, 600.0));
        assert_eq!(aligned_list_and_scrollbar_height(600.0, true), (576.0, 576.0));
    }

    #[test]
    fn test_hscroll_alignment_constants_and_heights() {
        assert_eq!(H_SCROLLBAR_TRACK_HEIGHT_PX, 12.0);
        assert_eq!(
            aligned_list_and_scrollbar_height_with_hscroll(600.0, false, false),
            (600.0, 600.0)
        );
        assert_eq!(
            aligned_list_and_scrollbar_height_with_hscroll(600.0, false, true),
            (588.0, 588.0)
        );
        assert_eq!(
            aligned_list_and_scrollbar_height_with_hscroll(600.0, true, true),
            (564.0, 564.0)
        );
        assert_eq!(
            aligned_list_and_scrollbar_height_with_hscroll(10.0, true, true),
            (0.0, 0.0)
        );
    }

    #[test]
    fn test_resolve_widest_visible_row_idx() {
        assert_eq!(resolve_widest_visible_row_idx(None), None);

        let empty = FilterResult::default();
        assert_eq!(resolve_widest_visible_row_idx(Some(&empty)), None);

        // Widest line in the middle of visible_indices
        let mid = FilterResult {
            visible_indices: vec![10, 25, 42, 99],
            widest_visible_source_idx: Some(42),
            ..Default::default()
        };
        assert_eq!(resolve_widest_visible_row_idx(Some(&mid)), Some(2));

        // Widest line at the last visible index (simulating post-trim shift)
        let last = FilterResult {
            visible_indices: vec![5, 12, 30],
            widest_visible_source_idx: Some(30),
            ..Default::default()
        };
        assert_eq!(resolve_widest_visible_row_idx(Some(&last)), Some(2));

        // Fallback to Some(0) when widest_visible_source_idx is None or temporarily unmatched
        let missing_idx = FilterResult {
            visible_indices: vec![5, 12, 30],
            widest_visible_source_idx: None,
            ..Default::default()
        };
        assert_eq!(resolve_widest_visible_row_idx(Some(&missing_idx)), Some(0));

        let unmatched_idx = FilterResult {
            visible_indices: vec![5, 12, 30],
            widest_visible_source_idx: Some(13),
            ..Default::default()
        };
        assert_eq!(
            resolve_widest_visible_row_idx(Some(&unmatched_idx)),
            Some(0)
        );

        // End-to-end ring-buffer trim reconciliation with StreamLogBuffer + append_streaming_batch
        let mut stream = StreamLogBuffer::new(4, 2);
        stream.append_chunk(vec![
            "01-01 00:00:00.000 I Tag: short_0".to_string(),
            format!("01-01 00:00:01.000 E Tag: widest_old_{}", "X".repeat(120)),
            "01-01 00:00:02.000 E Tag: mid_2_abcdef".to_string(),
            "01-01 00:00:03.000 I Tag: short_3".to_string(),
        ]);
        let rules = vec![FilterRule {
            text: "E Tag:".to_string(),
            active: true,
            ..Default::default()
        }];
        let compiled = CompiledStreamFilters::new(&rules);
        let mut res = execute_filtering(&stream, &rules, true, None, 1).unwrap();
        // Visible: [1 (widest_old), 2 (mid_2)] -> widest is visible row 0
        assert_eq!(res.visible_indices, vec![1, 2]);
        assert_eq!(resolve_widest_visible_row_idx(Some(&res)), Some(0));

        // Append 2 more lines and trim 2 leading lines (widest_old evicted, mid_2 shifts to source_idx 0)
        stream.append_chunk(vec![
            "01-01 00:00:04.000 I Tag: short_4".to_string(),
            format!("01-01 00:00:05.000 E Tag: new_widest_{}", "Y".repeat(40)),
        ]);
        let trimmed = stream.trim_if_needed().expect("ring buffer should trim");
        assert_eq!(trimmed, 2);
        let live_src = ActiveLogSource::Stream(stream);
        compiled.append_streaming_batch(&mut res, &live_src, 2..live_src.line_count(), trimmed, true);
        // Surviving visible: [0 (mid_2), 3 (new_widest)] -> widest is visible row 1
        assert_eq!(res.visible_indices, vec![0, 3]);
        assert_eq!(resolve_widest_visible_row_idx(Some(&res)), Some(1));
    }

    #[test]
    fn test_horizontal_scrollbar_math_and_snapshots() {
        // Non-finite and -0.0 normalization
        assert_eq!(clamp_horizontal_scroll_x(f32::NAN, 100.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(
            clamp_horizontal_scroll_x(f32::INFINITY, 100.0).to_bits(),
            0.0f32.to_bits()
        );
        assert_eq!(
            clamp_horizontal_scroll_x(f32::NEG_INFINITY, 100.0).to_bits(),
            0.0f32.to_bits()
        );
        assert_eq!(clamp_horizontal_scroll_x(-50.0, f32::NAN).to_bits(), 0.0f32.to_bits());
        assert_eq!(
            clamp_horizontal_scroll_x(-50.0, f32::INFINITY).to_bits(),
            0.0f32.to_bits()
        );
        assert_eq!(
            clamp_horizontal_scroll_x(-50.0, f32::NEG_INFINITY).to_bits(),
            0.0f32.to_bits()
        );
        assert_eq!(clamp_horizontal_scroll_x(-0.0, 100.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(clamp_horizontal_scroll_x(-50.0, 0.0), 0.0);
        assert_eq!(clamp_horizontal_scroll_x(25.0, 100.0), 0.0);
        assert_eq!(clamp_horizontal_scroll_x(-250.0, 100.0), -100.0);

        // No overflow within 0.5px hysteresis (including exact 0.5px boundary) -> None
        assert_eq!(compute_horizontal_scrollbar_metrics(400.0, 400.0, 0.0), None);
        assert_eq!(compute_horizontal_scrollbar_metrics(400.0, 400.4, 0.0), None);
        assert_eq!(compute_horizontal_scrollbar_metrics(400.0, 400.5, 0.0), None);
        assert!(compute_horizontal_scrollbar_metrics(400.0, 400.6, 0.0).is_some());
        assert_eq!(compute_horizontal_scrollbar_metrics(0.0, 400.0, 0.0), None);
        assert_eq!(compute_horizontal_scrollbar_metrics(f32::NAN, 400.0, 0.0), None);
        assert_eq!(compute_horizontal_scrollbar_metrics(400.0, f32::INFINITY, 0.0), None);

        // Narrow viewport (< 28.0px) clamps thumb width to viewport_width
        let narrow = compute_horizontal_scrollbar_metrics(20.0, 200.0, 0.0).unwrap();
        assert_eq!(narrow.thumb_width, 20.0);
        assert_eq!(narrow.thumb_left, 0.0);

        // Minimum thumb width of 28.0px on huge content
        let huge = compute_horizontal_scrollbar_metrics(300.0, 50_000.0, -999_999.0).unwrap();
        assert_eq!(huge.thumb_width, H_SCROLLBAR_MIN_THUMB_WIDTH_PX);
        assert_eq!(huge.max_scroll_x, 49_700.0);
        assert!((huge.thumb_left - (300.0 - 28.0)).abs() < 1e-4);

        // Proportional thumb and over-scroll clamping
        let mid = compute_horizontal_scrollbar_metrics(400.0, 800.0, -200.0).unwrap();
        assert_eq!(
            mid,
            HorizontalScrollbarMetrics {
                thumb_left: 100.0,
                thumb_width: 200.0,
                max_scroll_x: 400.0,
            }
        );

        // Mouse X -> scroll X: invalid / degenerate cases
        assert_eq!(scroll_x_from_mouse_x(150.0, 0.0, 200.0, 200.0, None, 100.0), 0.0);
        assert_eq!(scroll_x_from_mouse_x(150.0, 0.0, 20.0, 28.0, None, 100.0), 0.0);
        assert_eq!(scroll_x_from_mouse_x(150.0, 0.0, 400.0, 200.0, None, 0.0), 0.0);
        assert_eq!(scroll_x_from_mouse_x(f32::NAN, 0.0, 400.0, 200.0, None, 400.0), 0.0);
        assert_eq!(scroll_x_from_mouse_x(f32::INFINITY, 0.0, 400.0, 200.0, None, 400.0), 0.0);
        assert_eq!(scroll_x_from_mouse_x(150.0, 0.0, 400.0, 200.0, None, f32::NEG_INFINITY), 0.0);

        // Track click (centered): track spans [100, 500] -> center is 300 -> thumb_left 100 -> -200px
        assert!((scroll_x_from_mouse_x(300.0, 100.0, 400.0, 200.0, None, 400.0) - (-200.0)).abs() < 1e-4);
        assert_eq!(
            scroll_x_from_mouse_x(200.0, 100.0, 400.0, 200.0, None, 400.0).to_bits(),
            0.0f32.to_bits()
        );
        // Out-of-bounds mouse X clamps cleanly to [0.0, -max_scroll_x]
        assert_eq!(
            scroll_x_from_mouse_x(0.0, 100.0, 400.0, 200.0, None, 400.0).to_bits(),
            0.0f32.to_bits()
        );
        assert_eq!(scroll_x_from_mouse_x(900.0, 100.0, 400.0, 200.0, None, 400.0), -400.0);
        // Clicking at exact grab offset inside thumb keeps current scroll offset unchanged
        let grab_offset = 40.0;
        let click_x = 100.0 + mid.thumb_left + grab_offset;
        assert!(
            (scroll_x_from_mouse_x(click_x, 100.0, 400.0, mid.thumb_width, Some(grab_offset), mid.max_scroll_x)
                - (-200.0))
                .abs()
                < 1e-4
        );

        // Snapshot hysteresis check (0.5px threshold)
        let base = HScrollRenderSnapshot {
            has_h_scrollbar: true,
            content_width: 800.0,
            viewport_width: 400.0,
            clamped_scroll_x: -50.0,
        };
        let within_eps = HScrollRenderSnapshot {
            content_width: 800.3,
            viewport_width: 400.4,
            clamped_scroll_x: -50.2,
            ..base
        };
        assert!(!h_scroll_snapshots_differ(base, within_eps));
        let at_exact_eps = HScrollRenderSnapshot {
            content_width: 800.5,
            ..base
        };
        assert!(!h_scroll_snapshots_differ(base, at_exact_eps));
        let beyond_eps = HScrollRenderSnapshot {
            content_width: 800.6,
            ..base
        };
        assert!(h_scroll_snapshots_differ(base, beyond_eps));
        assert!(h_scroll_snapshots_differ(
            base,
            HScrollRenderSnapshot {
                viewport_width: 400.6,
                ..base
            }
        ));
        assert!(h_scroll_snapshots_differ(
            base,
            HScrollRenderSnapshot {
                clamped_scroll_x: -50.6,
                ..base
            }
        ));
        let toggled = HScrollRenderSnapshot {
            has_h_scrollbar: false,
            ..base
        };
        assert!(h_scroll_snapshots_differ(base, toggled));

        // read_h_scroll_render_snapshot normalizes when full_line_display is false
        let handle = UniformListScrollHandle::new();
        handle.0.borrow().base_handle.set_offset(point(px(-75.0), px(-30.0)));
        let disabled_snap = read_h_scroll_render_snapshot(&handle, false);
        assert!(!disabled_snap.has_h_scrollbar);
        assert_eq!(disabled_snap.clamped_scroll_x, 0.0);
        let enabled_snap = read_h_scroll_render_snapshot(&handle, true);
        assert!(!enabled_snap.has_h_scrollbar);
        assert_eq!(enabled_snap.clamped_scroll_x, 0.0);
    }

    #[test]
    fn test_format_line_detail_text_and_json_detection() {
        // 1. Plain text preserves body and strips trailing \r\n
        let plain = format_line_detail_text("11-06 18:00:00.000 I Tag: hello\r\n", true, 4096);
        assert_eq!(plain.display_text, "11-06 18:00:00.000 I Tag: hello");
        assert!(!plain.has_json);
        assert!(!plain.is_truncated_for_render);

        // 2. False positives must be rejected: [1234], [], {}, [1]
        assert!(!is_meaningful_json_value(&serde_json::json!({})));
        assert!(!is_meaningful_json_value(&serde_json::json!([])));
        assert!(!is_meaningful_json_value(&serde_json::json!([1234])));
        assert!(is_meaningful_json_value(&serde_json::json!([1, 2])));
        assert!(is_meaningful_json_value(&serde_json::json!([{"k": 1}])));
        assert!(is_meaningful_json_value(&serde_json::json!({"k": 1})));

        for fp in [
            "Process killed [1234]",
            "Pending queue: []",
            "Empty payload {}",
        ] {
            let res = format_line_detail_text(fp, true, 4096);
            assert!(!res.has_json, "Must reject false positive JSON in '{fp}'");
            assert_eq!(res.display_text, fp);
        }

        // 3. Pure JSON object & array formatting
        let pure_obj = format_line_detail_text("{\"b\":2,\"a\":1}\n", true, 4096);
        assert!(pure_obj.has_json);
        assert!(pure_obj.display_text.contains('\n'));
        assert!(pure_obj.display_text.contains("\"a\": 1"));

        let pure_raw = format_line_detail_text("{\"b\":2,\"a\":1}\n", false, 4096);
        assert!(pure_raw.has_json);
        assert_eq!(pure_raw.display_text, "{\"b\":2,\"a\":1}");

        let pure_arr = format_line_detail_text("[{\"id\":1},{\"id\":2}]\r\n", true, 4096);
        assert!(pure_arr.has_json);
        assert!(pure_arr.display_text.starts_with("[\n"));
        assert!(pure_arr.display_text.contains("\"id\": 1"));
        assert!(pure_arr.display_text.contains("\"id\": 2"));

        let pure_arr_raw = format_line_detail_text("[10,20,30]\n", false, 4096);
        assert!(pure_arr_raw.has_json);
        assert_eq!(pure_arr_raw.display_text, "[10,20,30]");

        // 4. Logcat line with >= 8 bracketed tags before JSON + trailing " (12ms)" suffix
        let complex = "11-06 18:00:00.123 100 100 I Net: [1] [2] [3] [4] [5] [6] [7] [8] [9] payload={\"status\":200,\"ok\":true} (12ms)\n";
        let complex_pretty = format_line_detail_text(complex, true, 4096);
        assert!(complex_pretty.has_json);
        assert!(complex_pretty.display_text.starts_with(
            "11-06 18:00:00.123 100 100 I Net: [1] [2] [3] [4] [5] [6] [7] [8] [9] payload=\n{"
        ));
        assert!(complex_pretty.display_text.ends_with("\n(12ms)"));

        // 5. Inputs <= 256 KiB parse JSON; inputs > 256 KiB skip JSON parsing
        let prefix = "{\"k\":\"";
        let suffix = "\"}";
        let exact_limit_payload = format!(
            "{prefix}{}{suffix}",
            "x".repeat(LINE_DETAIL_JSON_PARSE_MAX_BYTES - prefix.len() - suffix.len())
        );
        assert_eq!(exact_limit_payload.len(), LINE_DETAIL_JSON_PARSE_MAX_BYTES);
        let at_limit = format_line_detail_text(&exact_limit_payload, true, 4096);
        assert!(at_limit.has_json);
        assert!(at_limit.is_truncated_for_render);

        let huge_json = format!("{{\"k\":\"{}\"}}", "x".repeat(LINE_DETAIL_JSON_PARSE_MAX_BYTES));
        assert!(huge_json.len() > LINE_DETAIL_JSON_PARSE_MAX_BYTES);
        let skipped = format_line_detail_text(&huge_json, true, 4096);
        assert!(!skipped.has_json);
        assert!(skipped.is_truncated_for_render);
        assert!(skipped.display_text.len() <= 4096);
        assert!(skipped.display_text.ends_with(LINE_DETAIL_TRUNCATION_SUFFIX));

        // 6. 500+ char spaceless token & multibyte Korean UTF-8 truncation boundary safety
        let token = "A".repeat(600);
        let token_res = format_line_detail_text(&token, true, 256);
        assert!(token_res.is_truncated_for_render);
        assert!(token_res.display_text.len() <= 256);
        assert!(token_res.display_text.ends_with(LINE_DETAIL_TRUNCATION_SUFFIX));

        let korean = "가나다라마바사아자차카타파하".repeat(20); // 3 bytes per char
        for max_bytes in [
            0,
            1,
            2,
            5,
            LINE_DETAIL_TRUNCATION_SUFFIX.len(),
            LINE_DETAIL_TRUNCATION_SUFFIX.len() + 1,
            LINE_DETAIL_TRUNCATION_SUFFIX.len() + 2,
            LINE_DETAIL_TRUNCATION_SUFFIX.len() + 4,
            128,
        ] {
            let res = format_line_detail_text(&korean, true, max_bytes);
            assert!(res.is_truncated_for_render);
            assert!(
                res.display_text.len() <= max_bytes,
                "display_text len {} exceeded max_bytes {}",
                res.display_text.len(),
                max_bytes
            );
        }
    }

    #[test]
    fn test_log_row_height_and_compute_highlight_ranges() {
        assert_eq!(LOG_ROW_HEIGHT_MULTIPLIER, 1.55);
        assert!((log_row_height_px(12.0) - 18.6).abs() < 1e-5);

        let line = "2026-11-06 12:00:00 Normal log line";
        assert!(compute_highlight_ranges(line, &None).is_empty());

        let hit_matcher = Arc::new(CompiledMatcher::new("Normal", false, false).unwrap());
        assert_eq!(compute_highlight_ranges(line, &Some(hit_matcher)), vec![20..26]);

        let miss_matcher =
            Arc::new(CompiledMatcher::new("NonExistentPattern", false, false).unwrap());
        assert!(compute_highlight_ranges(line, &Some(miss_matcher)).is_empty());
    }

    #[test]
    fn test_build_heatmap_ticks_coalesces_runs_and_skips_normal() {
        assert!(build_heatmap_ticks(None, 200.0).is_empty());

        let heatmap = HeatmapData {
            buckets: vec![
                HeatmapBucket {
                    error_count: 1,
                    ..Default::default()
                },
                HeatmapBucket {
                    error_count: 2,
                    ..Default::default()
                },
                HeatmapBucket::default(),
                HeatmapBucket {
                    warn_count: 1,
                    ..Default::default()
                },
            ],
            total_lines: 4,
            ..Default::default()
        };

        assert!(build_heatmap_ticks(Some(&heatmap), 0.0).is_empty());
        assert_eq!(build_heatmap_ticks(Some(&heatmap), 400.0).len(), 2);
    }
}

use gpui::prelude::FluentBuilder;
use gpui::*;
use loganalysis_core::LogSource;

use crate::workspace::LogAnalysisWorkspace;

pub fn render_status_bar(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let row_count = workspace
        .filter_result
        .as_ref()
        .map(|r| r.visible_indices.len())
        .unwrap_or(0);
    let total_lines = workspace
        .log_source
        .as_ref()
        .map(|s| s.line_count())
        .unwrap_or(0);
    let total_matched: usize = workspace
        .filter_result
        .as_ref()
        .map(|r| r.total_matched_lines)
        .unwrap_or(0);
    let (total_errors, total_warnings) = workspace
        .heatmap_data
        .as_ref()
        .map(|h| (h.total_errors, h.total_warnings))
        .unwrap_or((0, 0));
    let selected_dt = workspace.get_selected_delta_time();
    let font_size = workspace.font_size;
    let font_size_f32: f32 = font_size.into();
    let selected_real_line = workspace.selection.anchor_source_idx;
    let bookmark_count = workspace.bookmarks.len();

    let visible_pct = if total_lines > 0 {
        (row_count as f64 / total_lines as f64) * 100.0
    } else {
        0.0
    };

    div()
        .h(px(28.0))
        .w_full()
        .bg(theme.bg_header)
        .border_t_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .justify_between()
        .px(px(12.0))
        // Left: Live Status Indicator Dot + Status Message
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
                        .bg(if workspace.is_streaming {
                            if workspace.is_stream_paused {
                                theme.warning_amber
                            } else {
                                theme.success_green
                            }
                        } else {
                            theme.accent
                        }),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .child(workspace.status_text.clone()),
                ),
        )
        // Right: Telemetry Metrics & Interactive Font Zoom
        .child(
            div()
                .flex()
                .items_center()
                .gap_4()
                .font_family("monospace")
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .child(format!(
                    "Visible: {} / {} ({:.1}%)",
                    row_count, total_lines, visible_pct
                ))
                .child(format!("Matched: {}", total_matched))
                .when(total_errors > 0 || total_warnings > 0, |parent| {
                    parent.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_1p5()
                            .py_0p5()
                            .rounded_xs()
                            .bg(theme.bg_card)
                            .border_1()
                            .border_color(theme.border)
                            .when(total_errors > 0, |s| {
                                s.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .text_color(rgb(0xef4444))
                                        .child(
                                            div()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .rounded_full()
                                                .bg(rgb(0xef4444)),
                                        )
                                        .child(format!("{} err", total_errors)),
                                )
                            })
                            .when(total_warnings > 0, |s| {
                                s.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .text_color(rgb(0xf59e0b))
                                        .child(
                                            div()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .rounded_full()
                                                .bg(rgb(0xf59e0b)),
                                        )
                                        .child(format!("{} warn", total_warnings)),
                                )
                            }),
                    )
                })
                .when(bookmark_count > 0, |parent| {
                    parent.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_1p5()
                            .py_0p5()
                            .rounded_xs()
                            .bg(theme.accent_muted)
                            .border_1()
                            .border_color(theme.accent)
                            .text_color(theme.accent)
                            .child("🔖")
                            .child(format!("{} bm", bookmark_count)),
                    )
                })
                .child(if let Some(line_idx) = selected_real_line {
                    format!("Line: {}", line_idx + 1)
                } else {
                    "Line: -".to_string()
                })
                .when(selected_dt.is_some(), |parent| {
                    let (dt_str, latency, _) = selected_dt.as_ref().unwrap();
                    let (badge_bg, badge_fg, icon) = match latency {
                        loganalysis_core::LatencyLevel::Normal => {
                            (theme.bg_card, theme.text_muted, "⏱️")
                        }
                        loganalysis_core::LatencyLevel::Warning => {
                            (rgba(0xf59e0b25), rgb(0xf59e0b), "▲")
                        }
                        loganalysis_core::LatencyLevel::Critical => {
                            (rgba(0xef444425), rgb(0xef4444), "⚠️")
                        }
                    };
                    parent.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_1p5()
                            .py_0p5()
                            .rounded_xs()
                            .bg(badge_bg)
                            .border_1()
                            .border_color(theme.border)
                            .text_color(badge_fg)
                            .child(icon)
                            .child(format!("Δt: {dt_str}")),
                    )
                })
                // Interactive Zoom Controls
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .id("btn-status-zoom-out")
                                .px(px(5.0))
                                .rounded_sm()
                                .bg(theme.bg_card)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.zoom(-1.0, cx);
                                }))
                                .child("-"),
                        )
                        .child(div().px(px(4.0)).child(format!("{:.0}px", font_size_f32)))
                        .child(
                            div()
                                .id("btn-status-zoom-in")
                                .px(px(5.0))
                                .rounded_sm()
                                .bg(theme.bg_card)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.zoom(1.0, cx);
                                }))
                                .child("+"),
                        ),
                )
                // App Version Badge
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_xs()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border)
                        .text_size(px(10.0))
                        .text_color(theme.text_muted)
                        .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                ),
        )
}

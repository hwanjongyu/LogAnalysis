use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::workspace::LogAnalysisWorkspace;

/// Renders the Throughput & EPS (Events Per Second) Timeline Sparkline ribbon below the toolbar.
pub fn render_timeline_sparkline(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    if !workspace.is_timeline_visible {
        return None;
    }

    let theme = workspace.theme;
    let timeline_opt = workspace.timeline_data.as_deref();

    let has_timestamps = timeline_opt.map(|d| d.has_timestamps).unwrap_or(false);
    let peak_eps = timeline_opt.map(|d| d.peak_eps).unwrap_or(0.0);
    let avg_eps = timeline_opt.map(|d| d.avg_eps).unwrap_or(0.0);
    let peak_count = timeline_opt.map(|d| d.peak_count).unwrap_or(0).max(1);
    let has_anomaly = timeline_opt
        .map(|d| d.buckets.iter().any(|b| b.is_anomaly))
        .unwrap_or(false);

    let bars_el = if let Some(data) = timeline_opt {
        if !data.buckets.is_empty() && data.has_timestamps {
            div()
                .id("timeline-bars-track")
                .flex_1()
                .h_full()
                .flex()
                .items_end()
                .gap(px(1.5))
                .px(px(4.0))
                .pb(px(4.0))
                .children(data.buckets.iter().enumerate().map(|(idx, bucket)| {
                    let height_px = (bucket.total_count as f32 / peak_count as f32 * 22.0).max(2.0);
                    let (bar_bg, bar_border) = if bucket.is_anomaly || bucket.error_count > 0 {
                        (rgb(0xef4444), rgb(0xdc2626))
                    } else if bucket.warn_count > 0 {
                        (rgb(0xf59e0b), rgb(0xd97706))
                    } else if bucket.total_count > 0 {
                        (theme.accent, theme.accent)
                    } else {
                        (theme.border_subtle, theme.border_subtle)
                    };

                    let target_row = bucket.start_visible_row;

                    div()
                        .id(("timeline-bucket-bar", idx))
                        .cursor_pointer()
                        .flex_1()
                        .h(px(height_px))
                        .bg(bar_bg)
                        .border_t_1()
                        .border_color(bar_border)
                        .rounded_t_xs()
                        .hover(|s| s.opacity(0.8))
                        .on_click(cx.listener(move |this, _, _window, cx| {
                            if let Some(res) = &this.filter_result {
                                if let Some(&src_idx) = res.visible_indices.get(target_row) {
                                    this.selection.select(target_row, src_idx);
                                    this.scroll_handle
                                        .scroll_to_item(target_row, ScrollStrategy::Center);
                                    this.refresh_status_line(cx);
                                    this.status_text =
                                        format!("Navigated to timeline event at row {}", target_row + 1)
                                            .into();
                                    cx.notify();
                                }
                            }
                        }))
                }))
                .into_any_element()
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .child("No timestamps detected in loaded log lines to build temporal sparkline")
                .into_any_element()
        }
    } else {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(11.0))
            .text_color(theme.text_muted)
            .child("Calculating throughput profile...")
            .into_any_element()
    };

    Some(
        div()
            .id("timeline-sparkline-ribbon")
            .h(px(36.0))
            .w_full()
            .bg(theme.bg_card)
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.0))
            // Left: Metric Labels
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_xs()
                            .bg(theme.accent_muted)
                            .font_family("monospace")
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(10.0))
                            .text_color(theme.accent)
                            .child("EPS"),
                    )
                    .when(has_timestamps, |parent| {
                        parent
                            .child(
                                div()
                                    .font_family("monospace")
                                    .text_size(px(11.0))
                                    .text_color(theme.text_primary)
                                    .child(format!("Peak: {:.0} • Avg: {:.0}", peak_eps, avg_eps)),
                            )
                            .when(has_anomaly, |s| {
                                s.child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_xs()
                                        .bg(rgba(0xef444422))
                                        .border_1()
                                        .border_color(rgb(0xef4444))
                                        .text_size(px(10.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(0xef4444))
                                        .child("⚠ SPIKE (>3σ)"),
                                )
                            })
                    }),
            )
            // Center: Interactive Sparkline Bar Track
            .child(bars_el)
            // Right: Close Button
            .child(
                div()
                    .id("btn-close-timeline-ribbon")
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .rounded_xs()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .hover(|s| s.bg(theme.row_hover).text_color(theme.text_primary))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.toggle_eps_timeline(cx);
                    }))
                    .child("✕"),
            ),
    )
}

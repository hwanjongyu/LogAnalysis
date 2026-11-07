use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::components::tooltip::render_tooltip;
use crate::theme::Theme;
use crate::workspace::LogAnalysisWorkspace;

/// Renders a single interactive forensic tag chip with 1-click Include (+) and Exclude (✕)
fn render_facet_chip(
    tag: String,
    count: usize,
    theme: Theme,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let tag_for_include = tag.clone();
    let tag_for_exclude = tag.clone();
    let tag_tooltip = tag.clone();
    let chip_id: SharedString = format!("facet-chip-{}", tag).into();
    let inc_id: SharedString = format!("facet-inc-{}", tag).into();
    let exc_id: SharedString = format!("facet-exc-{}", tag).into();

    div()
        .id(ElementId::Name(chip_id))
        .flex()
        .items_center()
        .h(px(20.0))
        .rounded_xs()
        .bg(theme.bg_card)
        .border_1()
        .border_color(theme.border_subtle)
        .hover(|s| s.border_color(theme.accent).bg(theme.btn_hover))
        .tooltip(render_tooltip(
            &format!("Include '{tag_tooltip}' (+), or click ✕ to exclude"),
            None,
            theme,
        ))
        // Main Tag Label + Count (Click to INCLUDE)
        .child(
            div()
                .id(ElementId::Name(inc_id))
                .cursor_pointer()
                .px(px(6.0))
                .h_full()
                .flex()
                .items_center()
                .gap(px(3.0))
                .text_size(px(10.0))
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.add_facet_filter(&tag_for_include, false, cx);
                }))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(tag),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(theme.text_muted)
                        .child(format!("({})", count)),
                ),
        )
        // Mini Exclude Button (Click to EXCLUDE)
        .child(
            div()
                .id(ElementId::Name(exc_id))
                .cursor_pointer()
                .px(px(4.0))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .border_l_1()
                .border_color(theme.border_subtle)
                .hover(|s| s.bg(theme.error_red).text_color(rgb(0xffffff)))
                .text_size(px(9.0))
                .text_color(theme.text_muted)
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.add_facet_filter(&tag_for_exclude, true, cx);
                }))
                .child("✕"),
        )
}

/// Renders the Forensic Faceted Quick Filter bar above the log viewport
pub fn render_facets_bar(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    if !workspace.is_facets_visible || workspace.log_source.is_none() {
        return None;
    }

    let theme = workspace.theme;
    let facets = workspace.facet_tags.clone();

    Some(
        div()
            .id("forensic-facets-bar")
            .w_full()
            .h(px(28.0))
            .bg(theme.bg_header)
            .border_b_1()
            .border_color(theme.border_subtle)
            .flex()
            .items_center()
            .justify_between()
            .px(px(10.0))
            // Left: Icon + Label + Chips
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_size(px(10.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text_muted)
                            .child("🏷️")
                            .child("Top Facets:"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .overflow_hidden()
                            .when(facets.is_empty(), |parent| {
                                parent.child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(theme.text_muted)
                                        .child("No common Android tags identified in sample window"),
                                )
                            })
                            .children(
                                facets
                                    .into_iter()
                                    .map(|(tag, count)| render_facet_chip(tag, count, theme, cx)),
                            ),
                    ),
            )
            // Right: Refresh & Dismiss Actions
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    // Rescan button
                    .child(
                        div()
                            .id("btn-facets-rescan")
                            .cursor_pointer()
                            .px(px(5.0))
                            .py(px(1.5))
                            .rounded_xs()
                            .bg(theme.bg_card)
                            .border_1()
                            .border_color(theme.border_subtle)
                            .hover(|s| s.bg(theme.btn_hover))
                            .tooltip(render_tooltip("Rescan top tags from buffer", None, theme))
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.refresh_facet_tags();
                                cx.notify();
                            }))
                            .child("↻ Rescan"),
                    )
                    // Close button
                    .child(
                        div()
                            .id("btn-facets-close")
                            .cursor_pointer()
                            .px(px(5.0))
                            .py(px(1.5))
                            .rounded_xs()
                            .hover(|s| s.bg(theme.btn_hover).text_color(theme.text_primary))
                            .tooltip(render_tooltip(
                                "Hide forensic facets bar",
                                Some("Alt+F"),
                                theme,
                            ))
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.toggle_facets_bar(cx);
                            }))
                            .child("✕"),
                    ),
            ),
    )
}

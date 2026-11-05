use gpui::*;

use crate::workspace::LogAnalysisWorkspace;

pub fn render_find_bar(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    if !workspace.is_search_visible {
        return None;
    }

    let theme = workspace.theme;

    Some(
        div()
            .h(px(38.0))
            .w_full()
            .bg(theme.bg_header)
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .justify_between()
            .px(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_muted)
                            .child("FIND"),
                    )
                    // Native TextInput Field with inline clear button
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .id("input-find-bar")
                                    .w(px(280.0))
                                    .h(px(26.0))
                                    .child(workspace.find_input.clone()),
                            )
                            .children(if !workspace.search_query.is_empty() {
                                Some(
                                    div()
                                        .id("btn-clear-find-query")
                                        .px(px(5.0))
                                        .py(px(2.0))
                                        .rounded_xs()
                                        .cursor_pointer()
                                        .hover(move |s| s.bg(theme.btn_hover))
                                        .text_size(px(10.5))
                                        .text_color(theme.text_muted)
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            this.find_input.update(cx, |inp, cx| inp.set_text("", cx));
                                            this.search_query.clear();
                                            this.search_match_status = None;
                                            this.update_cached_search_matcher();
                                            cx.notify();
                                        }))
                                        .child("✕"),
                                )
                            } else {
                                None
                            }),
                    )
                    // Live Search Match Status Pill
                    .children(if let Some(status) = &workspace.search_match_status {
                        let is_zero = status.contains('0') || status.contains("not found");
                        Some(
                            div()
                                .id("pill-find-status")
                                .px(px(7.0))
                                .py(px(2.5))
                                .rounded_sm()
                                .bg(if is_zero {
                                    theme.bg_card
                                } else {
                                    theme.accent_muted
                                })
                                .border_1()
                                .border_color(if is_zero {
                                    theme.error_red
                                } else {
                                    theme.accent
                                })
                                .text_color(if is_zero {
                                    theme.error_red
                                } else {
                                    theme.accent
                                })
                                .text_size(px(10.5))
                                .font_family("monospace")
                                .font_weight(FontWeight::MEDIUM)
                                .child(status.clone()),
                        )
                    } else {
                        None
                    })
                    // Regex & Case Toggles
                    .child(
                        div()
                            .id("btn-find-regex")
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded_sm()
                            .bg(if workspace.is_search_regex {
                                theme.accent
                            } else {
                                theme.bg_card
                            })
                            .hover(move |s| {
                                if !workspace.is_search_regex {
                                    s.bg(theme.btn_hover)
                                } else {
                                    s
                                }
                            })
                            .text_color(if workspace.is_search_regex {
                                rgb(0xffffff)
                            } else {
                                theme.text_muted
                            })
                            .text_size(px(10.0))
                            .font_family("monospace")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.toggle_search_regex(cx);
                            }))
                            .child(".* Regex"),
                    )
                    .child(
                        div()
                            .id("btn-find-case")
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded_sm()
                            .bg(if workspace.is_search_case_sensitive {
                                theme.accent
                            } else {
                                theme.bg_card
                            })
                            .hover(move |s| {
                                if !workspace.is_search_case_sensitive {
                                    s.bg(theme.btn_hover)
                                } else {
                                    s
                                }
                            })
                            .text_color(if workspace.is_search_case_sensitive {
                                rgb(0xffffff)
                            } else {
                                theme.text_muted
                            })
                            .text_size(px(10.0))
                            .font_family("monospace")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.toggle_search_case_sensitive(cx);
                            }))
                            .child("Aa Case"),
                    ),
            )
            // Prev / Next / Close Controls
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id("btn-find-prev")
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded_sm()
                            .bg(theme.bg_card)
                            .hover(move |s| s.bg(theme.btn_hover))
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.search(false, cx);
                            }))
                            .child("↑ Prev (Shift+F3)"),
                    )
                    .child(
                        div()
                            .id("btn-find-next")
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded_sm()
                            .bg(theme.accent)
                            .hover(move |s| s.bg(theme.accent_muted).text_color(theme.accent))
                            .text_color(rgb(0xffffff))
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.search(true, cx);
                            }))
                            .child("↓ Next (F3)"),
                    )
                    .child(
                        div()
                            .id("btn-find-close")
                            .px(px(6.0))
                            .py(px(3.0))
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(move |s| s.bg(theme.btn_hover))
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.toggle_search_bar(cx);
                            }))
                            .child("✕ (Esc)"),
                    ),
            ),
    )
}

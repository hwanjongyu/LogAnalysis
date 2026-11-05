use gpui::*;
use loganalysis_core::LogSource;

use crate::components::find_bar::render_find_bar;
use crate::workspace::LogAnalysisWorkspace;

pub fn render_log_viewport(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let row_count = workspace
        .filter_result
        .as_ref()
        .map(|r| r.visible_indices.len())
        .unwrap_or(0);
    let font_size = workspace.font_size;
    let row_height = font_size * 1.55;

    let has_log_source = workspace.log_source.is_some();
    let weak_self = cx.weak_entity();

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
        // Virtualized Log Rows or Polished Empty State
        .child(if has_log_source {
            let weak_handle = weak_self.clone();
            let mut list = uniform_list("log-view", row_count, move |range, _window, cx| {
                let ws_opt = weak_handle.upgrade();
                let ws = ws_opt.as_ref().map(|entity| entity.read(cx));
                let filter_res = ws.and_then(|w| w.filter_result.as_deref());
                let source = ws.and_then(|w| w.log_source.as_deref());
                let selected = ws.and_then(|w| w.selection.selected_row);
                let search_matcher = ws.and_then(|w| w.cached_search_matcher.clone());
                let row_theme = ws.map(|w| w.theme).unwrap_or(theme);

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

                        div()
                            .id(ElementId::NamedInteger("log-row".into(), idx as u64))
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
                                    this.select_log_row(idx, real_idx, cx);
                                });
                            })
                            // Line Number Gutter (Fixed 64px + Subtle Right Border)
                            .child(
                                div()
                                    .w(px(64.0))
                                    .h_full()
                                    .flex_shrink_0()
                                    .bg(if is_selected {
                                        theme.accent_muted
                                    } else {
                                        theme.bg_gutter
                                    })
                                    .border_r_1()
                                    .border_color(theme.border_subtle)
                                    .text_color(if is_selected {
                                        theme.accent
                                    } else {
                                        theme.text_gutter
                                    })
                                    .font_weight(if is_selected {
                                        FontWeight::BOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .pr(px(8.0))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(format!("{}", real_idx + 1)),
                            )
                            // Monospace Log Content
                            .child(
                                div()
                                    .flex_1()
                                    .px(px(10.0))
                                    .whitespace_nowrap()
                                    .child(if let Some(ref matcher) = search_matcher {
                                        let ranges = matcher.find_match_ranges(&text, 32);
                                        if !ranges.is_empty() {
                                            let highlights = ranges.into_iter().map(|r| (r, highlight_style));
                                            StyledText::new(text).with_highlights(highlights).into_any_element()
                                        } else {
                                            text.into_any_element()
                                        }
                                    } else {
                                        text.into_any_element()
                                    }),
                            )
                    })
                    .collect::<Vec<_>>()
            });

            if workspace.full_line_display {
                list =
                    list.with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained);
            }

            let list_el = list
                .track_scroll(workspace.scroll_handle.clone())
                .flex_1();

            // Compute scrollbar thumb position
            let (thumb_top, thumb_height) = {
                let scroll_state = workspace.scroll_handle.0.borrow();
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
                    (top, h)
                } else {
                    (0.0, track_height)
                }
            };

            let is_dragging = workspace.is_dragging_scrollbar;
            let scrollbar_thumb_color = if is_dragging {
                theme.accent
            } else if workspace.is_dark_theme {
                rgba(0xffffff40)
            } else {
                rgba(0x00000040)
            };

            let scrollbar_track = div()
                .id("log-scrollbar-track")
                .w(px(14.0))
                .h_full()
                .bg(theme.bg_gutter)
                .border_l_1()
                .border_color(theme.border_subtle)
                .cursor_pointer()
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
                )
                .child(
                    div()
                        .id("log-scrollbar-thumb")
                        .absolute()
                        .left(px(2.0))
                        .right(px(2.0))
                        .top(px(thumb_top))
                        .h(px(thumb_height))
                        .rounded_full()
                        .bg(scrollbar_thumb_color)
                        .hover(move |s| s.bg(theme.accent)),
                );

            div()
                .flex_1()
                .h_full()
                .flex()
                .flex_row()
                .overflow_hidden()
                .child(list_el)
                .child(scrollbar_track)
                .into_any_element()
        } else {
            // Modern Onboarding Empty State Card
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(460.0))
                        .p(px(28.0))
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
                                .text_size(px(18.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.text_primary)
                                .child("LogAnalysis Studio"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.text_muted)
                                .child(
                                    "Drop any log or filter JSON file here, or start a live ADB logcat stream.",
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .pt(px(4.0))
                                .child(
                                    div()
                                        .id("btn-empty-state-open")
                                        .px(px(16.0))
                                        .py(px(8.0))
                                        .rounded_md()
                                        .bg(theme.accent)
                                        .text_color(rgb(0xffffff))
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.prompt_open_file(Some(window), cx);
                                        }))
                                        .child("📂 Open Log File (Ctrl+O)"),
                                )
                                .child(
                                    div()
                                        .id("btn-empty-state-adb")
                                        .px(px(16.0))
                                        .py(px(8.0))
                                        .rounded_md()
                                        .bg(theme.bg_card)
                                        .border_1()
                                        .border_color(theme.border)
                                        .hover(move |s| s.bg(theme.btn_hover))
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            this.start_adb_stream(cx);
                                        }))
                                        .child("⚡ Start ADB Stream"),
                                ),
                        )
                        // Shortcut Cheat Sheet
                        .child(
                            div()
                                .w_full()
                                .mt(px(8.0))
                                .pt(px(12.0))
                                .border_t_1()
                                .border_color(theme.border_subtle)
                                .flex()
                                .flex_col()
                                .gap_1p5()
                                .text_size(px(11.0))
                                .font_family("monospace")
                                .text_color(theme.text_muted)
                                .child("Ctrl+F       Search in logs (F3 / Shift+F3)")
                                .child("Ctrl+Shift+F Add new highlight/exclude rule")
                                .child("Ctrl+H       Toggle Filtered Only view")
                                .child("Ctrl+Shift+L Toggle Filter Panel (Left / Bottom)")
                                .child("Ctrl+T / D   New / Delete filter tab"),
                        ),
                )
                .into_any_element()
        })
}

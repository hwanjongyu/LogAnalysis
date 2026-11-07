use gpui::*;
use loganalysis_core::{LineSeverity, LogSource};

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

                        let is_bookmarked =
                            ws.map(|w| w.bookmarks.is_bookmarked(real_idx)).unwrap_or(false);

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
                            // Line Number Gutter (Fixed 64px + Subtle Right Border + Active Left Accent Marker)
                            .child(
                                div()
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
                                    .child(div().child(format!("{}", real_idx + 1))),
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

            // Compute scrollbar thumb position and track dimensions
            let (thumb_top, thumb_height, track_height) = {
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
                    (top, h, track_height)
                } else {
                    (0.0, track_height, track_height)
                }
            };

            let heatmap_ticks = if let Some(heatmap) = &workspace.heatmap_data {
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
            };

            let is_dragging = workspace.is_dragging_scrollbar;
            let scrollbar_thumb_color = if is_dragging {
                theme.accent
            } else if workspace.is_dark_theme {
                rgba(0xffffff38)
            } else {
                rgba(0x00000038)
            };

            let scrollbar_track = div()
                .id("log-scrollbar-track")
                .relative()
                .w(px(15.0))
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
                .children(heatmap_ticks)
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
            // Modern Dual-Pillar Onboarding Layout (Local File vs Live Android ADB)
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
                                .child("Ctrl+F: Search  •  Ctrl+Shift+F: Add Filter  •  Ctrl+H: Filtered Only  •  Ctrl+Space: Pause Stream"),
                        ),
                )
                .into_any_element()
        })
}

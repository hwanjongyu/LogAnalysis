use gpui::*;
use loganalysis_core::{evaluate_contrast, parse_color_spec, ContrastLevel};

use crate::types::InputFocusTarget;
use crate::workspace::LogAnalysisWorkspace;

pub fn render_filter_edit_modal(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    if !workspace.filter_edit_modal.is_open {
        return None;
    }

    let theme = workspace.theme;
    let is_edit = workspace.filter_edit_modal.editing_index.is_some();
    let modal_pattern = workspace.filter_edit_modal.pattern.clone();
    let is_reg = workspace.filter_edit_modal.is_regex;
    let is_cs = workspace.filter_edit_modal.is_case_sensitive;
    let is_ex = workspace.filter_edit_modal.is_exclude;
    let bg_name = workspace.filter_edit_modal.bg_color_name.clone();
    let fg_name = workspace.filter_edit_modal.fg_color_name.clone();
    let is_modal_focused = workspace.input_focus == InputFocusTarget::ModalPattern;

    // WCAG 2.1 Contrast Calculation
    let bg_hex = parse_color_spec(&bg_name, true).unwrap_or(0xFFFFFF);
    let fg_hex = parse_color_spec(&fg_name, false).unwrap_or(0x000000);
    let (ratio, contrast_level) = evaluate_contrast(bg_hex, fg_hex);

    Some(
        div()
            .id("modal-overlay-filter-edit")
            .absolute()
            .inset_0()
            .bg(rgba(0x000000aa))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("modal-card-filter-edit")
                    .w(px(500.0))
                    .bg(theme.bg_panel)
                    .border_1()
                    .border_color(theme.accent)
                    .rounded_xl()
                    .shadow_lg()
                    .p(px(20.0))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                        this.handle_key_down(event, cx);
                    }))
                    // Modal Header
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(15.0))
                                    .child(if is_edit {
                                        "Edit Filter Rule"
                                    } else {
                                        "New Filter Rule"
                                    }),
                            )
                            .child(
                                div()
                                    .id("btn-modal-close-x")
                                    .cursor_pointer()
                                    .text_size(px(14.0))
                                    .text_color(theme.text_muted)
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.close_filter_modal(cx);
                                    }))
                                    .child("✕"),
                            ),
                    )
                    // Interactive Pattern Input Field
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_muted)
                                    .child("Pattern (type directly or pick a preset below):"),
                            )
                            .child(
                                div()
                                    .id("input-modal-pattern")
                                    .px(px(10.0))
                                    .py(px(8.0))
                                    .bg(theme.bg_dark)
                                    .border_1()
                                    .border_color(if is_modal_focused {
                                        theme.accent
                                    } else {
                                        theme.border
                                    })
                                    .rounded_md()
                                    .font_family("monospace")
                                    .text_size(px(12.5))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.input_focus = InputFocusTarget::ModalPattern;
                                        window.focus(&this.focus_handle);
                                        cx.notify();
                                    }))
                                    .text_color(if modal_pattern.is_empty() {
                                        theme.text_muted
                                    } else {
                                        theme.text_primary
                                    })
                                    .child(if modal_pattern.is_empty() {
                                        if is_modal_focused {
                                            "| Type pattern here...".to_string()
                                        } else {
                                            "Click to type pattern...".to_string()
                                        }
                                    } else if is_modal_focused {
                                        format!("{}|", modal_pattern)
                                    } else {
                                        modal_pattern
                                    }),
                            ),
                    )
                    // Rule Flags: Exclude, Regex, Case Sensitive
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .py(px(4.0))
                            .child(
                                div()
                                    .id("chk-modal-exclude")
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.filter_edit_modal.is_exclude =
                                            !this.filter_edit_modal.is_exclude;
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .w(px(15.0))
                                            .h(px(15.0))
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(if is_ex {
                                                theme.error_red
                                            } else {
                                                theme.bg_dark
                                            })
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(10.0))
                                            .text_color(rgb(0xffffff))
                                            .child(if is_ex { "✓" } else { "" }),
                                    )
                                    .child(div().text_size(px(11.5)).child("Exclude (NOT)")),
                            )
                            .child(
                                div()
                                    .id("chk-modal-regex")
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.filter_edit_modal.is_regex =
                                            !this.filter_edit_modal.is_regex;
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .w(px(15.0))
                                            .h(px(15.0))
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(if is_reg {
                                                theme.accent
                                            } else {
                                                theme.bg_dark
                                            })
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(10.0))
                                            .text_color(rgb(0xffffff))
                                            .child(if is_reg { "✓" } else { "" }),
                                    )
                                    .child(div().text_size(px(11.5)).child("Regex")),
                            )
                            .child(
                                div()
                                    .id("chk-modal-case")
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.filter_edit_modal.is_case_sensitive =
                                            !this.filter_edit_modal.is_case_sensitive;
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .w(px(15.0))
                                            .h(px(15.0))
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(if is_cs {
                                                theme.accent
                                            } else {
                                                theme.bg_dark
                                            })
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(px(10.0))
                                            .text_color(rgb(0xffffff))
                                            .child(if is_cs { "✓" } else { "" }),
                                    )
                                    .child(div().text_size(px(11.5)).child("Case Sensitive")),
                            ),
                    )
                    // Highlight Color Swatches with WCAG 2.1 Contrast Indicator
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
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_muted)
                                            .child("Highlight Color:"),
                                    )
                                    .child(
                                        // Real-time WCAG 2.1 Contrast Badge
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .px(px(6.0))
                                            .py(px(2.0))
                                            .rounded_md()
                                            .bg(match contrast_level {
                                                ContrastLevel::Pass => theme.success_green,
                                                ContrastLevel::Warning => theme.warning_amber,
                                                ContrastLevel::Critical => theme.error_red,
                                            })
                                            .text_size(px(10.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(match contrast_level {
                                                ContrastLevel::Warning => rgb(0x000000),
                                                _ => rgb(0xffffff),
                                            })
                                            .child(format!(
                                                "WCAG {:.1}:1 ({})",
                                                ratio,
                                                match contrast_level {
                                                    ContrastLevel::Pass => "PASS",
                                                    ContrastLevel::Warning => "WARN",
                                                    ContrastLevel::Critical => "FAIL",
                                                }
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2p5()
                                    .children(
                                        [
                                            ("Red", 0xFFB6B6, 0x000000),
                                            ("Yellow", 0xFFFF00, 0x000000),
                                            ("Green", 0x90EE90, 0x000000),
                                            ("Cyan", 0x00FFFF, 0x000000),
                                            ("Blue", 0xB6D0FF, 0x000000),
                                            ("Orange", 0xFFD580, 0x000000),
                                            ("Purple", 0xE6E6FA, 0x000000),
                                            ("White", 0xFFFFFF, 0x000000),
                                        ]
                                        .into_iter()
                                        .map(|(name, bg_val, _fg_val)| {
                                            let is_curr = bg_name == name;
                                            div()
                                                .id(ElementId::NamedInteger(
                                                    "palette-color".into(),
                                                    bg_val as u64,
                                                ))
                                                .w(px(26.0))
                                                .h(px(26.0))
                                                .rounded_full()
                                                .bg(rgb(bg_val))
                                                .border_2()
                                                .border_color(if is_curr {
                                                    theme.accent
                                                } else {
                                                    theme.border
                                                })
                                                .cursor_pointer()
                                                .on_click(cx.listener(
                                                    move |this, _, _window, cx| {
                                                        this.filter_edit_modal.bg_color_name =
                                                            name.to_string();
                                                        this.filter_edit_modal.fg_color_name =
                                                            "Black".to_string();
                                                        cx.notify();
                                                    },
                                                ))
                                        })
                                        .collect::<Vec<_>>(),
                                    ),
                            ),
                    )
                    // Quick Pattern Presets
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_muted)
                                    .child("Quick Presets:"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_1p5()
                                    .children(
                                        [
                                            ("Fatal", "Red"),
                                            ("Exception", "Yellow"),
                                            ("Error", "Red"),
                                            ("Warning", "Yellow"),
                                            ("ActivityTaskManager", "Cyan"),
                                            ("AndroidRuntime", "Red"),
                                        ]
                                        .into_iter()
                                        .enumerate()
                                        .map(|(p_idx, (pat, col))| {
                                            div()
                                                .id(ElementId::NamedInteger(
                                                    "quick-preset".into(),
                                                    p_idx as u64,
                                                ))
                                                .px(px(8.0))
                                                .py(px(3.0))
                                                .rounded_md()
                                                .bg(theme.bg_card)
                                                .border_1()
                                                .border_color(theme.border_subtle)
                                                .hover(move |s| s.bg(theme.btn_hover))
                                                .font_family("monospace")
                                                .text_size(px(10.5))
                                                .cursor_pointer()
                                                .on_click(cx.listener(
                                                    move |this, _, _window, cx| {
                                                        this.filter_edit_modal.pattern =
                                                            pat.to_string();
                                                        this.filter_edit_modal.bg_color_name =
                                                            col.to_string();
                                                        cx.notify();
                                                    },
                                                ))
                                                .child(pat)
                                        })
                                        .collect::<Vec<_>>(),
                                    ),
                            ),
                    )
                    // Action Buttons (Cancel / Save)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .pt(px(8.0))
                            .child(
                                div()
                                    .id("btn-modal-cancel")
                                    .px(px(14.0))
                                    .py(px(6.0))
                                    .rounded_md()
                                    .bg(theme.bg_card)
                                    .hover(move |s| s.bg(theme.btn_hover))
                                    .text_size(px(11.5))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.close_filter_modal(cx);
                                    }))
                                    .child("Cancel (Esc)"),
                            )
                            .child(
                                div()
                                    .id("btn-modal-save")
                                    .px(px(18.0))
                                    .py(px(6.0))
                                    .rounded_md()
                                    .bg(theme.accent)
                                    .text_color(rgb(0xffffff))
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.save_modal_filter(cx);
                                    }))
                                    .child(if is_edit {
                                        "Update Rule (Enter)"
                                    } else {
                                        "Save Rule (Enter)"
                                    }),
                            ),
                    ),
            ),
    )
}

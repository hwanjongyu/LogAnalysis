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
    let modal_pattern = workspace.modal_pattern_input.read(cx).text();
    let is_reg = workspace.filter_edit_modal.is_regex;
    let is_cs = workspace.filter_edit_modal.is_case_sensitive;
    let is_ex = workspace.filter_edit_modal.is_exclude;
    let bg_name = workspace.filter_edit_modal.bg_color_name.clone();
    let fg_name = workspace.filter_edit_modal.fg_color_name.clone();
    let is_modal_focused = workspace.input_focus == InputFocusTarget::ModalPattern;

    // WCAG 2.1 Contrast Calculation with Theme Fallbacks for None / Default
    let effective_bg = if workspace.is_dark_theme { 0x1E1E1E } else { 0xFFFFFF };
    let effective_fg = if workspace.is_dark_theme { 0xD4D4D4 } else { 0x1E1E1E };

    let bg_hex = parse_color_spec(&bg_name, true).unwrap_or(effective_bg);
    let fg_hex = parse_color_spec(&fg_name, false).unwrap_or(effective_fg);
    let (ratio, contrast_level) = evaluate_contrast(bg_hex, fg_hex);

    let preview_bg = parse_color_spec(&bg_name, true)
        .map(rgb)
        .unwrap_or(theme.bg_dark);
    let preview_fg = parse_color_spec(&fg_name, false)
        .map(rgb)
        .unwrap_or(theme.text_primary);

    Some(
        div()
            .id("modal-overlay-filter-edit")
            .occlude()
            .absolute()
            .inset_0()
            .bg(rgba(0x000000aa))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("modal-card-filter-edit")
                    .occlude()
                    .w(px(520.0))
                    .bg(theme.bg_panel)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_xl()
                    .shadow_xl()
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(1.0))
                            .w_full()
                            .bg(theme.border_highlight),
                    )
                    .p(px(20.0))
                    .flex()
                    .flex_col()
                    .gap_3()
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
                                        cx.stop_propagation();
                                    }))
                                    .child("✕"),
                            ),
                    )
                    // Interactive Pattern Input Field
                    .child(
                        div()
                            .w_full()
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
                                    .w_full()
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
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .cursor_text()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.input_focus = InputFocusTarget::ModalPattern;
                                        window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                                        cx.stop_propagation();
                                    }))
                                    .child(
                                        div()
                                            .flex_1()
                                            .overflow_hidden()
                                            .child(workspace.modal_pattern_input.clone()),
                                    )
                                    .children(if !modal_pattern.is_empty() {
                                        Some(
                                            div()
                                                .id("btn-clear-modal-pattern")
                                                .flex_shrink_0()
                                                .ml(px(8.0))
                                                .px(px(6.0))
                                                .py(px(2.0))
                                                .rounded_sm()
                                                .cursor_pointer()
                                                .text_size(px(11.0))
                                                .text_color(theme.text_muted)
                                                .hover(move |s| {
                                                    s.text_color(theme.text_primary)
                                                        .bg(theme.btn_hover)
                                                })
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.modal_pattern_input.update(cx, |input, cx| {
                                                        input.clear(cx);
                                                    });
                                                    window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                }))
                                                .child("✕"),
                                        )
                                    } else {
                                        None
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
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.filter_edit_modal.is_exclude =
                                            !this.filter_edit_modal.is_exclude;
                                        this.input_focus = InputFocusTarget::ModalPattern;
                                        window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                                        cx.stop_propagation();
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
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.filter_edit_modal.is_regex =
                                            !this.filter_edit_modal.is_regex;
                                        this.input_focus = InputFocusTarget::ModalPattern;
                                        window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                                        cx.stop_propagation();
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
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.filter_edit_modal.is_case_sensitive =
                                            !this.filter_edit_modal.is_case_sensitive;
                                        this.input_focus = InputFocusTarget::ModalPattern;
                                        window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                                        cx.stop_propagation();
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
                    // Live Preview Box + WCAG Contrast Badge
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
                                            .child("Live Style Preview:"),
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
                                    .id("modal-preview-box")
                                    .w_full()
                                    .px(px(10.0))
                                    .py(px(8.0))
                                    .rounded_md()
                                    .bg(preview_bg)
                                    .text_color(preview_fg)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .font_family("monospace")
                                    .text_size(px(11.5))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(if modal_pattern.trim().is_empty() {
                                        "2026-11-03 15:40:00.123 [Sample Log] Process started successfully".to_string()
                                    } else {
                                        format!("2026-11-03 15:40:00.123 [Sample Log] Found: {}", modal_pattern)
                                    }),
                            ),
                    )
                    // Row 1: Background Color Swatches
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
                                    .child(format!("Background Color (배경색): {}", bg_name)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .children(
                                        [
                                            ("None", None),
                                            ("Red", Some(0xFFB6B6)),
                                            ("Yellow", Some(0xFFFF00)),
                                            ("Green", Some(0x90EE90)),
                                            ("Cyan", Some(0x00FFFF)),
                                            ("Blue", Some(0xB6D0FF)),
                                            ("Orange", Some(0xFFD580)),
                                            ("Purple", Some(0xE6E6FA)),
                                            ("Gray", Some(0xD3D3D3)),
                                            ("White", Some(0xFFFFFF)),
                                        ]
                                        .into_iter()
                                        .map(|(name, bg_val)| {
                                            let is_curr = bg_name.eq_ignore_ascii_case(name);
                                            let bg_color = bg_val.map(rgb).unwrap_or(theme.bg_card);
                                            div()
                                                .id(ElementId::NamedInteger(
                                                    "bg-palette-color".into(),
                                                    bg_val.unwrap_or(0) as u64,
                                                ))
                                                .w(px(24.0))
                                                .h(px(24.0))
                                                .rounded_full()
                                                .bg(bg_color)
                                                .border_2()
                                                .border_color(if is_curr {
                                                    theme.accent
                                                } else {
                                                    theme.border
                                                })
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor_pointer()
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.filter_edit_modal.bg_color_name =
                                                            name.to_string();
                                                        this.input_focus = InputFocusTarget::ModalPattern;
                                                        window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                                                        cx.stop_propagation();
                                                        cx.notify();
                                                    },
                                                ))
                                                .child(if bg_val.is_none() {
                                                    div().text_size(px(10.0)).text_color(theme.text_muted).child("⊘")
                                                } else if is_curr {
                                                    div().text_size(px(10.0)).text_color(rgb(0x000000)).child("✓")
                                                } else {
                                                    div().child("")
                                                })
                                        })
                                        .collect::<Vec<_>>(),
                                    ),
                            ),
                    )
                    // Row 2: Text / Foreground Color Swatches
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
                                    .child(format!("Text Color (글자색): {}", fg_name)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .children(
                                        [
                                            ("Default", None),
                                            ("Black", Some(0x000000)),
                                            ("White", Some(0xFFFFFF)),
                                            ("Red", Some(0xFF0000)),
                                            ("DarkRed", Some(0x8B0000)),
                                            ("Blue", Some(0x0000FF)),
                                            ("Green", Some(0x008000)),
                                            ("Gray", Some(0x808080)),
                                            ("Orange", Some(0xFFA500)),
                                            ("Purple", Some(0x800080)),
                                        ]
                                        .into_iter()
                                        .map(|(name, fg_val)| {
                                            let is_curr = fg_name.eq_ignore_ascii_case(name)
                                                || (name == "Default" && (fg_name.eq_ignore_ascii_case("none") || fg_name.is_empty()));
                                            div()
                                                .id(ElementId::NamedInteger(
                                                    "fg-palette-color".into(),
                                                    fg_val.unwrap_or(0) as u64,
                                                ))
                                                .w(px(24.0))
                                                .h(px(24.0))
                                                .rounded_full()
                                                .bg(theme.bg_card)
                                                .border_2()
                                                .border_color(if is_curr {
                                                    theme.accent
                                                } else {
                                                    theme.border
                                                })
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor_pointer()
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.filter_edit_modal.fg_color_name =
                                                            name.to_string();
                                                        this.input_focus = InputFocusTarget::ModalPattern;
                                                        window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                                                        cx.stop_propagation();
                                                        cx.notify();
                                                    },
                                                ))
                                                .child(if let Some(val) = fg_val {
                                                    div()
                                                        .text_size(px(12.0))
                                                        .font_weight(FontWeight::BOLD)
                                                        .text_color(rgb(val))
                                                        .child("A")
                                                } else {
                                                    div()
                                                        .text_size(px(9.0))
                                                        .font_weight(FontWeight::BOLD)
                                                        .text_color(theme.text_muted)
                                                        .child("Def")
                                                })
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
                                            ("🔴 ERROR", "error", "Red", "DarkRed"),
                                            ("🟡 WARN", "warn", "Orange", "Black"),
                                            ("🔵 INFO", "info", "Blue", "DarkBlue"),
                                            ("🟣 DEBUG", "debug", "Purple", "Purple"),
                                            ("⚪ TRACE", "trace", "Gray", "Black"),
                                            ("⚡ FATAL", "fatal", "Salmon", "DarkRed"),
                                            ("✓ SUCCESS", "success", "Green", "DarkGreen"),
                                            ("⚙ ActivityTaskManager", "ActivityTaskManager", "Cyan", "Black"),
                                            ("⚙ System", "System", "Blue", "DarkBlue"),
                                        ]
                                        .into_iter()
                                        .enumerate()
                                        .map(|(p_idx, (label, pat, bg, fg))| {
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
                                                    move |this, _, window, cx| {
                                                        this.modal_pattern_input.update(cx, |input, cx| {
                                                            input.set_text(pat, cx);
                                                        });
                                                        this.filter_edit_modal.bg_color_name =
                                                            bg.to_string();
                                                        this.filter_edit_modal.fg_color_name =
                                                            fg.to_string();
                                                        this.input_focus = InputFocusTarget::ModalPattern;
                                                        window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                                                        cx.stop_propagation();
                                                        cx.notify();
                                                    },
                                                ))
                                                .child(label)
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
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.close_filter_modal(cx);
                                        cx.stop_propagation();
                                    }))
                                    .child("Cancel")
                                    .child(crate::components::toolbar::render_kbd_badge("Esc", theme)),
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
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.save_modal_filter(cx);
                                        cx.stop_propagation();
                                    }))
                                    .child(if is_edit { "Update Rule" } else { "Save Rule" })
                                    .child(crate::components::toolbar::render_kbd_badge_on_accent("Enter")),
                            ),
                    ),
            ),
    )
}

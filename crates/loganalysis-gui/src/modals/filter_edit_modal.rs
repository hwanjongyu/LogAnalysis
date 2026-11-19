use gpui::prelude::FluentBuilder;
use gpui::*;
use loganalysis_core::{evaluate_contrast, parse_color_spec, ContrastLevel};

use crate::components::tooltip::render_tooltip;
use crate::types::InputFocusTarget;
use crate::workspace::LogAnalysisWorkspace;

/// Curated 24-color background palette (Row 1: Light/Pastel, Row 2: Neutral + Dark Surfaces).
pub const BG_PALETTE_SWATCHES: &[(&str, Option<u32>)] = &[
    // Row 1 — Light / Pastel (indices 0..12)
    ("None", None),
    ("Red", Some(0xFFB6B6)),
    ("Salmon", Some(0xFA8072)),
    ("Orange", Some(0xFFD580)),
    ("Yellow", Some(0xFFFF00)),
    ("Green", Some(0x90EE90)),
    ("Teal", Some(0xB2DFDB)),
    ("Cyan", Some(0x00FFFF)),
    ("Blue", Some(0xB6D0FF)),
    ("Purple", Some(0xE6E6FA)),
    ("Orchid", Some(0xDA70D6)),
    ("Gray", Some(0xD3D3D3)),
    // Row 2 — Neutral + Dark Surface (indices 12..24)
    ("White", Some(0xFFFFFF)),
    ("DarkRed", Some(0x450A0A)),
    ("DarkRose", Some(0x4C0519)),
    ("DarkOrange", Some(0x451A03)),
    ("DarkAmber", Some(0x422006)),
    ("DarkGreen", Some(0x064E3B)),
    ("DarkTeal", Some(0x134E4A)),
    ("DarkCyan", Some(0x164E63)),
    ("DarkBlue", Some(0x1E3A5F)),
    ("DarkIndigo", Some(0x312E81)),
    ("DarkPurple", Some(0x3B0764)),
    ("DarkSlate", Some(0x1E293B)),
];

/// Curated 24-color text foreground palette (Row 1: Deep/Classic, Row 2: Luminous/Light).
pub const FG_PALETTE_SWATCHES: &[(&str, Option<u32>)] = &[
    // Row 1 — Deep / Classic (indices 0..12)
    ("Default", None),
    ("Black", Some(0x000000)),
    ("DarkRed", Some(0x8B0000)),
    ("Crimson", Some(0xDC143C)),
    ("DeepAmber", Some(0xB45309)),
    ("DarkGreen", Some(0x006400)),
    ("Teal", Some(0x008080)),
    ("RoyalBlue", Some(0x4169E1)),
    ("DarkBlue", Some(0x00008B)),
    ("Indigo", Some(0x4B0082)),
    ("Purple", Some(0x800080)),
    ("Slate", Some(0x475569)),
    // Row 2 — Luminous / Light (indices 12..24)
    ("White", Some(0xFFFFFF)),
    ("CoralRed", Some(0xFCA5A5)),
    ("Rose", Some(0xFB7185)),
    ("Amber", Some(0xFBBF24)),
    ("WarmAmber", Some(0xFDE68A)),
    ("Mint", Some(0x6EE7B7)),
    ("LightTeal", Some(0x2DD4BF)),
    ("BrightCyan", Some(0x22D3EE)),
    ("Sky", Some(0x38BDF8)),
    ("Periwinkle", Some(0xA5B4FC)),
    ("Lavender", Some(0xA78BFA)),
    ("LightSlate", Some(0xCBD5E1)),
];

/// Built-in quick filter rule presets `(label, pattern, bg_color, fg_color)`.
pub const QUICK_FILTER_PRESETS: &[(&str, &str, &str, &str)] = &[
    ("🔴 ERROR", "error", "Red", "DarkRed"),
    ("🟡 WARN", "warn", "Orange", "Black"),
    ("🔵 INFO", "info", "Blue", "DarkBlue"),
    ("🟣 DEBUG", "debug", "Purple", "Purple"),
    ("⚪ TRACE", "trace", "Gray", "Black"),
    ("⚡ FATAL", "fatal", "Salmon", "DarkRed"),
    ("✓ SUCCESS", "success", "Green", "DarkGreen"),
    (
        "⚙ ActivityTaskManager",
        "ActivityTaskManager",
        "Cyan",
        "Black",
    ),
    ("⚙ System", "System", "Blue", "DarkBlue"),
];

/// Returns a unique GPUI `ElementId` for the `idx`-th color swatch under `prefix`.
pub fn swatch_element_id(prefix: &'static str, idx: usize) -> ElementId {
    ElementId::NamedInteger(prefix.into(), idx as u64)
}

/// Formats the background color section heading for the Filter Rule Editor modal.
pub fn bg_color_section_label(bg_name: &str) -> String {
    format!("Background Color: {bg_name}")
}

/// Formats the foreground text color section heading for the Filter Rule Editor modal.
pub fn fg_color_section_label(fg_name: &str) -> String {
    format!("Text Color: {fg_name}")
}

/// Selects the higher-contrast checkmark foreground (`0xFFFFFF` or `0x000000`) for a BG swatch.
pub fn swatch_checkmark_rgb(bg_rgb: u32) -> u32 {
    if loganalysis_core::contrast_ratio(bg_rgb, 0xFFFFFF)
        > loganalysis_core::contrast_ratio(bg_rgb, 0x000000)
    {
        0xFFFFFF
    } else {
        0x000000
    }
}

/// Selects an accessible circular backing plate RGB for a foreground swatch `"A"` glyph.
/// Uses `card_bg_rgb` when contrast meets WCAG AA (>= 4.5:1); otherwise chooses the
/// higher-contrast plate between light paper (`0xF8FAFC`) and dark obsidian (`0x161722`).
pub fn fg_swatch_backing_rgb(fg_rgb: u32, card_bg_rgb: u32) -> u32 {
    if loganalysis_core::contrast_ratio(card_bg_rgb, fg_rgb) >= 4.5 {
        return card_bg_rgb;
    }
    const LIGHT_PLATE: u32 = 0xF8FAFC;
    const DARK_PLATE: u32 = 0x161722;
    if loganalysis_core::contrast_ratio(LIGHT_PLATE, fg_rgb)
        >= loganalysis_core::contrast_ratio(DARK_PLATE, fg_rgb)
    {
        LIGHT_PLATE
    } else {
        DARK_PLATE
    }
}

/// Formats the hover tooltip string for a background or foreground color swatch.
pub fn format_swatch_tooltip(name: &str, rgb_opt: Option<u32>, is_bg: bool) -> String {
    match rgb_opt {
        Some(hex) => format!("{name} (#{hex:06X})"),
        None if is_bg => "None (Theme Default)".to_string(),
        None => "Default (Theme Text)".to_string(),
    }
}

/// Parsed representation of the modal's inline custom `#RRGGBB` hex input text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HexInputParse {
    Empty,
    Valid { rgb: u32, normalized: String },
    Invalid,
}

/// Parses a user-entered custom hex input (`#RRGGBB` or `RRGGBB`, trimmed, ASCII hex only).
pub fn parse_custom_hex_input(raw: &str) -> HexInputParse {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return HexInputParse::Empty;
    }
    let hex_part = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex_part.len() == 6 && hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
        if let Ok(rgb) = u32::from_str_radix(hex_part, 16) {
            return HexInputParse::Valid {
                rgb,
                normalized: format!("#{rgb:06X}"),
            };
        }
    }
    HexInputParse::Invalid
}

/// Formats an existing `FilterRule` color spec (`None`/`Default`/named/`#RRGGBB`)
/// into the initial string shown in the modal's hex input box (`""` or `"#RRGGBB"`).
pub fn format_hex_input_for_color_spec(spec: &str, is_bg: bool) -> String {
    parse_color_spec(spec, is_bg)
        .map(|rgb| format!("#{rgb:06X}"))
        .unwrap_or_default()
}

/// Cycles keyboard focus across the Filter Rule Editor modal's text fields (`Pattern`, `BG Hex`, `FG Hex`).
pub fn cycle_modal_focus_target(current: InputFocusTarget, shift: bool) -> InputFocusTarget {
    if shift {
        match current {
            InputFocusTarget::ModalPattern => InputFocusTarget::ModalFgHex,
            InputFocusTarget::ModalFgHex => InputFocusTarget::ModalBgHex,
            InputFocusTarget::ModalBgHex => InputFocusTarget::ModalPattern,
            _ => InputFocusTarget::ModalFgHex,
        }
    } else {
        match current {
            InputFocusTarget::ModalPattern => InputFocusTarget::ModalBgHex,
            InputFocusTarget::ModalBgHex => InputFocusTarget::ModalFgHex,
            InputFocusTarget::ModalFgHex => InputFocusTarget::ModalPattern,
            _ => InputFocusTarget::ModalBgHex,
        }
    }
}

/// Returns whether `(swatch_name, swatch_rgb)` matches `current_spec` while honoring ADR-2
/// (typed `#RRGGBB` highlights a matching RGB swatch) and legacy/invalid name protection.
pub fn is_swatch_selected(
    current_spec: &str,
    swatch_name: &str,
    swatch_rgb: Option<u32>,
    is_fg: bool,
    hex_invalid: bool,
) -> bool {
    if hex_invalid {
        return false;
    }
    let trimmed = current_spec.trim();
    match swatch_rgb {
        None => {
            trimmed.is_empty()
                || trimmed.eq_ignore_ascii_case("none")
                || (is_fg && trimmed.eq_ignore_ascii_case("default"))
        }
        Some(target_rgb) => {
            trimmed.eq_ignore_ascii_case(swatch_name)
                || matches!(
                    parse_custom_hex_input(trimmed),
                    HexInputParse::Valid { rgb, .. } if rgb == target_rgb
                )
        }
    }
}

/// Formats a history pattern for dropdown display, truncating very long patterns with an ellipsis.
pub fn truncate_suggestion_label(pattern: &str, max_chars: usize) -> String {
    if pattern.chars().count() <= max_chars {
        pattern.to_string()
    } else {
        let head: String = pattern.chars().take(max_chars).collect();
        format!("{head}…")
    }
}

/// Renders the pattern history suggestion dropdown anchored below the pattern input.
fn render_pattern_suggestions(
    workspace: &LogAnalysisWorkspace,
    selected: Option<usize>,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<AnyElement> {
    if !workspace.filter_edit_modal.suggestions_open {
        return None;
    }
    let suggestions = workspace.current_pattern_suggestions(cx);
    if suggestions.is_empty() {
        return None;
    }
    let theme = workspace.theme;
    let rows: Vec<AnyElement> = suggestions
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let is_sel = selected == Some(idx);
            div()
                .id(("pattern-suggestion", idx))
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px(px(10.0))
                .py(px(5.0))
                .cursor_pointer()
                .font_family("monospace")
                .text_size(px(12.0))
                .text_color(theme.text_primary)
                .when(is_sel, |d| d.bg(theme.btn_hover))
                .hover(move |s| s.bg(theme.btn_hover))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.apply_pattern_suggestion(idx, cx);
                    this.input_focus = InputFocusTarget::ModalPattern;
                    window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(truncate_suggestion_label(&entry.pattern, 80)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .flex_shrink_0()
                        .text_size(px(10.0))
                        .text_color(theme.text_muted)
                        .when(entry.regex, |d| d.child(".*"))
                        .when(entry.case_sensitive, |d| d.child("Aa"))
                        .child(
                            div()
                                .id(("pattern-suggestion-delete", idx))
                                .px(px(4.0))
                                .rounded_sm()
                                .cursor_pointer()
                                .hover(move |s| s.text_color(theme.text_primary))
                                .on_click(cx.listener(move |this, _, _window, cx| {
                                    this.delete_pattern_suggestion(idx, cx);
                                    cx.stop_propagation();
                                }))
                                .child("✕"),
                        ),
                )
                .into_any_element()
        })
        .collect();

    Some(
        deferred(
            div()
                .id("pattern-suggestions-dropdown")
                .occlude()
                .absolute()
                .top(px(56.0))
                .left_0()
                .right_0()
                .bg(theme.bg_panel)
                .border_1()
                .border_color(theme.border)
                .rounded_md()
                .shadow_lg()
                .overflow_hidden()
                .flex()
                .flex_col()
                .children(rows),
        )
        .with_priority(10)
        .into_any_element(),
    )
}

/// Renders the Pattern label, input field and suggestion dropdown (boxed to bound element type depth).
fn render_pattern_field(
    workspace: &LogAnalysisWorkspace,
    is_modal_focused: bool,
    modal_pattern: &str,
    suggestion_dropdown: Option<AnyElement>,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> AnyElement {
    let theme = workspace.theme;
    div()
        .relative()
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
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.input_focus = InputFocusTarget::ModalPattern;
                        window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
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
                            .hover(move |s| s.text_color(theme.text_primary).bg(theme.btn_hover))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.modal_pattern_input.update(cx, |input, cx| {
                                    input.clear(cx);
                                });
                                this.input_focus = InputFocusTarget::ModalPattern;
                                window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                                cx.stop_propagation();
                                cx.notify();
                            }))
                            .child("✕"),
                    )
                } else {
                    None
                }),
        )
        .children(suggestion_dropdown)
        .into_any_element()
}

struct ColorHexHeaderProps<'a> {
    section_label: String,
    hex_input_id: &'static str,
    clear_btn_id: &'static str,
    hex_input: &'a Entity<crate::components::text_input::TextInput>,
    hex_text: &'a str,
    hex_invalid: bool,
    hex_focused: bool,
    chip_color: Rgba,
    is_bg: bool,
    theme: crate::theme::Theme,
}

fn render_color_section_header(
    props: ColorHexHeaderProps<'_>,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> AnyElement {
    let ColorHexHeaderProps {
        section_label,
        hex_input_id,
        clear_btn_id,
        hex_input,
        hex_text,
        hex_invalid,
        hex_focused,
        chip_color,
        is_bg,
        theme,
    } = props;

    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_muted)
                .child(section_label),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .flex_shrink_0()
                .when(hex_invalid, |d| {
                    d.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.error_red)
                            .whitespace_nowrap()
                            .child("Invalid #RRGGBB"),
                    )
                })
                .child(
                    div()
                        .id(hex_input_id)
                        .w(px(124.0))
                        .h(px(26.0))
                        .px(px(7.0))
                        .bg(theme.bg_dark)
                        .border_1()
                        .border_color(if hex_invalid {
                            theme.error_red
                        } else if hex_focused {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .rounded_md()
                        .font_family("monospace")
                        .text_size(px(11.0))
                        .text_color(theme.text_primary)
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .cursor_text()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.filter_edit_modal.suggestions_open = false;
                                this.filter_edit_modal.suggestion_index = None;
                                if is_bg {
                                    this.input_focus = InputFocusTarget::ModalBgHex;
                                    window.focus(&this.modal_bg_hex_input.read(cx).focus_handle);
                                } else {
                                    this.input_focus = InputFocusTarget::ModalFgHex;
                                    window.focus(&this.modal_fg_hex_input.read(cx).focus_handle);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .child(
                            div()
                                .w(px(12.0))
                                .h(px(12.0))
                                .flex_shrink_0()
                                .rounded_full()
                                .border_1()
                                .border_color(theme.border)
                                .bg(chip_color),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .child(hex_input.clone()),
                        )
                        .when(!hex_text.is_empty(), |d| {
                            d.child(
                                div()
                                    .id(clear_btn_id)
                                    .flex_shrink_0()
                                    .px(px(2.0))
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_muted)
                                    .hover(move |s| {
                                        s.text_color(theme.text_primary).bg(theme.btn_hover)
                                    })
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.filter_edit_modal.suggestions_open = false;
                                        this.filter_edit_modal.suggestion_index = None;
                                        if is_bg {
                                            this.filter_edit_modal.bg_color_name =
                                                "None".to_string();
                                            this.sync_modal_bg_hex_input(cx);
                                            this.input_focus = InputFocusTarget::ModalBgHex;
                                            window.focus(
                                                &this.modal_bg_hex_input.read(cx).focus_handle,
                                            );
                                        } else {
                                            this.filter_edit_modal.fg_color_name =
                                                "Default".to_string();
                                            this.sync_modal_fg_hex_input(cx);
                                            this.input_focus = InputFocusTarget::ModalFgHex;
                                            window.focus(
                                                &this.modal_fg_hex_input.read(cx).focus_handle,
                                            );
                                        }
                                        cx.stop_propagation();
                                        cx.notify();
                                    }))
                                    .child("✕"),
                            )
                        }),
                ),
        )
        .into_any_element()
}

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
    let bg_hex_text = workspace.modal_bg_hex_input.read(cx).text();
    let fg_hex_text = workspace.modal_fg_hex_input.read(cx).text();
    let bg_hex_invalid = workspace.filter_edit_modal.bg_hex_invalid;
    let fg_hex_invalid = workspace.filter_edit_modal.fg_hex_invalid;
    let is_modal_focused = workspace.input_focus == InputFocusTarget::ModalPattern;
    let is_bg_hex_focused = workspace.input_focus == InputFocusTarget::ModalBgHex;
    let is_fg_hex_focused = workspace.input_focus == InputFocusTarget::ModalFgHex;
    let suggestion_index = workspace.filter_edit_modal.suggestion_index;
    let suggestion_dropdown = render_pattern_suggestions(workspace, suggestion_index, cx);

    // WCAG 2.1 Contrast Calculation with Theme Fallbacks for None / Default
    let effective_bg = crate::theme::rgba_to_rgb24(theme.bg_dark);
    let effective_fg = crate::theme::rgba_to_rgb24(theme.text_primary);
    let card_bg_rgb = crate::theme::rgba_to_rgb24(theme.bg_card);

    let bg_hex = parse_color_spec(&bg_name, true).unwrap_or(effective_bg);
    let fg_hex = parse_color_spec(&fg_name, false).unwrap_or(effective_fg);
    let (ratio, contrast_level) = evaluate_contrast(bg_hex, fg_hex);

    let preview_bg = parse_color_spec(&bg_name, true)
        .map(rgb)
        .unwrap_or(theme.bg_dark);
    let preview_fg = parse_color_spec(&fg_name, false)
        .map(rgb)
        .unwrap_or(theme.text_primary);
    let bg_chip_color = parse_color_spec(&bg_name, true)
        .map(rgb)
        .unwrap_or(theme.bg_card);
    let fg_chip_color = parse_color_spec(&fg_name, false)
        .map(rgb)
        .unwrap_or(theme.text_primary);

    let bg_header_el = render_color_section_header(
        ColorHexHeaderProps {
            section_label: bg_color_section_label(&bg_name),
            hex_input_id: "input-modal-bg-hex",
            clear_btn_id: "btn-clear-modal-bg-hex",
            hex_input: &workspace.modal_bg_hex_input,
            hex_text: &bg_hex_text,
            hex_invalid: bg_hex_invalid,
            hex_focused: is_bg_hex_focused,
            chip_color: bg_chip_color,
            is_bg: true,
            theme,
        },
        cx,
    );
    let fg_header_el = render_color_section_header(
        ColorHexHeaderProps {
            section_label: fg_color_section_label(&fg_name),
            hex_input_id: "input-modal-fg-hex",
            clear_btn_id: "btn-clear-modal-fg-hex",
            hex_input: &workspace.modal_fg_hex_input,
            hex_text: &fg_hex_text,
            hex_invalid: fg_hex_invalid,
            hex_focused: is_fg_hex_focused,
            chip_color: fg_chip_color,
            is_bg: false,
            theme,
        },
        cx,
    );

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
                        render_pattern_field(workspace, is_modal_focused, &modal_pattern, suggestion_dropdown, cx)
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
                    // Background Color Swatches (2 rows x 12 columns) + Custom #RRGGBB Hex Input
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(bg_header_el)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1p5()
                                    .children(
                                        BG_PALETTE_SWATCHES
                                            .chunks(12)
                                            .enumerate()
                                            .map(|(row_idx, row)| {
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .children(
                                                        row.iter()
                                                            .copied()
                                                            .enumerate()
                                                            .map(|(col_idx, (name, bg_val))| {
                                                                let idx = row_idx * 12 + col_idx;
                                                                let is_curr = is_swatch_selected(
                                                                    &bg_name,
                                                                    name,
                                                                    bg_val,
                                                                    false,
                                                                    bg_hex_invalid,
                                                                );
                                                                let bg_color = bg_val
                                                                    .map(rgb)
                                                                    .unwrap_or(theme.bg_card);
                                                                div()
                                                                    .id(swatch_element_id(
                                                                        "bg-palette-color",
                                                                        idx,
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
                                                                    .tooltip(render_tooltip(
                                                                        format_swatch_tooltip(
                                                                            name, bg_val, true,
                                                                        ),
                                                                        None,
                                                                        theme,
                                                                    ))
                                                                    .on_click(cx.listener(
                                                                        move |this, _, window, cx| {
                                                                            this.filter_edit_modal
                                                                                .bg_color_name =
                                                                                name.to_string();
                                                                            this.sync_modal_bg_hex_input(cx);
                                                                            this.input_focus =
                                                                                InputFocusTarget::ModalPattern;
                                                                            window.focus(
                                                                                &this
                                                                                    .modal_pattern_input
                                                                                    .read(cx)
                                                                                    .focus_handle,
                                                                            );
                                                                            cx.stop_propagation();
                                                                            cx.notify();
                                                                        },
                                                                    ))
                                                                    .child(
                                                                        if let Some(rgb_val) =
                                                                            bg_val
                                                                        {
                                                                            if is_curr {
                                                                                div()
                                                                                    .text_size(px(10.0))
                                                                                    .text_color(rgb(
                                                                                        swatch_checkmark_rgb(
                                                                                            rgb_val,
                                                                                        ),
                                                                                    ))
                                                                                    .child("✓")
                                                                            } else {
                                                                                div().child("")
                                                                            }
                                                                        } else {
                                                                            div()
                                                                                .text_size(px(10.0))
                                                                                .text_color(
                                                                                    theme
                                                                                        .text_muted,
                                                                                )
                                                                                .child("⊘")
                                                                        },
                                                                    )
                                                            })
                                                            .collect::<Vec<_>>(),
                                                    )
                                            })
                                            .collect::<Vec<_>>(),
                                    ),
                            ),
                    )
                    // Text / Foreground Color Swatches (2 rows x 12 columns) + Custom #RRGGBB Hex Input
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(fg_header_el)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1p5()
                                    .children(
                                        FG_PALETTE_SWATCHES
                                            .chunks(12)
                                            .enumerate()
                                            .map(|(row_idx, row)| {
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .children(
                                                        row.iter()
                                                            .copied()
                                                            .enumerate()
                                                            .map(|(col_idx, (name, fg_val))| {
                                                                let idx = row_idx * 12 + col_idx;
                                                                let is_curr = is_swatch_selected(
                                                                    &fg_name,
                                                                    name,
                                                                    fg_val,
                                                                    true,
                                                                    fg_hex_invalid,
                                                                );
                                                                let swatch_bg = match fg_val {
                                                                    Some(val) => rgb(
                                                                        fg_swatch_backing_rgb(
                                                                            val,
                                                                            card_bg_rgb,
                                                                        ),
                                                                    ),
                                                                    None if is_curr => {
                                                                        theme.accent_muted
                                                                    }
                                                                    None => theme.bg_card,
                                                                };
                                                                div()
                                                                    .id(swatch_element_id(
                                                                        "fg-palette-color",
                                                                        idx,
                                                                    ))
                                                                    .w(px(24.0))
                                                                    .h(px(24.0))
                                                                    .rounded_full()
                                                                    .bg(swatch_bg)
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
                                                                    .tooltip(render_tooltip(
                                                                        format_swatch_tooltip(
                                                                            name, fg_val, false,
                                                                        ),
                                                                        None,
                                                                        theme,
                                                                    ))
                                                                    .on_click(cx.listener(
                                                                        move |this, _, window, cx| {
                                                                            this.filter_edit_modal
                                                                                .fg_color_name =
                                                                                name.to_string();
                                                                            this.sync_modal_fg_hex_input(cx);
                                                                            this.input_focus =
                                                                                InputFocusTarget::ModalPattern;
                                                                            window.focus(
                                                                                &this
                                                                                    .modal_pattern_input
                                                                                    .read(cx)
                                                                                    .focus_handle,
                                                                            );
                                                                            cx.stop_propagation();
                                                                            cx.notify();
                                                                        },
                                                                    ))
                                                                    .child(if let Some(val) = fg_val
                                                                    {
                                                                        div()
                                                                            .text_size(px(12.0))
                                                                            .font_weight(
                                                                                FontWeight::BOLD,
                                                                            )
                                                                            .text_color(rgb(val))
                                                                            .child("A")
                                                                    } else {
                                                                        div()
                                                                            .text_size(px(9.0))
                                                                            .font_weight(
                                                                                FontWeight::BOLD,
                                                                            )
                                                                            .text_color(if is_curr {
                                                                                theme.text_primary
                                                                            } else {
                                                                                theme.text_muted
                                                                            })
                                                                            .child("Def")
                                                                    })
                                                            })
                                                            .collect::<Vec<_>>(),
                                                    )
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
                                        QUICK_FILTER_PRESETS
                                            .iter()
                                            .copied()
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
                                                        this.filter_edit_modal.programmatic_text =
                                                            Some(pat.to_string());
                                                        this.filter_edit_modal.suggestions_open =
                                                            false;
                                                        this.filter_edit_modal.suggestion_index =
                                                            None;
                                                        this.modal_pattern_input.update(
                                                            cx,
                                                            |input, cx| {
                                                                input.set_text(pat, cx);
                                                            },
                                                        );
                                                        this.filter_edit_modal.bg_color_name =
                                                            bg.to_string();
                                                        this.filter_edit_modal.fg_color_name =
                                                            fg.to_string();
                                                        this.sync_modal_bg_hex_input(cx);
                                                        this.sync_modal_fg_hex_input(cx);
                                                        this.input_focus =
                                                            InputFocusTarget::ModalPattern;
                                                        window.focus(
                                                            &this
                                                                .modal_pattern_input
                                                                .read(cx)
                                                                .focus_handle,
                                                        );
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

#[cfg(test)]
mod tests {
    // Explicit imports: `super::*` would pull in `gpui::test`, shadowing std `#[test]`.
    use super::{
        bg_color_section_label, cycle_modal_focus_target, fg_color_section_label,
        fg_swatch_backing_rgb, format_hex_input_for_color_spec, format_swatch_tooltip,
        is_swatch_selected, parse_custom_hex_input, swatch_checkmark_rgb, swatch_element_id,
        HexInputParse, BG_PALETTE_SWATCHES, FG_PALETTE_SWATCHES, QUICK_FILTER_PRESETS,
    };
    use crate::theme::{rgba_to_rgb24, Theme};
    use crate::types::InputFocusTarget;
    use crate::workspace::FACET_FILTER_PALETTE;
    use loganalysis_core::{contrast_ratio, parse_color_spec};
    use std::collections::HashSet;

    #[test]
    fn test_palette_swatch_element_ids_are_distinct() {
        let bg_ids: HashSet<_> = (0..BG_PALETTE_SWATCHES.len())
            .map(|idx| swatch_element_id("bg-palette-color", idx))
            .collect();
        assert_eq!(bg_ids.len(), BG_PALETTE_SWATCHES.len());

        let fg_ids: HashSet<_> = (0..FG_PALETTE_SWATCHES.len())
            .map(|idx| swatch_element_id("fg-palette-color", idx))
            .collect();
        assert_eq!(fg_ids.len(), FG_PALETTE_SWATCHES.len());

        let default_idx = FG_PALETTE_SWATCHES
            .iter()
            .position(|(name, _)| *name == "Default")
            .expect("Default FG swatch must exist");
        let black_idx = FG_PALETTE_SWATCHES
            .iter()
            .position(|(name, _)| *name == "Black")
            .expect("Black FG swatch must exist");
        assert_ne!(
            swatch_element_id("fg-palette-color", default_idx),
            swatch_element_id("fg-palette-color", black_idx)
        );

        let bg_label = bg_color_section_label("Yellow");
        let fg_label = fg_color_section_label("Black");
        assert_eq!(bg_label, "Background Color: Yellow");
        assert_eq!(fg_label, "Text Color: Black");
        assert!(!bg_label.contains("배경색"));
        assert!(!fg_label.contains("글자색"));
    }

    #[test]
    fn test_24x24_swatch_coverage_and_distinctness() {
        assert_eq!(BG_PALETTE_SWATCHES.len(), 24);
        assert_eq!(FG_PALETTE_SWATCHES.len(), 24);
        assert_eq!(BG_PALETTE_SWATCHES[0], ("None", None));
        assert_eq!(FG_PALETTE_SWATCHES[0], ("Default", None));

        // All 24 + 24 = 48 ElementIds across both grids must be distinct
        let mut all_ids = HashSet::new();
        for idx in 0..BG_PALETTE_SWATCHES.len() {
            assert!(
                all_ids.insert(swatch_element_id("bg-palette-color", idx)),
                "Duplicate BG ElementId at index {idx}"
            );
        }
        for idx in 0..FG_PALETTE_SWATCHES.len() {
            assert!(
                all_ids.insert(swatch_element_id("fg-palette-color", idx)),
                "Duplicate FG ElementId at index {idx}"
            );
        }
        assert_eq!(all_ids.len(), 48);

        let mut bg_hexes = HashSet::new();
        for &(name, val) in &BG_PALETTE_SWATCHES[1..] {
            let rgb_val = val.expect("Colored BG swatch must have RGB");
            assert!(
                bg_hexes.insert(rgb_val),
                "Duplicate BG swatch RGB for {name}"
            );
            assert_eq!(parse_color_spec(name, true), Some(rgb_val));
        }
        assert_eq!(bg_hexes.len(), 23);

        let mut fg_hexes = HashSet::new();
        for &(name, val) in &FG_PALETTE_SWATCHES[1..] {
            let rgb_val = val.expect("Colored FG swatch must have RGB");
            assert!(
                fg_hexes.insert(rgb_val),
                "Duplicate FG swatch RGB for {name}"
            );
            assert_eq!(parse_color_spec(name, false), Some(rgb_val));
        }
        assert_eq!(fg_hexes.len(), 23);

        for &(_label, _pat, bg, fg) in QUICK_FILTER_PRESETS {
            assert!(
                BG_PALETTE_SWATCHES.iter().any(|&(name, _)| name == bg),
                "Preset BG '{bg}' missing from BG_PALETTE_SWATCHES"
            );
            assert!(
                FG_PALETTE_SWATCHES.iter().any(|&(name, _)| name == fg),
                "Preset FG '{fg}' missing from FG_PALETTE_SWATCHES"
            );
        }

        for &facet_bg in &FACET_FILTER_PALETTE {
            assert!(
                BG_PALETTE_SWATCHES
                    .iter()
                    .any(|&(name, _)| name == facet_bg),
                "Facet palette color '{facet_bg}' missing from BG_PALETTE_SWATCHES"
            );
        }
    }

    #[test]
    fn test_24x24_palette_wcag_aa_contrast_matrix() {
        let light = Theme::light();
        let dark = Theme::dark();
        assert_eq!(rgba_to_rgb24(light.text_primary), 0x0F172A);
        assert_eq!(rgba_to_rgb24(light.bg_dark), 0xFFFFFF);
        assert_eq!(rgba_to_rgb24(dark.text_primary), 0xE6E7EE);
        assert_eq!(rgba_to_rgb24(dark.bg_dark), 0x12131A);
        assert_eq!(rgba_to_rgb24(dark.bg_card), 0x1C1E2A);

        // Row-1 Light/Pastel BGs (1..=11) + White (12) pass WCAG AA with Black & Light text_primary
        for &(name, val) in &BG_PALETTE_SWATCHES[1..=12] {
            let bg_rgb = val.unwrap();
            assert!(
                contrast_ratio(bg_rgb, 0x000000) >= 4.5,
                "BG swatch {name} must have >= 4.5:1 vs #000000"
            );
            assert!(
                contrast_ratio(bg_rgb, 0x0F172A) >= 4.5,
                "BG swatch {name} must have >= 4.5:1 vs #0F172A"
            );
        }

        // Row-2 Dark BGs (13..=23) pass WCAG AA with White & Dark text_primary
        for &(name, val) in &BG_PALETTE_SWATCHES[13..=23] {
            let bg_rgb = val.unwrap();
            assert!(
                contrast_ratio(bg_rgb, 0xFFFFFF) >= 4.5,
                "Dark BG swatch {name} must have >= 4.5:1 vs #FFFFFF"
            );
            assert!(
                contrast_ratio(bg_rgb, 0xE6E7EE) >= 4.5,
                "Dark BG swatch {name} must have >= 4.5:1 vs #E6E7EE"
            );
        }

        // Row-1 Deep FGs (1..=11) pass WCAG AA on Light bg_dark (#FFFFFF)
        for &(name, val) in &FG_PALETTE_SWATCHES[1..=11] {
            let fg_rgb = val.unwrap();
            assert!(
                contrast_ratio(0xFFFFFF, fg_rgb) >= 4.5,
                "Row-1 FG swatch {name} must have >= 4.5:1 vs #FFFFFF"
            );
        }

        // Row-2 Luminous FGs (12..=23) pass WCAG AA on Dark bg_dark (#12131A), Dark bg_card (#1C1E2A),
        // and paired Row-2 Dark BG surface (White paired with DarkSlate, 13..=23 paired by column)
        for (idx, &(fg_name, fg_val)) in FG_PALETTE_SWATCHES.iter().enumerate().skip(12) {
            let fg_rgb = fg_val.unwrap();
            assert!(
                contrast_ratio(0x12131A, fg_rgb) >= 4.5,
                "Row-2 FG swatch {fg_name} must have >= 4.5:1 vs #12131A"
            );
            assert!(
                contrast_ratio(0x1C1E2A, fg_rgb) >= 4.5,
                "Row-2 FG swatch {fg_name} must have >= 4.5:1 vs #1C1E2A"
            );
            let paired_idx = if idx == 12 { 23 } else { idx };
            let (bg_name, bg_val) = BG_PALETTE_SWATCHES[paired_idx];
            let bg_rgb = bg_val.unwrap();
            assert!(
                contrast_ratio(bg_rgb, fg_rgb) >= 4.5,
                "Row-2 FG swatch {fg_name} must have >= 4.5:1 on paired {bg_name}"
            );
        }
    }

    #[test]
    fn test_swatch_checkmark_rgb_and_fg_backing_plate() {
        assert_eq!(swatch_checkmark_rgb(0x000000), 0xFFFFFF);
        assert_eq!(swatch_checkmark_rgb(0xFFFFFF), 0x000000);

        for &(name, val) in &BG_PALETTE_SWATCHES[1..=12] {
            let bg_rgb = val.unwrap();
            let check = swatch_checkmark_rgb(bg_rgb);
            assert_eq!(
                check, 0x000000,
                "Light swatch {name} should use black check"
            );
            assert!(contrast_ratio(bg_rgb, check) >= 7.0);
        }
        for &(name, val) in &BG_PALETTE_SWATCHES[13..=23] {
            let bg_rgb = val.unwrap();
            let check = swatch_checkmark_rgb(bg_rgb);
            assert_eq!(check, 0xFFFFFF, "Dark swatch {name} should use white check");
            assert!(contrast_ratio(bg_rgb, check) >= 7.0);
        }

        let light_card = rgba_to_rgb24(Theme::light().bg_card);
        let dark_card = rgba_to_rgb24(Theme::dark().bg_card);
        assert_eq!(light_card, 0xFFFFFF);
        assert_eq!(dark_card, 0x1C1E2A);

        for &(name, val) in &FG_PALETTE_SWATCHES[1..=11] {
            let fg_rgb = val.unwrap();
            assert_eq!(
                fg_swatch_backing_rgb(fg_rgb, light_card),
                light_card,
                "Row-1 FG swatch {name} should render directly on Light bg_card"
            );
            assert_eq!(
                fg_swatch_backing_rgb(fg_rgb, dark_card),
                0xF8FAFC,
                "Row-1 FG swatch {name} should use light paper backing plate on Dark bg_card"
            );
        }
        for &(name, val) in &FG_PALETTE_SWATCHES[12..=23] {
            let fg_rgb = val.unwrap();
            assert_eq!(
                fg_swatch_backing_rgb(fg_rgb, light_card),
                0x161722,
                "Row-2 FG swatch {name} should use dark obsidian backing plate on Light bg_card"
            );
            assert_eq!(
                fg_swatch_backing_rgb(fg_rgb, dark_card),
                dark_card,
                "Row-2 FG swatch {name} should render directly on Dark bg_card"
            );
        }
        for &(name, val) in &FG_PALETTE_SWATCHES[1..] {
            let fg_rgb = val.unwrap();
            let light_plate = fg_swatch_backing_rgb(fg_rgb, light_card);
            let dark_plate = fg_swatch_backing_rgb(fg_rgb, dark_card);
            assert!(
                contrast_ratio(light_plate, fg_rgb) >= 4.5,
                "FG swatch {name} failed light theme backing contrast"
            );
            assert!(
                contrast_ratio(dark_plate, fg_rgb) >= 4.5,
                "FG swatch {name} failed dark theme backing contrast"
            );
        }
    }

    #[test]
    fn test_format_swatch_tooltip() {
        assert_eq!(
            format_swatch_tooltip("None", None, true),
            "None (Theme Default)"
        );
        assert_eq!(
            format_swatch_tooltip("Default", None, false),
            "Default (Theme Text)"
        );
        assert_eq!(
            format_swatch_tooltip("DarkRed", Some(0x450A0A), true),
            "DarkRed (#450A0A)"
        );
        assert_eq!(
            format_swatch_tooltip("CoralRed", Some(0xFCA5A5), false),
            "CoralRed (#FCA5A5)"
        );
        assert_eq!(
            format_swatch_tooltip("Black", Some(0x000000), false),
            "Black (#000000)"
        );
        assert_eq!(
            format_swatch_tooltip("DarkGreen", Some(0x064E3B), true),
            "DarkGreen (#064E3B)"
        );
    }

    #[test]
    fn test_parse_custom_hex_input_and_format_for_spec() {
        assert_eq!(parse_custom_hex_input(""), HexInputParse::Empty);
        assert_eq!(parse_custom_hex_input("   \t"), HexInputParse::Empty);
        assert_eq!(
            parse_custom_hex_input("#1a2b3c"),
            HexInputParse::Valid {
                rgb: 0x1A2B3C,
                normalized: "#1A2B3C".to_string(),
            }
        );
        assert_eq!(
            parse_custom_hex_input("1a2b3c"),
            HexInputParse::Valid {
                rgb: 0x1A2B3C,
                normalized: "#1A2B3C".to_string(),
            }
        );
        assert_eq!(
            parse_custom_hex_input("  #ff00aa  "),
            HexInputParse::Valid {
                rgb: 0xFF00AA,
                normalized: "#FF00AA".to_string(),
            }
        );
        assert_eq!(
            parse_custom_hex_input("#000000"),
            HexInputParse::Valid {
                rgb: 0x000000,
                normalized: "#000000".to_string(),
            }
        );
        assert_eq!(
            parse_custom_hex_input("ffffff"),
            HexInputParse::Valid {
                rgb: 0xFFFFFF,
                normalized: "#FFFFFF".to_string(),
            }
        );

        for invalid in [
            "#", "#123", "#12345", "#1234567", "#12345G", "12345G", "+12345", "#+12345", "-12345",
            "#-12345", "Red",
        ] {
            assert_eq!(
                parse_custom_hex_input(invalid),
                HexInputParse::Invalid,
                "Expected Invalid for '{invalid}'"
            );
        }

        assert_eq!(format_hex_input_for_color_spec("None", true), "");
        assert_eq!(format_hex_input_for_color_spec("Default", false), "");
        assert_eq!(format_hex_input_for_color_spec("", true), "");
        assert_eq!(format_hex_input_for_color_spec("Yellow", true), "#FFFF00");
        assert_eq!(format_hex_input_for_color_spec("DarkRed", true), "#450A0A");
        assert_eq!(format_hex_input_for_color_spec("DarkRed", false), "#8B0000");
        assert_eq!(format_hex_input_for_color_spec("#1a2b3c", true), "#1A2B3C");
        assert_eq!(format_hex_input_for_color_spec("1a2b3c", false), "#1A2B3C");
        assert_eq!(format_hex_input_for_color_spec("Magenta", true), "");
    }

    #[test]
    fn test_cycle_modal_focus_target_and_swatch_selection() {
        assert_eq!(
            cycle_modal_focus_target(InputFocusTarget::ModalPattern, false),
            InputFocusTarget::ModalBgHex
        );
        assert_eq!(
            cycle_modal_focus_target(InputFocusTarget::ModalBgHex, false),
            InputFocusTarget::ModalFgHex
        );
        assert_eq!(
            cycle_modal_focus_target(InputFocusTarget::ModalFgHex, false),
            InputFocusTarget::ModalPattern
        );
        for other in [
            InputFocusTarget::None,
            InputFocusTarget::SearchBar,
            InputFocusTarget::CommandPalette,
            InputFocusTarget::BookmarkNote,
        ] {
            assert_eq!(
                cycle_modal_focus_target(other, false),
                InputFocusTarget::ModalBgHex
            );
            assert_eq!(
                cycle_modal_focus_target(other, true),
                InputFocusTarget::ModalFgHex
            );
        }

        assert_eq!(
            cycle_modal_focus_target(InputFocusTarget::ModalPattern, true),
            InputFocusTarget::ModalFgHex
        );
        assert_eq!(
            cycle_modal_focus_target(InputFocusTarget::ModalFgHex, true),
            InputFocusTarget::ModalBgHex
        );
        assert_eq!(
            cycle_modal_focus_target(InputFocusTarget::ModalBgHex, true),
            InputFocusTarget::ModalPattern
        );

        // ADR-2: typed #FFB6B6 highlights matching Red BG swatch
        assert!(is_swatch_selected(
            "#FFB6B6",
            "Red",
            Some(0xFFB6B6),
            false,
            false
        ));
        assert!(is_swatch_selected(
            "#ffb6b6",
            "Red",
            Some(0xFFB6B6),
            false,
            false
        ));
        assert!(is_swatch_selected(
            "ffb6b6",
            "Red",
            Some(0xFFB6B6),
            false,
            false
        ));
        assert!(is_swatch_selected(
            "  #ffb6b6  ",
            "Red",
            Some(0xFFB6B6),
            false,
            false
        ));
        assert!(is_swatch_selected(
            "#FCA5A5",
            "CoralRed",
            Some(0xFCA5A5),
            true,
            false
        ));
        assert!(!is_swatch_selected(
            "#123456",
            "Red",
            Some(0xFFB6B6),
            false,
            false
        ));
        assert!(!is_swatch_selected(
            "Aqua",
            "Cyan",
            Some(0x00FFFF),
            false,
            false
        ));

        // Legacy/invalid name protection for None / Default
        assert!(is_swatch_selected("None", "None", None, false, false));
        assert!(is_swatch_selected("", "None", None, false, false));
        assert!(!is_swatch_selected("White", "None", None, false, false));
        assert!(!is_swatch_selected("Default", "None", None, false, false));
        assert!(!is_swatch_selected("Magenta", "None", None, false, false));
        assert!(is_swatch_selected("Default", "Default", None, true, false));
        assert!(is_swatch_selected("none", "Default", None, true, false));
        assert!(is_swatch_selected("", "Default", None, true, false));
        assert!(!is_swatch_selected("Unknown", "Default", None, true, false));

        // Invalid hex state suppresses swatch selection ring
        assert!(!is_swatch_selected(
            "Red",
            "Red",
            Some(0xFFB6B6),
            false,
            true
        ));
        assert!(!is_swatch_selected("None", "None", None, false, true));
    }
}

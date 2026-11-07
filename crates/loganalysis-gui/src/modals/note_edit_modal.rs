use gpui::prelude::FluentBuilder;
use gpui::*;
use loganalysis_core::LogSource;

use crate::workspace::LogAnalysisWorkspace;

/// Renders the floating modal for adding/editing a triage note on a bookmarked line.
pub fn render_note_edit_modal(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    if !workspace.is_note_modal_open {
        return None;
    }

    let theme = workspace.theme;
    let source_idx = workspace.note_editing_source_idx?;
    let line_number = source_idx + 1;

    let line_text = workspace
        .log_source
        .as_ref()
        .and_then(|s| s.get_line(source_idx))
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("Line {line_number}"));

    let existing_note = workspace
        .bookmarks
        .get(source_idx)
        .and_then(|b| b.note.as_deref());

    Some(
        div()
            .id("modal-overlay-note-edit")
            .occlude()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000088))
            .flex()
            .flex_col()
            .items_center()
            .pt(px(90.0))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _window, cx| {
                    this.close_note_modal(cx);
                }),
            )
            .child(
                div()
                    .id("note-edit-card")
                    .w(px(560.0))
                    .bg(theme.bg_card)
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _window, cx| {
                        cx.stop_propagation();
                    })
                    // Header
                    .child(
                        div()
                            .h(px(44.0))
                            .px(px(16.0))
                            .bg(theme.bg_header)
                            .border_b_1()
                            .border_color(theme.border)
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
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.text_primary)
                                            .child(format!("📝 Bookmark Note — Line {line_number}")),
                                    ),
                            )
                            .child(
                                div()
                                    .id("btn-close-note-modal")
                                    .cursor_pointer()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_xs()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_muted)
                                    .hover(|s| s.bg(theme.row_hover).text_color(theme.text_primary))
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.close_note_modal(cx);
                                    }))
                                    .child("✕"),
                            ),
                    )
                    // Content Body
                    .child(
                        div()
                            .p(px(16.0))
                            .flex()
                            .flex_col()
                            .gap_3()
                            // Log Line Preview Box
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_muted)
                                            .child("LOG STATEMENT:"),
                                    )
                                    .child(
                                        div()
                                            .p(px(8.0))
                                            .rounded_xs()
                                            .bg(theme.bg_dark)
                                            .border_1()
                                            .border_color(theme.border)
                                            .font_family("monospace")
                                            .text_size(px(11.0))
                                            .text_color(theme.text_muted)
                                            .max_h(px(60.0))
                                            .overflow_hidden()
                                            .child(line_text),
                                    ),
                            )
                            // Note Input Section
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_muted)
                                            .child("TRIAGE NOTE / ROOT CAUSE:"),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .h(px(36.0))
                                            .rounded_xs()
                                            .border_1()
                                            .border_color(theme.accent)
                                            .bg(theme.bg_dark)
                                            .p_1()
                                            .child(workspace.note_input.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_muted)
                                    .child("↵ Save • Esc Cancel • Notes are automatically included in HTML incident exports"),
                            ),
                    )
                    // Footer Actions
                    .child(
                        div()
                            .h(px(46.0))
                            .px(px(16.0))
                            .bg(theme.bg_header)
                            .border_t_1()
                            .border_color(theme.border)
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div().when(existing_note.is_some(), |parent| {
                                    parent.child(
                                        div()
                                            .id("btn-clear-bookmark-note")
                                            .cursor_pointer()
                                            .px(px(10.0))
                                            .py(px(4.0))
                                            .rounded_xs()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(theme.bg_card)
                                            .text_size(px(11.0))
                                            .text_color(rgb(0xef4444))
                                            .hover(|s| s.bg(rgba(0xef444415)))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.clear_bookmark_note(cx);
                                            }))
                                            .child("Clear Note"),
                                    )
                                }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("btn-cancel-note-modal")
                                            .cursor_pointer()
                                            .px(px(12.0))
                                            .py(px(5.0))
                                            .rounded_xs()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(theme.bg_card)
                                            .text_size(px(12.0))
                                            .text_color(theme.text_muted)
                                            .hover(|s| s.bg(theme.row_hover).text_color(theme.text_primary))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.close_note_modal(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-save-bookmark-note")
                                            .cursor_pointer()
                                            .px(px(14.0))
                                            .py(px(5.0))
                                            .rounded_xs()
                                            .bg(theme.accent)
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0xffffff))
                                            .hover(|s| s.opacity(0.9))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.save_bookmark_note(cx);
                                            }))
                                            .child("Save Note"),
                                    ),
                            ),
                    ),
            ),
    )
}

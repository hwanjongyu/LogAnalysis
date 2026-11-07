use gpui::prelude::FluentBuilder;
use gpui::*;
use loganalysis_core::LineSeverity;

use crate::workspace::LogAnalysisWorkspace;

/// Renders the slide-over Bookmark Drawer on the right of the screen.
pub fn render_bookmark_drawer(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    if !workspace.is_bookmark_drawer_open {
        return None;
    }

    let theme = workspace.theme;
    let bookmarks = workspace.bookmarks.items().to_vec();
    let total_bookmarks = bookmarks.len();
    let selected_idx = workspace
        .bookmark_drawer_selected_index
        .min(total_bookmarks.saturating_sub(1));

    let items_el = if bookmarks.is_empty() {
        div()
            .w_full()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .p_8()
            .text_color(theme.text_muted)
            .child(div().text_size(px(32.0)).child("🔖"))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("No Bookmarks Set"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child("Press F2 on any log line to add it to your incident triage list."),
            )
            .into_any_element()
    } else {
        div()
            .id("bookmark-drawer-list")
            .w_full()
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .children(bookmarks.iter().enumerate().map(|(idx, b)| {
                let is_selected = idx == selected_idx;
                let b_source_idx = b.source_idx;
                let b_line_num = b.line_number;
                let (sev_color, sev_bg, sev_label) = match b.severity {
                    LineSeverity::Error => (rgb(0xef4444), rgba(0xef444422), "ERROR"),
                    LineSeverity::Warning => (rgb(0xf59e0b), rgba(0xf59e0b22), "WARN"),
                    LineSeverity::RuleMatch => (rgb(0x3b82f6), rgba(0x3b82f622), "MATCH"),
                    LineSeverity::Normal => (theme.text_muted, theme.bg_card, "INFO"),
                };

                let note_opt = b.note.clone();

                div()
                    .id(("bookmark-item", idx))
                    .w_full()
                    .px(px(16.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(if is_selected {
                        theme.row_hover
                    } else {
                        theme.bg_card
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.row_hover))
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        this.bookmark_drawer_selected_index = idx;
                        this.jump_to_bookmark_source_idx(b_source_idx, cx);
                        this.close_bookmark_drawer(cx);
                    }))
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    // Top row: Line Number badge + Severity badge + Actions
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
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_xs()
                                            .bg(theme.accent_muted)
                                            .border_1()
                                            .border_color(theme.accent)
                                            .font_family("monospace")
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.accent)
                                            .child(format!("#{b_line_num}")),
                                    )
                                    .child(
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_xs()
                                            .bg(sev_bg)
                                            .font_family("monospace")
                                            .text_size(px(10.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(sev_color)
                                            .child(sev_label),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .id(("btn-drawer-note", idx))
                                            .cursor_pointer()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_xs()
                                            .text_size(px(11.0))
                                            .text_color(theme.accent)
                                            .hover(|s| s.bg(theme.accent_muted))
                                            .on_click(cx.listener(move |this, _, _window, cx| {
                                                this.open_note_modal_for_source_idx(b_source_idx, cx);
                                            }))
                                            .child("📝 Note"),
                                    )
                                    .child(
                                        div()
                                            .id(("btn-drawer-del", idx))
                                            .cursor_pointer()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_xs()
                                            .text_size(px(11.0))
                                            .text_color(rgb(0xef4444))
                                            .hover(|s| s.bg(rgba(0xef444415)))
                                            .on_click(cx.listener(move |this, _, _window, cx| {
                                                this.bookmarks.remove(b_source_idx);
                                                this.status_text =
                                                    format!("Removed bookmark on line {b_line_num}").into();
                                                cx.notify();
                                            }))
                                            .child("✕"),
                                    ),
                            ),
                    )
                    // Middle: Log Text preview
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(11.0))
                            .text_color(theme.text_primary)
                            .line_height(px(16.0))
                            .max_h(px(32.0))
                            .overflow_hidden()
                            .child(b.text.clone()),
                    )
                    // Bottom: Note annotation if present
                    .when(note_opt.is_some(), |parent| {
                        let note_str = note_opt.unwrap();
                        parent.child(
                            div()
                                .flex()
                                .items_start()
                                .gap_1()
                                .p_1p5()
                                .rounded_xs()
                                .bg(theme.bg_dark)
                                .border_1()
                                .border_color(theme.border)
                                .text_size(px(11.0))
                                .text_color(theme.text_primary)
                                .child(div().text_size(px(11.0)).child("📝"))
                                .child(div().flex_1().child(note_str)),
                        )
                    })
            }))
            .into_any_element()
    };

    Some(
        div()
            .id("modal-overlay-bookmark-drawer")
            .occlude()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000066))
            .flex()
            .justify_end()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _window, cx| {
                    this.close_bookmark_drawer(cx);
                }),
            )
            .child(
                div()
                    .id("bookmark-drawer-panel")
                    .w(px(520.0))
                    .h_full()
                    .bg(theme.bg_card)
                    .border_l_1()
                    .border_color(theme.border)
                    .shadow_xl()
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_, _window, cx| {
                        cx.stop_propagation();
                    })
                    // Drawer Header
                    .child(
                        div()
                            .h(px(48.0))
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
                                            .child(format!("🔖 Bookmarks ({total_bookmarks})")),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("btn-drawer-export-html")
                                            .cursor_pointer()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(theme.bg_card)
                                            .text_size(px(11.0))
                                            .text_color(theme.accent)
                                            .hover(|s| s.bg(theme.accent_muted))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.prompt_export_html_report(Some(window), cx);
                                            }))
                                            .child("Export HTML..."),
                                    )
                                    .child(
                                        div()
                                            .id("btn-drawer-clear-all")
                                            .cursor_pointer()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(theme.bg_card)
                                            .text_size(px(11.0))
                                            .text_color(rgb(0xef4444))
                                            .hover(|s| s.bg(rgba(0xef444415)))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.clear_all_bookmarks(cx);
                                            }))
                                            .child("Clear All"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-drawer-close")
                                            .cursor_pointer()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .text_size(px(12.0))
                                            .text_color(theme.text_muted)
                                            .hover(|s| {
                                                s.bg(theme.row_hover).text_color(theme.text_primary)
                                            })
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.close_bookmark_drawer(cx);
                                            }))
                                            .child("✕"),
                                    ),
                            ),
                    )
                    // Drawer List Content
                    .child(items_el)
                    // Drawer Footer Key Hints
                    .child(
                        div()
                            .h(px(32.0))
                            .px(px(16.0))
                            .bg(theme.bg_header)
                            .border_t_1()
                            .border_color(theme.border)
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child("↑↓ navigate • ↵ jump to line • esc close"),
                    ),
            ),
    )
}

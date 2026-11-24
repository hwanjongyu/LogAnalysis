use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::workspace::updater::{
    format_display_tag, format_published_date, validate_release_url, ReleaseInfo, UpdateCheckState,
};
use crate::workspace::LogAnalysisWorkspace;

fn active_release_info(workspace: &LogAnalysisWorkspace) -> Option<&ReleaseInfo> {
    match &workspace.update_check_state {
        UpdateCheckState::UpdateAvailable(info) => Some(info),
        UpdateCheckState::UpToDate {
            release: Some(info),
            ..
        } => Some(info),
        _ => None,
    }
}

/// Renders the floating Software Update modal showing release status, notes, and actions.
pub fn render_update_modal(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    if !workspace.is_update_modal_open {
        return None;
    }

    let theme = workspace.theme;
    let is_checking = matches!(workspace.update_check_state, UpdateCheckState::Checking);
    let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let release_info = active_release_info(workspace);

    let (pill_label, pill_bg, pill_fg, pill_border) = match &workspace.update_check_state {
        UpdateCheckState::Idle => ("Ready", theme.bg_dark, theme.text_muted, theme.border),
        UpdateCheckState::Checking => (
            "Checking...",
            theme.accent_muted,
            theme.accent_muted_fg,
            theme.accent,
        ),
        UpdateCheckState::UpdateAvailable(_) => (
            "Update Available",
            theme.accent_muted,
            theme.accent_muted_fg,
            theme.accent,
        ),
        UpdateCheckState::UpToDate { .. } => (
            "Up to Date",
            rgba(0x10b98122),
            theme.success_green,
            theme.success_green,
        ),
        UpdateCheckState::Error(_) => (
            "Check Failed",
            rgba(0xef444422),
            theme.error_red,
            theme.error_red,
        ),
    };

    let latest_version_text = release_info
        .map(|info| format_display_tag(&info.tag_name))
        .unwrap_or_else(|| "—".to_string());
    let published_date = release_info.and_then(|info| format_published_date(info.published_at.as_deref()));
    let is_skipped = release_info
        .is_some_and(|info| workspace.update_settings.is_version_skipped(&info.tag_name));
    let can_skip = matches!(&workspace.update_check_state, UpdateCheckState::UpdateAvailable(info) if !workspace.update_settings.is_version_skipped(&info.tag_name));
    let has_valid_release_url =
        release_info.is_some_and(|info| validate_release_url(&info.html_url).is_ok());

    let check_button_label = match &workspace.update_check_state {
        UpdateCheckState::Idle => "Check Now",
        UpdateCheckState::Checking => "Checking...",
        _ => "Check Again",
    };

    let auto_check_label = if workspace.update_settings.auto_check_updates {
        "[✓] Auto-check for updates on startup (every 24h)"
    } else {
        "[ ] Auto-check for updates on startup"
    };

    let notes_section = match &workspace.update_check_state {
        UpdateCheckState::UpdateAvailable(info) => Some(
            info.body
                .clone()
                .unwrap_or_else(|| {
                    "Release notes unavailable (rate-limit fallback or empty release description)."
                        .to_string()
                }),
        ),
        UpdateCheckState::UpToDate {
            release: Some(info),
            ..
        } => info.body.clone(),
        _ => None,
    };

    Some(
        div()
            .id("modal-overlay-update")
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
                    this.close_update_modal(cx);
                }),
            )
            .child(
                div()
                    .id("update-modal-card")
                    .w(px(540.0))
                    .bg(theme.bg_card)
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .shadow_xl()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _window, cx| {
                        cx.stop_propagation();
                    })
                    .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                        if event.keystroke.key == "escape" {
                            this.close_update_modal(cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(div().h(px(1.0)).w_full().bg(theme.border_highlight))
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
                                    .gap_2p5()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.text_primary)
                                            .child("Software Update"),
                                    )
                                    .child(
                                        div()
                                            .px(px(8.0))
                                            .py(px(2.0))
                                            .rounded_xs()
                                            .bg(pill_bg)
                                            .border_1()
                                            .border_color(pill_border)
                                            .text_size(px(10.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(pill_fg)
                                            .child(pill_label),
                                    ),
                            )
                            .child(
                                div()
                                    .id("btn-close-update-modal")
                                    .cursor_pointer()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_xs()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_muted)
                                    .hover(|s| s.bg(theme.row_hover).text_color(theme.text_primary))
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.close_update_modal(cx);
                                    }))
                                    .child("✕"),
                            ),
                    )
                    // Body
                    .child(
                        div()
                            .p(px(16.0))
                            .flex()
                            .flex_col()
                            .gap_3()
                            // Version Comparison Strip
                            .child(
                                div()
                                    .px(px(12.0))
                                    .py(px(10.0))
                                    .bg(theme.bg_dark)
                                    .border_1()
                                    .border_color(theme.border)
                                    .rounded_xs()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_size(px(10.5))
                                                    .text_color(theme.text_muted)
                                                    .child("Current Version"),
                                            )
                                            .child(
                                                div()
                                                    .font_family("monospace")
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(theme.text_primary)
                                                    .child(current_version),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .items_end()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_size(px(10.5))
                                                    .text_color(theme.text_muted)
                                                    .child(if let Some(date) = published_date {
                                                        format!("Latest Release ({date})")
                                                    } else {
                                                        "Latest Release".to_string()
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .when(is_skipped, |parent| {
                                                        parent.child(
                                                            div()
                                                                .px_1p5()
                                                                .py_0p5()
                                                                .rounded_xs()
                                                                .bg(rgba(0xf59e0b22))
                                                                .text_size(px(10.0))
                                                                .text_color(theme.warning_text)
                                                                .child("Skipped"),
                                                        )
                                                    })
                                                    .child(
                                                        div()
                                                            .font_family("monospace")
                                                            .text_size(px(13.0))
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(theme.text_primary)
                                                            .child(latest_version_text),
                                                    ),
                                            ),
                                    ),
                            )
                            // State-specific message
                            .when(is_checking, |parent| {
                                parent.child(
                                    div()
                                        .px(px(12.0))
                                        .py(px(10.0))
                                        .rounded_xs()
                                        .bg(theme.bg_dark)
                                        .border_1()
                                        .border_color(theme.border_subtle)
                                        .text_size(px(12.0))
                                        .text_color(theme.text_muted)
                                        .child("Contacting GitHub Releases (api.github.com)..."),
                                )
                            })
                            .when(
                                matches!(
                                    workspace.update_check_state,
                                    UpdateCheckState::UpToDate { .. }
                                ),
                                |parent| {
                                    parent.child(
                                        div()
                                            .px(px(12.0))
                                            .py(px(10.0))
                                            .rounded_xs()
                                            .bg(rgba(0x10b98114))
                                            .border_1()
                                            .border_color(theme.success_green)
                                            .text_size(px(12.0))
                                            .text_color(theme.text_primary)
                                            .child("You are running the latest version of LogAnalysis."),
                                    )
                                },
                            )
                            .when_some(
                                match &workspace.update_check_state {
                                    UpdateCheckState::Error(msg) => Some(msg.clone()),
                                    _ => None,
                                },
                                |parent, msg| {
                                    parent.child(
                                        div()
                                            .px(px(12.0))
                                            .py(px(10.0))
                                            .rounded_xs()
                                            .bg(rgba(0xef444416))
                                            .border_1()
                                            .border_color(theme.error_red)
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(theme.error_red)
                                                    .child(msg),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .text_color(theme.text_muted)
                                                    .child("Check your network connection or proxy settings and click Check Again."),
                                            ),
                                    )
                                },
                            )
                            // Scrollable Release Notes Preview
                            .when_some(notes_section, |parent, notes| {
                                parent.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme.text_muted)
                                                .child("RELEASE NOTES"),
                                        )
                                        .child(
                                            div()
                                                .id("update-release-notes-scroll")
                                                .w_full()
                                                .max_h(px(200.0))
                                                .p(px(10.0))
                                                .rounded_xs()
                                                .bg(theme.bg_dark)
                                                .border_1()
                                                .border_color(theme.border)
                                                .overflow_y_scroll()
                                                .track_scroll(&workspace.update_notes_scroll_handle)
                                                .text_size(px(11.5))
                                                .line_height(px(17.0))
                                                .text_color(theme.text_primary)
                                                .child(notes),
                                        ),
                                )
                            })
                            // Startup Auto-Check Preference Toggle
                            .child(
                                div()
                                    .id("btn-toggle-auto-update-check")
                                    .cursor_pointer()
                                    .px(px(10.0))
                                    .py(px(7.0))
                                    .rounded_xs()
                                    .bg(theme.bg_dark)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .text_size(px(11.5))
                                    .text_color(theme.text_primary)
                                    .hover(|s| s.bg(theme.row_hover))
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.toggle_auto_check_updates(cx);
                                    }))
                                    .child(auto_check_label),
                            ),
                    )
                    // Footer
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
                            .child(div().when(can_skip, |parent| {
                                parent.child(
                                    div()
                                        .id("btn-skip-update-version")
                                        .cursor_pointer()
                                        .px(px(10.0))
                                        .py(px(5.0))
                                        .rounded_xs()
                                        .border_1()
                                        .border_color(theme.border)
                                        .bg(theme.bg_card)
                                        .text_size(px(11.5))
                                        .text_color(theme.text_muted)
                                        .hover(|s| {
                                            s.bg(theme.row_hover).text_color(theme.text_primary)
                                        })
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            this.skip_available_update_version(cx);
                                        }))
                                        .child("Skip This Version"),
                                )
                            }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("btn-check-updates-now")
                                            .px(px(12.0))
                                            .py(px(5.0))
                                            .rounded_xs()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(theme.bg_card)
                                            .text_size(px(12.0))
                                            .text_color(if is_checking {
                                                theme.text_muted
                                            } else {
                                                theme.text_primary
                                            })
                                            .when(!is_checking, |s| {
                                                s.cursor_pointer()
                                                    .hover(|h| h.bg(theme.row_hover))
                                                    .on_click(cx.listener(|this, _, _window, cx| {
                                                        this.check_for_updates_manual(cx);
                                                    }))
                                            })
                                            .child(check_button_label),
                                    )
                                    .when(has_valid_release_url, |parent| {
                                        parent.child(
                                            div()
                                                .id("btn-open-release-page")
                                                .cursor_pointer()
                                                .px(px(14.0))
                                                .py(px(5.0))
                                                .rounded_xs()
                                                .bg(theme.accent)
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(0xffffff))
                                                .hover(|s| s.bg(theme.accent_hover))
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.open_verified_release_page(cx);
                                                }))
                                                .child("Open Release Page"),
                                        )
                                    })
                                    .child(
                                        div()
                                            .id("btn-update-modal-close")
                                            .cursor_pointer()
                                            .px(px(12.0))
                                            .py(px(5.0))
                                            .rounded_xs()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(theme.bg_card)
                                            .text_size(px(12.0))
                                            .text_color(theme.text_muted)
                                            .hover(|s| {
                                                s.bg(theme.row_hover).text_color(theme.text_primary)
                                            })
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.close_update_modal(cx);
                                            }))
                                            .child("Close"),
                                    ),
                            ),
                    ),
            ),
    )
}

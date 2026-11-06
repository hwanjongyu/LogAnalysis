use gpui::*;

use crate::workspace::LogAnalysisWorkspace;

/// Returns whether Client-Side Decorations (CSD) titlebar should be rendered.
/// Automatically enabled when the compositor/OS does not provide server-side decorations (e.g. Wayland GNOME).
pub fn should_render_csd(window: &Window) -> bool {
    if std::env::var("LOGANALYSIS_DISABLE_CSD").is_ok() {
        return false;
    }
    if std::env::var("LOGANALYSIS_FORCE_CSD").is_ok() {
        return true;
    }
    if matches!(window.window_decorations(), Decorations::Client { .. }) {
        return true;
    }

    #[cfg(target_os = "linux")]
    {
        // Fail-safe defense-in-depth: If running under Wayland, ensure CSD titlebar is always rendered
        std::env::var_os("WAYLAND_DISPLAY")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
            || std::env::var("XDG_SESSION_TYPE")
                .map(|v| v.eq_ignore_ascii_case("wayland"))
                .unwrap_or(false)
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Renders the polished CSD titlebar with centered document breadcrumb and micro-control buttons
pub fn render_titlebar(
    workspace: &LogAnalysisWorkspace,
    window: &mut Window,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    if !should_render_csd(window) {
        // Under X11 with server-side decorations, do not render a duplicate titlebar
        return div().id("titlebar-empty");
    }

    let theme = workspace.theme;

    // Determine center status badge content
    let (center_icon, center_label, is_live) = if workspace.is_streaming {
        let dev_name = workspace
            .get_selected_device_info()
            .map(|i| i.short_name())
            .or_else(|| workspace.selected_device.clone())
            .unwrap_or_else(|| "Android Device".to_string());
        let status = if workspace.is_stream_paused {
            "Paused"
        } else {
            "Live Stream"
        };
        ("📱", format!("{} • {}", dev_name, status), !workspace.is_stream_paused)
    } else if let Some(file_name) = &workspace.active_file_name {
        ("📄", file_name.to_string(), false)
    } else {
        ("⚡", "LogAnalysis — Ready".to_string(), false)
    };

    div()
        .id("custom-csd-titlebar")
        .w_full()
        .h(px(34.0))
        .bg(theme.bg_header)
        .border_b_1()
        .border_color(theme.border)
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        // ==========================================
        // Left: Sleek Brand & Badge
        // ==========================================
        .child(
            div()
                .id("titlebar-left-brand")
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .h_full()
                .on_mouse_down(MouseButton::Left, cx.listener(|_, event: &MouseDownEvent, window, _| {
                    if event.click_count == 2 {
                        window.zoom_window();
                    } else if event.click_count == 1 {
                        window.start_window_move();
                    }
                }))
                // App Logo Pill
                .child(
                    div()
                        .w(px(18.0))
                        .h(px(18.0))
                        .rounded_sm()
                        .bg(theme.accent)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(rgb(0xffffff))
                        .font_weight(FontWeight::BOLD)
                        .child("L"),
                )
                // App Name
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child("LogAnalysis"),
                )
                // Engine Badge
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_xs()
                        .bg(theme.bg_badge)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .text_xs()
                        .text_size(px(10.0))
                        .text_color(theme.text_muted)
                        .child("GPU"),
                )
                // Version Badge
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_xs()
                        .bg(theme.bg_badge)
                        .border_1()
                        .border_color(theme.border_subtle)
                        .text_xs()
                        .text_size(px(10.0))
                        .text_color(theme.text_muted)
                        .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                ),
        )
        // ==========================================
        // Center: Interactive Draggable Breadcrumb Pill
        // ==========================================
        .child(
            div()
                .id("titlebar-center-container")
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, cx.listener(|_, event: &MouseDownEvent, window, _| {
                    if event.click_count == 2 {
                        window.zoom_window();
                    } else if event.click_count == 1 {
                        window.start_window_move();
                    }
                }))
                .child(
                    div()
                        .id("titlebar-breadcrumb-pill")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .px_3()
                        .py_0p5()
                        .rounded_md()
                        .bg(theme.bg_card)
                        .border_1()
                        .border_color(theme.border_subtle)
                        // Live green dot or static icon
                        .child(
                            if is_live {
                                div()
                                    .w(px(6.5))
                                    .h(px(6.5))
                                    .rounded_full()
                                    .bg(theme.success_green)
                            } else {
                                div()
                                    .text_xs()
                                    .child(center_icon)
                            },
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_primary)
                                .child(center_label),
                        ),
                ),
        )
        // ==========================================
        // Right: Window Controls (Min, Max, Close)
        // ==========================================
        .child(
            div()
                .id("titlebar-window-controls")
                .flex()
                .flex_row()
                .items_center()
                .h_full()
                // Minimize Button (Crisp horizontal bar)
                .child(
                    div()
                        .id("btn-window-minimize")
                        .w(px(40.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(|s| s.bg(theme.btn_hover))
                        .on_click(cx.listener(|_, _, window, _| {
                            window.minimize_window();
                        }))
                        .child(
                            div()
                                .w(px(10.0))
                                .h(px(1.5))
                                .bg(theme.text_muted),
                        ),
                )
                // Maximize / Restore Button (Crisp square outline)
                .child(
                    div()
                        .id("btn-window-maximize")
                        .w(px(40.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(|s| s.bg(theme.btn_hover))
                        .on_click(cx.listener(|_, _, window, _| {
                            window.zoom_window();
                        }))
                        .child(
                            div()
                                .w(px(9.5))
                                .h(px(9.5))
                                .border_1()
                                .border_color(theme.text_muted)
                                .rounded_xs(),
                        ),
                )
                // Close Button (Smooth red hover on close)
                .child(
                    div()
                        .id("btn-window-close")
                        .w(px(44.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.text_muted)
                        .hover(|s| s.bg(rgb(0xe81123)).text_color(rgb(0xffffff)))
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.remove_window();
                            cx.quit();
                        }))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .child("✕"),
                        ),
                ),
        )
}

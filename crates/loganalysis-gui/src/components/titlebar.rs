use gpui::*;

use crate::theme::Theme;
use crate::workspace::LogAnalysisWorkspace;

/// Target hex color for the window close button hover state (#e81123 / Windows/GNOME crimson).
pub const WINDOW_CLOSE_HOVER_COLOR: u32 = 0xe81123;

/// Returns explicit override for CSD rendering from environment variables.
/// Priority hierarchy:
/// 1. `LOGANALYSIS_DISABLE_CSD` => Some(false) (takes top priority)
/// 2. `LOGANALYSIS_FORCE_CSD` => Some(true)
/// 3. Neither set => None (defers to window decoration / OS detection)
pub fn check_csd_env_override() -> Option<bool> {
    if std::env::var("LOGANALYSIS_DISABLE_CSD")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        Some(false)
    } else if std::env::var("LOGANALYSIS_FORCE_CSD")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        Some(true)
    } else {
        None
    }
}

/// Returns whether Client-Side Decorations (CSD) titlebar/window controls should be rendered.
/// Automatically enabled when the compositor/OS does not provide server-side decorations (e.g. Wayland GNOME).
pub fn should_render_csd(window: &Window) -> bool {
    if let Some(forced) = check_csd_env_override() {
        return forced;
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

/// Renders CSD window controls (minimize, maximize, close) for integration into the far-right
/// of the 38px unified header bar. Returns None if Server-Side Decorations (SSD) are active.
pub fn render_window_controls(
    window: &Window,
    theme: Theme,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Option<impl IntoElement> {
    if !should_render_csd(window) {
        return None;
    }

    Some(
        div()
            .id("titlebar-window-controls")
            .flex()
            .flex_row()
            .items_center()
            .h_full()
            // Minimize Button (Crisp horizontal bar, 40px)
            .child(
                div()
                    .id("btn-window-minimize")
                    .w(px(40.0))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(move |s| s.bg(theme.btn_hover))
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
            // Maximize / Restore Button (Crisp square outline, 40px)
            .child(
                div()
                    .id("btn-window-maximize")
                    .w(px(40.0))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(move |s| s.bg(theme.btn_hover))
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
            // Close Button (Smooth red hover on close, 44px)
            .child(
                div()
                    .id("btn-window-close")
                    .w(px(44.0))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.text_muted)
                    .hover(|s| s.bg(rgb(WINDOW_CLOSE_HOVER_COLOR)).text_color(rgb(0xffffff)))
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

/// Backwards compatibility stub for render_titlebar.
/// In the unified header architecture, all titlebar features are consolidated into the 38px unified header.
#[allow(dead_code)]
pub fn render_titlebar(
    _workspace: &LogAnalysisWorkspace,
    _window: &mut Window,
    _cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    div().id("titlebar-empty")
}



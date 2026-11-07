use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::components::tooltip::render_tooltip;
use crate::theme::Theme;
use crate::workspace::LogAnalysisWorkspace;

/// Configuration options for an item in the Activity Bar
struct ActivityItemConfig {
    id: &'static str,
    icon: &'static str,
    tooltip_title: &'static str,
    shortcut: Option<&'static str>,
    is_active: bool,
    badge_count: Option<usize>,
    has_status_dot: Option<Rgba>,
}

/// Renders an individual button on the Activity Bar rail with active indicator and optional badge
fn render_activity_item(
    cfg: ActivityItemConfig,
    theme: Theme,
    cx: &mut Context<LogAnalysisWorkspace>,
    on_click: impl Fn(&mut LogAnalysisWorkspace, &mut Window, &mut Context<LogAnalysisWorkspace>)
        + 'static,
) -> impl IntoElement {
    let count_val = cfg.badge_count.unwrap_or(0);

    div()
        .id(cfg.id)
        .relative()
        .w(px(34.0))
        .h(px(34.0))
        .rounded_md()
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .when(cfg.is_active, |s| {
            s.bg(theme.bg_card)
                .border_l_2()
                .border_color(theme.accent)
        })
        .when(!cfg.is_active, |s| {
            s.border_l_2().border_color(rgba(0x00000000))
        })
        .hover(|s| s.bg(theme.btn_hover))
        .tooltip(render_tooltip(cfg.tooltip_title, cfg.shortcut, theme))
        .on_click(cx.listener(move |this, _, window, cx| {
            on_click(this, window, cx);
        }))
        // Main icon glyph
        .child(
            div()
                .text_size(px(14.0))
                .text_color(if cfg.is_active {
                    theme.accent
                } else {
                    theme.text_muted
                })
                .child(cfg.icon),
        )
        // Numeric badge (e.g. active filter count or bookmark count)
        .when(count_val > 0, |parent| {
            parent.child(
                div()
                    .absolute()
                    .top(px(2.0))
                    .right(px(2.0))
                    .px(px(3.5))
                    .py(px(0.5))
                    .rounded_full()
                    .bg(theme.accent)
                    .text_color(rgb(0xffffff))
                    .text_size(px(8.5))
                    .font_weight(FontWeight::BOLD)
                    .child(if count_val > 99 {
                        "99+".to_string()
                    } else {
                        count_val.to_string()
                    }),
            )
        })
        // Live status dot (e.g. pulsing green for active ADB stream)
        .when(cfg.has_status_dot.is_some(), |parent| {
            let dot_color = cfg.has_status_dot.unwrap();
            parent.child(
                div()
                    .absolute()
                    .bottom(px(4.0))
                    .right(px(4.0))
                    .w(px(6.0))
                    .h(px(6.0))
                    .rounded_full()
                    .bg(dot_color),
            )
        })
}

/// Renders the slim Left Navigation Rail / Activity Bar
pub fn render_activity_bar(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;

    let active_filter_count = workspace
        .filter_tabs
        .get(workspace.active_tab_index)
        .map(|tab| tab.filters.iter().filter(|f| f.active).count())
        .unwrap_or(0);

    let bookmark_count = workspace.bookmarks.len();

    let adb_dot_color = if workspace.is_streaming {
        if workspace.is_stream_paused {
            Some(theme.warning_amber)
        } else {
            Some(theme.success_green)
        }
    } else if !workspace.adb_device_details.is_empty() {
        Some(rgb(0x3b82f6)) // Blue for connected but idle
    } else {
        None
    };

    div()
        .id("activity-bar")
        .w(px(42.0))
        .h_full()
        .flex_shrink_0()
        .bg(theme.bg_header)
        .border_r_1()
        .border_color(theme.border_subtle)
        .flex()
        .flex_col()
        .items_center()
        .py(px(6.0))
        .gap(px(4.0))
        // 1. Files & Recent Workspace
        .child(render_activity_item(
            ActivityItemConfig {
                id: "act-files",
                icon: "📁",
                tooltip_title: "Open Log File / Ingestion",
                shortcut: Some("Ctrl+O"),
                is_active: workspace.log_source.is_some(),
                badge_count: None,
                has_status_dot: None,
            },
            theme,
            cx,
            |this, _, cx| {
                this.prompt_open_file(None, cx);
            },
        ))
        // 2. Filter Rules Dock Toggle
        .child(render_activity_item(
            ActivityItemConfig {
                id: "act-filters",
                icon: "🔍",
                tooltip_title: "Toggle Filter Dock",
                shortcut: Some("Shift+F"),
                is_active: workspace.is_filter_dock_visible,
                badge_count: if active_filter_count > 0 {
                    Some(active_filter_count)
                } else {
                    None
                },
                has_status_dot: None,
            },
            theme,
            cx,
            |this, _, cx| {
                this.toggle_filter_dock(cx);
            },
        ))
        // 3. Forensic Facets Bar
        .child(render_activity_item(
            ActivityItemConfig {
                id: "act-facets",
                icon: "🏷️",
                tooltip_title: "Toggle Forensic Facets Bar",
                shortcut: Some("Alt+F"),
                is_active: workspace.is_facets_visible,
                badge_count: if !workspace.facet_tags.is_empty() {
                    Some(workspace.facet_tags.len())
                } else {
                    None
                },
                has_status_dot: None,
            },
            theme,
            cx,
            |this, _, cx| {
                this.toggle_facets_bar(cx);
            },
        ))
        // 4. Bookmarks Drawer Toggle
        .child(render_activity_item(
            ActivityItemConfig {
                id: "act-bookmarks",
                icon: "🔖",
                tooltip_title: "Toggle Bookmark Drawer",
                shortcut: Some("Ctrl+B"),
                is_active: workspace.is_bookmark_drawer_open,
                badge_count: if bookmark_count > 0 {
                    Some(bookmark_count)
                } else {
                    None
                },
                has_status_dot: None,
            },
            theme,
            cx,
            |this, _, cx| {
                this.toggle_bookmark_drawer(cx);
            },
        ))
        // 5. Timeline Sparkline Ribbon
        .child(render_activity_item(
            ActivityItemConfig {
                id: "act-timeline",
                icon: "📈",
                tooltip_title: "Toggle Anomaly Timeline Sparkline",
                shortcut: Some("Ctrl+T"),
                is_active: workspace.is_timeline_visible,
                badge_count: None,
                has_status_dot: None,
            },
            theme,
            cx,
            |this, _, cx| {
                this.toggle_eps_timeline(cx);
            },
        ))
        // 6. ADB Streaming & Devices
        .child(render_activity_item(
            ActivityItemConfig {
                id: "act-adb",
                icon: "📱",
                tooltip_title: "ADB Devices & Live Streaming",
                shortcut: Some("Ctrl+R"),
                is_active: workspace.is_streaming || workspace.is_device_selector_open,
                badge_count: None,
                has_status_dot: adb_dot_color,
            },
            theme,
            cx,
            |this, _, cx| {
                this.toggle_device_selector(cx);
            },
        ))
        // 7. Dual-Pane Split View
        .child(render_activity_item(
            ActivityItemConfig {
                id: "act-split",
                icon: "🪟",
                tooltip_title: "Toggle Dual-Pane Split View",
                shortcut: Some("Ctrl+D"),
                is_active: workspace.is_split_view,
                badge_count: None,
                has_status_dot: None,
            },
            theme,
            cx,
            |this, _, cx| {
                this.toggle_split_view(cx);
            },
        ))
        // 8. Structured Column View
        .child(render_activity_item(
            ActivityItemConfig {
                id: "act-column",
                icon: "📋",
                tooltip_title: "Toggle Structured Column View",
                shortcut: Some("Ctrl+L"),
                is_active: workspace.is_column_view,
                badge_count: None,
                has_status_dot: None,
            },
            theme,
            cx,
            |this, _, cx| {
                this.toggle_column_view(cx);
            },
        ))
        // Flexible spacer to push utility actions to bottom
        .child(div().flex_1())
        // 9. Command Palette Quick Trigger
        .child(render_activity_item(
            ActivityItemConfig {
                id: "act-palette",
                icon: "⌨️",
                tooltip_title: "Command Palette & Shortcuts",
                shortcut: Some("Ctrl+P"),
                is_active: workspace.is_command_palette_open,
                badge_count: None,
                has_status_dot: None,
            },
            theme,
            cx,
            |this, window, cx| {
                this.toggle_command_palette(cx);
                if this.is_command_palette_open {
                    window.focus(&this.palette_input.read(cx).focus_handle);
                }
            },
        ))
        // 10. Theme Switcher (Dark / Light)
        .child(render_activity_item(
            ActivityItemConfig {
                id: "act-theme",
                icon: if workspace.is_dark_theme { "☀️" } else { "🌙" },
                tooltip_title: "Toggle Dark / Light Theme",
                shortcut: Some("Ctrl+U"),
                is_active: false,
                badge_count: None,
                has_status_dot: None,
            },
            theme,
            cx,
            |this, _, cx| {
                this.toggle_theme(cx);
            },
        ))
}

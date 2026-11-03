use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::types::{FilterPanelPosition, InputFocusTarget};
use crate::workspace::LogAnalysisWorkspace;

pub fn render_filter_dock(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let is_bottom_dock = workspace.filter_panel_position == FilterPanelPosition::Bottom;
    let input_focus = workspace.input_focus;

    div()
        .id("filter-panel-container")
        .bg(theme.bg_panel)
        .flex()
        .flex_col()
        .overflow_hidden()
        .when(!is_bottom_dock, |s| {
            s.w(px(310.0))
                .h_full()
                .border_r_1()
                .border_color(theme.border)
        })
        .when(is_bottom_dock, |s| {
            s.w_full()
                .h(px(220.0))
                .border_t_1()
                .border_color(theme.border)
        })
        // Tab Bar Header
        .child(
            div()
                .h(px(36.0))
                .w_full()
                .bg(theme.bg_header)
                .border_b_1()
                .border_color(theme.border)
                .flex()
                .items_center()
                .justify_between()
                .px(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .overflow_hidden()
                        .children(workspace.filter_tabs.iter().enumerate().map(
                            |(t_idx, tab)| {
                                let is_active_tab = t_idx == workspace.active_tab_index;
                                div()
                                    .id(ElementId::NamedInteger(
                                        "tab-header-btn".into(),
                                        t_idx as u64,
                                    ))
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .rounded_md()
                                    .bg(if is_active_tab {
                                        theme.accent
                                    } else {
                                        theme.bg_card
                                    })
                                    .text_color(if is_active_tab {
                                        rgb(0xffffff)
                                    } else {
                                        theme.text_muted
                                    })
                                    .text_size(px(11.0))
                                    .font_weight(if is_active_tab {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                        this.select_tab(t_idx, cx);
                                    }))
                                    .child(tab.name.clone())
                            },
                        )),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .id("btn-add-tab")
                                .px(px(6.0))
                                .py(px(2.0))
                                .rounded_sm()
                                .bg(theme.bg_card)
                                .hover(move |s| s.bg(theme.btn_hover))
                                .text_size(px(10.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.add_filter_tab(cx);
                                }))
                                .child("+ Tab"),
                        )
                        .child(
                            div()
                                .id("btn-del-tab")
                                .px(px(5.0))
                                .py(px(2.0))
                                .rounded_sm()
                                .bg(theme.bg_card)
                                .hover(move |s| s.bg(theme.btn_hover))
                                .text_size(px(10.0))
                                .text_color(theme.error_red)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.delete_current_tab(cx);
                                }))
                                .child("✕"),
                        ),
                ),
        )
        // Section Header + Inline/Dedicated Search Bar + Add Rule CTA
        .child(
            div()
                .h(px(34.0))
                .w_full()
                .px(px(10.0))
                .border_b_1()
                .border_color(theme.border_subtle)
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
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_muted)
                                .child("FILTER RULES"),
                        )
                        .children(if is_bottom_dock {
                            Some(
                                div()
                                    .id("input-filter-rule-search-inline")
                                    .h(px(24.0))
                                    .w(px(220.0))
                                    .px(px(8.0))
                                    .rounded_md()
                                    .bg(theme.bg_dark)
                                    .border_1()
                                    .border_color(
                                        if input_focus == InputFocusTarget::FilterRuleSearch {
                                            theme.accent
                                        } else {
                                            theme.border_subtle
                                        },
                                    )
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.input_focus = InputFocusTarget::FilterRuleSearch;
                                        window.focus(&this.focus_handle);
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(10.5))
                                            .text_color(
                                                if workspace.filter_search_query.is_empty() {
                                                    theme.text_muted
                                                } else {
                                                    theme.text_primary
                                                },
                                            )
                                            .child(if workspace.filter_search_query.is_empty() {
                                                if input_focus == InputFocusTarget::FilterRuleSearch
                                                {
                                                    "🔍 | Type to filter...".to_string()
                                                } else {
                                                    "🔍 Filter rules...".to_string()
                                                }
                                            } else if input_focus
                                                == InputFocusTarget::FilterRuleSearch
                                            {
                                                format!("🔍 {}|", workspace.filter_search_query)
                                            } else {
                                                format!("🔍 {}", workspace.filter_search_query)
                                            }),
                                    )
                                    .children(if !workspace.filter_search_query.is_empty() {
                                        Some(
                                            div()
                                                .id("btn-clear-filter-search-inline")
                                                .px(px(4.0))
                                                .cursor_pointer()
                                                .text_size(px(10.0))
                                                .text_color(theme.text_muted)
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.filter_search_query.clear();
                                                    cx.notify();
                                                }))
                                                .child("✕"),
                                        )
                                    } else {
                                        None
                                    }),
                            )
                        } else {
                            None
                        }),
                )
                .child(
                    div()
                        .id("btn-add-filter-rule")
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded_md()
                        .bg(theme.accent)
                        .text_color(rgb(0xffffff))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_add_filter_modal(cx);
                            window.focus(&this.focus_handle);
                        }))
                        .child("+ New Rule"),
                ),
        )
        // In Left Sidebar mode, display the full-width dedicated search box
        .children(if !is_bottom_dock {
            Some(
                div()
                    .p(px(8.0))
                    .border_b_1()
                    .border_color(theme.border_subtle)
                    .child(
                        div()
                            .id("input-filter-rule-search")
                            .h(px(28.0))
                            .w_full()
                            .px(px(8.0))
                            .rounded_md()
                            .bg(theme.bg_dark)
                            .border_1()
                            .border_color(if input_focus == InputFocusTarget::FilterRuleSearch {
                                theme.accent
                            } else {
                                theme.border_subtle
                            })
                            .flex()
                            .items_center()
                            .justify_between()
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.input_focus = InputFocusTarget::FilterRuleSearch;
                                window.focus(&this.focus_handle);
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(if workspace.filter_search_query.is_empty() {
                                        theme.text_muted
                                    } else {
                                        theme.text_primary
                                    })
                                    .child(if workspace.filter_search_query.is_empty() {
                                        if input_focus == InputFocusTarget::FilterRuleSearch {
                                            "🔍 | Type to filter rules...".to_string()
                                        } else {
                                            "🔍 Filter rules in tab...".to_string()
                                        }
                                    } else if input_focus == InputFocusTarget::FilterRuleSearch {
                                        format!("🔍 {}|", workspace.filter_search_query)
                                    } else {
                                        format!("🔍 {}", workspace.filter_search_query)
                                    }),
                            )
                            .children(if !workspace.filter_search_query.is_empty() {
                                Some(
                                    div()
                                        .id("btn-clear-filter-search")
                                        .px(px(4.0))
                                        .cursor_pointer()
                                        .text_size(px(10.0))
                                        .text_color(theme.text_muted)
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            this.filter_search_query.clear();
                                            cx.notify();
                                        }))
                                        .child("✕"),
                                )
                            } else {
                                None
                            }),
                    ),
            )
        } else {
            None
        })
        // Scrollable Color-Coded Filter Rule Cards (Vertical List format)
        .child(
            div()
                .id("filter-rules-scroll")
                .flex_1()
                .overflow_y_scroll()
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap_1p5()
                .children(
                    workspace
                        .filter_tabs
                        .get(workspace.active_tab_index)
                        .map(|tab| {
                            let is_filtering_rules = !workspace.filter_search_query.trim().is_empty();
                            let total_rules = tab.filters.len();
                            let counts = workspace
                                .filter_result
                                .as_ref()
                                .map(|r| r.filter_counts.clone())
                                .unwrap_or_default();
                            let search_q = workspace.filter_search_query.to_lowercase();
                            tab.filters
                                .iter()
                                .enumerate()
                                .filter(|(_, r)| {
                                    if search_q.is_empty() {
                                        true
                                    } else {
                                        r.text.to_lowercase().contains(&search_q)
                                    }
                                })
                                .map(|(r_idx, rule)| {
                                    let count = counts.get(r_idx).copied().unwrap_or(0);
                                    let is_active = rule.active;
                                    let is_exclude = rule.exclude;
                                    let is_regex = rule.regex;
                                    let bg_opt = loganalysis_core::parse_color_spec(&rule.bg_color, true);
                                    let fg_opt = loganalysis_core::parse_color_spec(&rule.text_color, false);
                                    let swatch_bg = bg_opt
                                        .map(rgb)
                                        .unwrap_or(if is_exclude {
                                            theme.error_red
                                        } else {
                                            theme.bg_card
                                        });
                                    let swatch_fg = fg_opt
                                        .map(rgb)
                                        .unwrap_or(if bg_opt.is_some() {
                                            rgb(0x000000)
                                        } else {
                                            theme.text_primary
                                        });

                                    div()
                                        .id(ElementId::NamedInteger(
                                            "filter-row".into(),
                                            r_idx as u64,
                                        ))
                                        .w_full()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .px(px(8.0))
                                        .py(px(6.0))
                                        .rounded_md()
                                        .bg(if is_active {
                                            theme.bg_card
                                        } else {
                                            theme.bg_panel
                                        })
                                        .border_1()
                                        .border_color(if is_active {
                                            theme.border
                                        } else {
                                            theme.border_subtle
                                        })
                                        .hover(move |s| s.bg(theme.btn_hover))
                                        // Left side: Color swatch + Checkbox + Pattern Text
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .overflow_hidden()
                                                // Dual-Color Swatch Preview Badge
                                                .child(
                                                    div()
                                                        .w(px(20.0))
                                                        .h(px(18.0))
                                                        .rounded_sm()
                                                        .flex_shrink_0()
                                                        .bg(swatch_bg)
                                                        .border_1()
                                                        .border_color(if bg_opt.is_none() {
                                                            theme.border
                                                        } else {
                                                            swatch_bg
                                                        })
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .child(
                                                            div()
                                                                .font_weight(FontWeight::BOLD)
                                                                .text_size(px(9.5))
                                                                .text_color(swatch_fg)
                                                                .child("Aa"),
                                                        ),
                                                )
                                                // Active Checkbox
                                                .child(
                                                    div()
                                                        .id(ElementId::NamedInteger(
                                                            "chk-rule".into(),
                                                            r_idx as u64,
                                                        ))
                                                        .w(px(15.0))
                                                        .h(px(15.0))
                                                        .flex_shrink_0()
                                                        .rounded_sm()
                                                        .border_1()
                                                        .border_color(if is_active {
                                                            theme.accent
                                                        } else {
                                                            theme.border
                                                        })
                                                        .bg(if is_active {
                                                            theme.accent
                                                        } else {
                                                            theme.bg_dark
                                                        })
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(
                                                            move |this, _, _window, cx| {
                                                                this.toggle_rule_active(r_idx, cx);
                                                            },
                                                        ))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(0xffffff))
                                                        .child(if is_active { "✓" } else { "" }),
                                                )
                                                // Pattern String (Monospace)
                                                .child(
                                                    div()
                                                        .font_family("monospace")
                                                        .text_size(px(11.5))
                                                        .whitespace_nowrap()
                                                        .overflow_hidden()
                                                        .text_color(if is_active {
                                                            theme.text_primary
                                                        } else {
                                                            theme.text_muted
                                                        })
                                                        .child(rule.text.clone()),
                                                ),
                                        )
                                        // Right side: Micro Badges + Match Count Pill + Edit + Delete
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .flex_shrink_0()
                                                .children(if is_regex {
                                                    Some(
                                                        div()
                                                            .px(px(4.0))
                                                            .py(px(1.0))
                                                            .rounded_sm()
                                                            .bg(theme.bg_badge)
                                                            .text_size(px(9.0))
                                                            .text_color(theme.accent)
                                                            .child(".*"),
                                                    )
                                                } else {
                                                    None
                                                })
                                                .children(if is_exclude {
                                                    Some(
                                                        div()
                                                            .px(px(4.0))
                                                            .py(px(1.0))
                                                            .rounded_sm()
                                                            .bg(theme.bg_badge)
                                                            .text_size(px(9.0))
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(theme.error_red)
                                                            .child("NOT"),
                                                    )
                                                } else {
                                                    None
                                                })
                                                // Match Count Pill
                                                .child(
                                                    div()
                                                        .px(px(6.0))
                                                        .py(px(1.0))
                                                        .rounded_full()
                                                        .bg(theme.bg_badge)
                                                        .font_family("monospace")
                                                        .text_size(px(10.0))
                                                        .text_color(if count > 0 {
                                                            theme.text_primary
                                                        } else {
                                                            theme.text_muted
                                                        })
                                                        .child(format!("{}", count)),
                                                )
                                                // Reorder Up
                                                .children(if !is_filtering_rules && r_idx > 0 {
                                                    Some(
                                                        div()
                                                            .id(ElementId::NamedInteger(
                                                                "up-rule".into(),
                                                                r_idx as u64,
                                                            ))
                                                            .px(px(3.0))
                                                            .cursor_pointer()
                                                            .text_size(px(10.0))
                                                            .text_color(theme.text_muted)
                                                            .hover(move |s| s.text_color(theme.accent))
                                                            .on_click(cx.listener(
                                                                move |this, _, _window, cx| {
                                                                    this.move_rule_up(r_idx, cx);
                                                                },
                                                            ))
                                                            .child("▲"),
                                                    )
                                                } else {
                                                    None
                                                })
                                                // Reorder Down
                                                .children(if !is_filtering_rules && r_idx + 1 < total_rules {
                                                    Some(
                                                        div()
                                                            .id(ElementId::NamedInteger(
                                                                "down-rule".into(),
                                                                r_idx as u64,
                                                            ))
                                                            .px(px(3.0))
                                                            .cursor_pointer()
                                                            .text_size(px(10.0))
                                                            .text_color(theme.text_muted)
                                                            .hover(move |s| s.text_color(theme.accent))
                                                            .on_click(cx.listener(
                                                                move |this, _, _window, cx| {
                                                                    this.move_rule_down(r_idx, cx);
                                                                },
                                                            ))
                                                            .child("▼"),
                                                    )
                                                } else {
                                                    None
                                                })
                                                // Edit Rule Button
                                                .child(
                                                    div()
                                                        .id(ElementId::NamedInteger(
                                                            "edit-rule".into(),
                                                            r_idx as u64,
                                                        ))
                                                        .px(px(4.0))
                                                        .cursor_pointer()
                                                        .text_size(px(11.0))
                                                        .text_color(theme.text_muted)
                                                        .on_click(cx.listener(
                                                            move |this, _, window, cx| {
                                                                this.open_edit_filter_modal(
                                                                    r_idx, cx,
                                                                );
                                                                window.focus(&this.focus_handle);
                                                            },
                                                        ))
                                                        .child("✎"),
                                                )
                                                // Delete Rule Button
                                                .child(
                                                    div()
                                                        .id(ElementId::NamedInteger(
                                                            "del-rule".into(),
                                                            r_idx as u64,
                                                        ))
                                                        .px(px(4.0))
                                                        .cursor_pointer()
                                                        .text_size(px(11.0))
                                                        .text_color(theme.text_muted)
                                                        .on_click(cx.listener(
                                                            move |this, _, _window, cx| {
                                                                this.delete_rule(r_idx, cx);
                                                            },
                                                        ))
                                                        .child("✕"),
                                                ),
                                        )
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                ),
        )
}

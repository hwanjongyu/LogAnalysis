use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::components::tooltip::render_tooltip;
pub use crate::theme::rgba_to_rgb24;
use crate::theme::Theme;
use crate::types::{DraggedFilterRule, DropPosition, FilterDropTarget, FilterPanelPosition};
use crate::workspace::{compute_drop_target_index, LogAnalysisWorkspace};

/// Filters `rules` by `raw_query` using `.trim().to_lowercase()`, matching the DnD filter gate.
pub fn filter_matching_rules<'a>(
    rules: &'a [loganalysis_core::FilterRule],
    raw_query: &str,
) -> Vec<(usize, &'a loganalysis_core::FilterRule)> {
    let search_q = raw_query.trim().to_lowercase();
    rules
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            if search_q.is_empty() {
                true
            } else {
                r.text.to_lowercase().contains(&search_q)
            }
        })
        .collect()
}

/// Resolved visual styling and contrast telemetry for a single Filter Dock row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilterRowStyle {
    pub outer_bg: Rgba,
    pub strip_bg: Rgba,
    pub strip_fg: Rgba,
    pub left_accent_bar: Option<Rgba>,
    pub has_custom_bg: bool,
    pub contrast_level: loganalysis_core::ContrastLevel,
    pub contrast_ratio: f64,
    pub show_low_contrast_hint: bool,
}

/// Resolves WYSIWYG row colors, inactive contrast protection, exclude accenting,
/// and low-contrast telemetry for a filter rule in the Filter Dock.
pub fn resolve_filter_row_style(
    rule: &loganalysis_core::FilterRule,
    theme: Theme,
) -> FilterRowStyle {
    let bg_opt = loganalysis_core::parse_color_spec(&rule.bg_color, true);
    let fg_opt = loganalysis_core::parse_color_spec(&rule.text_color, false);
    let has_custom_bg = bg_opt.is_some();

    let outer_bg = if rule.active {
        theme.bg_card
    } else {
        theme.bg_panel
    };

    let strip_bg = if rule.active {
        if let Some(hex) = bg_opt {
            rgb(hex)
        } else if rule.exclude {
            theme.bg_dark.blend(Rgba {
                a: 0.14,
                ..theme.error_red
            })
        } else {
            theme.bg_dark
        }
    } else if let Some(hex) = bg_opt {
        theme.bg_panel.blend(Rgba {
            a: 0.28,
            ..rgb(hex)
        })
    } else if rule.exclude {
        theme.bg_dark.blend(Rgba {
            a: 0.06,
            ..theme.error_red
        })
    } else {
        theme.bg_panel
    };

    let strip_fg = if rule.active {
        fg_opt.map(rgb).unwrap_or(theme.text_primary)
    } else {
        let candidate_fg = if let Some(fg_hex) = fg_opt {
            theme.text_muted.blend(Rgba {
                a: 0.65,
                ..rgb(fg_hex)
            })
        } else {
            theme.text_muted
        };
        let bg_rgb24 = rgba_to_rgb24(strip_bg);
        if loganalysis_core::contrast_ratio(bg_rgb24, rgba_to_rgb24(candidate_fg)) >= 3.0 {
            candidate_fg
        } else {
            [theme.text_primary, rgb(0xffffff), rgb(0x000000)]
                .into_iter()
                .max_by(|a, b| {
                    let ca = loganalysis_core::contrast_ratio(bg_rgb24, rgba_to_rgb24(*a));
                    let cb = loganalysis_core::contrast_ratio(bg_rgb24, rgba_to_rgb24(*b));
                    ca.total_cmp(&cb)
                })
                .unwrap_or(theme.text_primary)
        }
    };

    let left_accent_bar = if rule.exclude {
        Some(theme.error_red)
    } else {
        None
    };

    let (contrast_ratio, contrast_level) =
        loganalysis_core::evaluate_contrast(rgba_to_rgb24(strip_bg), rgba_to_rgb24(strip_fg));

    let show_low_contrast_hint = rule.active
        && !rule.exclude
        && (bg_opt.is_some() || fg_opt.is_some())
        && contrast_level == loganalysis_core::ContrastLevel::Critical;

    FilterRowStyle {
        outer_bg,
        strip_bg,
        strip_fg,
        left_accent_bar,
        has_custom_bg,
        contrast_level,
        contrast_ratio,
        show_low_contrast_hint,
    }
}

/// Folds a 128-bit rule UUID into a stable 64-bit `ElementId` key across reorders.
#[inline]
pub fn filter_rule_element_key(rule_id: loganalysis_core::uuid::Uuid) -> u64 {
    let raw = rule_id.as_u128();
    (raw as u64) ^ ((raw >> 64) as u64)
}

/// Returns whether the inline Edit (`✎`) button should be rendered for the current dock geometry.
#[inline]
pub fn should_show_inline_edit_button(is_bottom_dock: bool, dock_width: f32) -> bool {
    is_bottom_dock || dock_width >= 230.0
}

/// Returns whether compact row spacing should be used in narrow Left Dock widths (< 230px).
#[inline]
pub fn is_compact_filter_row(is_bottom_dock: bool, dock_width: f32) -> bool {
    !is_bottom_dock && dock_width < 230.0
}

/// Returns horizontal slot padding in logical pixels for the current dock geometry.
#[inline]
fn filter_row_slot_px(is_bottom_dock: bool, dock_width: f32) -> f32 {
    if is_compact_filter_row(is_bottom_dock, dock_width) {
        4.0
    } else {
        8.0
    }
}

/// Normalizes painted slot or tail bounds into screen-space hit-test bounds by adding
/// `scroll_offset`, trimming `slot_px` horizontal padding on both sides, and removing any
/// asymmetric `extra_top_trim_px` (such as row 0's `5.0px` extra top padding) so slot
/// midpoints align identically with their inner `filter-row` midpoints.
#[inline]
pub fn normalize_slot_hit_bounds(
    raw_bounds: Bounds<Pixels>,
    scroll_offset: Point<Pixels>,
    slot_px: f32,
    extra_top_trim_px: f32,
) -> Bounds<Pixels> {
    let trim_x = px(slot_px.max(0.0));
    let trim_top = px(extra_top_trim_px.max(0.0));
    Bounds::new(
        Point::new(
            raw_bounds.origin.x + scroll_offset.x + trim_x,
            raw_bounds.origin.y + scroll_offset.y + trim_top,
        ),
        Size::new(
            (raw_bounds.size.width - trim_x * 2.0).max(px(0.0)),
            (raw_bounds.size.height - trim_top).max(px(0.0)),
        ),
    )
}

/// Determines `Before`/`After` drop position by comparing `mouse_y` to `row_bounds` midpoint.
#[inline]
pub fn resolve_drop_position_from_bounds(
    mouse_y: Pixels,
    row_bounds: Bounds<Pixels>,
) -> DropPosition {
    let mid_y = row_bounds.origin.y + row_bounds.size.height / 2.0;
    if mouse_y < mid_y {
        DropPosition::Before
    } else {
        DropPosition::After
    }
}

/// Resolves the hovered `FilterDropTarget` inside `scroll_bounds` from painted row/tail bounds.
/// Returns `None` if `pointer_pos` is outside `scroll_bounds` or in padding/scrollbar gutters.
pub fn resolve_drop_target_at_position(
    pointer_pos: Point<Pixels>,
    scroll_bounds: Bounds<Pixels>,
    row_screen_bounds: &[Bounds<Pixels>],
    tail_screen_bounds: Option<Bounds<Pixels>>,
) -> Option<FilterDropTarget> {
    if !scroll_bounds.contains(&pointer_pos) || row_screen_bounds.is_empty() {
        return None;
    }
    for (idx, bounds) in row_screen_bounds.iter().enumerate() {
        if bounds.size.width > px(0.0)
            && bounds.size.height > px(0.0)
            && bounds.contains(&pointer_pos)
        {
            return Some(FilterDropTarget {
                target_row_index: idx,
                position: resolve_drop_position_from_bounds(pointer_pos.y, *bounds),
            });
        }
    }
    if let Some(tail_bounds) = tail_screen_bounds {
        if tail_bounds.size.width > px(0.0)
            && tail_bounds.size.height > px(0.0)
            && tail_bounds.contains(&pointer_pos)
        {
            return Some(FilterDropTarget {
                target_row_index: row_screen_bounds.len() - 1,
                position: DropPosition::After,
            });
        }
    }
    None
}

/// Computes clamped scroll offset delta when dragging near the top/bottom edge of `scroll_bounds`.
/// Uses a dynamic edge zone `min(height * 0.25, 24px)` so minimal Bottom Dock heights never overlap.
pub fn compute_drag_auto_scroll_offset(
    pointer_y: Pixels,
    scroll_bounds: Bounds<Pixels>,
    current_offset: Point<Pixels>,
    max_offset: Size<Pixels>,
) -> Option<Point<Pixels>> {
    if scroll_bounds.size.height <= px(0.0) || max_offset.height <= px(0.0) {
        return None;
    }
    let edge_zone = (scroll_bounds.size.height * 0.25).min(px(24.0));
    let step = px(12.0);
    let next_y = if pointer_y < scroll_bounds.origin.y + edge_zone {
        (current_offset.y + step).min(px(0.0))
    } else if pointer_y > scroll_bounds.bottom() - edge_zone {
        (current_offset.y - step).max(-max_offset.height)
    } else {
        return None;
    };
    if (next_y - current_offset.y).abs() > px(0.5) {
        Some(Point::new(current_offset.x, next_y))
    } else {
        None
    }
}

/// Restricts drag auto-scrolling to pointers inside `scroll_bounds` (both horizontally and
/// vertically) so dragging over adjacent log viewports never scrolls the filter list.
#[inline]
pub fn resolve_drag_auto_scroll_in_viewport(
    pointer_pos: Point<Pixels>,
    scroll_bounds: Bounds<Pixels>,
    current_offset: Point<Pixels>,
    max_offset: Size<Pixels>,
) -> Option<Point<Pixels>> {
    if !scroll_bounds.contains(&pointer_pos) {
        return None;
    }
    compute_drag_auto_scroll_offset(pointer_pos.y, scroll_bounds, current_offset, max_offset)
}

struct DraggedFilterRuleView {
    payload: DraggedFilterRule,
    theme: Theme,
}

impl Render for DraggedFilterRuleView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let bg_opt = loganalysis_core::parse_color_spec(&self.payload.bg_color, true);
        let fg_opt = loganalysis_core::parse_color_spec(&self.payload.text_color, false);
        let bg = bg_opt.map(rgb).unwrap_or_else(|| {
            if self.payload.is_exclude {
                self.theme.bg_card.blend(Rgba {
                    a: 0.14,
                    ..self.theme.error_red
                })
            } else {
                self.theme.bg_card
            }
        });
        let fg = fg_opt.map(rgb).unwrap_or(self.theme.text_primary);

        div()
            .max_w(px(260.0))
            .overflow_hidden()
            .px(px(10.0))
            .py(px(5.0))
            .rounded_md()
            .bg(bg)
            .border_1()
            .border_color(self.theme.accent)
            .shadow_lg()
            .opacity(0.92)
            .flex()
            .items_center()
            .gap_1p5()
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .text_color(if bg_opt.is_some() {
                        fg
                    } else {
                        self.theme.text_muted
                    })
                    .child("⠿"),
            )
            .when(self.payload.is_exclude, |s| {
                s.child(
                    div()
                        .flex_shrink_0()
                        .px(px(3.0))
                        .py(px(0.5))
                        .rounded_sm()
                        .bg(self.theme.bg_badge)
                        .text_size(px(9.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(self.theme.error_red)
                        .child("NOT"),
                )
            })
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .font_family("monospace")
                    .text_size(px(11.5))
                    .text_color(fg)
                    .child(self.payload.pattern.clone()),
            )
    }
}

#[inline]
fn has_blocking_filter_drag_overlay_flags(
    filter_edit_modal_open: bool,
    overflow_menu_open: bool,
    command_palette_open: bool,
    bookmark_drawer_open: bool,
    bookmark_note_modal_open: bool,
    adb_device_selector_open: bool,
) -> bool {
    filter_edit_modal_open
        || overflow_menu_open
        || command_palette_open
        || bookmark_drawer_open
        || bookmark_note_modal_open
        || adb_device_selector_open
}

#[inline]
fn has_blocking_filter_drag_overlay(workspace: &LogAnalysisWorkspace) -> bool {
    has_blocking_filter_drag_overlay_flags(
        workspace.filter_edit_modal.is_open,
        workspace.is_overflow_menu_open,
        workspace.is_command_palette_open,
        workspace.bookmarks.is_drawer_open,
        workspace.bookmarks.is_note_modal_open,
        workspace.adb.is_device_selector_open,
    )
}

fn can_accept_filter_drop_in_tabs(
    has_blocking_overlay: bool,
    filter_search_query: &str,
    active_tab_index: usize,
    filter_tabs: &[loganalysis_core::FilterTabData],
    dragged: &DraggedFilterRule,
    target_rule: Option<(usize, loganalysis_core::uuid::Uuid)>,
) -> Option<usize> {
    if has_blocking_overlay
        || !filter_search_query.trim().is_empty()
        || dragged.tab_index != active_tab_index
    {
        return None;
    }
    let tab = filter_tabs.get(active_tab_index)?;
    if tab.filters.is_empty()
        || tab
            .filters
            .get(dragged.source_index)
            .is_none_or(|r| r.id != dragged.rule_id)
    {
        return None;
    }
    if let Some((r_idx, rule_id)) = target_rule {
        if tab.filters.get(r_idx).is_none_or(|r| r.id != rule_id) {
            return None;
        }
    }
    Some(tab.filters.len())
}

fn can_accept_filter_drop(
    workspace: &LogAnalysisWorkspace,
    dragged: &DraggedFilterRule,
    target_rule: Option<(usize, loganalysis_core::uuid::Uuid)>,
) -> Option<usize> {
    can_accept_filter_drop_in_tabs(
        has_blocking_filter_drag_overlay(workspace),
        &workspace.filter_search_query,
        workspace.active_tab_index,
        &workspace.filter_tabs,
        dragged,
        target_rule,
    )
}

fn render_filter_rule_search_box(
    workspace: &LogAnalysisWorkspace,
    input_id: &'static str,
    clear_id: &'static str,
    height_px: f32,
    width_px: Option<f32>,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> Stateful<Div> {
    let theme = workspace.theme;
    div()
        .id(input_id)
        .h(px(height_px))
        .when_some(width_px, |s, w| s.w(px(w)))
        .when(width_px.is_none(), |s| s.w_full())
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .flex_1()
                .h_full()
                .child(workspace.filter_rule_input.clone()),
        )
        .children(if !workspace.filter_search_query.is_empty() {
            Some(
                div()
                    .id(clear_id)
                    .px(px(4.0))
                    .cursor_pointer()
                    .text_size(px(10.0))
                    .text_color(theme.text_muted)
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.filter_rule_input
                            .update(cx, |inp, cx| inp.set_text("", cx));
                        this.filter_search_query.clear();
                        this.clear_filter_drag_state_fields();
                        cx.notify();
                    }))
                    .child("✕"),
            )
        } else {
            None
        })
}

#[allow(clippy::too_many_arguments)]
fn render_filter_rule_slot(
    workspace: &LogAnalysisWorkspace,
    vis_idx: usize,
    r_idx: usize,
    rule: &loganalysis_core::FilterRule,
    total_rules: usize,
    is_filtering_rules: bool,
    is_internal_rule_drag: bool,
    is_compact: bool,
    show_edit_btn: bool,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> AnyElement {
    let theme = workspace.theme;
    let active_tab_index = workspace.active_tab_index;
    let rule_id = rule.id;
    let key = filter_rule_element_key(rule_id);
    let slot_id = ElementId::NamedInteger("filter-row-slot".into(), key);
    let row_id = ElementId::NamedInteger("filter-row".into(), key);
    let grip_id = ElementId::NamedInteger("drag-rule-grip".into(), key);
    let chk_id = ElementId::NamedInteger("chk-rule".into(), key);
    let strip_id = ElementId::NamedInteger("pattern-strip".into(), key);
    let contrast_warn_id = ElementId::NamedInteger("contrast-warn".into(), key);
    let edit_id = ElementId::NamedInteger("edit-rule".into(), key);
    let del_id = ElementId::NamedInteger("del-rule".into(), key);
    let row_group: SharedString = format!("filter-row-group-{key}").into();

    let count = workspace
        .filter_result
        .as_ref()
        .map(|r| r.count_for_rule(&rule.id))
        .unwrap_or(0);
    let is_active = rule.active;
    let is_exclude = rule.exclude;
    let is_regex = rule.regex;
    let style = resolve_filter_row_style(rule, theme);
    let slot_px = filter_row_slot_px(
        workspace.filter_panel_position == FilterPanelPosition::Bottom,
        workspace.filter_dock_width,
    );

    let drop_pos_for_slot = if is_internal_rule_drag && !is_filtering_rules {
        workspace.filter_drag_drop_target.and_then(|target| {
            if target.target_row_index != r_idx {
                return None;
            }
            let src = workspace.dragging_filter_rule_index?;
            compute_drop_target_index(src, r_idx, target.position, total_rules)?;
            Some(target.position)
        })
    } else {
        None
    };

    let is_drag_source =
        is_internal_rule_drag && workspace.dragging_filter_rule_index == Some(r_idx);

    let row_bounds_cell = Rc::new(Cell::new(Bounds::<Pixels>::default()));
    let cell_for_prepaint = row_bounds_cell.clone();
    let cell_for_drop = row_bounds_cell;

    let workspace_weak = cx.entity().downgrade();
    let drag_payload = DraggedFilterRule {
        tab_index: active_tab_index,
        source_index: r_idx,
        rule_id,
        pattern: rule.text.clone().into(),
        is_exclude,
        bg_color: rule.bg_color.clone(),
        text_color: rule.text_color.clone(),
    };

    let grip_el = div()
        .id(grip_id)
        .w(px(if is_compact { 12.0 } else { 14.0 }))
        .h(px(if is_compact { 20.0 } else { 22.0 }))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .text_size(px(11.0))
        .text_color(theme.text_muted)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(|_, _, cx| cx.stop_propagation())
        .when(is_filtering_rules, |s| {
            s.opacity(0.25)
                .tooltip(render_tooltip("Clear search to reorder rules", None, theme))
        })
        .when(!is_filtering_rules, |s| {
            s.cursor(CursorStyle::OpenHand)
                .hover(move |h| h.text_color(theme.text_primary))
                .tooltip(render_tooltip("Drag to reorder rule", None, theme))
                .on_drag(drag_payload, move |payload, _offset, window, cx| {
                    let _ = workspace_weak.update(cx, |ws, cx| {
                        window.focus(&ws.focus_handle);
                        ws.dragging_filter_rule_index = Some(payload.source_index);
                        cx.notify();
                    });
                    cx.new(|_| DraggedFilterRuleView {
                        payload: payload.clone(),
                        theme,
                    })
                })
        })
        .child("⠿");

    let checkbox_el = div()
        .id(chk_id)
        .w(px(if is_compact { 18.0 } else { 22.0 }))
        .h(px(if is_compact { 20.0 } else { 22.0 }))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .tooltip(render_tooltip(
            if is_active {
                "Disable filter rule"
            } else {
                "Enable filter rule"
            },
            None,
            theme,
        ))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |this, _, _window, cx| {
            cx.stop_propagation();
            this.toggle_rule_active(r_idx, cx);
        }))
        .child(
            div()
                .w(px(15.0))
                .h(px(15.0))
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
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(0xffffff))
                .child(if is_active { "✓" } else { "" }),
        );

    let left_gutter_el = div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .when(!is_compact, |s| s.px(px(4.0)).gap_1())
        .when(is_compact, |s| s.px(px(3.0)).gap(px(2.0)))
        .bg(theme.bg_panel)
        .when(!style.has_custom_bg, |s| {
            s.group_hover(row_group.clone(), move |h| h.bg(theme.btn_hover))
        })
        .child(grip_el)
        .child(checkbox_el);

    let (warn_bg, warn_fg) =
        crate::theme::severity_badge_colors(Some('W'), workspace.is_dark_theme);

    let pattern_strip_el = div()
        .id(strip_id)
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_between()
        .gap_1()
        .pl(px(if is_compact { 4.0 } else { 8.0 }))
        .pr(px(if is_compact { 4.0 } else { 6.0 }))
        .py(px(5.0))
        .bg(style.strip_bg)
        .border_l_1()
        .border_r_1()
        .border_color(theme.border_subtle)
        .when(is_compact && style.show_low_contrast_hint, |s| {
            s.tooltip(render_tooltip(
                format!(
                    "Low text contrast ({:.1}:1) — double-click to edit",
                    style.contrast_ratio
                ),
                None,
                theme,
            ))
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .font_family("monospace")
                .text_size(px(11.5))
                .text_color(style.strip_fg)
                .child(rule.text.clone()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap(px(if is_compact { 2.0 } else { 4.0 }))
                .when(!is_compact && style.show_low_contrast_hint, |s| {
                    s.child(
                        div()
                            .id(contrast_warn_id)
                            .flex_shrink_0()
                            .px(px(3.5))
                            .py(px(0.5))
                            .rounded_sm()
                            .bg(warn_bg)
                            .border_1()
                            .border_color(theme.warning_amber)
                            .text_size(px(9.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(warn_fg)
                            .tooltip(render_tooltip(
                                format!(
                                    "Low text contrast ({:.1}:1) — double-click to edit",
                                    style.contrast_ratio
                                ),
                                None,
                                theme,
                            ))
                            .child("⚠"),
                    )
                })
                .when(is_regex, |s| {
                    s.child(
                        div()
                            .px(px(if is_compact { 2.5 } else { 4.0 }))
                            .py(px(1.0))
                            .rounded_sm()
                            .bg(theme.bg_badge)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(px(9.0))
                            .text_color(theme.accent)
                            .child(".*"),
                    )
                })
                .when(is_exclude, |s| {
                    s.child(
                        div()
                            .px(px(if is_compact { 3.0 } else { 4.0 }))
                            .py(px(1.0))
                            .rounded_sm()
                            .bg(theme.bg_badge)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.error_red)
                            .child(if is_compact { "!" } else { "NOT" }),
                    )
                })
                .child(
                    div()
                        .px(px(if is_compact { 4.0 } else { 6.0 }))
                        .py(px(1.0))
                        .rounded_full()
                        .bg(theme.bg_badge)
                        .border_1()
                        .border_color(theme.border)
                        .font_family("monospace")
                        .text_size(px(if is_compact { 9.5 } else { 10.0 }))
                        .text_color(if count > 0 {
                            theme.text_primary
                        } else {
                            theme.text_muted
                        })
                        .child(format!("{}", count)),
                ),
        );

    let right_controls_el = div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .when(!is_compact, |s| s.px(px(4.0)).gap_1())
        .when(is_compact, |s| s.px(px(3.0)))
        .bg(theme.bg_panel)
        .when(!style.has_custom_bg, |s| {
            s.group_hover(row_group.clone(), move |h| h.bg(theme.btn_hover))
        })
        .when(show_edit_btn, |s| {
            s.child(
                div()
                    .id(edit_id)
                    .px(px(4.0))
                    .py(px(2.0))
                    .rounded_sm()
                    .cursor_pointer()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .opacity(0.65)
                    .group_hover(row_group.clone(), |h| h.opacity(1.0))
                    .hover(move |h| h.text_color(theme.accent).opacity(1.0))
                    .tooltip(render_tooltip("Edit filter rule", None, theme))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_edit_filter_modal(r_idx, cx);
                        window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                    }))
                    .child("✎"),
            )
        })
        .child(
            div()
                .id(del_id)
                .px(px(if is_compact { 2.0 } else { 4.0 }))
                .py(px(2.0))
                .rounded_sm()
                .cursor_pointer()
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .opacity(0.65)
                .group_hover(row_group.clone(), |h| h.opacity(1.0))
                .hover(move |h| h.text_color(theme.error_red).opacity(1.0))
                .tooltip(render_tooltip("Delete filter rule", None, theme))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, _window, cx| {
                    cx.stop_propagation();
                    this.delete_rule(r_idx, cx);
                }))
                .child("✕"),
        );

    let row_el = div()
        .id(row_id)
        .group(row_group)
        .w_full()
        .min_h(px(30.0))
        .flex()
        .rounded_md()
        .overflow_hidden()
        .bg(style.outer_bg)
        .border_1()
        .border_color(if is_active {
            theme.border
        } else {
            theme.border_subtle
        })
        .when(is_drag_source, |s| s.opacity(0.4))
        .hover(move |s| {
            if !style.has_custom_bg {
                s.bg(theme.btn_hover)
            } else {
                s.border_color(theme.accent)
            }
        })
        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
            if event.standard_click() && event.click_count() == 2 {
                this.open_edit_filter_modal(r_idx, cx);
                window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                cx.stop_propagation();
            }
        }))
        .when_some(style.left_accent_bar, |s, bar_color| {
            s.child(div().w(px(3.0)).flex_shrink_0().bg(bar_color))
        })
        .child(left_gutter_el)
        .child(pattern_strip_el)
        .child(right_controls_el);

    div()
        .relative()
        .w_full()
        .px(px(slot_px))
        .py(px(3.0))
        .when(vis_idx == 0, |s| s.pt(px(8.0)))
        .on_children_prepainted(move |children_bounds, _window, _cx| {
            if let Some(&row_bounds) = children_bounds.first() {
                cell_for_prepaint.set(row_bounds);
            }
        })
        .id(slot_id)
        .on_drop::<DraggedFilterRule>(cx.listener(
            move |this, dragged: &DraggedFilterRule, window, cx| {
                let Some(len) = can_accept_filter_drop(this, dragged, Some((r_idx, rule_id)))
                else {
                    this.clear_filter_drag_state(cx);
                    return;
                };
                let row_bounds = cell_for_drop.get();
                let pos = if row_bounds.size.height > px(0.0) {
                    resolve_drop_position_from_bounds(window.mouse_position().y, row_bounds)
                } else {
                    DropPosition::After
                };
                if let Some(dest) =
                    compute_drop_target_index(dragged.source_index, r_idx, pos, len)
                {
                    this.reorder_filter_rule(dragged.source_index, dest, cx);
                } else {
                    this.clear_filter_drag_state(cx);
                }
            },
        ))
        .child(row_el)
        .when_some(drop_pos_for_slot, |s, pos| {
            s.child(
                div()
                    .absolute()
                    .left(px(slot_px))
                    .right(px(slot_px))
                    .h(px(2.0))
                    .rounded_full()
                    .bg(theme.accent)
                    .when(pos == DropPosition::Before, |b| {
                        b.top(px(if vis_idx == 0 { 5.0 } else { 0.0 }))
                    })
                    .when(pos == DropPosition::After, |b| b.bottom(px(0.0))),
            )
        })
        .into_any_element()
}

/// Renders an interactive splitter/divider between Filter Dock and Log Viewport.
pub fn render_filter_dock_splitter(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let is_bottom_dock = workspace.filter_panel_position == FilterPanelPosition::Bottom;
    let is_resizing = workspace.is_resizing_filter_dock;

    div()
        .id("filter-dock-splitter")
        .when(!is_bottom_dock, |s| {
            s.w(px(4.0))
                .h_full()
                .cursor_col_resize()
                .bg(if is_resizing {
                    theme.accent
                } else {
                    theme.border
                })
                .hover(|s| s.bg(theme.accent))
        })
        .when(is_bottom_dock, |s| {
            s.w_full()
                .h(px(4.0))
                .flex_shrink_0()
                .cursor_row_resize()
                .bg(if is_resizing {
                    theme.accent
                } else {
                    theme.border
                })
                .hover(|s| s.bg(theme.accent))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                if event.click_count == 2 {
                    this.reset_filter_dock_size(cx);
                } else {
                    this.start_resizing_filter_dock(event.position, cx);
                }
            }),
        )
}

/// Renders the Filter Rules Dock panel (tabs, search, and reorderable color-coded rules list).
pub fn render_filter_dock(
    workspace: &LogAnalysisWorkspace,
    cx: &mut Context<LogAnalysisWorkspace>,
) -> impl IntoElement {
    let theme = workspace.theme;
    let is_bottom_dock = workspace.filter_panel_position == FilterPanelPosition::Bottom;
    let is_compact = is_compact_filter_row(is_bottom_dock, workspace.filter_dock_width);
    let show_edit_btn =
        should_show_inline_edit_button(is_bottom_dock, workspace.filter_dock_width);
    let show_bottom_wins_hint = is_bottom_dock || workspace.filter_dock_width >= 300.0;
    let slot_px = filter_row_slot_px(is_bottom_dock, workspace.filter_dock_width);
    let is_internal_rule_drag =
        cx.has_active_drag() && workspace.dragging_filter_rule_index.is_some();

    div()
        .id("filter-panel-container")
        .bg(theme.bg_panel)
        .flex()
        .flex_col()
        .overflow_hidden()
        .when(!is_bottom_dock, |s| {
            s.w(px(workspace.filter_dock_width)).h_full()
        })
        .when(is_bottom_dock, |s| {
            s.w_full().h(px(workspace.filter_dock_height)).flex_shrink_0()
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
                                .tooltip(render_tooltip("Add a new filter rule tab", None, theme))
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
                                .tooltip(render_tooltip(
                                    "Delete currently active tab",
                                    None,
                                    theme,
                                ))
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
                        .min_w_0()
                        .overflow_hidden()
                        .child(
                            div()
                                .id("filter-rules-heading")
                                .flex_shrink_0()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_muted)
                                .tooltip(render_tooltip(
                                    "Last-match-wins: bottom-most matching rule takes precedence",
                                    None,
                                    theme,
                                ))
                                .child("FILTER RULES"),
                        )
                        .when(show_bottom_wins_hint, |s| {
                            s.child(
                                div()
                                    .id("filter-rules-precedence-hint")
                                    .flex_shrink_0()
                                    .text_size(px(9.5))
                                    .text_color(theme.text_muted)
                                    .tooltip(render_tooltip(
                                        "Last-match-wins: bottom-most matching rule takes precedence",
                                        None,
                                        theme,
                                    ))
                                    .child("Bottom wins"),
                            )
                        })
                        .children(if is_bottom_dock {
                            Some(render_filter_rule_search_box(
                                workspace,
                                "input-filter-rule-search-inline",
                                "btn-clear-filter-search-inline",
                                24.0,
                                Some(220.0),
                                cx,
                            ))
                        } else {
                            None
                        }),
                )
                .child(
                    div()
                        .id("btn-add-filter-rule")
                        .flex_shrink_0()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded_md()
                        .bg(theme.accent)
                        .text_color(rgb(0xffffff))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .tooltip(render_tooltip(
                            "Create new filter rule",
                            Some("Ctrl+Shift+F"),
                            theme,
                        ))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_add_filter_modal(cx);
                            window.focus(&this.modal_pattern_input.read(cx).focus_handle);
                        }))
                        .child("+ New Rule")
                        .child(crate::components::toolbar::render_kbd_badge_on_accent(
                            "Ctrl+Shift+F",
                        )),
                ),
        )
        // In Left Sidebar mode, display the full-width dedicated search box
        .children(if !is_bottom_dock {
            Some(
                div()
                    .p(px(8.0))
                    .border_b_1()
                    .border_color(theme.border_subtle)
                    .child(render_filter_rule_search_box(
                        workspace,
                        "input-filter-rule-search",
                        "btn-clear-filter-search",
                        28.0,
                        None,
                        cx,
                    )),
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
                .track_scroll(&workspace.filter_rules_scroll_handle)
                .flex()
                .flex_col()
                .gap_0()
                .on_drag_move::<DraggedFilterRule>(cx.listener(
                    |this, event: &DragMoveEvent<DraggedFilterRule>, _window, cx| {
                        let dragged = event.drag(cx);
                        let is_invalid_state = has_blocking_filter_drag_overlay(this)
                            || !this.filter_search_query.trim().is_empty()
                            || this.dragging_filter_rule_index.is_none()
                            || dragged.tab_index != this.active_tab_index;
                        if is_invalid_state {
                            if this.filter_drag_drop_target.take().is_some() {
                                cx.notify();
                            }
                            return;
                        }

                        let Some(tab) = this.filter_tabs.get(this.active_tab_index) else {
                            if this.filter_drag_drop_target.take().is_some() {
                                cx.notify();
                            }
                            return;
                        };
                        let rule_count = tab.filters.len();
                        if rule_count == 0
                            || tab
                                .filters
                                .get(dragged.source_index)
                                .is_none_or(|r| r.id != dragged.rule_id)
                        {
                            if this.filter_drag_drop_target.take().is_some() {
                                cx.notify();
                            }
                            return;
                        }

                        let scroll_bounds = event.bounds;
                        let scroll_offset = this.filter_rules_scroll_handle.offset();
                        let slot_px = filter_row_slot_px(
                            this.filter_panel_position == FilterPanelPosition::Bottom,
                            this.filter_dock_width,
                        );
                        let children_count = this.filter_rules_scroll_handle.children_count();
                        let row_limit = rule_count.min(children_count);
                        let mut row_screen_bounds = Vec::with_capacity(row_limit);
                        for ix in 0..row_limit {
                            let Some(raw_bounds) =
                                this.filter_rules_scroll_handle.bounds_for_item(ix)
                            else {
                                if this.filter_drag_drop_target.take().is_some() {
                                    cx.notify();
                                }
                                return;
                            };
                            let extra_top_trim = if ix == 0 { 5.0 } else { 0.0 };
                            row_screen_bounds.push(normalize_slot_hit_bounds(
                                raw_bounds,
                                scroll_offset,
                                slot_px,
                                extra_top_trim,
                            ));
                        }
                        let tail_screen_bounds = if children_count > rule_count {
                            this.filter_rules_scroll_handle
                                .bounds_for_item(rule_count)
                                .map(|raw_bounds| {
                                    normalize_slot_hit_bounds(raw_bounds, scroll_offset, slot_px, 0.0)
                                })
                        } else {
                            None
                        };

                        let next_target = resolve_drop_target_at_position(
                            event.event.position,
                            scroll_bounds,
                            &row_screen_bounds,
                            tail_screen_bounds,
                        );

                        let mut changed = false;
                        if this.filter_drag_drop_target != next_target {
                            this.filter_drag_drop_target = next_target;
                            changed = true;
                        }

                        if let Some(next_offset) = resolve_drag_auto_scroll_in_viewport(
                            event.event.position,
                            scroll_bounds,
                            scroll_offset,
                            this.filter_rules_scroll_handle.max_offset(),
                        ) {
                            this.filter_rules_scroll_handle.set_offset(next_offset);
                            changed = true;
                        }

                        if changed {
                            cx.notify();
                        }
                    },
                ))
                .children(
                    workspace
                        .filter_tabs
                        .get(workspace.active_tab_index)
                        .map(|tab| {
                            if tab.filters.is_empty() {
                                return vec![div()
                                    .id("filter-dock-empty-state")
                                    .w_full()
                                    .py(px(24.0))
                                    .px(px(12.0))
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .gap_2()
                                    .child(div().text_size(px(24.0)).child("🎯"))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_size(px(12.0))
                                            .text_color(theme.text_primary)
                                            .child("No Filter Rules"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(theme.text_muted)
                                            .child(
                                                "Click '+ New Rule' or select log text to filter.",
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("btn-empty-add-rule")
                                            .mt(px(4.0))
                                            .px(px(12.0))
                                            .py(px(4.0))
                                            .rounded_md()
                                            .bg(theme.accent)
                                            .hover(move |s| s.bg(theme.btn_hover))
                                            .text_color(rgb(0xffffff))
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.open_add_filter_modal(cx);
                                                window.focus(
                                                    &this.modal_pattern_input.read(cx).focus_handle,
                                                );
                                            }))
                                            .child("+ Add Rule")
                                            .child(
                                                crate::components::toolbar::render_kbd_badge_on_accent(
                                                    "Ctrl+Shift+F",
                                                ),
                                            ),
                                    )
                                    .into_any_element()];
                            }

                            let is_filtering_rules =
                                !workspace.filter_search_query.trim().is_empty();
                            let total_rules = tab.filters.len();
                            let matching_rules = filter_matching_rules(
                                &tab.filters,
                                &workspace.filter_search_query,
                            );

                            if matching_rules.is_empty() {
                                return vec![div()
                                    .id("filter-dock-no-match-state")
                                    .w_full()
                                    .py(px(20.0))
                                    .px(px(8.0))
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(theme.text_muted)
                                            .child(format!(
                                                "No rules match \"{}\"",
                                                workspace.filter_search_query
                                            )),
                                    )
                                    .child(
                                        div()
                                            .id("btn-empty-clear-rules-search")
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded_sm()
                                            .bg(theme.bg_card)
                                            .hover(move |s| s.bg(theme.btn_hover))
                                            .text_size(px(10.5))
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.filter_rule_input
                                                    .update(cx, |inp, cx| inp.set_text("", cx));
                                                this.filter_search_query.clear();
                                                this.clear_filter_drag_state_fields();
                                                cx.notify();
                                            }))
                                            .child("Clear Search (✕)"),
                                    )
                                    .into_any_element()];
                            }

                            let mut elements: Vec<AnyElement> = matching_rules
                                .into_iter()
                                .enumerate()
                                .map(|(vis_idx, (r_idx, rule))| {
                                    render_filter_rule_slot(
                                        workspace,
                                        vis_idx,
                                        r_idx,
                                        rule,
                                        total_rules,
                                        is_filtering_rules,
                                        is_internal_rule_drag,
                                        is_compact,
                                        show_edit_btn,
                                        cx,
                                    )
                                })
                                .collect();

                            elements.push(
                                div()
                                    .id("filter-rules-tail-drop")
                                    .w_full()
                                    .flex_1()
                                    .px(px(slot_px))
                                    .on_drop::<DraggedFilterRule>(cx.listener(
                                        move |this, dragged: &DraggedFilterRule, _window, cx| {
                                            let Some(len) =
                                                can_accept_filter_drop(this, dragged, None)
                                            else {
                                                this.clear_filter_drag_state(cx);
                                                return;
                                            };
                                            if let Some(dest) = compute_drop_target_index(
                                                dragged.source_index,
                                                len - 1,
                                                DropPosition::After,
                                                len,
                                            ) {
                                                this.reorder_filter_rule(
                                                    dragged.source_index,
                                                    dest,
                                                    cx,
                                                );
                                            } else {
                                                this.clear_filter_drag_state(cx);
                                            }
                                        },
                                    ))
                                    .into_any_element(),
                            );

                            elements
                        })
                        .unwrap_or_default(),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        can_accept_filter_drop_in_tabs, compute_drag_auto_scroll_offset, filter_rule_element_key,
        has_blocking_filter_drag_overlay_flags, is_compact_filter_row, normalize_slot_hit_bounds,
        resolve_drag_auto_scroll_in_viewport, resolve_drop_position_from_bounds,
        resolve_drop_target_at_position, resolve_filter_row_style, rgba_to_rgb24,
        should_show_inline_edit_button,
    };
    use crate::theme::Theme;
    use crate::types::{DraggedFilterRule, DropPosition, FilterDropTarget};
    use gpui::{px, rgb, Bounds, Point, Size};
    use loganalysis_core::{ContrastLevel, FilterRule, FilterTabData};

    #[test]
    fn test_resolve_filter_row_style_none_and_default_colors() {
        for theme in [Theme::dark(), Theme::light()] {
            for (bg_color, text_color) in [
                ("None", "Default"),
                ("", ""),
                ("none", "None"),
            ] {
                let rule = FilterRule {
                    text: "ActivityManager".into(),
                    bg_color: bg_color.into(),
                    text_color: text_color.into(),
                    active: true,
                    exclude: false,
                    ..Default::default()
                };
                let style = resolve_filter_row_style(&rule, theme);
                assert_eq!(style.strip_bg, theme.bg_dark);
                assert_eq!(style.strip_fg, theme.text_primary);
                assert!(!style.has_custom_bg);
                assert!(style.left_accent_bar.is_none());
                assert!(!style.show_low_contrast_hint);
            }
        }
    }

    #[test]
    fn test_resolve_filter_row_style_wysiwyg_matches_log_view() {
        for theme in [Theme::dark(), Theme::light()] {
            let rule = FilterRule {
                text: "Fatal".into(),
                bg_color: "Red".into(),
                text_color: "Default".into(),
                active: true,
                exclude: false,
                ..Default::default()
            };
            let style = resolve_filter_row_style(&rule, theme);
            let expected_bg = rgb(loganalysis_core::parse_color_spec("Red", true).unwrap());
            assert_eq!(style.strip_bg, expected_bg);
            assert_eq!(style.strip_fg, theme.text_primary);
            assert!(style.has_custom_bg);
        }
    }

    #[test]
    fn test_resolve_filter_row_style_inactive_yellow_black_contrast_regression() {
        let rule = FilterRule {
            text: "Exception".into(),
            bg_color: "Yellow".into(),
            text_color: "Black".into(),
            active: false,
            exclude: false,
            ..Default::default()
        };
        for theme in [Theme::dark(), Theme::light()] {
            let style = resolve_filter_row_style(&rule, theme);
            assert!(
                style.contrast_ratio >= 3.0,
                "Inactive Yellow/Black rule contrast {:.2}:1 must stay >= 3.0:1",
                style.contrast_ratio
            );
            assert!(!style.show_low_contrast_hint);
        }
    }

    #[test]
    fn test_resolve_filter_row_style_exclude_tint_and_accent() {
        for theme in [Theme::dark(), Theme::light()] {
            let active_rule = FilterRule {
                text: "Noise".into(),
                bg_color: "None".into(),
                text_color: "Default".into(),
                active: true,
                exclude: true,
                ..Default::default()
            };
            let active_style = resolve_filter_row_style(&active_rule, theme);
            assert_eq!(active_style.left_accent_bar, Some(theme.error_red));
            assert!(!active_style.has_custom_bg);
            assert_ne!(active_style.strip_bg, theme.bg_dark);
            assert_eq!(
                active_style.strip_bg,
                theme.bg_dark.blend(gpui::Rgba {
                    a: 0.14,
                    ..theme.error_red
                })
            );
            assert!(!active_style.show_low_contrast_hint);

            let inactive_rule = FilterRule {
                active: false,
                ..active_rule
            };
            let inactive_style = resolve_filter_row_style(&inactive_rule, theme);
            assert_eq!(inactive_style.left_accent_bar, Some(theme.error_red));
            assert_eq!(
                inactive_style.strip_bg,
                theme.bg_dark.blend(gpui::Rgba {
                    a: 0.06,
                    ..theme.error_red
                })
            );
        }
    }

    #[test]
    fn test_resolve_filter_row_style_low_contrast_hint() {
        let dark = Theme::dark();
        let low_contrast_rule = FilterRule {
            text: "DarkOnDark".into(),
            bg_color: "None".into(),
            text_color: "Black".into(),
            active: true,
            exclude: false,
            ..Default::default()
        };
        let style = resolve_filter_row_style(&low_contrast_rule, dark);
        assert_eq!(style.contrast_level, ContrastLevel::Critical);
        assert!(style.contrast_ratio < 3.0);
        assert!(style.show_low_contrast_hint);
        // WYSIWYG [INV-11]: active colors are preserved without silent distortion
        assert_eq!(rgba_to_rgb24(style.strip_fg), 0x000000);
    }

    #[test]
    fn test_filter_rule_element_key_and_compact_thresholds() {
        let r1 = FilterRule::default();
        let r2 = FilterRule::default();
        assert_ne!(
            filter_rule_element_key(r1.id),
            filter_rule_element_key(r2.id)
        );
        assert_eq!(
            filter_rule_element_key(r1.id),
            filter_rule_element_key(r1.id)
        );

        // Left Dock boundary checks: 180.0, 229.9, 230.0, 310.0
        assert!(is_compact_filter_row(false, 180.0));
        assert!(is_compact_filter_row(false, 229.9));
        assert!(!is_compact_filter_row(false, 230.0));
        assert!(!is_compact_filter_row(false, 310.0));

        assert!(!should_show_inline_edit_button(false, 180.0));
        assert!(!should_show_inline_edit_button(false, 229.9));
        assert!(should_show_inline_edit_button(false, 230.0));
        assert!(should_show_inline_edit_button(false, 310.0));

        // Bottom Dock is never compact and always shows inline edit button even if narrow width stored
        assert!(!is_compact_filter_row(true, 180.0));
        assert!(should_show_inline_edit_button(true, 180.0));
    }

    #[test]
    fn test_resolve_drop_target_at_position_and_auto_scroll() {
        let scroll_bounds = Bounds::new(Point::new(px(10.0), px(100.0)), Size::new(px(280.0), px(200.0)));
        let rows = [
            Bounds::new(Point::new(px(18.0), px(108.0)), Size::new(px(254.0), px(36.0))),
            Bounds::new(Point::new(px(18.0), px(144.0)), Size::new(px(254.0), px(36.0))),
        ];
        let tail = Bounds::new(Point::new(px(18.0), px(180.0)), Size::new(px(254.0), px(120.0)));

        // Direct midpoint helper
        assert_eq!(
            resolve_drop_position_from_bounds(px(120.0), rows[0]),
            DropPosition::Before
        );
        assert_eq!(
            resolve_drop_position_from_bounds(px(130.0), rows[0]),
            DropPosition::After
        );

        // Upper/lower halves of rows
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(100.0), px(115.0)),
                scroll_bounds,
                &rows,
                Some(tail)
            ),
            Some(FilterDropTarget {
                target_row_index: 0,
                position: DropPosition::Before
            })
        );
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(100.0), px(168.0)),
                scroll_bounds,
                &rows,
                Some(tail)
            ),
            Some(FilterDropTarget {
                target_row_index: 1,
                position: DropPosition::After
            })
        );

        // Tail drop zone -> After last row
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(100.0), px(240.0)),
                scroll_bounds,
                &rows,
                Some(tail)
            ),
            Some(FilterDropTarget {
                target_row_index: 1,
                position: DropPosition::After
            })
        );

        // Left/right gutters & scrollbar gutter & outside scroll bounds -> None
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(12.0), px(120.0)),
                scroll_bounds,
                &rows,
                Some(tail)
            ),
            None
        );
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(282.0), px(120.0)),
                scroll_bounds,
                &rows,
                Some(tail)
            ),
            None
        );
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(100.0), px(90.0)),
                scroll_bounds,
                &rows,
                Some(tail)
            ),
            None
        );

        // Auto-scroll with minimal Bottom Dock height (e.g. 40px -> dynamic edge_zone = 10px)
        let small_scroll_bounds =
            Bounds::new(Point::new(px(0.0), px(100.0)), Size::new(px(300.0), px(40.0)));
        let max_off = Size::new(px(0.0), px(60.0));
        // Middle of 40px viewport (y = 120) must NOT trigger edge scroll
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(120.0),
                small_scroll_bounds,
                Point::new(px(0.0), px(-20.0)),
                max_off
            ),
            None
        );
        // Top edge (y = 105 < 110) scrolls up clamped to 0.0
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(105.0),
                small_scroll_bounds,
                Point::new(px(0.0), px(-8.0)),
                max_off
            ),
            Some(Point::new(px(0.0), px(0.0)))
        );
        // Bottom edge (y = 135 > 130) scrolls down clamped to -60.0
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(135.0),
                small_scroll_bounds,
                Point::new(px(0.0), px(-55.0)),
                max_off
            ),
            Some(Point::new(px(0.0), px(-60.0)))
        );
        // Already at limit -> None
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(105.0),
                small_scroll_bounds,
                Point::new(px(0.0), px(0.0)),
                max_off
            ),
            None
        );
    }

    #[test]
    fn test_resolve_filter_row_style_light_vs_dark_neutral_strip_and_inactive() {
        for theme in [Theme::dark(), Theme::light()] {
            let active_neutral = FilterRule {
                text: "Neutral".into(),
                bg_color: "None".into(),
                text_color: "Default".into(),
                active: true,
                exclude: false,
                ..Default::default()
            };
            let active_style = resolve_filter_row_style(&active_neutral, theme);
            assert_eq!(active_style.outer_bg, theme.bg_card);
            assert_eq!(active_style.strip_bg, theme.bg_dark);
            assert_eq!(active_style.strip_fg, theme.text_primary);
            assert!(!active_style.has_custom_bg);
            assert!(!active_style.show_low_contrast_hint);

            let inactive_neutral = FilterRule {
                active: false,
                ..active_neutral
            };
            let inactive_style = resolve_filter_row_style(&inactive_neutral, theme);
            assert_eq!(inactive_style.outer_bg, theme.bg_panel);
            assert_eq!(inactive_style.strip_bg, theme.bg_panel);
            assert_eq!(inactive_style.strip_fg, theme.text_muted);
            assert!(!inactive_style.has_custom_bg);
            assert!(!inactive_style.show_low_contrast_hint);
        }

        // Light theme keeps outer/strip neutral surfaces distinct from bg_panel on active rows
        let light = Theme::light();
        assert_ne!(light.bg_dark, light.bg_panel);
        assert_eq!(light.bg_card, light.bg_dark);
    }

    #[test]
    fn test_resolve_filter_row_style_low_contrast_and_exclude_variants() {
        // Light theme active White-on-None low contrast preserves WYSIWYG and raises hint
        let light = Theme::light();
        let light_low_contrast = FilterRule {
            text: "LightOnLight".into(),
            bg_color: "None".into(),
            text_color: "White".into(),
            active: true,
            exclude: false,
            ..Default::default()
        };
        let light_style = resolve_filter_row_style(&light_low_contrast, light);
        assert_eq!(light_style.contrast_level, ContrastLevel::Critical);
        assert!(light_style.contrast_ratio < 3.0);
        assert!(light_style.show_low_contrast_hint);
        assert_eq!(rgba_to_rgb24(light_style.strip_fg), 0xffffff);

        // Exclude rules suppress low-contrast warning badge while preserving accent bar
        let dark = Theme::dark();
        let exclude_low_contrast = FilterRule {
            text: "ExcludedDark".into(),
            bg_color: "None".into(),
            text_color: "Black".into(),
            active: true,
            exclude: true,
            ..Default::default()
        };
        let excl_style = resolve_filter_row_style(&exclude_low_contrast, dark);
        assert_eq!(excl_style.left_accent_bar, Some(dark.error_red));
        assert!(!excl_style.show_low_contrast_hint);

        // Exclude rule with explicit custom background keeps custom fill + accent bar
        let exclude_custom_bg = FilterRule {
            text: "ExcludedRed".into(),
            bg_color: "Red".into(),
            text_color: "White".into(),
            active: true,
            exclude: true,
            ..Default::default()
        };
        let excl_custom_style = resolve_filter_row_style(&exclude_custom_bg, dark);
        assert!(excl_custom_style.has_custom_bg);
        assert_eq!(
            excl_custom_style.strip_bg,
            rgb(loganalysis_core::parse_color_spec("Red", true).unwrap())
        );
        assert_eq!(excl_custom_style.left_accent_bar, Some(dark.error_red));

        // Inactive White-on-White auto-corrects contrast >= 3.0 in both themes
        let inactive_white_on_white = FilterRule {
            text: "DimWhite".into(),
            bg_color: "White".into(),
            text_color: "White".into(),
            active: false,
            exclude: false,
            ..Default::default()
        };
        for theme in [dark, light] {
            let style = resolve_filter_row_style(&inactive_white_on_white, theme);
            assert!(
                style.contrast_ratio >= 3.0,
                "Inactive White/White contrast {:.2}:1 must stay >= 3.0:1",
                style.contrast_ratio
            );
            assert!(!style.show_low_contrast_hint);
        }
    }

    #[test]
    fn test_resolve_drop_position_midpoint_and_edge_geometry() {
        let row = Bounds::new(Point::new(px(18.0), px(108.0)), Size::new(px(254.0), px(36.0)));
        // Exact midpoint (108 + 18 = 126) -> After; just below -> Before
        assert_eq!(
            resolve_drop_position_from_bounds(px(126.0), row),
            DropPosition::After
        );
        assert_eq!(
            resolve_drop_position_from_bounds(px(125.99), row),
            DropPosition::Before
        );
        assert_eq!(
            resolve_drop_position_from_bounds(px(108.0), row),
            DropPosition::Before
        );
        assert_eq!(
            resolve_drop_position_from_bounds(px(144.0), row),
            DropPosition::After
        );

        let zero_height_row =
            Bounds::new(Point::new(px(0.0), px(50.0)), Size::new(px(200.0), px(0.0)));
        assert_eq!(
            resolve_drop_position_from_bounds(px(50.0), zero_height_row),
            DropPosition::After
        );

        let scroll_bounds =
            Bounds::new(Point::new(px(0.0), px(100.0)), Size::new(px(300.0), px(400.0)));
        let tail = Bounds::new(Point::new(px(8.0), px(200.0)), Size::new(px(284.0), px(200.0)));
        // Empty rows must return None even when tail bounds contain the pointer
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(50.0), px(250.0)),
                scroll_bounds,
                &[],
                Some(tail),
            ),
            None
        );

        // Zero-sized row/tail bounds must be ignored
        let zero_rows = [Bounds::new(
            Point::new(px(8.0), px(110.0)),
            Size::new(px(284.0), px(0.0)),
        )];
        let zero_tail = Bounds::new(Point::new(px(8.0), px(110.0)), Size::new(px(284.0), px(0.0)));
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(50.0), px(110.0)),
                scroll_bounds,
                &zero_rows,
                Some(zero_tail),
            ),
            None
        );
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(50.0), px(250.0)),
                scroll_bounds,
                &[row],
                None,
            ),
            None
        );

        // Tall 400px viewport caps edge_zone at 24px (not 100px = 400 * 0.25)
        let max_off = Size::new(px(0.0), px(120.0));
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(125.0), // origin.y + 25.0 > origin.y + 24.0
                scroll_bounds,
                Point::new(px(0.0), px(-40.0)),
                max_off
            ),
            None
        );
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(123.0), // inside top 24px edge zone
                scroll_bounds,
                Point::new(px(0.0), px(-40.0)),
                max_off
            ),
            Some(Point::new(px(0.0), px(-28.0)))
        );
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(475.0), // bottom - 25.0 < bottom - 24.0
                scroll_bounds,
                Point::new(px(0.0), px(-40.0)),
                max_off
            ),
            None
        );
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(477.0), // inside bottom 24px edge zone
                scroll_bounds,
                Point::new(px(0.0), px(-40.0)),
                max_off
            ),
            Some(Point::new(px(0.0), px(-52.0)))
        );
        // Zero max_offset, zero scroll height, tiny <=0.5px delta, and bottom clamp return None
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(105.0),
                scroll_bounds,
                Point::new(px(0.0), px(-10.0)),
                Size::new(px(0.0), px(0.0)),
            ),
            None
        );
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(105.0),
                Bounds::new(Point::new(px(0.0), px(100.0)), Size::new(px(300.0), px(0.0))),
                Point::new(px(0.0), px(-10.0)),
                max_off,
            ),
            None
        );
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(105.0),
                scroll_bounds,
                Point::new(px(0.0), px(-0.5)),
                max_off,
            ),
            None
        );
        assert_eq!(
            compute_drag_auto_scroll_offset(
                px(495.0),
                scroll_bounds,
                Point::new(px(0.0), px(-120.0)),
                max_off,
            ),
            None
        );
    }

    #[test]
    fn test_has_blocking_filter_drag_overlay_and_can_accept_filter_drop() {
        // Every individual overlay flag blocks drag-and-drop; all false permits it
        assert!(!has_blocking_filter_drag_overlay_flags(
            false, false, false, false, false, false
        ));
        for i in 0..6 {
            assert!(has_blocking_filter_drag_overlay_flags(
                i == 0,
                i == 1,
                i == 2,
                i == 3,
                i == 4,
                i == 5,
            ));
        }

        let r0 = FilterRule {
            text: "Fatal".into(),
            ..Default::default()
        };
        let r1 = FilterRule {
            text: "Exception".into(),
            ..Default::default()
        };
        let tabs = vec![
            FilterTabData {
                name: "Tab0".into(),
                enabled: true,
                filters: vec![r0.clone(), r1.clone()],
                ..Default::default()
            },
            FilterTabData {
                name: "EmptyTab".into(),
                enabled: true,
                filters: Vec::new(),
                ..Default::default()
            },
        ];
        let dragged = DraggedFilterRule {
            tab_index: 0,
            source_index: 0,
            rule_id: r0.id,
            pattern: r0.text.clone().into(),
            is_exclude: false,
            bg_color: r0.bg_color.clone(),
            text_color: r0.text_color.clone(),
        };

        // Valid slot drop and tail drop (target_rule = None), including whitespace-only search
        assert_eq!(
            can_accept_filter_drop_in_tabs(false, "", 0, &tabs, &dragged, Some((1, r1.id))),
            Some(2)
        );
        assert_eq!(
            can_accept_filter_drop_in_tabs(false, "   \t", 0, &tabs, &dragged, None),
            Some(2)
        );

        // Blocking overlay or non-empty search query rejects drop
        assert_eq!(
            can_accept_filter_drop_in_tabs(true, "", 0, &tabs, &dragged, Some((1, r1.id))),
            None
        );
        assert_eq!(
            can_accept_filter_drop_in_tabs(false, "Fatal", 0, &tabs, &dragged, Some((1, r1.id))),
            None
        );

        // Tab index mismatch, out-of-bounds tab, and empty filter list reject drop
        assert_eq!(
            can_accept_filter_drop_in_tabs(false, "", 1, &tabs, &dragged, None),
            None
        );
        assert_eq!(
            can_accept_filter_drop_in_tabs(false, "", 99, &tabs, &dragged, None),
            None
        );
        let empty_tab_drag = DraggedFilterRule {
            tab_index: 1,
            ..dragged.clone()
        };
        assert_eq!(
            can_accept_filter_drop_in_tabs(false, "", 1, &tabs, &empty_tab_drag, None),
            None
        );

        // Stale source index / UUID mismatch rejects drop
        let stale_source_idx = DraggedFilterRule {
            source_index: 5,
            ..dragged.clone()
        };
        assert_eq!(
            can_accept_filter_drop_in_tabs(false, "", 0, &tabs, &stale_source_idx, None),
            None
        );
        let stale_source_id = DraggedFilterRule {
            rule_id: r1.id, // source_index is 0 (r0), but rule_id is r1.id
            ..dragged.clone()
        };
        assert_eq!(
            can_accept_filter_drop_in_tabs(false, "", 0, &tabs, &stale_source_id, None),
            None
        );

        // Stale target index / UUID mismatch rejects drop
        assert_eq!(
            can_accept_filter_drop_in_tabs(false, "", 0, &tabs, &dragged, Some((5, r1.id))),
            None
        );
        assert_eq!(
            can_accept_filter_drop_in_tabs(false, "", 0, &tabs, &dragged, Some((1, r0.id))),
            None
        );
    }

    #[test]
    fn test_normalize_slot_hit_bounds_row0_midpoint_alignment_and_horizontal_deadzone_trim() {
        let scroll_bounds =
            Bounds::new(Point::new(px(0.0), px(100.0)), Size::new(px(300.0), px(300.0)));
        let scroll_offset = Point::new(px(0.0), px(-10.0));

        // Row 0 raw slot: pt(8.0) + row_height(30.0) + pb(3.0) = 41.0px at y=110.0
        let raw_slot_0 =
            Bounds::new(Point::new(px(0.0), px(110.0)), Size::new(px(300.0), px(41.0)));
        // Inner child filter-row in screen coords: y = (110 - 10) + 8 = 108.0, height = 30.0
        let inner_row_0 =
            Bounds::new(Point::new(px(8.0), px(108.0)), Size::new(px(284.0), px(30.0)));

        let norm_0 = normalize_slot_hit_bounds(raw_slot_0, scroll_offset, 8.0, 5.0);
        // Screen origin.y = 110 - 10 + 5 = 105.0; height = 41 - 5 = 36.0; midpoint = 123.0
        assert_eq!(norm_0.origin, Point::new(px(8.0), px(105.0)));
        assert_eq!(norm_0.size, Size::new(px(284.0), px(36.0)));

        // Verify midpoint matches inner_row_0 across the entire boundary (including the old 2.5px band)
        for sample_y in [120.5_f32, 121.5, 122.9, 123.0, 124.0] {
            assert_eq!(
                resolve_drop_position_from_bounds(px(sample_y), norm_0),
                resolve_drop_position_from_bounds(px(sample_y), inner_row_0),
                "Row 0 midpoint mismatch at y={sample_y}"
            );
        }

        // Row 1 raw slot: pt(3.0) + row_height(30.0) + pb(3.0) = 36.0px at y=151.0
        let raw_slot_1 =
            Bounds::new(Point::new(px(0.0), px(151.0)), Size::new(px(300.0), px(36.0)));
        let norm_1 = normalize_slot_hit_bounds(raw_slot_1, scroll_offset, 8.0, 0.0);
        // Preserves symmetric 3px inter-row gap (105+36 = 141 == norm_1.origin.y) -> zero flicker
        assert_eq!(norm_0.bottom(), norm_1.origin.y);

        let raw_tail =
            Bounds::new(Point::new(px(0.0), px(187.0)), Size::new(px(300.0), px(123.0)));
        let norm_tail = normalize_slot_hit_bounds(raw_tail, scroll_offset, 8.0, 0.0);
        assert_eq!(norm_tail.origin, Point::new(px(8.0), px(177.0)));
        assert_eq!(norm_tail.size, Size::new(px(284.0), px(123.0)));

        let rows = [norm_0, norm_1];

        // Hovering inside 8px horizontal padding (x=4.0 or x=296.0) or top 5px margin (y=102.0) -> None
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(4.0), px(115.0)),
                scroll_bounds,
                &rows,
                Some(norm_tail),
            ),
            None
        );
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(296.0), px(190.0)),
                scroll_bounds,
                &rows,
                Some(norm_tail),
            ),
            None
        );
        assert_eq!(
            resolve_drop_target_at_position(
                Point::new(px(150.0), px(102.0)),
                scroll_bounds,
                &rows,
                Some(norm_tail),
            ),
            None
        );

        // Compact 4px trim clamps safely without negative sizes
        let narrow_norm = normalize_slot_hit_bounds(
            Bounds::new(Point::new(px(0.0), px(100.0)), Size::new(px(4.0), px(2.0))),
            Point::new(px(0.0), px(0.0)),
            4.0,
            5.0,
        );
        assert_eq!(narrow_norm.size, Size::new(px(0.0), px(0.0)));
    }

    #[test]
    fn test_resolve_drag_auto_scroll_in_viewport_requires_horizontal_containment() {
        let scroll_bounds =
            Bounds::new(Point::new(px(0.0), px(100.0)), Size::new(px(300.0), px(200.0)));
        let current_offset = Point::new(px(0.0), px(-30.0));
        let max_offset = Size::new(px(0.0), px(120.0));

        // Pointer over log viewport to the right (x=450) in edge_zone y=110 -> must NOT scroll
        assert_eq!(
            resolve_drag_auto_scroll_in_viewport(
                Point::new(px(450.0), px(110.0)),
                scroll_bounds,
                current_offset,
                max_offset,
            ),
            None
        );
        // Pointer above scroll_bounds (over search bar, y=90) -> must NOT scroll
        assert_eq!(
            resolve_drag_auto_scroll_in_viewport(
                Point::new(px(100.0), px(90.0)),
                scroll_bounds,
                current_offset,
                max_offset,
            ),
            None
        );
        // Pointer inside scroll_bounds and inside top edge_zone -> scrolls up
        assert_eq!(
            resolve_drag_auto_scroll_in_viewport(
                Point::new(px(100.0), px(110.0)),
                scroll_bounds,
                current_offset,
                max_offset,
            ),
            Some(Point::new(px(0.0), px(-18.0)))
        );
        // Pointer inside scroll_bounds and inside bottom edge_zone -> scrolls down
        assert_eq!(
            resolve_drag_auto_scroll_in_viewport(
                Point::new(px(100.0), px(290.0)),
                scroll_bounds,
                current_offset,
                max_offset,
            ),
            Some(Point::new(px(0.0), px(-42.0)))
        );
    }

    #[test]
    fn test_filter_matching_rules_trims_whitespace_consistently_with_dnd_gate() {
        use super::filter_matching_rules;

        let rules = vec![
            FilterRule {
                text: "FATAL error".into(),
                ..Default::default()
            },
            FilterRule {
                text: "ActivityManager".into(),
                ..Default::default()
            },
            FilterRule {
                text: "FatalException".into(),
                ..Default::default()
            },
        ];

        // Empty and whitespace-only queries match all rules with original indices,
        // consistent with `!query.trim().is_empty()` in the DnD gate.
        for empty_or_ws in ["", "   ", " \t\n "] {
            let matched = filter_matching_rules(&rules, empty_or_ws);
            let indices: Vec<usize> = matched.iter().map(|(i, _)| *i).collect();
            assert_eq!(indices, vec![0, 1, 2], "query {empty_or_ws:?} must keep all rules");
        }

        // Surrounding whitespace is trimmed and matched case-insensitively with original indices preserved
        let matched_fatal = filter_matching_rules(&rules, "  fatal  ");
        let fatal_indices: Vec<usize> = matched_fatal.iter().map(|(i, _)| *i).collect();
        assert_eq!(fatal_indices, vec![0, 2]);

        let matched_am = filter_matching_rules(&rules, "\tactivitymanager\n");
        assert_eq!(matched_am.len(), 1);
        assert_eq!(matched_am[0].0, 1);

        // Non-matching trimmed query returns empty
        assert!(filter_matching_rules(&rules, "  nonexistent  ").is_empty());
    }
}

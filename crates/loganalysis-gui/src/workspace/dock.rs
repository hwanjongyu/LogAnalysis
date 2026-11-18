use gpui::*;
use loganalysis_core::*;

use crate::types::FilterPanelPosition;
use super::LogAnalysisWorkspace;

pub const FACET_FILTER_PALETTE: [&str; 5] = ["Cyan", "Green", "Orchid", "Yellow", "Blue"];

/// Default filter dock width in logical pixels when docked on the left.
pub const DEFAULT_FILTER_DOCK_WIDTH: f32 = 310.0;
/// Default filter dock height in logical pixels when docked at the bottom.
pub const DEFAULT_FILTER_DOCK_HEIGHT: f32 = 220.0;
/// Minimum allowed filter dock width in logical pixels.
pub const MIN_FILTER_DOCK_WIDTH: f32 = 180.0;
/// Maximum allowed filter dock width in logical pixels.
pub const MAX_FILTER_DOCK_WIDTH: f32 = 800.0;
/// Minimum allowed filter dock height in logical pixels.
pub const MIN_FILTER_DOCK_HEIGHT: f32 = 100.0;
/// Maximum allowed filter dock height in logical pixels.
pub const MAX_FILTER_DOCK_HEIGHT: f32 = 700.0;

/// Clamps filter dock width adjustment from `start_width + delta` to `[180.0, 800.0]`.
#[inline]
pub fn clamp_filter_dock_width(start_width: f32, delta: f32) -> f32 {
    (start_width + delta).clamp(MIN_FILTER_DOCK_WIDTH, MAX_FILTER_DOCK_WIDTH)
}

/// Clamps filter dock height adjustment from `start_height + delta` to `[100.0, 700.0]`.
#[inline]
pub fn clamp_filter_dock_height(start_height: f32, delta: f32) -> f32 {
    (start_height + delta).clamp(MIN_FILTER_DOCK_HEIGHT, MAX_FILTER_DOCK_HEIGHT)
}

/// Returns the default `(width, height)` preset dimensions for the filter dock.
#[inline]
pub fn default_filter_dock_size() -> (f32, f32) {
    (DEFAULT_FILTER_DOCK_WIDTH, DEFAULT_FILTER_DOCK_HEIGHT)
}

/// Applies, toggles, or switches a non-regex facet filter rule for `tag` inside `tab`,
/// always marking `tab.modified = true` and returning a status summary message.
pub fn apply_facet_filter_to_tab(
    tab: &mut FilterTabData,
    tag: &str,
    is_exclude: bool,
) -> String {
    let color_idx = tag.bytes().map(|b| b as usize).sum::<usize>() % FACET_FILTER_PALETTE.len();
    let bg_color = if is_exclude {
        "None".to_string()
    } else {
        FACET_FILTER_PALETTE[color_idx].to_string()
    };
    let text_color = if is_exclude {
        "None".to_string()
    } else {
        "Black".to_string()
    };
    let mode_str = if is_exclude { "exclude" } else { "include" };

    if let Some(existing) = tab.filters.iter_mut().find(|r| r.text == tag && !r.regex) {
        tab.modified = true;
        if existing.exclude == is_exclude {
            existing.active = !existing.active;
            let state_str = if existing.active { "Enabled" } else { "Disabled" };
            format!("{state_str} {mode_str} facet filter for '{tag}'")
        } else {
            existing.exclude = is_exclude;
            existing.bg_color = bg_color;
            existing.text_color = text_color;
            existing.active = true;
            format!("Switched facet filter '{tag}' to {mode_str}")
        }
    } else {
        tab.filters.push(FilterRule {
            text: tag.to_string(),
            exclude: is_exclude,
            regex: false,
            case_sensitive: false,
            bg_color,
            text_color,
            active: true,
            ..Default::default()
        });
        tab.modified = true;
        format!("Added {mode_str} facet filter for '{tag}'")
    }
}

/// Computes the top 7 tag facets by fast-sampling up to 25,000 lines from `source`.
pub fn compute_facet_tags(source: &ActiveLogSource) -> Vec<(String, usize)> {
    let total = source.line_count();
    if total == 0 {
        return Vec::new();
    }

    let sample_limit = total.min(25_000);
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for i in 0..sample_limit {
        if let Some(line) = source.get_line(i) {
            let parsed = loganalysis_core::parse_column_line_auto(&line);
            if let Some(tag) = parsed.tag {
                let tag_trimmed = tag.trim();
                if !tag_trimmed.is_empty() && tag_trimmed.len() <= 32 {
                    *counts.entry(tag_trimmed.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sorted.truncate(7);
    sorted
}

impl LogAnalysisWorkspace {
    /// Toggles filter dock visibility between shown and hidden
    pub fn toggle_filter_dock(&mut self, cx: &mut Context<Self>) {
        self.clear_filter_drag_state_fields();
        self.is_filter_dock_visible = !self.is_filter_dock_visible;
        self.status_text = if self.is_filter_dock_visible {
            "Filter Dock: Visible".into()
        } else {
            "Filter Dock: Hidden (Full Viewport)".into()
        };
        cx.notify();
    }

    /// Toggles forensic facets bar visibility
    pub fn toggle_facets_bar(&mut self, cx: &mut Context<Self>) {
        self.is_facets_visible = !self.is_facets_visible;
        self.status_text = if self.is_facets_visible {
            "Forensic Facets Bar: Visible".into()
        } else {
            "Forensic Facets Bar: Hidden".into()
        };
        cx.notify();
    }

    /// Starts resizing the filter dock from mouse position
    pub fn start_resizing_filter_dock(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        self.is_resizing_filter_dock = true;
        self.filter_dock_drag_start_pos = Some(pos);
        self.filter_dock_drag_start_size = if self.filter_panel_position == FilterPanelPosition::Bottom {
            self.filter_dock_height
        } else {
            self.filter_dock_width
        };
        cx.notify();
    }

    /// Updates filter dock dimensions during interactive mouse drag
    pub fn update_resizing_filter_dock(&mut self, current_pos: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.is_resizing_filter_dock {
            return;
        }
        let Some(start_pos) = self.filter_dock_drag_start_pos else {
            return;
        };

        let start_x: f32 = start_pos.x.into();
        let current_x: f32 = current_pos.x.into();
        let start_y: f32 = start_pos.y.into();
        let current_y: f32 = current_pos.y.into();

        if self.filter_panel_position == FilterPanelPosition::Bottom {
            // Dragging upwards decreases Y, expanding bottom dock height
            let delta = start_y - current_y;
            let new_height = clamp_filter_dock_height(self.filter_dock_drag_start_size, delta);
            if (self.filter_dock_height - new_height).abs() > 0.5 {
                self.filter_dock_height = new_height;
                cx.notify();
            }
        } else {
            // Dragging rightwards increases X, expanding left dock width
            let delta = current_x - start_x;
            let new_width = clamp_filter_dock_width(self.filter_dock_drag_start_size, delta);
            if (self.filter_dock_width - new_width).abs() > 0.5 {
                self.filter_dock_width = new_width;
                cx.notify();
            }
        }
    }

    /// Finishes interactive filter dock resizing
    pub fn stop_resizing_filter_dock(&mut self, cx: &mut Context<Self>) {
        if self.is_resizing_filter_dock {
            self.is_resizing_filter_dock = false;
            self.filter_dock_drag_start_pos = None;
            cx.notify();
        }
    }

    /// Resets filter dock dimension to default preset
    pub fn reset_filter_dock_size(&mut self, cx: &mut Context<Self>) {
        let (default_width, default_height) = default_filter_dock_size();
        if self.filter_panel_position == FilterPanelPosition::Bottom {
            self.filter_dock_height = default_height;
        } else {
            self.filter_dock_width = default_width;
        }
        cx.notify();
    }

    /// Asynchronously samples the active buffer on a background thread to extract top 7 tags without blocking the UI.
    pub fn refresh_facet_tags_async(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.log_source.clone() else {
            self.facet_tags.clear();
            cx.notify();
            return;
        };

        let source_for_check = source.clone();
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();

            async move {
                let computed = bg_executor
                    .spawn(async move { compute_facet_tags(&source) })
                    .await;

                let _ = this.update(&mut async_cx, |workspace, cx| {
                    if let Some(current) = &workspace.log_source {
                        if std::sync::Arc::ptr_eq(current, &source_for_check) {
                            workspace.facet_tags = computed;
                            cx.notify();
                        }
                    }
                });
            }
        })
        .detach();
    }

    /// Adds or toggles a quick filter rule directly from a facet tag chip
    pub fn add_facet_filter(&mut self, tag: &str, is_exclude: bool, cx: &mut Context<Self>) {
        self.clear_filter_drag_state_fields();
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            let status = apply_facet_filter_to_tab(tab, tag, is_exclude);
            self.mark_filter_tabs_modified();
            self.status_text = status.into();
            self.reapply_filters(cx);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_facet_filter_to_tab, clamp_filter_dock_height, clamp_filter_dock_width,
        default_filter_dock_size, DEFAULT_FILTER_DOCK_HEIGHT, DEFAULT_FILTER_DOCK_WIDTH,
        FACET_FILTER_PALETTE,
    };
    use loganalysis_core::{evaluate_contrast, parse_color_spec, ContrastLevel, FilterTabData};

    #[test]
    fn test_facet_filter_palette_colors_and_wcag_contrast() {
        assert_eq!(FACET_FILTER_PALETTE.len(), 5);
        assert!(FACET_FILTER_PALETTE.contains(&"Orchid"));
        assert!(!FACET_FILTER_PALETTE.contains(&"Magenta"));
        assert!(
            parse_color_spec("Magenta", true).is_none(),
            "'Magenta' is not in COLOR_MAP and must stay excluded from FACET_FILTER_PALETTE"
        );

        let fg_hex = parse_color_spec("Black", false)
            .expect("'Black' foreground must resolve in TEXT_COLOR_MAP");

        for color in FACET_FILTER_PALETTE {
            let bg_hex = parse_color_spec(color, true)
                .unwrap_or_else(|| panic!("Palette color '{color}' must resolve via parse_color_spec"));
            let (ratio, level) = evaluate_contrast(bg_hex, fg_hex);
            assert!(
                ratio >= 4.5,
                "Palette color '{color}' contrast ratio {ratio:.2}:1 with Black must be >= 4.5:1"
            );
            assert_eq!(
                level,
                ContrastLevel::Pass,
                "Palette color '{color}' must pass WCAG AA contrast with Black"
            );
        }
    }

    #[test]
    fn test_apply_facet_filter_to_tab_sets_modified_across_all_paths() {
        let mut tab = FilterTabData::default();
        assert!(!tab.modified);

        // 1. Add new include rule -> modified = true
        let status_add = apply_facet_filter_to_tab(&mut tab, "ActivityManager", false);
        assert!(tab.modified);
        assert_eq!(tab.filters.len(), 1);
        assert!(tab.filters[0].active);
        assert!(!tab.filters[0].exclude);
        assert_eq!(tab.filters[0].text_color, "Black");
        assert!(status_add.contains("Added include"));

        // 2. Toggle existing rule off -> modified = true
        tab.modified = false;
        let status_toggle = apply_facet_filter_to_tab(&mut tab, "ActivityManager", false);
        assert!(tab.modified);
        assert!(!tab.filters[0].active);
        assert!(status_toggle.contains("Disabled include"));

        // 3. Switch include -> exclude -> modified = true, active = true, None/None colors
        tab.modified = false;
        let status_switch = apply_facet_filter_to_tab(&mut tab, "ActivityManager", true);
        assert!(tab.modified);
        assert!(tab.filters[0].active);
        assert!(tab.filters[0].exclude);
        assert_eq!(tab.filters[0].bg_color, "None");
        assert_eq!(tab.filters[0].text_color, "None");
        assert!(status_switch.contains("Switched facet filter"));
    }

    #[test]
    fn test_dock_dimension_helpers() {
        assert_eq!(
            default_filter_dock_size(),
            (DEFAULT_FILTER_DOCK_WIDTH, DEFAULT_FILTER_DOCK_HEIGHT)
        );
        assert_eq!(clamp_filter_dock_width(310.0, -500.0), 180.0);
        assert_eq!(clamp_filter_dock_width(310.0, 999.0), 800.0);
        assert_eq!(clamp_filter_dock_height(220.0, -300.0), 100.0);
        assert_eq!(clamp_filter_dock_height(220.0, 1000.0), 700.0);
    }
}

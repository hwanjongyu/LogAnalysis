use gpui::*;
use loganalysis_core::*;

use crate::types::FilterPanelPosition;
use super::LogAnalysisWorkspace;

impl LogAnalysisWorkspace {
    /// Toggles filter dock visibility between shown and hidden
    pub fn toggle_filter_dock(&mut self, cx: &mut Context<Self>) {
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
            let new_height = (self.filter_dock_drag_start_size + delta).clamp(100.0, 700.0);
            if (self.filter_dock_height - new_height).abs() > 0.5 {
                self.filter_dock_height = new_height;
                cx.notify();
            }
        } else {
            // Dragging rightwards increases X, expanding left dock width
            let delta = current_x - start_x;
            let new_width = (self.filter_dock_drag_start_size + delta).clamp(180.0, 800.0);
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
        if self.filter_panel_position == FilterPanelPosition::Bottom {
            self.filter_dock_height = 220.0;
        } else {
            self.filter_dock_width = 310.0;
        }
        cx.notify();
    }

    /// Fast-samples the active buffer (up to 25,000 lines) to extract top 7 tags
    pub fn refresh_facet_tags(&mut self) {
        let Some(source) = &self.log_source else {
            self.facet_tags.clear();
            return;
        };

        let total = source.line_count();
        if total == 0 {
            self.facet_tags.clear();
            return;
        }

        let sample_limit = total.min(25_000);
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for i in 0..sample_limit {
            if let Some(line) = source.get_line(i) {
                let parsed = loganalysis_core::parse_logcat_column_line(&line);
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
        self.facet_tags = sorted;
    }

    /// Adds or toggles a quick filter rule directly from a facet tag chip
    pub fn add_facet_filter(&mut self, tag: &str, is_exclude: bool, cx: &mut Context<Self>) {
        let palette = ["Cyan", "Green", "Magenta", "Yellow", "Blue"];
        let color_idx = tag.bytes().map(|b| b as usize).sum::<usize>() % palette.len();
        let bg_color = if is_exclude {
            "None".to_string()
        } else {
            palette[color_idx].to_string()
        };
        let text_color = if is_exclude {
            "None".to_string()
        } else {
            "Black".to_string()
        };

        // Deduplicate: if a rule for this exact tag already exists in the active tab, toggle or update it
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if let Some(existing) = tab.filters.iter_mut().find(|r| r.text == tag && !r.regex) {
                if existing.exclude == is_exclude {
                    existing.active = !existing.active;
                    let state_str = if existing.active { "Enabled" } else { "Disabled" };
                    let mode_str = if is_exclude { "exclude" } else { "include" };
                    self.status_text = format!("{state_str} {mode_str} facet filter for '{tag}'").into();
                } else {
                    existing.exclude = is_exclude;
                    existing.bg_color = bg_color;
                    existing.text_color = text_color;
                    existing.active = true;
                    let mode_str = if is_exclude { "exclude" } else { "include" };
                    self.status_text = format!("Switched facet filter '{tag}' to {mode_str}").into();
                }
                self.reapply_filters(cx);
                return;
            }
        }

        let rule = FilterRule {
            text: tag.to_string(),
            exclude: is_exclude,
            regex: false,
            case_sensitive: false,
            bg_color,
            text_color,
            active: true,
            ..Default::default()
        };

        self.add_filter_rule(rule, cx);
        self.status_text = format!(
            "Added {} facet filter for '{}'",
            if is_exclude { "exclude" } else { "include" },
            tag
        )
        .into();
        cx.notify();
    }
}

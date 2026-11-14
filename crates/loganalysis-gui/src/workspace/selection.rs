use gpui::*;
use loganalysis_core::*;

use crate::types::InputFocusTarget;
use super::LogAnalysisWorkspace;

impl LogAnalysisWorkspace {
    /// Selects a log row and updates status line attribution
    pub fn select_log_row(&mut self, visible_idx: usize, source_idx: usize, cx: &mut Context<Self>) {
        self.selection.select(visible_idx, source_idx);
        self.active_split_pane_is_secondary = false;
        self.input_focus = InputFocusTarget::None;
        if self.is_split_view && self.is_scroll_synced {
            self.sync_scroll_to_secondary(source_idx, cx);
        }
        self.refresh_status_line(cx);
    }

    /// Selects a log row in the secondary pane and syncs primary pane if lockstep is enabled
    pub fn select_secondary_log_row(&mut self, visible_idx: usize, source_idx: usize, cx: &mut Context<Self>) {
        self.secondary_selection.select(visible_idx, source_idx);
        self.active_split_pane_is_secondary = true;
        self.input_focus = InputFocusTarget::None;
        if self.is_split_view && self.is_scroll_synced {
            self.sync_scroll_to_primary(source_idx, cx);
        }
        self.refresh_status_line(cx);
    }

    /// Copies selected log row to clipboard (Ctrl+C)
    pub fn copy_selection(&mut self, cx: &mut Context<Self>) {
        let Some(sel_row) = self.selection.selected_row else {
            self.status_text = "No log line selected to copy".into();
            cx.notify();
            return;
        };

        let text_to_copy = if let (Some(res), Some(src)) = (&self.filter_result, &self.log_source) {
            let r_idx = res.visible_indices.get(sel_row).copied().unwrap_or(0);
            src.get_line(r_idx)
                .map(|s| s.to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };

        if !text_to_copy.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text_to_copy));
            self.status_text = "✓ Copied line to clipboard".into();
            cx.notify();

            cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let bg_executor = cx.background_executor().clone();
                let mut async_cx = cx.clone();
                async move {
                    bg_executor.timer(std::time::Duration::from_secs(2)).await;
                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        workspace.refresh_status_line(cx);
                    });
                }
            })
            .detach();
        }
    }

    /// Adds current selected line as an Include or Exclude filter
    pub fn add_quick_filter_from_selection(&mut self, exclude: bool, cx: &mut Context<Self>) {
        let Some(sel_row) = self.selection.selected_row else {
            self.status_text = "Select a log line first".into();
            cx.notify();
            return;
        };

        let raw_line = if let (Some(res), Some(src)) = (&self.filter_result, &self.log_source) {
            let r_idx = res.visible_indices.get(sel_row).copied().unwrap_or(0);
            src.get_line(r_idx)
                .map(|s| s.to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };

        let line_trimmed = raw_line.trim();
        if line_trimmed.is_empty() {
            return;
        }

        // Take up to 60 characters for quick filter pattern
        let pattern: String = line_trimmed.chars().take(60).collect();
        let rule = FilterRule {
            text: pattern.clone(),
            exclude,
            regex: false,
            case_sensitive: false,
            bg_color: if exclude {
                "None".to_string()
            } else {
                "Green".to_string()
            },
            text_color: if exclude {
                "None".to_string()
            } else {
                "Black".to_string()
            },
            active: true,
            ..Default::default()
        };

        self.add_filter_rule(rule, cx);
        self.status_text = format!(
            "Added quick {} filter: '{}'",
            if exclude { "exclude" } else { "include" },
            pattern
        )
        .into();
        cx.notify();
    }

    /// Computes the delta time (Δt) between the selected visible row and its nearest preceding timestamped line.
    pub fn get_selected_delta_time(&self) -> Option<(String, loganalysis_core::LatencyLevel, i64)> {
        let selected_row = self.selection.selected_row?;
        let filter_res = self.filter_result.as_ref()?;
        let source = self.log_source.as_ref()?;

        let curr_src_idx = *filter_res.visible_indices.get(selected_row)?;
        let t_curr_us = source.get_timestamp_micros(curr_src_idx).or_else(|| {
            source
                .get_raw_line(curr_src_idx)
                .and_then(loganalysis_core::extract_timestamp)
                .map(|t| t.total_micros)
        })?;

        for back_step in 1..=10 {
            if selected_row < back_step {
                break;
            }
            let prev_src_idx = *filter_res.visible_indices.get(selected_row - back_step)?;
            let prev_us = source.get_timestamp_micros(prev_src_idx).or_else(|| {
                source
                    .get_raw_line(prev_src_idx)
                    .and_then(loganalysis_core::extract_timestamp)
                    .map(|t| t.total_micros)
            });
            if let Some(t_prev_us) = prev_us {
                let diff = if matches!(&**source, loganalysis_core::ActiveLogSource::Merged(_)) {
                    t_curr_us.saturating_sub(t_prev_us)
                } else {
                    loganalysis_core::compute_delta_micros(
                        loganalysis_core::LogTimestamp {
                            total_micros: t_prev_us,
                        },
                        loganalysis_core::LogTimestamp {
                            total_micros: t_curr_us,
                        },
                    )
                };
                let formatted = loganalysis_core::format_delta_micros(diff);
                let latency = loganalysis_core::LatencyLevel::from_micros(diff);
                return Some((formatted, latency, diff));
            }
        }

        None
    }

    /// Clears the active log row selection and refreshes status line
    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selection.clear();
        self.secondary_selection.clear();
        self.refresh_status_line(cx);
        cx.notify();
    }
}

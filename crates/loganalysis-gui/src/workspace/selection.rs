use gpui::*;
use loganalysis_core::*;

use crate::selection::SelectionModel;
use crate::types::InputFocusTarget;
use super::LogAnalysisWorkspace;

/// Snapshot of the currently selected log line across either the primary or secondary split pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveSelectedLine {
    pub is_secondary_pane: bool,
    pub visible_row: usize,
    pub source_idx: usize,
    /// Stable monotonic line index (`source_idx + total_trimmed`) used for scroll/cache tracking.
    pub abs_source_idx: usize,
    /// Log line text with trailing `\r` / `\n` stripped.
    pub line_text: SharedString,
    pub source_badge: Option<(u8, String)>,
    pub parsed_time: Option<String>,
    pub level_char: Option<char>,
    pub tag: Option<String>,
}

fn build_active_selected_line(
    is_secondary_pane: bool,
    sel: &SelectionModel,
    filter_result: Option<&FilterResult>,
    log_source: Option<&ActiveLogSource>,
) -> Option<ActiveSelectedLine> {
    let visible_row = sel.selected_row?;
    let res = filter_result?;
    let src = log_source?;
    let source_idx =
        crate::selection::resolve_selected_source_idx(&res.visible_indices, Some(visible_row))?;
    let arc_line = src.get_line_arc(source_idx)?;
    let trimmed = arc_line.trim_end_matches(['\r', '\n']);
    let line_text = if trimmed.len() == arc_line.len() {
        SharedString::new(arc_line)
    } else {
        SharedString::from(trimmed.to_string())
    };
    let abs_source_idx = source_idx.saturating_add(src.total_trimmed());
    let source_badge = src
        .get_line_source_info(source_idx)
        .map(|(id, name)| (id, name.to_string()));
    let parsed = parse_column_line_auto(line_text.as_ref());
    let parsed_time = parsed.time.map(|s| s.to_string());
    let level_char = parsed.level_char;
    let tag = parsed.tag.map(|s| s.to_string());
    Some(ActiveSelectedLine {
        is_secondary_pane,
        visible_row,
        source_idx,
        abs_source_idx,
        line_text,
        source_badge,
        parsed_time,
        level_char,
        tag,
    })
}

/// Resolves the active selected log line across Primary and (when split view is active) Secondary panes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_active_selected_line_from_panes(
    is_split_view: bool,
    active_pane_is_secondary: bool,
    primary_selection: &SelectionModel,
    primary_filter_result: Option<&FilterResult>,
    primary_log_source: Option<&ActiveLogSource>,
    secondary_selection: &SelectionModel,
    secondary_filter_result: Option<&FilterResult>,
    secondary_log_source: Option<&ActiveLogSource>,
) -> Option<ActiveSelectedLine> {
    if is_split_view && active_pane_is_secondary {
        build_active_selected_line(
            true,
            secondary_selection,
            secondary_filter_result,
            secondary_log_source,
        )
        .or_else(|| {
            build_active_selected_line(
                false,
                primary_selection,
                primary_filter_result,
                primary_log_source,
            )
        })
    } else {
        build_active_selected_line(
            false,
            primary_selection,
            primary_filter_result,
            primary_log_source,
        )
        .or_else(|| {
            if is_split_view {
                build_active_selected_line(
                    true,
                    secondary_selection,
                    secondary_filter_result,
                    secondary_log_source,
                )
            } else {
                None
            }
        })
    }
}

impl LogAnalysisWorkspace {
    /// Resolves the active selected line across Primary and Secondary panes.
    pub fn resolve_active_selected_line(&self) -> Option<ActiveSelectedLine> {
        resolve_active_selected_line_from_panes(
            self.split_view.is_split_view,
            self.split_view.active_pane_is_secondary,
            &self.selection,
            self.filter_result.as_deref(),
            self.log_source.as_deref(),
            &self.secondary_selection,
            self.split_view.secondary_filter_result.as_deref(),
            self.split_view.secondary_log_source.as_deref(),
        )
    }

    /// Selects a log row and updates status line attribution
    pub fn select_log_row(&mut self, visible_idx: usize, source_idx: usize, cx: &mut Context<Self>) {
        self.selection.select(visible_idx, source_idx);
        self.split_view.active_pane_is_secondary = false;
        self.input_focus = InputFocusTarget::None;
        if self.split_view.is_split_view && self.split_view.is_scroll_synced {
            self.sync_scroll_to_secondary(source_idx, cx);
        }
        self.reset_line_detail_scroll_if_line_changed();
        self.refresh_status_line(cx);
    }

    /// Selects a log row in the secondary pane and syncs primary pane if lockstep is enabled
    pub fn select_secondary_log_row(&mut self, visible_idx: usize, source_idx: usize, cx: &mut Context<Self>) {
        self.secondary_selection.select(visible_idx, source_idx);
        self.split_view.active_pane_is_secondary = true;
        self.input_focus = InputFocusTarget::None;
        if self.split_view.is_split_view && self.split_view.is_scroll_synced {
            self.sync_scroll_to_primary(source_idx, cx);
        }
        self.reset_line_detail_scroll_if_line_changed();
        self.refresh_status_line(cx);
    }

    /// Copies selected log row to clipboard (Ctrl+C)
    pub fn copy_selection(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.resolve_active_selected_line() else {
            self.status_text = "No log line selected to copy".into();
            cx.notify();
            return;
        };

        let text_to_copy = active.line_text.to_string();

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
        self.clear_filter_drag_state_fields();
        let Some(active) = self.resolve_active_selected_line() else {
            self.status_text = "Select a log line first".into();
            cx.notify();
            return;
        };

        let line_trimmed = active.line_text.as_ref().trim();
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
        let is_merged = matches!(&**source, loganalysis_core::ActiveLogSource::Merged(_));

        let diff = if is_merged {
            let t_curr_us = source.get_timestamp_micros(curr_src_idx)?;
            let mut found_diff = None;
            for back_step in 1..=10 {
                if selected_row < back_step {
                    break;
                }
                let prev_src_idx = *filter_res.visible_indices.get(selected_row - back_step)?;
                if let Some(t_prev_us) = source.get_timestamp_micros(prev_src_idx) {
                    found_diff = Some(t_curr_us.saturating_sub(t_prev_us));
                    break;
                }
            }
            found_diff?
        } else {
            let t_curr = source
                .get_raw_line(curr_src_idx)
                .and_then(loganalysis_core::extract_timestamp_with_kind)?;
            let mut found_diff = None;
            for back_step in 1..=10 {
                if selected_row < back_step {
                    break;
                }
                let prev_src_idx = *filter_res.visible_indices.get(selected_row - back_step)?;
                if let Some(t_prev) = source
                    .get_raw_line(prev_src_idx)
                    .and_then(loganalysis_core::extract_timestamp_with_kind)
                {
                    found_diff = Some(loganalysis_core::compute_delta_micros_typed(t_prev, t_curr));
                    break;
                }
            }
            found_diff?
        };

        let formatted = loganalysis_core::format_delta_micros(diff);
        let latency = loganalysis_core::LatencyLevel::from_micros(diff);
        Some((formatted, latency, diff))
    }

    /// Clears the active log row selection and refreshes status line
    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selection.clear();
        self.secondary_selection.clear();
        self.reset_line_detail_scroll_if_line_changed();
        self.refresh_status_line(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_active_selected_line_from_panes;
    use crate::selection::SelectionModel;
    use loganalysis_core::{ActiveLogSource, FilterResult, StreamLogBuffer};

    #[test]
    fn test_resolve_active_selected_line_split_panes_and_bounds() {
        let mut primary_stream = StreamLogBuffer::new(100, 10);
        primary_stream.append_chunk(vec![
            "11-06 18:00:00.000 100 100 I TagA: primary line zero\r\n".to_string(),
            "11-06 18:00:01.000 100 100 E TagA: primary line one\n".to_string(),
        ]);
        let primary_src = ActiveLogSource::Stream(primary_stream);
        let primary_res = FilterResult {
            visible_indices: vec![0, 1],
            ..Default::default()
        };

        let mut secondary_stream = StreamLogBuffer::new(100, 10);
        secondary_stream.append_chunk(vec![
            "11-06 18:00:02.000 200 200 W TagB: secondary line zero\n".to_string(),
        ]);
        let secondary_src = ActiveLogSource::Stream(secondary_stream);
        let secondary_res = FilterResult {
            visible_indices: vec![0],
            ..Default::default()
        };

        let mut primary_sel = SelectionModel::new();
        let mut secondary_sel = SelectionModel::new();
        secondary_sel.select(0, 0);

        // 1. When split view is OFF, stale secondary_selection must be ignored
        assert!(resolve_active_selected_line_from_panes(
            false,
            true,
            &primary_sel,
            Some(&primary_res),
            Some(&primary_src),
            &secondary_sel,
            Some(&secondary_res),
            Some(&secondary_src),
        )
        .is_none());

        // 2. When split view is ON and primary has no selection, falls back to secondary
        let fallback_sec = resolve_active_selected_line_from_panes(
            true,
            false,
            &primary_sel,
            Some(&primary_res),
            Some(&primary_src),
            &secondary_sel,
            Some(&secondary_res),
            Some(&secondary_src),
        )
        .expect("should fall back to secondary when split view is active");
        assert!(fallback_sec.is_secondary_pane);
        assert_eq!(
            fallback_sec.line_text.as_ref(),
            "11-06 18:00:02.000 200 200 W TagB: secondary line zero"
        );
        assert_eq!(fallback_sec.tag.as_deref(), Some("TagB"));

        // 3. When both have selection, active_pane_is_secondary selects Secondary vs Primary
        primary_sel.select(1, 1);
        let active_sec = resolve_active_selected_line_from_panes(
            true,
            true,
            &primary_sel,
            Some(&primary_res),
            Some(&primary_src),
            &secondary_sel,
            Some(&secondary_res),
            Some(&secondary_src),
        )
        .unwrap();
        assert!(active_sec.is_secondary_pane);
        assert_eq!(active_sec.source_idx, 0);

        let active_pri = resolve_active_selected_line_from_panes(
            true,
            false,
            &primary_sel,
            Some(&primary_res),
            Some(&primary_src),
            &secondary_sel,
            Some(&secondary_res),
            Some(&secondary_src),
        )
        .unwrap();
        assert!(!active_pri.is_secondary_pane);
        assert_eq!(active_pri.source_idx, 1);

        // 4. Out-of-bounds selection returns None when single-pane, and falls back cleanly in split view
        primary_sel.select(99, 99);
        let oob = resolve_active_selected_line_from_panes(
            false,
            false,
            &primary_sel,
            Some(&primary_res),
            Some(&primary_src),
            &secondary_sel,
            Some(&secondary_res),
            Some(&secondary_src),
        );
        assert!(oob.is_none());

        let oob_primary_fallback = resolve_active_selected_line_from_panes(
            true,
            false,
            &primary_sel,
            Some(&primary_res),
            Some(&primary_src),
            &secondary_sel,
            Some(&secondary_res),
            Some(&secondary_src),
        )
        .expect("OOB primary should fall back to valid secondary in split view");
        assert!(oob_primary_fallback.is_secondary_pane);
        assert_eq!(oob_primary_fallback.source_idx, 0);

        primary_sel.select(0, 0);
        secondary_sel.select(99, 99);
        let oob_secondary_fallback = resolve_active_selected_line_from_panes(
            true,
            true,
            &primary_sel,
            Some(&primary_res),
            Some(&primary_src),
            &secondary_sel,
            Some(&secondary_res),
            Some(&secondary_src),
        )
        .expect("OOB secondary should fall back to valid primary in split view");
        assert!(!oob_secondary_fallback.is_secondary_pane);
        assert_eq!(oob_secondary_fallback.source_idx, 0);

        // When split view is OFF, even with active_pane_is_secondary == true and valid secondary, primary wins
        secondary_sel.select(0, 0);
        let single_pane_ignores_secondary = resolve_active_selected_line_from_panes(
            false,
            true,
            &primary_sel,
            Some(&primary_res),
            Some(&primary_src),
            &secondary_sel,
            Some(&secondary_res),
            Some(&secondary_src),
        )
        .unwrap();
        assert!(!single_pane_ignores_secondary.is_secondary_pane);
        assert_eq!(single_pane_ignores_secondary.source_idx, 0);

        // 5. Trailing \r\n stripped on primary line 0 & abs_source_idx tracks total_trimmed
        let trimmed_pri = resolve_active_selected_line_from_panes(
            false,
            false,
            &primary_sel,
            Some(&primary_res),
            Some(&primary_src),
            &secondary_sel,
            Some(&secondary_res),
            Some(&secondary_src),
        )
        .unwrap();
        assert!(!trimmed_pri.line_text.ends_with('\n'));
        assert!(!trimmed_pri.line_text.ends_with('\r'));
        assert_eq!(trimmed_pri.abs_source_idx, 0);

        let mut trimmed_stream = StreamLogBuffer::new(2, 2);
        trimmed_stream.append_chunk(vec![
            "11-06 18:00:00.000 100 100 I TagA: dropped_0\n".to_string(),
            "11-06 18:00:01.000 100 100 I TagA: dropped_1\n".to_string(),
            "11-06 18:00:02.000 100 100 I TagA: surviving_2\n".to_string(),
            "11-06 18:00:03.000 100 100 I TagA: surviving_3\n".to_string(),
        ]);
        assert_eq!(trimmed_stream.trim_if_needed(), Some(2));
        let trimmed_src = ActiveLogSource::Stream(trimmed_stream);
        let trimmed_res = FilterResult {
            visible_indices: vec![0, 1],
            ..Default::default()
        };
        primary_sel.select(0, 0);
        let post_trim = resolve_active_selected_line_from_panes(
            false,
            false,
            &primary_sel,
            Some(&trimmed_res),
            Some(&trimmed_src),
            &secondary_sel,
            Some(&secondary_res),
            Some(&secondary_src),
        )
        .unwrap();
        assert_eq!(post_trim.source_idx, 0);
        assert_eq!(post_trim.abs_source_idx, 2);
        assert!(post_trim.line_text.as_ref().contains("surviving_2"));
    }
}

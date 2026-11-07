use std::sync::Arc;
use gpui::*;
use loganalysis_core::*;

use super::LogAnalysisWorkspace;

/// Extracts timestamp for a given log row, probing backwards up to 10 lines if None (e.g. multi-line stack traces).
pub fn probe_parent_timestamp(source: &ActiveLogSource, target_idx: usize) -> Option<i64> {
    let raw_line = source.get_raw_line(target_idx)?;
    if let Some(ts) = extract_timestamp(raw_line) {
        return Some(ts.total_micros);
    }

    let min_idx = target_idx.saturating_sub(10);
    for idx in (min_idx..target_idx).rev() {
        if let Some(prev_raw) = source.get_raw_line(idx) {
            if let Some(t) = extract_timestamp(prev_raw) {
                return Some(t.total_micros);
            }
        }
    }
    None
}

impl LogAnalysisWorkspace {
    /// Toggles Dual-Pane Split View (Ctrl+\ / Cmd+\)
    pub fn toggle_split_view(&mut self, cx: &mut Context<Self>) {
        self.is_split_view = !self.is_split_view;
        if self.is_split_view {
            if self.secondary_log_source.is_none() {
                self.secondary_log_source = self.log_source.clone();
                self.secondary_file_name = self.active_file_name.clone();
                self.secondary_filter_result = self.filter_result.clone();
            }
            self.status_text = format!(
                "Split View enabled ({}, {})",
                if self.is_split_vertical { "Side-by-Side" } else { "Stacked" },
                if self.is_scroll_synced { "🔗 Synced" } else { "Unlinked" }
            )
            .into();
        } else {
            self.status_text = "Split View disabled".into();
        }
        cx.notify();
    }

    /// Toggles Split Orientation between Vertical (Side-by-Side) and Horizontal (Stacked) (Ctrl+Alt+\)
    pub fn toggle_split_orientation(&mut self, cx: &mut Context<Self>) {
        self.is_split_vertical = !self.is_split_vertical;
        self.status_text = format!(
            "Split View orientation: {}",
            if self.is_split_vertical { "Vertical (Side-by-Side)" } else { "Horizontal (Stacked)" }
        )
        .into();
        cx.notify();
    }

    /// Toggles Timestamp-Synchronized Lockstep Scrolling (Ctrl+Alt+S)
    pub fn toggle_synced_scroll(&mut self, cx: &mut Context<Self>) {
        self.is_scroll_synced = !self.is_scroll_synced;
        self.status_text = format!(
            "Timestamp Sync: {}",
            if self.is_scroll_synced { "🔗 Synced (Lockstep active)" } else { "Unlinked (Independent scrolling)" }
        )
        .into();
        cx.notify();
    }

    /// Prompts user to select a log file for the secondary split pane
    pub fn prompt_open_secondary_file(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
        let base_dialog = crate::dialogs::build_open_log_dialog_base();
        let is_x11 = crate::dialogs::is_x11_session();
        let dialog = if !is_x11 {
            crate::dialogs::attach_dialog_parent(base_dialog, parent)
        } else {
            base_dialog
        };

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();
            async move {
                let dialog = if is_x11 {
                    let xid = bg_executor
                        .spawn(async move {
                            #[cfg(target_os = "linux")]
                            {
                                crate::dialogs::get_active_x11_window_id()
                            }
                            #[cfg(not(target_os = "linux"))]
                            {
                                None
                            }
                        })
                        .await;
                    crate::dialogs::attach_dialog_x11_id(dialog, xid)
                } else {
                    dialog
                };

                let file = dialog.pick_file().await;
                if let Some(handle) = file {
                    let path = handle.path().to_path_buf();
                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        workspace.open_secondary_file(&path, cx);
                    });
                }
            }
        })
        .detach();
    }

    /// Loads a log file for the secondary pane asynchronously and runs filtering
    pub fn open_secondary_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let path_buf = path.to_path_buf();
        let file_name: SharedString = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
            .into();

        self.status_text = format!("Opening secondary log {:?}...", file_name).into();
        self.secondary_selection.clear();
        self.is_split_view = true;
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();
            let path_for_bg = path_buf.clone();

            async move {
                let open_result = bg_executor
                    .spawn(async move { FileLogBuffer::open(&path_for_bg) })
                    .await;

                let _ = this.update(&mut async_cx, |workspace, cx| {
                    match open_result {
                        Ok(buf) => {
                            let arc_buf = Arc::new(ActiveLogSource::File(buf));
                            workspace.secondary_log_source = Some(arc_buf.clone());
                            workspace.secondary_file_name = Some(file_name);
                            workspace.status_text = format!(
                                "Secondary pane loaded {} lines from {:?}",
                                arc_buf.line_count(),
                                path_buf
                            )
                            .into();
                            workspace.reapply_secondary_filters(cx);
                        }
                        Err(e) => {
                            workspace.status_text = format!("Failed to open secondary file: {}", e).into();
                            cx.notify();
                        }
                    }
                });
            }
        })
        .detach();
    }

    /// Re-evaluates active filters on the secondary log source in the background
    pub fn reapply_secondary_filters(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.secondary_log_source.clone() else {
            return;
        };

        let mut active_filters = Vec::new();
        for tab in &self.filter_tabs {
            if tab.enabled {
                for rule in &tab.filters {
                    if rule.active {
                        active_filters.push(rule.clone());
                    }
                }
            }
        }

        let show_only = self.show_only_filtered;
        let source_for_bg = source.clone();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();

            async move {
                let result = bg_executor
                    .spawn(async move {
                        execute_filtering(
                            &*source_for_bg,
                            &active_filters,
                            show_only,
                            None,
                            0,
                        )
                    })
                    .await;

                if let Some(res) = result {
                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        workspace.secondary_filter_result = Some(Arc::new(res));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Synchronizes secondary scroll position to match the timestamp of `primary_source_idx`
    pub fn sync_scroll_to_secondary(&mut self, primary_source_idx: usize, cx: &mut Context<Self>) {
        let (Some(primary_src), Some(secondary_src), Some(secondary_res)) = (
            &self.log_source,
            &self.secondary_log_source,
            &self.secondary_filter_result,
        ) else {
            return;
        };

        let Some(ts_micros) = probe_parent_timestamp(primary_src, primary_source_idx) else {
            return;
        };

        if let Some(target_vis_idx) = find_nearest_timestamp_row(
            &**secondary_src,
            &secondary_res.visible_indices,
            ts_micros,
        ) {
            let target_source_idx = secondary_res.visible_indices.get(target_vis_idx).copied().unwrap_or(0);
            self.secondary_selection.select(target_vis_idx, target_source_idx);
            self.secondary_scroll_handle.scroll_to_item_strict(target_vis_idx, ScrollStrategy::Center);
            cx.notify();
        }
    }

    /// Synchronizes primary scroll position to match the timestamp of `secondary_source_idx`
    pub fn sync_scroll_to_primary(&mut self, secondary_source_idx: usize, cx: &mut Context<Self>) {
        let (Some(secondary_src), Some(primary_src), Some(primary_res)) = (
            &self.secondary_log_source,
            &self.log_source,
            &self.filter_result,
        ) else {
            return;
        };

        let Some(ts_micros) = probe_parent_timestamp(secondary_src, secondary_source_idx) else {
            return;
        };

        if let Some(target_vis_idx) = find_nearest_timestamp_row(
            &**primary_src,
            &primary_res.visible_indices,
            ts_micros,
        ) {
            let target_source_idx = primary_res.visible_indices.get(target_vis_idx).copied().unwrap_or(0);
            self.selection.select(target_vis_idx, target_source_idx);
            self.scroll_handle.scroll_to_item_strict(target_vis_idx, ScrollStrategy::Center);
            cx.notify();
        }
    }
}

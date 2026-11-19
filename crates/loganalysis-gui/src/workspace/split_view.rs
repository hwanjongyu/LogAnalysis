use std::sync::atomic::Ordering;
use std::sync::Arc;
use gpui::*;
use loganalysis_core::*;

use super::LogAnalysisWorkspace;

/// Extracts timestamp for a given log row, probing backwards up to 10 lines if None (e.g. multi-line stack traces).
pub fn probe_parent_timestamp(source: &ActiveLogSource, target_idx: usize) -> Option<i64> {
    if let Some(ts) = source.get_timestamp_micros(target_idx) {
        return Some(ts);
    }

    let min_idx = target_idx.saturating_sub(10);
    for idx in (min_idx..target_idx).rev() {
        if let Some(ts) = source.get_timestamp_micros(idx) {
            return Some(ts);
        }
    }
    None
}

impl LogAnalysisWorkspace {
    /// Toggles Dual-Pane Split View (Ctrl+\ / Cmd+\)
    pub fn toggle_split_view(&mut self, cx: &mut Context<Self>) {
        self.split_view.is_split_view = !self.split_view.is_split_view;
        if self.split_view.is_split_view {
            if self.split_view.secondary_log_source.is_none() {
                self.split_view.secondary_log_source = self.log_source.clone();
                self.split_view.secondary_file_name = self.active_file_name.clone();
                self.split_view.secondary_filter_result = self.filter_result.clone();
            }
            self.status_text = format!(
                "Split View enabled ({}, {})",
                if self.split_view.is_split_vertical { "Side-by-Side" } else { "Stacked" },
                if self.split_view.is_scroll_synced { "🔗 Synced" } else { "Unlinked" }
            )
            .into();
        } else {
            self.split_view.active_pane_is_secondary = false;
            self.secondary_selection.clear();
            self.is_dragging_secondary_scrollbar = false;
            self.is_dragging_secondary_h_scrollbar = false;
            self.h_scrollbar_drag_grab_offset = None;
            self.reset_line_detail_scroll_if_line_changed();
            self.status_text = "Split View disabled".into();
        }
        cx.notify();
    }

    /// Toggles Split Orientation between Vertical (Side-by-Side) and Horizontal (Stacked) (Ctrl+Alt+\)
    pub fn toggle_split_orientation(&mut self, cx: &mut Context<Self>) {
        self.split_view.is_split_vertical = !self.split_view.is_split_vertical;
        self.status_text = format!(
            "Split View orientation: {}",
            if self.split_view.is_split_vertical { "Vertical (Side-by-Side)" } else { "Horizontal (Stacked)" }
        )
        .into();
        cx.notify();
    }

    /// Toggles Timestamp-Synchronized Lockstep Scrolling (Ctrl+Alt+S)
    pub fn toggle_synced_scroll(&mut self, cx: &mut Context<Self>) {
        self.split_view.is_scroll_synced = !self.split_view.is_scroll_synced;
        self.status_text = format!(
            "Timestamp Sync: {}",
            if self.split_view.is_scroll_synced { "🔗 Synced (Lockstep active)" } else { "Unlinked (Independent scrolling)" }
        )
        .into();
        cx.notify();
    }

    /// Prompts user to select a log file for the secondary split pane
    pub fn prompt_open_secondary_file(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
        let dialog = crate::dialogs::prepare_dialog_with_parent(
            crate::dialogs::build_open_log_dialog_base(),
            parent,
        );

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();
            async move {
                let dialog =
                    crate::dialogs::resolve_dialog_for_display(dialog, &bg_executor).await;

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
        let sec_gen = self.split_view.advance_source_gen();
        self.secondary_filter_cancel_gen
            .fetch_add(1, Ordering::SeqCst);
        self.split_view.secondary_log_source = None;
        self.split_view.secondary_file_name = None;
        self.split_view.secondary_filter_result = None;
        self.secondary_selection.clear();
        self.secondary_scroll_handle
            .0
            .borrow()
            .base_handle
            .set_offset(Point::default());
        self.is_dragging_secondary_scrollbar = false;
        self.is_dragging_secondary_h_scrollbar = false;
        self.h_scrollbar_drag_grab_offset = None;
        self.reset_line_detail_scroll_if_line_changed();
        self.split_view.is_split_view = true;

        let path_buf = path.to_path_buf();
        let file_name: SharedString = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
            .into();

        self.status_text = format!("Opening secondary log {:?}...", file_name).into();
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
                    if workspace.split_view.secondary_source_gen != sec_gen {
                        return;
                    }
                    match open_result {
                        Ok(buf) => {
                            let arc_buf = Arc::new(ActiveLogSource::File(buf));
                            workspace.split_view.secondary_log_source = Some(arc_buf.clone());
                            workspace.split_view.secondary_file_name = Some(file_name);
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
        let Some(source) = self.split_view.secondary_log_source.clone() else {
            return;
        };

        let active_filters = self.collect_active_flat_filters();

        let show_only = self.show_only_filtered;
        let source_for_bg = source.clone();
        let generation = self
            .secondary_filter_cancel_gen
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        let cancel_token = self.secondary_filter_cancel_gen.clone();

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
                            Some(&cancel_token),
                            generation,
                        )
                    })
                    .await;

                if let Some(res) = result {
                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        if workspace
                            .secondary_filter_cancel_gen
                            .load(Ordering::SeqCst)
                            == generation
                        {
                            workspace.split_view.secondary_filter_result = Some(Arc::new(res));
                            super::preserve_selection_after_filter(
                                &mut workspace.secondary_selection,
                                workspace.split_view.secondary_filter_result.as_deref(),
                                &workspace.secondary_scroll_handle,
                            );
                            workspace.reset_line_detail_scroll_if_line_changed();
                            cx.notify();
                        }
                    });
                }
            }
        })
        .detach();
    }

    /// Synchronizes secondary scroll position to match the timestamp of `primary_source_idx`
    pub fn sync_scroll_to_secondary(&mut self, primary_source_idx: usize, cx: &mut Context<Self>) {
        if sync_pane_to_timestamp(
            self.log_source.as_ref(),
            primary_source_idx,
            self.split_view.secondary_log_source.as_ref(),
            self.split_view.secondary_filter_result.as_ref(),
            &mut self.secondary_selection,
            &self.secondary_scroll_handle,
        ) {
            cx.notify();
        }
    }

    /// Synchronizes primary scroll position to match the timestamp of `secondary_source_idx`
    pub fn sync_scroll_to_primary(&mut self, secondary_source_idx: usize, cx: &mut Context<Self>) {
        if sync_pane_to_timestamp(
            self.split_view.secondary_log_source.as_ref(),
            secondary_source_idx,
            self.log_source.as_ref(),
            self.filter_result.as_ref(),
            &mut self.selection,
            &self.scroll_handle,
        ) {
            cx.notify();
        }
    }
}

fn sync_pane_to_timestamp(
    from_source: Option<&Arc<ActiveLogSource>>,
    from_source_idx: usize,
    to_source: Option<&Arc<ActiveLogSource>>,
    to_filter_result: Option<&Arc<FilterResult>>,
    to_selection: &mut crate::selection::SelectionModel,
    to_scroll_handle: &UniformListScrollHandle,
) -> bool {
    let (Some(from_src), Some(to_src), Some(to_res)) = (from_source, to_source, to_filter_result)
    else {
        return false;
    };

    let Some(ts_micros) = probe_parent_timestamp(from_src, from_source_idx) else {
        return false;
    };

    let Some(target_vis_idx) =
        find_nearest_timestamp_row(&**to_src, &to_res.visible_indices, ts_micros)
    else {
        return false;
    };

    let Some(target_source_idx) =
        crate::selection::resolve_selected_source_idx(&to_res.visible_indices, Some(target_vis_idx))
    else {
        return false;
    };

    to_selection.select(target_vis_idx, target_source_idx);
    to_scroll_handle.scroll_to_item_strict(target_vis_idx, ScrollStrategy::Center);
    true
}

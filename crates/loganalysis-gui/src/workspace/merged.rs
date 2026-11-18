use std::sync::Arc;
use gpui::*;
use loganalysis_core::*;

use super::LogAnalysisWorkspace;

impl LogAnalysisWorkspace {
    /// Prompts user to select multiple log files to merge chronologically
    pub fn prompt_open_merged_files(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
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
                let files = dialog.pick_files().await;
                if let Some(handles) = files {
                    if !handles.is_empty() {
                        let paths: Vec<std::path::PathBuf> =
                            handles.into_iter().map(|h| h.path().to_path_buf()).collect();
                        let _ = this.update(&mut async_cx, |workspace, cx| {
                            workspace.open_merged_files(&paths, cx);
                        });
                    }
                }
            }
        })
        .detach();
    }

    /// Loads multiple log files and merges them chronologically using MergedLogBuffer
    pub fn open_merged_files(&mut self, paths: &[std::path::PathBuf], cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let merge_gen = self.begin_primary_source_transition(cx);

        let paths_vec = paths.to_vec();
        let file_names: Vec<String> = paths
            .iter()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
            .collect();
        let summary_name: SharedString =
            format!("Merged ({} files: {})", paths.len(), file_names.join(", ")).into();

        self.status_text = format!("Merging and opening {} log files...", paths.len()).into();
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();
            let paths_for_bg = paths_vec.clone();
            let paths_count = paths_for_bg.len();

            async move {
                let open_result = bg_executor
                    .spawn(async move { MergedLogBuffer::open_multiple(&paths_for_bg) })
                    .await;

                let _ = this.update(&mut async_cx, |workspace, cx| {
                    if workspace.primary_source_gen != merge_gen {
                        return;
                    }
                    match open_result {
                        Ok(buf) => {
                            let arc_buf = Arc::new(ActiveLogSource::Merged(buf));
                            workspace.log_source = Some(arc_buf.clone());
                            workspace.active_file_name = Some(summary_name);
                            workspace.refresh_facet_tags_async(cx);
                            workspace.status_text = format!(
                                "Merged {} lines chronologically across {} files",
                                arc_buf.line_count(),
                                paths_count
                            )
                            .into();
                            workspace.reapply_filters(cx);
                        }
                        Err(e) => {
                            workspace.status_text = format!("Failed to merge files: {}", e).into();
                            cx.notify();
                        }
                    }
                });
            }
        })
        .detach();
    }
}

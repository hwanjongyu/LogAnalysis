use std::sync::Arc;
use gpui::*;
use loganalysis_core::*;

use super::LogAnalysisWorkspace;

impl LogAnalysisWorkspace {
    /// Prompts user to select multiple log files to merge chronologically
    pub fn prompt_open_merged_files(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
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
        if self.is_streaming {
            self.stop_adb_stream(cx);
        }
        if paths.is_empty() {
            return;
        }

        let paths_vec = paths.to_vec();
        let file_names: Vec<String> = paths
            .iter()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
            .collect();
        let summary_name: SharedString =
            format!("Merged ({} files: {})", paths.len(), file_names.join(", ")).into();

        self.status_text = format!("Merging and opening {} log files...", paths.len()).into();
        self.selection.clear();
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
                    match open_result {
                        Ok(buf) => {
                            let arc_buf = Arc::new(ActiveLogSource::Merged(buf));
                            workspace.log_source = Some(arc_buf.clone());
                            workspace.active_file_name = Some(summary_name);
                            workspace.bookmarks.clear();
                            workspace.refresh_facet_tags();
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

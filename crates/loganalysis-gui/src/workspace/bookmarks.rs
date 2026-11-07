use gpui::*;
use loganalysis_core::*;

use crate::types::InputFocusTarget;
use super::LogAnalysisWorkspace;

impl LogAnalysisWorkspace {
    /// Toggles bookmark on the currently selected row (F2)
    pub fn toggle_bookmark_on_selected(&mut self, cx: &mut Context<Self>) {
        let Some(sel_row) = self.selection.selected_row else {
            self.status_text = "Select a log line first to bookmark".into();
            cx.notify();
            return;
        };

        let (src_idx, line_text, severity) = if let (Some(filter_res), Some(source)) =
            (&self.filter_result, &self.log_source)
        {
            if let Some(&idx) = filter_res.visible_indices.get(sel_row) {
                let text = source.get_line(idx).map(|s| s.to_string()).unwrap_or_default();
                let raw = source.get_raw_line(idx);
                let style = filter_res.visible_styles.get(sel_row).copied().unwrap_or_default();
                let sev = detect_line_severity(raw, &style);
                (idx, text, sev)
            } else {
                return;
            }
        } else {
            return;
        };

        let line_num = src_idx + 1;
        let added = self.bookmarks.toggle(src_idx, line_text, severity);
        if added {
            self.status_text = format!("🔖 Added bookmark on line {line_num}").into();
        } else {
            self.status_text = format!("Removed bookmark on line {line_num}").into();
        }
        cx.notify();
    }

    /// Jumps to the next bookmark cyclically (F2 / palette)
    pub fn jump_next_bookmark(&mut self, cx: &mut Context<Self>) {
        if self.bookmarks.is_empty() {
            self.status_text = "No bookmarks set (press F2 to add bookmark)".into();
            cx.notify();
            return;
        }

        let curr_src_idx = self.selection.anchor_source_idx.unwrap_or(0);
        let next_idx = self.bookmarks.next_bookmark(curr_src_idx)
            .or_else(|| self.bookmarks.items().first().map(|b| b.source_idx));

        let Some(target_source_idx) = next_idx else {
            return;
        };

        if let Some(res) = &self.filter_result {
            if let Some(vis_row) = res
                .visible_indices
                .iter()
                .position(|&idx| idx == target_source_idx)
            {
                self.selection.select(vis_row, target_source_idx);
                self.scroll_handle
                    .scroll_to_item(vis_row, ScrollStrategy::Center);
                self.refresh_status_line(cx);
                self.status_text =
                    format!("Jumped to bookmark on line {}", target_source_idx + 1).into();
                cx.notify();
                return;
            }
        }

        self.status_text = format!(
            "Bookmark on line {} is hidden by active filter",
            target_source_idx + 1
        )
        .into();
        cx.notify();
    }

    /// Jumps to the previous bookmark cyclically (Shift+F2)
    pub fn jump_prev_bookmark(&mut self, cx: &mut Context<Self>) {
        if self.bookmarks.is_empty() {
            self.status_text = "No bookmarks set (press F2 to add bookmark)".into();
            cx.notify();
            return;
        }

        let curr_src_idx = self.selection.anchor_source_idx.unwrap_or(usize::MAX);
        let prev_idx = self.bookmarks.prev_bookmark(curr_src_idx)
            .or_else(|| self.bookmarks.items().last().map(|b| b.source_idx));

        let Some(target_source_idx) = prev_idx else {
            return;
        };

        if let Some(res) = &self.filter_result {
            if let Some(vis_row) = res
                .visible_indices
                .iter()
                .position(|&idx| idx == target_source_idx)
            {
                self.selection.select(vis_row, target_source_idx);
                self.scroll_handle
                    .scroll_to_item(vis_row, ScrollStrategy::Center);
                self.refresh_status_line(cx);
                self.status_text =
                    format!("Jumped to bookmark on line {}", target_source_idx + 1).into();
                cx.notify();
                return;
            }
        }

        self.status_text = format!(
            "Bookmark on line {} is hidden by active filter",
            target_source_idx + 1
        )
        .into();
        cx.notify();
    }

    /// Clears all bookmarks
    pub fn clear_all_bookmarks(&mut self, cx: &mut Context<Self>) {
        let count = self.bookmarks.len();
        self.bookmarks.clear();
        self.status_text = format!("Cleared {count} bookmarks").into();
        cx.notify();
    }

    /// Prompts user to export an interactive HTML incident report via native file dialog
    pub fn prompt_export_html_report(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
        if self.bookmarks.is_empty() {
            self.status_text = "No bookmarks to export - bookmark lines first with F2".into();
            cx.notify();
            return;
        }

        let file_name = self
            .active_file_name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "incident_report".to_string());
        let clean_name = file_name
            .trim_end_matches(".log")
            .trim_end_matches(".txt");
        let default_export_name = format!("{clean_name}_incident_report.html");

        let total_lines = self
            .log_source
            .as_ref()
            .map(|s| s.line_count())
            .unwrap_or(0);
        let options = HtmlReportOptions {
            file_name,
            total_lines,
            bookmarks: self.bookmarks.items().to_vec(),
        };
        let html_content = generate_html_report(&options);

        let base_dialog =
            crate::dialogs::build_export_html_report_dialog_base(&default_export_name);
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

                let file = dialog.save_file().await;
                if let Some(handle) = file {
                    let path = handle.path().to_path_buf();
                    let write_res = bg_executor
                        .spawn(async move {
                            std::fs::write(&path, html_content.as_bytes()).map(|_| path)
                        })
                        .await;

                    let _ = this.update(&mut async_cx, |workspace, cx| match write_res {
                        Ok(p) => {
                            workspace.status_text = format!(
                                "✓ Exported HTML incident report to {:?}",
                                p.file_name().unwrap_or_default()
                            )
                            .into();
                            cx.notify();
                        }
                        Err(e) => {
                            workspace.status_text =
                                format!("Failed to export HTML report: {e}").into();
                            cx.notify();
                        }
                    });
                }
            }
        })
        .detach();
    }

    /// Toggles the slide-over Bookmark Drawer (Ctrl+B)
    pub fn toggle_bookmark_drawer(&mut self, cx: &mut Context<Self>) {
        self.is_bookmark_drawer_open = !self.is_bookmark_drawer_open;
        if self.is_bookmark_drawer_open {
            self.bookmark_drawer_selected_index = 0;
            self.is_command_palette_open = false;
            self.is_note_modal_open = false;
            self.input_focus = InputFocusTarget::None;
        }
        cx.notify();
    }

    /// Closes the Bookmark Drawer
    pub fn close_bookmark_drawer(&mut self, cx: &mut Context<Self>) {
        self.is_bookmark_drawer_open = false;
        cx.notify();
    }

    /// Jumps directly to a specific bookmarked source line index in the log viewport
    pub fn jump_to_bookmark_source_idx(&mut self, target_source_idx: usize, cx: &mut Context<Self>) {
        if let Some(res) = &self.filter_result {
            if let Some(vis_row) = res
                .visible_indices
                .iter()
                .position(|&idx| idx == target_source_idx)
            {
                self.selection.select(vis_row, target_source_idx);
                self.scroll_handle
                    .scroll_to_item(vis_row, ScrollStrategy::Center);
                self.refresh_status_line(cx);
                self.status_text =
                    format!("Jumped to bookmark on line {}", target_source_idx + 1).into();
                cx.notify();
                return;
            }
        }

        self.status_text = format!(
            "Bookmark on line {} is hidden by active filter",
            target_source_idx + 1
        )
        .into();
        cx.notify();
    }

    /// Opens the note editor modal for the currently selected log row
    pub fn open_note_modal_for_selected(&mut self, cx: &mut Context<Self>) {
        let Some(sel_row) = self.selection.selected_row else {
            self.status_text = "Select a log line first to edit note".into();
            cx.notify();
            return;
        };

        let src_idx = if let Some(filter_res) = &self.filter_result {
            if let Some(&idx) = filter_res.visible_indices.get(sel_row) {
                idx
            } else {
                return;
            }
        } else {
            return;
        };

        // If not already bookmarked, bookmark it first
        if !self.bookmarks.is_bookmarked(src_idx) {
            self.toggle_bookmark_on_selected(cx);
        }

        self.open_note_modal_for_source_idx(src_idx, cx);
    }

    /// Opens the note editor modal for a specific source line index
    pub fn open_note_modal_for_source_idx(&mut self, source_idx: usize, cx: &mut Context<Self>) {
        let existing = self
            .bookmarks
            .get(source_idx)
            .and_then(|b| b.note.clone())
            .unwrap_or_default();

        self.note_input
            .update(cx, |input, cx| input.set_text(&existing, cx));
        self.note_editing_source_idx = Some(source_idx);
        self.is_note_modal_open = true;
        self.input_focus = InputFocusTarget::BookmarkNote;
        cx.notify();
    }

    /// Saves the note from the note input into the current bookmark
    pub fn save_bookmark_note(&mut self, cx: &mut Context<Self>) {
        let Some(src_idx) = self.note_editing_source_idx else {
            self.close_note_modal(cx);
            return;
        };

        let note_text = self.note_input.read(cx).text().to_string();
        let updated = self.bookmarks.set_note(src_idx, Some(note_text));

        if updated {
            self.status_text = format!("✓ Updated note for line {}", src_idx + 1).into();
        }

        self.close_note_modal(cx);
    }

    /// Clears the note on the current bookmark
    pub fn clear_bookmark_note(&mut self, cx: &mut Context<Self>) {
        if let Some(src_idx) = self.note_editing_source_idx {
            self.bookmarks.set_note(src_idx, None);
            self.status_text = format!("Cleared note for line {}", src_idx + 1).into();
        }
        self.close_note_modal(cx);
    }

    /// Closes the note edit modal
    pub fn close_note_modal(&mut self, cx: &mut Context<Self>) {
        self.is_note_modal_open = false;
        self.note_editing_source_idx = None;
        if self.input_focus == InputFocusTarget::BookmarkNote {
            self.input_focus = InputFocusTarget::None;
        }
        cx.notify();
    }
}

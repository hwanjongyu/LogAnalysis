use gpui::*;
use loganalysis_core::*;

use crate::types::{FilterEditState, InputFocusTarget};
use super::LogAnalysisWorkspace;

pub const MAX_FILTER_PROFILE_BYTES: u64 = 10 * 1024 * 1024;

static TEMP_FILE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Builds a unique hidden temporary file path adjacent to `target` for crash-safe atomic writes.
pub fn build_unique_temp_path(target: &std::path::Path) -> std::path::PathBuf {
    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("temp");
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = TEMP_FILE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique_nanos = nanos.wrapping_add(seq as u128);
    target.with_file_name(format!(".{file_name}.{pid}.{unique_nanos}.tmp"))
}

/// Reads a filter profile JSON file up to `MAX_FILTER_PROFILE_BYTES` to prevent OOM on oversized files.
pub fn read_filter_profile_bounded(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let meta_len = file.metadata()?.len();
    if meta_len > MAX_FILTER_PROFILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Filter profile exceeds maximum size ({meta_len} > {MAX_FILTER_PROFILE_BYTES} bytes)"
            ),
        ));
    }
    let mut bytes = Vec::with_capacity((meta_len as usize).min(1024 * 1024));
    file.take(MAX_FILTER_PROFILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILTER_PROFILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Filter profile exceeds maximum size ({MAX_FILTER_PROFILE_BYTES} bytes)"),
        ));
    }
    String::from_utf8(bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

impl LogAnalysisWorkspace {
    /// Imports filter profile JSON supporting all 3 legacy Python formats
    pub fn import_filters_json(
        &mut self,
        json_str: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let tabs = parse_filters_json(json_str).map_err(|e| e.to_string())?;
        self.filter_tabs = tabs;
        self.active_tab_index = 0;
        self.reapply_filters(cx);
        self.status_text = format!("Imported {} filter tabs", self.filter_tabs.len()).into();
        Ok(())
    }

    /// Exports active filter tabs to JSON string
    pub fn export_filters_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&self.filter_tabs).map_err(|e| e.to_string())
    }

    pub fn add_filter_rule(&mut self, rule: FilterRule, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            tab.filters.push(rule);
            self.reapply_filters(cx);
        }
    }

    /// Toggles active state of a specific rule
    pub fn toggle_rule_active(&mut self, rule_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if let Some(rule) = tab.filters.get_mut(rule_idx) {
                rule.active = !rule.active;
                self.reapply_filters(cx);
            }
        }
    }

    /// Deletes a rule from active tab
    pub fn delete_rule(&mut self, rule_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if rule_idx < tab.filters.len() {
                tab.filters.remove(rule_idx);
                self.reapply_filters(cx);
            }
        }
    }

    /// Moves a rule up in the active tab (increases precedence)
    pub fn move_rule_up(&mut self, rule_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if rule_idx > 0 && rule_idx < tab.filters.len() {
                tab.filters.swap(rule_idx, rule_idx - 1);
                if self.filter_edit_modal.is_open {
                    if self.filter_edit_modal.editing_index == Some(rule_idx) {
                        self.filter_edit_modal.editing_index = Some(rule_idx - 1);
                    } else if self.filter_edit_modal.editing_index == Some(rule_idx - 1) {
                        self.filter_edit_modal.editing_index = Some(rule_idx);
                    }
                }
                cx.notify();
                self.reapply_filters(cx);
            }
        }
    }

    /// Moves a rule down in the active tab (decreases precedence)
    pub fn move_rule_down(&mut self, rule_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if rule_idx + 1 < tab.filters.len() {
                tab.filters.swap(rule_idx, rule_idx + 1);
                if self.filter_edit_modal.is_open {
                    if self.filter_edit_modal.editing_index == Some(rule_idx) {
                        self.filter_edit_modal.editing_index = Some(rule_idx + 1);
                    } else if self.filter_edit_modal.editing_index == Some(rule_idx + 1) {
                        self.filter_edit_modal.editing_index = Some(rule_idx);
                    }
                }
                cx.notify();
                self.reapply_filters(cx);
            }
        }
    }

    /// Duplicates a rule in active tab
    pub fn duplicate_rule(&mut self, rule_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if let Some(mut rule) = tab.filters.get(rule_idx).cloned() {
                rule.id = FilterRule::default().id;
                tab.filters.insert(rule_idx + 1, rule);
                self.reapply_filters(cx);
            }
        }
    }

    /// Opens modal to add new filter rule
    pub fn open_add_filter_modal(&mut self, cx: &mut Context<Self>) {
        self.modal_pattern_input.update(cx, |input, cx| {
            input.clear(cx);
        });
        self.filter_edit_modal = FilterEditState {
            is_open: true,
            editing_index: None,
            is_regex: false,
            is_case_sensitive: false,
            is_exclude: false,
            bg_color_name: "Yellow".to_string(),
            fg_color_name: "Black".to_string(),
        };
        self.input_focus = InputFocusTarget::ModalPattern;
        cx.notify();
    }

    /// Opens modal to edit existing filter rule
    pub fn open_edit_filter_modal(&mut self, rule_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get(self.active_tab_index) {
            if let Some(rule) = tab.filters.get(rule_idx) {
                let pattern = rule.text.clone();
                self.modal_pattern_input.update(cx, |input, cx| {
                    input.set_text(&pattern, cx);
                });
                self.filter_edit_modal = FilterEditState {
                    is_open: true,
                    editing_index: Some(rule_idx),
                    is_regex: rule.regex,
                    is_case_sensitive: rule.case_sensitive,
                    is_exclude: rule.exclude,
                    bg_color_name: rule.bg_color.clone(),
                    fg_color_name: rule.text_color.clone(),
                };
                self.input_focus = InputFocusTarget::ModalPattern;
                cx.notify();
            }
        }
    }

    /// Closes filter edit modal
    pub fn close_filter_modal(&mut self, cx: &mut Context<Self>) {
        self.filter_edit_modal.is_open = false;
        self.input_focus = InputFocusTarget::None;
        cx.notify();
    }

    /// Saves filter from modal into tab
    pub fn save_modal_filter(&mut self, cx: &mut Context<Self>) {
        let pattern = self.modal_pattern_input.read(cx).text().trim().to_string();
        if pattern.is_empty() {
            self.status_text = "Filter pattern cannot be empty".into();
            cx.notify();
            return;
        }

        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if let Some(idx) = self.filter_edit_modal.editing_index {
                if let Some(existing) = tab.filters.get_mut(idx) {
                    existing.text = pattern;
                    existing.regex = self.filter_edit_modal.is_regex;
                    existing.case_sensitive = self.filter_edit_modal.is_case_sensitive;
                    existing.exclude = self.filter_edit_modal.is_exclude;
                    existing.bg_color = self.filter_edit_modal.bg_color_name.clone();
                    existing.text_color = self.filter_edit_modal.fg_color_name.clone();
                }
            } else {
                let new_rule = FilterRule {
                    text: pattern,
                    regex: self.filter_edit_modal.is_regex,
                    case_sensitive: self.filter_edit_modal.is_case_sensitive,
                    exclude: self.filter_edit_modal.is_exclude,
                    bg_color: self.filter_edit_modal.bg_color_name.clone(),
                    text_color: self.filter_edit_modal.fg_color_name.clone(),
                    active: true,
                    ..Default::default()
                };
                tab.filters.push(new_rule);
            }
            self.filter_edit_modal.is_open = false;
            self.input_focus = InputFocusTarget::None;
            cx.notify();
            self.reapply_filters(cx);
        }
    }

    /// Prompts user to save filter profiles to a JSON file atomically with filters
    pub fn prompt_save_filters(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
        let Ok(json_str) = self.export_filters_json() else {
            self.status_text = "Failed to serialize filters".into();
            cx.notify();
            return;
        };

        let base_dialog = crate::dialogs::build_save_filters_dialog_base();
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
                    let path_for_bg = path.clone();
                    let json_bytes = json_str.into_bytes();
                    let write_res = bg_executor
                        .spawn(async move {
                            let tmp_path = build_unique_temp_path(&path_for_bg);
                            let res = std::fs::write(&tmp_path, &json_bytes)
                                .and_then(|_| std::fs::rename(&tmp_path, &path_for_bg));
                            if res.is_err() {
                                let _ = std::fs::remove_file(&tmp_path);
                            }
                            res
                        })
                        .await;

                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        match write_res {
                            Ok(_) => {
                                workspace.status_text =
                                    format!("Saved filters to {:?}", path).into();
                            }
                            Err(e) => {
                                workspace.status_text = format!("Save failed: {}", e).into();
                            }
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Loads filter profiles asynchronously from a specific file path on disk
    pub fn load_filters_from_path(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let path_buf = path.to_path_buf();
        self.status_text = "Loading filter profile...".into();
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();
            async move {
                let path_clone = path_buf.clone();
                let read_res = bg_executor
                    .spawn(async move { read_filter_profile_bounded(&path_clone) })
                    .await;

                let _ = this.update(&mut async_cx, |workspace, cx| {
                    match read_res {
                        Ok(content) => match workspace.import_filters_json(&content, cx) {
                            Ok(_) => {
                                let fname =
                                    path_buf.file_name().unwrap_or_default().to_string_lossy();
                                workspace.status_text =
                                    format!("✓ Loaded filter profile: {}", fname).into();
                                crate::logging::log_event(
                                    "INFO",
                                    &format!("Loaded filter profile from {:?}", path_buf),
                                );
                            }
                            Err(e) => {
                                workspace.status_text =
                                    format!("Failed to parse profile JSON: {}", e).into();
                                crate::logging::log_event(
                                    "WARN",
                                    &format!(
                                        "Failed to parse filter profile from {:?}: {}",
                                        path_buf, e
                                    ),
                                );
                            }
                        },
                        Err(e) => {
                            workspace.status_text = format!("Failed to read file: {}", e).into();
                            crate::logging::log_event(
                                "WARN",
                                &format!("Failed to read filter profile file {:?}: {}", path_buf, e),
                            );
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Prompts user to select and load a filter profile JSON file with modal file dialog
    pub fn prompt_load_filters(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
        let base_dialog = crate::dialogs::build_load_filters_dialog_base();
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
                        workspace.load_filters_from_path(&path, cx);
                    });
                }
            }
        })
        .detach();
    }

    /// Adds a new filter tab (Ctrl+T)
    pub fn add_filter_tab(&mut self, cx: &mut Context<Self>) {
        let new_tab_name = format!("Tab {}", self.filter_tabs.len() + 1);
        self.filter_tabs.push(FilterTabData {
            name: new_tab_name.clone(),
            enabled: true,
            filters: Vec::new(),
            ..Default::default()
        });
        self.active_tab_index = self.filter_tabs.len() - 1;
        self.reapply_filters(cx);
        self.status_text = format!("Added new filter tab: {}", new_tab_name).into();
        cx.notify();
    }

    /// Deletes the current filter tab (Ctrl+D)
    pub fn delete_current_tab(&mut self, cx: &mut Context<Self>) {
        if self.filter_tabs.len() <= 1 {
            self.status_text = "Cannot delete the last remaining filter tab".into();
            cx.notify();
            return;
        }

        let removed = self.filter_tabs.remove(self.active_tab_index);
        if self.active_tab_index >= self.filter_tabs.len() {
            self.active_tab_index = self.filter_tabs.len() - 1;
        }
        self.reapply_filters(cx);
        self.status_text = format!("Deleted filter tab: {}", removed.name).into();
        cx.notify();
    }

    /// Renames the current filter tab (Ctrl+R)
    pub fn rename_current_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            let next_name = format!("{}_renamed", tab.name);
            tab.name = next_name.clone();
            self.status_text = format!("Renamed tab to: {}", next_name).into();
            cx.notify();
        }
    }

    /// Toggles active state of a whole tab
    pub fn toggle_tab_enabled(&mut self, tab_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(tab_idx) {
            tab.enabled = !tab.enabled;
            self.reapply_filters(cx);
            cx.notify();
        }
    }

    /// Switches active filter tab
    pub fn select_tab(&mut self, tab_idx: usize, cx: &mut Context<Self>) {
        if tab_idx < self.filter_tabs.len() {
            self.active_tab_index = tab_idx;
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_FILTER_PROFILE_BYTES, build_unique_temp_path, read_filter_profile_bounded,
    };

    #[test]
    fn test_build_unique_temp_path() {
        let target = std::path::Path::new("/tmp/filters.json");
        let p1 = build_unique_temp_path(target);
        let p2 = build_unique_temp_path(target);
        assert_ne!(p1, p2);
        assert_eq!(p1.parent(), target.parent());
        let name = p1.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with(".filters.json."));
        assert!(name.ends_with(".tmp"));
    }

    #[test]
    fn test_read_filter_profile_bounded() {
        let dir = std::env::temp_dir();
        let ok_path = build_unique_temp_path(&dir.join("ok_filter_profile.json"));
        std::fs::write(&ok_path, "{\"tabs\":[]}").unwrap();
        let content = read_filter_profile_bounded(&ok_path).unwrap();
        assert_eq!(content, "{\"tabs\":[]}");
        let _ = std::fs::remove_file(&ok_path);

        let oversized_path = build_unique_temp_path(&dir.join("oversized_filter_profile.json"));
        let file = std::fs::File::create(&oversized_path).unwrap();
        file.set_len(MAX_FILTER_PROFILE_BYTES + 1).unwrap();
        let err = read_filter_profile_bounded(&oversized_path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        let _ = std::fs::remove_file(&oversized_path);
    }
}

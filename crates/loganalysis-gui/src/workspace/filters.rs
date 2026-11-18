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

/// Atomically replaces `to` with `from` across platforms (including Windows where `std::fs::rename`
/// fails if the destination file already exists unless `MOVEFILE_REPLACE_EXISTING` is passed).
pub fn atomic_replace_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileExW(
                lpExistingFileName: *const u16,
                lpNewFileName: *const u16,
                dwFlags: u32,
            ) -> i32;
        }
        let from_wide: Vec<u16> = from
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let to_wide: Vec<u16> = to
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let res = unsafe {
            MoveFileExW(
                from_wide.as_ptr(),
                to_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING,
            )
        };
        if res == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to)
    }
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

/// Validates a filter rule pattern before saving.
/// When `is_regex` is true, validates syntax, DFA size limits, and ReDoS nested-repetition rules
/// identically to `loganalysis_core::CompiledMatcher`.
pub fn validate_filter_pattern(
    pattern: &str,
    is_regex: bool,
    is_case_sensitive: bool,
) -> Result<(), String> {
    if pattern.trim().is_empty() {
        return Err("Filter pattern cannot be empty".to_string());
    }
    if is_regex {
        loganalysis_core::CompiledMatcher::new(pattern, is_case_sensitive, true).map_err(|e| {
            match e {
                loganalysis_core::FilterError::InvalidRegex(msg) => {
                    format!("Invalid filter regex: {msg}")
                }
                other => format!("Invalid filter regex: {other}"),
            }
        })?;
    }
    Ok(())
}

/// Validates that imported filter tabs are non-empty so `active_tab_index` stays in bounds.
pub(crate) fn apply_imported_filter_tabs(
    tabs: Vec<FilterTabData>,
) -> Result<Vec<FilterTabData>, String> {
    if tabs.is_empty() {
        return Err("Filter profile contains no tabs".to_string());
    }
    Ok(tabs)
}

/// Atomically writes `bytes` to `path` via a unique adjacent temporary file,
/// creating missing parent directories and removing temp artifacts on failure.
pub(crate) fn write_file_atomically(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp_path = build_unique_temp_path(path);
    let res = std::fs::write(&tmp_path, bytes).and_then(|_| atomic_replace_file(&tmp_path, path));
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    res
}

/// Marks all filter tabs as having unsaved edits (`modified = true`).
pub(crate) fn mark_tabs_modified(tabs: &mut [FilterTabData]) {
    for tab in tabs {
        tab.modified = true;
    }
}

/// Records that the filter profile was saved or loaded from `path`, clearing `modified`
/// and associating all tabs with `path`.
pub(crate) fn mark_tabs_saved(
    tabs: &mut [FilterTabData],
    active_filter_profile_path: &mut Option<std::path::PathBuf>,
    path: &std::path::Path,
) {
    let path_str = path.to_string_lossy().to_string();
    *active_filter_profile_path = Some(path.to_path_buf());
    for tab in tabs {
        tab.file_path = Some(path_str.clone());
        tab.modified = false;
    }
}

/// Resolves the destination path for `SaveFilters`, returning `None` when a Save As prompt is needed.
pub(crate) fn resolve_save_target_path(
    active_filter_profile_path: Option<&std::path::Path>,
    tabs: &[FilterTabData],
    active_tab_index: usize,
) -> Option<std::path::PathBuf> {
    active_filter_profile_path
        .map(std::path::Path::to_path_buf)
        .or_else(|| {
            tabs.get(active_tab_index)
                .and_then(|t| t.file_path.as_ref().map(std::path::PathBuf::from))
        })
}

impl LogAnalysisWorkspace {
    pub(crate) fn mark_filter_tabs_modified(&mut self) {
        mark_tabs_modified(&mut self.filter_tabs);
    }

    pub(crate) fn set_filter_profile_saved(&mut self, path: &std::path::Path) {
        mark_tabs_saved(
            &mut self.filter_tabs,
            &mut self.active_filter_profile_path,
            path,
        );
    }

    /// Imports filter profile JSON supporting all 3 legacy Python formats
    pub fn import_filters_json(
        &mut self,
        json_str: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let parsed = parse_filters_json(json_str).map_err(|e| e.to_string())?;
        let mut tabs = apply_imported_filter_tabs(parsed)?;
        for tab in &mut tabs {
            tab.modified = false;
        }
        self.filter_tabs = tabs;
        self.active_tab_index = 0;
        self.active_filter_profile_path = None;
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
            self.mark_filter_tabs_modified();
            self.reapply_filters(cx);
        }
    }

    /// Toggles active state of a specific rule
    pub fn toggle_rule_active(&mut self, rule_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if let Some(rule) = tab.filters.get_mut(rule_idx) {
                rule.active = !rule.active;
                self.mark_filter_tabs_modified();
                self.reapply_filters(cx);
            }
        }
    }

    /// Deletes a rule from active tab
    pub fn delete_rule(&mut self, rule_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if rule_idx < tab.filters.len() {
                tab.filters.remove(rule_idx);
                self.mark_filter_tabs_modified();
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
                self.mark_filter_tabs_modified();
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
                self.mark_filter_tabs_modified();
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
                self.mark_filter_tabs_modified();
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
        if let Err(err) = validate_filter_pattern(
            &pattern,
            self.filter_edit_modal.is_regex,
            self.filter_edit_modal.is_case_sensitive,
        ) {
            self.status_text = err.into();
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
            self.mark_filter_tabs_modified();
            cx.notify();
            self.reapply_filters(cx);
        }
    }

    fn save_filters_to_path_async(
        &mut self,
        path: std::path::PathBuf,
        json_str: String,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();
            let path_for_bg = path.clone();
            let json_bytes = json_str.into_bytes();

            async move {
                let write_res = bg_executor
                    .spawn(async move { write_file_atomically(&path_for_bg, &json_bytes) })
                    .await;

                let _ = this.update(&mut async_cx, |workspace, cx| {
                    match write_res {
                        Ok(_) => {
                            workspace.set_filter_profile_saved(&path);
                            workspace.status_text = format!("Saved filters to {:?}", path).into();
                        }
                        Err(e) => {
                            workspace.status_text = format!("Save failed: {}", e).into();
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Saves filter profiles directly to the active profile path if known, or falls back to Save As dialog.
    pub fn save_filters(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
        let target_path = resolve_save_target_path(
            self.active_filter_profile_path.as_deref(),
            &self.filter_tabs,
            self.active_tab_index,
        );

        if let Some(path) = target_path {
            let Ok(json_str) = self.export_filters_json() else {
                self.status_text = "Failed to serialize filters".into();
                cx.notify();
                return;
            };
            self.save_filters_to_path_async(path, json_str, cx);
        } else {
            self.prompt_save_filters(parent, cx);
        }
    }

    /// Prompts user to save filter profiles to a JSON file atomically with filters (Save As)
    pub fn prompt_save_filters(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
        let Ok(json_str) = self.export_filters_json() else {
            self.status_text = "Failed to serialize filters".into();
            cx.notify();
            return;
        };

        let dialog = crate::dialogs::prepare_dialog_with_parent(
            crate::dialogs::build_save_filters_dialog_base(),
            parent,
        );

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();
            async move {
                let dialog =
                    crate::dialogs::resolve_dialog_for_display(dialog, &bg_executor).await;
                let file = dialog.save_file().await;

                if let Some(handle) = file {
                    let path = handle.path().to_path_buf();
                    let path_for_bg = path.clone();
                    let json_bytes = json_str.into_bytes();
                    let write_res = bg_executor
                        .spawn(async move { write_file_atomically(&path_for_bg, &json_bytes) })
                        .await;

                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        match write_res {
                            Ok(_) => {
                                workspace.set_filter_profile_saved(&path);
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
                                workspace.set_filter_profile_saved(&path_buf);
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
        let dialog = crate::dialogs::prepare_dialog_with_parent(
            crate::dialogs::build_load_filters_dialog_base(),
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
        self.mark_filter_tabs_modified();
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
        self.mark_filter_tabs_modified();
        self.reapply_filters(cx);
        self.status_text = format!("Deleted filter tab: {}", removed.name).into();
        cx.notify();
    }

    /// Renames the current filter tab (Ctrl+R)
    pub fn rename_current_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            let next_name = format!("{}_renamed", tab.name);
            tab.name = next_name.clone();
            self.mark_filter_tabs_modified();
            self.status_text = format!("Renamed tab to: {}", next_name).into();
            cx.notify();
        }
    }

    /// Toggles active state of a whole tab
    pub fn toggle_tab_enabled(&mut self, tab_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(tab_idx) {
            tab.enabled = !tab.enabled;
            self.mark_filter_tabs_modified();
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
        MAX_FILTER_PROFILE_BYTES, atomic_replace_file, build_unique_temp_path,
        read_filter_profile_bounded, validate_filter_pattern,
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
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let ok_path = dir.path().join("ok_filter_profile.json");
        std::fs::write(&ok_path, "{\"tabs\":[]}").unwrap();
        let content = read_filter_profile_bounded(&ok_path).unwrap();
        assert_eq!(content, "{\"tabs\":[]}");

        let oversized_path = dir.path().join("oversized_filter_profile.json");
        let file = std::fs::File::create(&oversized_path).unwrap();
        file.set_len(MAX_FILTER_PROFILE_BYTES + 1).unwrap();
        let err = read_filter_profile_bounded(&oversized_path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_atomic_replace_file_overwrites_existing() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let target = dir.path().join("atomic_replace_target.json");
        let tmp1 = build_unique_temp_path(&target);
        let tmp2 = build_unique_temp_path(&target);

        std::fs::write(&tmp1, b"first").unwrap();
        atomic_replace_file(&tmp1, &target).unwrap();
        assert!(!tmp1.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"first");

        std::fs::write(&tmp2, b"second").unwrap();
        atomic_replace_file(&tmp2, &target).unwrap();
        assert!(!tmp2.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
    }

    #[test]
    fn test_validate_filter_pattern_rejects_empty_and_whitespace() {
        for empty in ["", "   ", "\t\n"] {
            assert!(
                validate_filter_pattern(empty, false, false).is_err(),
                "Literal empty/whitespace '{empty:?}' should be rejected"
            );
            assert!(
                validate_filter_pattern(empty, true, false).is_err(),
                "Regex empty/whitespace '{empty:?}' should be rejected"
            );
            assert!(
                validate_filter_pattern(empty, true, true).is_err(),
                "Case-sensitive regex empty/whitespace '{empty:?}' should be rejected"
            );
        }
    }

    #[test]
    fn test_validate_filter_pattern_rejects_invalid_regex_syntax() {
        for invalid in ["[unclosed", "(abc"] {
            let err = validate_filter_pattern(invalid, true, false)
                .expect_err("Invalid regex syntax should be rejected");
            assert!(
                err.contains("Invalid filter regex:"),
                "Expected 'Invalid filter regex:' in error, got: {err}"
            );
        }
    }

    #[test]
    fn test_validate_filter_pattern_rejects_redos_nested_repetition() {
        let err = validate_filter_pattern("(?=(a+)+)b", true, false)
            .expect_err("ReDoS lookaround nested repetition should be rejected");
        assert!(
            err.contains("Invalid filter regex:"),
            "Expected 'Invalid filter regex:' in error, got: {err}"
        );
        assert!(
            err.contains("ReDoS"),
            "Expected ReDoS protection message, got: {err}"
        );
    }

    #[test]
    fn test_validate_filter_pattern_accepts_valid_regex_and_literal_brackets() {
        assert_eq!(
            validate_filter_pattern(r"^E/.*timeout\d+$", true, false),
            Ok(())
        );
        assert_eq!(validate_filter_pattern("[ERROR", false, false), Ok(()));
        assert_eq!(validate_filter_pattern("foo(", false, true), Ok(()));
    }

    #[test]
    fn test_apply_imported_filter_tabs_rejects_empty() {
        assert!(super::apply_imported_filter_tabs(Vec::new()).is_err());
        let non_empty = vec![loganalysis_core::FilterTabData::default()];
        assert_eq!(
            super::apply_imported_filter_tabs(non_empty)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_filter_tab_dirty_and_save_target_transitions() {
        use super::{mark_tabs_modified, mark_tabs_saved, resolve_save_target_path};
        use loganalysis_core::FilterTabData;
        use std::path::{Path, PathBuf};

        let mut tabs = vec![
            FilterTabData {
                name: "Tab1".into(),
                ..Default::default()
            },
            FilterTabData {
                name: "Tab2".into(),
                ..Default::default()
            },
        ];
        let mut active_path: Option<PathBuf> = None;

        // 1. Fresh tabs have no saved path -> SaveFilters falls back to Save As prompt
        assert!(tabs.iter().all(|t| !t.modified && t.file_path.is_none()));
        assert_eq!(
            resolve_save_target_path(active_path.as_deref(), &tabs, 0),
            None
        );

        // 2. Editing marks all tabs dirty while still lacking a saved path
        mark_tabs_modified(&mut tabs);
        assert!(tabs.iter().all(|t| t.modified));
        assert_eq!(
            resolve_save_target_path(active_path.as_deref(), &tabs, 0),
            None
        );

        // 3. SaveFiltersAs / load_filters_from_path records path and clears dirty flags
        let path1 = Path::new("/tmp/automotive_profile.json");
        mark_tabs_saved(&mut tabs, &mut active_path, path1);
        assert_eq!(active_path.as_deref(), Some(path1));
        assert!(tabs
            .iter()
            .all(|t| !t.modified && t.file_path.as_deref() == Some("/tmp/automotive_profile.json")));
        assert_eq!(
            resolve_save_target_path(active_path.as_deref(), &tabs, 0),
            Some(path1.to_path_buf())
        );

        // 4. Subsequent edit sets modified = true while keeping target path for direct SaveFilters
        mark_tabs_modified(&mut tabs);
        assert!(tabs.iter().all(|t| t.modified));
        assert_eq!(
            resolve_save_target_path(active_path.as_deref(), &tabs, 1),
            Some(path1.to_path_buf())
        );

        // 5. Fallback to tab.file_path when active_filter_profile_path is None, and SaveFiltersAs rebinds path
        active_path = None;
        assert_eq!(
            resolve_save_target_path(active_path.as_deref(), &tabs, 0),
            Some(path1.to_path_buf())
        );
        let path2 = Path::new("/tmp/automotive_profile_v2.json");
        mark_tabs_saved(&mut tabs, &mut active_path, path2);
        assert_eq!(active_path.as_deref(), Some(path2));
        assert!(tabs
            .iter()
            .all(|t| !t.modified && t.file_path.as_deref() == Some("/tmp/automotive_profile_v2.json")));
    }
}

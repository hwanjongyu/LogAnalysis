mod actions;
mod selection;
mod theme;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use crossbeam_channel::unbounded;
use gpui::*;
use gpui::prelude::FluentBuilder;
use loganalysis_core::*;

use actions::*;
use selection::SelectionModel;
use theme::Theme;

pub enum ActiveLogSource {
    File(FileLogBuffer),
    Stream(StreamLogBuffer),
}

impl LogSource for ActiveLogSource {
    #[inline]
    fn line_count(&self) -> usize {
        match self {
            Self::File(f) => f.line_count(),
            Self::Stream(s) => s.line_count(),
        }
    }

    #[inline]
    fn get_raw_line(&self, idx: usize) -> Option<&[u8]> {
        match self {
            Self::File(f) => f.get_raw_line(idx),
            Self::Stream(s) => s.get_raw_line(idx),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FilterPanelPosition {
    Left,
    #[default]
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InputFocusTarget {
    #[default]
    None,
    SearchBar,
    FilterRuleSearch,
    ModalPattern,
}

#[derive(Clone, Debug, Default)]
pub struct FilterEditState {
    pub is_open: bool,
    pub editing_index: Option<usize>, // None = Add new, Some = Edit existing
    pub pattern: String,
    pub is_regex: bool,
    pub is_case_sensitive: bool,
    pub is_exclude: bool,
    pub bg_color_name: String,
    pub fg_color_name: String,
}

pub struct LogAnalysisWorkspace {
    focus_handle: FocusHandle,
    log_source: Option<Arc<ActiveLogSource>>,
    filter_result: Option<Arc<FilterResult>>,
    scroll_handle: UniformListScrollHandle,
    selection: SelectionModel,
    theme: Theme,
    input_focus: InputFocusTarget,

    // Filter state
    filter_tabs: Vec<FilterTabData>,
    active_tab_index: usize,
    show_only_filtered: bool,
    cancel_generation: Arc<AtomicU64>,
    filter_edit_modal: FilterEditState,
    filter_search_query: String,

    // Search state
    search_query: String,
    is_search_visible: bool,
    is_search_regex: bool,
    is_search_case_sensitive: bool,

    // ADB state
    adb_devices: Vec<String>,
    selected_device: Option<String>,
    adb_stream_handle: Option<LogcatStreamHandle>,
    is_streaming: bool,
    is_stream_paused: bool,
    auto_scroll: bool,

    // Display & Status
    font_size: Pixels,
    status_text: SharedString,
    active_file_name: Option<SharedString>,
    is_dark_theme: bool,
    full_line_display: bool,
    filter_panel_position: FilterPanelPosition,
}

impl LogAnalysisWorkspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let default_tab = FilterTabData {
            name: "Default".to_string(),
            enabled: true,
            filters: vec![
                FilterRule {
                    text: "Fatal".to_string(),
                    bg_color: "Red".to_string(),
                    text_color: "White".to_string(),
                    active: true,
                    ..Default::default()
                },
                FilterRule {
                    text: "Exception".to_string(),
                    bg_color: "Yellow".to_string(),
                    text_color: "Black".to_string(),
                    active: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        Self {
            focus_handle: cx.focus_handle(),
            log_source: None,
            filter_result: None,
            scroll_handle: UniformListScrollHandle::new(),
            selection: SelectionModel::new(),
            theme: Theme::default(),
            input_focus: InputFocusTarget::None,

            filter_tabs: vec![default_tab],
            active_tab_index: 0,
            show_only_filtered: false,
            cancel_generation: Arc::new(AtomicU64::new(0)),
            filter_edit_modal: FilterEditState::default(),
            filter_search_query: String::new(),

            search_query: String::new(),
            is_search_visible: false,
            is_search_regex: false,
            is_search_case_sensitive: false,

            adb_devices: Vec::new(),
            selected_device: None,
            adb_stream_handle: None,
            is_streaming: false,
            is_stream_paused: false,
            auto_scroll: true,

            font_size: px(12.0),
            status_text: "Ready | Open a file or start ADB streaming".into(),
            active_file_name: None,
            is_dark_theme: false,
            full_line_display: false,
            filter_panel_position: FilterPanelPosition::Bottom,
        }
    }

    /// Prompts user to select a log file using native OS file dialog
    pub fn prompt_open_file(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Log File".into()),
        });

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut async_cx = cx.clone();
            async move {
                if let Ok(Ok(Some(paths))) = rx.await {
                    if let Some(path) = paths.into_iter().next() {
                        let _ = this.update(&mut async_cx, |workspace, cx| {
                            workspace.open_file(&path, cx);
                        });
                    }
                }
            }
        }).detach();
    }

    /// Loads a log file using FileLogBuffer and executes filtering
    pub fn open_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        match FileLogBuffer::open(path) {
            Ok(buf) => {
                let arc_buf = Arc::new(ActiveLogSource::File(buf));
                self.log_source = Some(arc_buf.clone());
                self.active_file_name = Some(path.file_name().unwrap_or_default().to_string_lossy().to_string().into());
                self.status_text = format!("Loaded {} lines from {:?}", arc_buf.line_count(), path).into();
                self.reapply_filters(cx);
            }
            Err(e) => {
                self.status_text = format!("Failed to open file: {}", e).into();
                cx.notify();
            }
        }
    }

    /// Re-evaluates active filters in the background using Rayon
    pub fn reapply_filters(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.log_source.clone() else {
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

        let generation = self.cancel_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let token = self.cancel_generation.clone();
        let show_only = self.show_only_filtered;

        let source_for_bg = source.clone();
        let source_for_ui = source.clone();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();

            async move {
                let result = bg_executor.spawn(async move {
                    execute_filtering(&*source_for_bg, &active_filters, show_only, Some(&token), generation)
                }).await;

                if let Some(res) = result {
                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        let prev_anchor = workspace.selection.anchor_source_idx;
                        workspace.filter_result = Some(Arc::new(res));

                        // Viewport anchor preservation
                        if let (Some(anchor), Some(res)) = (prev_anchor, &workspace.filter_result) {
                            if let Some(new_row) = find_nearest_anchor_row(&res.visible_indices, anchor) {
                                workspace.selection.selected_row = Some(new_row);
                                workspace.scroll_handle.scroll_to_item(new_row, ScrollStrategy::Center);
                            }
                        }

                        let visible_count = workspace.filter_result.as_ref().map(|r| r.visible_indices.len()).unwrap_or(0);
                        let total_count = source_for_ui.line_count();
                        workspace.status_text = format!(
                            "Visible: {} / {} lines ({} matched)",
                            visible_count,
                            total_count,
                            workspace.filter_result.as_ref().map(|r| r.total_matched_lines).unwrap_or(0)
                        ).into();
                        cx.notify();
                    });
                }
            }
        }).detach();
    }

    /// Toggles "Show Only Filtered" mode (Ctrl+H) with anchor preservation
    pub fn toggle_filtered_only(&mut self, cx: &mut Context<Self>) {
        self.show_only_filtered = !self.show_only_filtered;
        self.reapply_filters(cx);
    }

    /// Scans for connected ADB devices
    pub fn refresh_adb_devices(&mut self, cx: &mut Context<Self>) {
        match scan_adb_devices(Duration::from_secs(2)) {
            Ok(devices) => {
                self.selected_device = devices.first().cloned();
                self.adb_devices = devices;
                self.status_text = format!("Found {} ADB devices", self.adb_devices.len()).into();
            }
            Err(e) => {
                self.status_text = format!("ADB scan error: {}", e).into();
            }
        }
        cx.notify();
    }

    /// Starts streaming logcat from selected ADB device
    pub fn start_adb_stream(&mut self, cx: &mut Context<Self>) {
        if self.is_streaming {
            return;
        }

        let (tx, rx) = unbounded();
        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let serial = self.selected_device.as_deref();

        match start_logcat_stream(serial, tx, stop_flag.clone()) {
            Ok(handle) => {
                self.adb_stream_handle = Some(handle);
                self.is_streaming = true;

                let stream_buf = StreamLogBuffer::default();
                self.log_source = Some(Arc::new(ActiveLogSource::Stream(stream_buf)));
                self.status_text = "Streaming adb logcat...".into();

                // Worker task to poll incoming log chunks
                cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                    let mut async_cx = cx.clone();
                    async move {
                        while let Ok(chunk) = rx.recv() {
                            let should_continue = this.update(&mut async_cx, |workspace, cx| {
                                if !workspace.is_stream_paused {
                                    if let Some(src) = &workspace.log_source {
                                        if let ActiveLogSource::Stream(ref stream) = **src {
                                            let mut new_stream = stream.clone();
                                            new_stream.append_chunk(chunk);
                                            new_stream.trim_if_needed();
                                            workspace.log_source = Some(Arc::new(ActiveLogSource::Stream(new_stream)));
                                            workspace.reapply_filters(cx);

                                            // Smart Auto-Scroll to bottom if enabled
                                            if workspace.auto_scroll {
                                                if let Some(res) = &workspace.filter_result {
                                                    let count = res.visible_indices.len();
                                                    if count > 0 {
                                                        workspace.scroll_handle.scroll_to_item(count - 1, ScrollStrategy::Bottom);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                cx.notify();
                                workspace.is_streaming
                            }).unwrap_or(false);

                            if !should_continue {
                                break;
                            }
                        }
                    }
                }).detach();
            }
            Err(e) => {
                self.status_text = format!("Failed to start ADB stream: {}", e).into();
            }
        }
        cx.notify();
    }

    /// Stops ADB streaming cleanly via RAII handle
    pub fn stop_adb_stream(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.adb_stream_handle.take() {
            handle.stop();
        }
        self.is_streaming = false;
        self.status_text = "ADB stream stopped".into();
        cx.notify();
    }

    /// Cyclic search forward or backward
    pub fn search(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.search_query.is_empty() {
            return;
        }

        let Some(source) = self.log_source.clone() else { return };
        let Some(res) = self.filter_result.clone() else { return };

        let query = SearchQuery {
            pattern: self.search_query.clone(),
            case_sensitive: self.is_search_case_sensitive,
            is_regex: self.is_search_regex,
            forward,
        };

        let start_row = self.selection.selected_row;

        if let Ok(Some(found)) = search_in_log(
            &*source,
            &res.visible_indices,
            &query,
            start_row,
            None,
            0,
            None,
        ) {
            self.selection.select(found.visible_row, found.real_idx);
            self.scroll_handle.scroll_to_item(found.visible_row, ScrollStrategy::Center);
            self.status_text = format!("Found match at line {}", found.real_idx + 1).into();
            cx.notify();
        } else {
            self.status_text = "Search pattern not found".into();
            cx.notify();
        }
    }

    /// Imports filter profile JSON supporting all 3 legacy Python formats
    pub fn import_filters_json(&mut self, json_str: &str, cx: &mut Context<Self>) -> Result<(), String> {
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

    /// Toggles the floating find bar
    pub fn toggle_search_bar(&mut self, cx: &mut Context<Self>) {
        self.is_search_visible = !self.is_search_visible;
        if self.is_search_visible {
            self.input_focus = InputFocusTarget::SearchBar;
        } else if self.input_focus == InputFocusTarget::SearchBar {
            self.input_focus = InputFocusTarget::None;
        }
        cx.notify();
    }

    /// Handles keyboard input for active inline text boxes
    pub fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.input_focus == InputFocusTarget::None {
            return;
        }

        let key = event.keystroke.key.as_str();
        let m = &event.keystroke.modifiers;

        // Support common text box shortcuts (Ctrl+V paste, Ctrl+U clear)
        if m.control && !m.alt && !m.platform {
            if key == "v" || key == "V" {
                if let Some(clipboard) = cx.read_from_clipboard() {
                    if let Some(text) = clipboard.text() {
                        let clean = text.replace(['\r', '\n'], "");
                        match self.input_focus {
                            InputFocusTarget::ModalPattern => {
                                self.filter_edit_modal.pattern.push_str(&clean);
                                cx.notify();
                            }
                            InputFocusTarget::SearchBar => {
                                self.search_query.push_str(&clean);
                                cx.notify();
                            }
                            InputFocusTarget::FilterRuleSearch => {
                                self.filter_search_query.push_str(&clean);
                                cx.notify();
                            }
                            InputFocusTarget::None => {}
                        }
                    }
                }
                return;
            } else if key == "u" || key == "U" {
                match self.input_focus {
                    InputFocusTarget::ModalPattern => {
                        self.filter_edit_modal.pattern.clear();
                        cx.notify();
                    }
                    InputFocusTarget::SearchBar => {
                        self.search_query.clear();
                        cx.notify();
                    }
                    InputFocusTarget::FilterRuleSearch => {
                        self.filter_search_query.clear();
                        cx.notify();
                    }
                    InputFocusTarget::None => {}
                }
                return;
            }
        }

        // Ignore other shortcut combinations (Ctrl/Alt/Platform) except Shift
        if m.control || m.alt || m.platform {
            return;
        }

        match key {
            "escape" => {
                match self.input_focus {
                    InputFocusTarget::ModalPattern => {
                        self.close_filter_modal(cx);
                    }
                    InputFocusTarget::SearchBar => {
                        self.is_search_visible = false;
                        self.input_focus = InputFocusTarget::None;
                        cx.notify();
                    }
                    InputFocusTarget::FilterRuleSearch => {
                        self.filter_search_query.clear();
                        self.input_focus = InputFocusTarget::None;
                        cx.notify();
                    }
                    InputFocusTarget::None => {}
                }
            }
            "enter" => {
                match self.input_focus {
                    InputFocusTarget::ModalPattern => {
                        self.save_modal_filter(cx);
                    }
                    InputFocusTarget::SearchBar => {
                        self.search(true, cx);
                    }
                    InputFocusTarget::FilterRuleSearch => {
                        self.input_focus = InputFocusTarget::None;
                        cx.notify();
                    }
                    InputFocusTarget::None => {}
                }
            }
            "backspace" => {
                match self.input_focus {
                    InputFocusTarget::ModalPattern => {
                        self.filter_edit_modal.pattern.pop();
                        cx.notify();
                    }
                    InputFocusTarget::SearchBar => {
                        self.search_query.pop();
                        cx.notify();
                    }
                    InputFocusTarget::FilterRuleSearch => {
                        self.filter_search_query.pop();
                        cx.notify();
                    }
                    InputFocusTarget::None => {}
                }
            }
            "space" => {
                match self.input_focus {
                    InputFocusTarget::ModalPattern => {
                        self.filter_edit_modal.pattern.push(' ');
                        cx.notify();
                    }
                    InputFocusTarget::SearchBar => {
                        self.search_query.push(' ');
                        cx.notify();
                    }
                    InputFocusTarget::FilterRuleSearch => {
                        self.filter_search_query.push(' ');
                        cx.notify();
                    }
                    InputFocusTarget::None => {}
                }
            }
            _ => {
                if let Some(ch) = &event.keystroke.key_char {
                    if !ch.is_empty() && !ch.chars().any(|c| c.is_control()) {
                        match self.input_focus {
                            InputFocusTarget::ModalPattern => {
                                self.filter_edit_modal.pattern.push_str(ch);
                                cx.notify();
                            }
                            InputFocusTarget::SearchBar => {
                                self.search_query.push_str(ch);
                                cx.notify();
                            }
                            InputFocusTarget::FilterRuleSearch => {
                                self.filter_search_query.push_str(ch);
                                cx.notify();
                            }
                            InputFocusTarget::None => {}
                        }
                    }
                } else if key.len() == 1 {
                    let c = if m.shift {
                        match key {
                            "1" => "!".to_string(),
                            "2" => "@".to_string(),
                            "3" => "#".to_string(),
                            "4" => "$".to_string(),
                            "5" => "%".to_string(),
                            "6" => "^".to_string(),
                            "7" => "&".to_string(),
                            "8" => "*".to_string(),
                            "9" => "(".to_string(),
                            "0" => ")".to_string(),
                            "-" => "_".to_string(),
                            "=" => "+".to_string(),
                            "[" => "{".to_string(),
                            "]" => "}".to_string(),
                            "\\" => "|".to_string(),
                            ";" => ":".to_string(),
                            "'" => "\"".to_string(),
                            "," => "<".to_string(),
                            "." => ">".to_string(),
                            "/" => "?".to_string(),
                            "`" => "~".to_string(),
                            _ => key.to_uppercase(),
                        }
                    } else {
                        key.to_string()
                    };
                    match self.input_focus {
                        InputFocusTarget::ModalPattern => {
                            self.filter_edit_modal.pattern.push_str(&c);
                            cx.notify();
                        }
                        InputFocusTarget::SearchBar => {
                            self.search_query.push_str(&c);
                            cx.notify();
                        }
                        InputFocusTarget::FilterRuleSearch => {
                            self.filter_search_query.push_str(&c);
                            cx.notify();
                        }
                        InputFocusTarget::None => {}
                    }
                }
            }
        }
    }

    /// Adds a new filter rule to the active tab
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

    /// Duplicates a rule in active tab
    pub fn duplicate_rule(&mut self, rule_idx: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if let Some(rule) = tab.filters.get(rule_idx).cloned() {
                tab.filters.insert(rule_idx + 1, rule);
                self.reapply_filters(cx);
            }
        }
    }

    /// Opens modal to add new filter rule
    pub fn open_add_filter_modal(&mut self, cx: &mut Context<Self>) {
        self.filter_edit_modal = FilterEditState {
            is_open: true,
            editing_index: None,
            pattern: String::new(),
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
                self.filter_edit_modal = FilterEditState {
                    is_open: true,
                    editing_index: Some(rule_idx),
                    pattern: rule.text.clone(),
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
        let pattern = self.filter_edit_modal.pattern.trim().to_string();
        if pattern.is_empty() {
            self.status_text = "Filter pattern cannot be empty".into();
            cx.notify();
            return;
        }

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

        if let Some(tab) = self.filter_tabs.get_mut(self.active_tab_index) {
            if let Some(idx) = self.filter_edit_modal.editing_index {
                if idx < tab.filters.len() {
                    tab.filters[idx] = new_rule;
                }
            } else {
                tab.filters.push(new_rule);
            }
            self.filter_edit_modal.is_open = false;
            self.input_focus = InputFocusTarget::None;
            self.reapply_filters(cx);
        }
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
            src.get_line(r_idx).map(|s| s.to_string()).unwrap_or_default()
        } else {
            String::new()
        };

        if !text_to_copy.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text_to_copy));
            self.status_text = "Copied line to clipboard".into();
            cx.notify();
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
            src.get_line(r_idx).map(|s| s.to_string()).unwrap_or_default()
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
            bg_color: if exclude { "None".to_string() } else { "Green".to_string() },
            text_color: if exclude { "None".to_string() } else { "Black".to_string() },
            active: true,
            ..Default::default()
        };

        self.add_filter_rule(rule, cx);
        self.status_text = format!("Added quick {} filter: '{}'", if exclude { "exclude" } else { "include" }, pattern).into();
        cx.notify();
    }

    /// Prompts user to save filter profiles to a JSON file
    pub fn prompt_save_filters(&mut self, cx: &mut Context<Self>) {
        let Ok(json_str) = self.export_filters_json() else {
            self.status_text = "Failed to serialize filters".into();
            cx.notify();
            return;
        };

        let current_dir = std::env::current_dir().unwrap_or_default();
        let rx = cx.prompt_for_new_path(&current_dir, Some("filters.json"));

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut async_cx = cx.clone();
            async move {
                if let Ok(Ok(Some(path))) = rx.await {
                    let write_res = std::fs::write(&path, json_str.as_bytes());
                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        match write_res {
                            Ok(_) => {
                                workspace.status_text = format!("Saved filters to {:?}", path).into();
                            }
                            Err(e) => {
                                workspace.status_text = format!("Save failed: {}", e).into();
                            }
                        }
                        cx.notify();
                    });
                }
            }
        }).detach();
    }

    /// Clears currently loaded logs (Ctrl+K)
    pub fn clear_logs(&mut self, cx: &mut Context<Self>) {
        self.log_source = None;
        self.filter_result = None;
        self.selection.clear();
        self.active_file_name = None;
        self.status_text = "Logs cleared".into();
        cx.notify();
    }

    /// Adjusts font size (Zoom in/out)
    pub fn zoom(&mut self, delta: f32, cx: &mut Context<Self>) {
        let current: f32 = self.font_size.into();
        let new_size = (current + delta).clamp(8.0, 28.0);
        self.font_size = px(new_size);
        self.status_text = format!("Font size: {:.0}px", new_size).into();
        cx.notify();
    }

    /// Toggles ADB stream pause
    pub fn toggle_pause_stream(&mut self, cx: &mut Context<Self>) {
        if self.input_focus != InputFocusTarget::None {
            return;
        }
        if self.is_streaming {
            self.is_stream_paused = !self.is_stream_paused;
            self.status_text = if self.is_stream_paused {
                "ADB streaming PAUSED (Press Ctrl+Space to resume)".into()
            } else {
                "ADB streaming RESUMED".into()
            };
            cx.notify();
        }
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

    /// Toggles light / dark theme
    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.is_dark_theme = !self.is_dark_theme;
        self.theme = if self.is_dark_theme {
            Theme::dark()
        } else {
            Theme::light()
        };
        self.status_text = if self.is_dark_theme {
            "Switched to Dark Theme".into()
        } else {
            "Switched to Light Theme".into()
        };
        cx.notify();
    }

    /// Toggles full line display mode (horizontal scroll / truncate)
    pub fn toggle_full_line_display(&mut self, cx: &mut Context<Self>) {
        self.full_line_display = !self.full_line_display;
        self.status_text = if self.full_line_display {
            "Full Line Display: ON (Horizontal scroll enabled)".into()
        } else {
            "Full Line Display: OFF (Truncated)".into()
        };
        cx.notify();
    }

    /// Toggles smart auto-scroll for streaming
    pub fn toggle_auto_scroll(&mut self, cx: &mut Context<Self>) {
        self.auto_scroll = !self.auto_scroll;
        self.status_text = if self.auto_scroll {
            "Auto-scroll: ON".into()
        } else {
            "Auto-scroll: OFF".into()
        };
        cx.notify();
    }

    /// Toggles filter panel dock position between Left and Bottom
    pub fn toggle_filter_panel_position(&mut self, cx: &mut Context<Self>) {
        self.filter_panel_position = match self.filter_panel_position {
            FilterPanelPosition::Left => FilterPanelPosition::Bottom,
            FilterPanelPosition::Bottom => FilterPanelPosition::Left,
        };
        self.status_text = match self.filter_panel_position {
            FilterPanelPosition::Left => "Filter Panel docked to LEFT".into(),
            FilterPanelPosition::Bottom => "Filter Panel docked to BOTTOM".into(),
        };
        cx.notify();
    }
}

impl Render for LogAnalysisWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus_handle.is_focused(window) {
            window.focus(&self.focus_handle);
        }
        let theme = self.theme;
        let row_count = self.filter_result.as_ref().map(|r| r.visible_indices.len()).unwrap_or(0);
        let total_lines = self.log_source.as_ref().map(|s| s.line_count()).unwrap_or(0);
        let total_matched: usize = self
            .filter_result
            .as_ref()
            .map(|r| r.filter_counts.iter().sum())
            .unwrap_or(0);
        let font_size = self.font_size;
        let font_size_f32: f32 = font_size.into();
        let row_height = font_size * 1.55;

        let filter_res = self.filter_result.clone();
        let source = self.log_source.clone();
        let selected = self.selection.selected_row;
        let selected_real_line = self.selection.anchor_source_idx;
        let has_log_source = self.log_source.is_some();
        let weak_self = cx.weak_entity();
        let input_focus = self.input_focus;

        let visible_pct = if total_lines > 0 {
            (row_count as f64 / total_lines as f64) * 100.0
        } else {
            0.0
        };

        div()
            .id("workspace-container")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                this.handle_key_down(event, cx);
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _window, cx| {
                if let Some(path) = paths.paths().first() {
                    this.open_file(path, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenFile, _window, cx| {
                this.prompt_open_file(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFilteredOnly, _window, cx| {
                this.toggle_filtered_only(cx);
            }))
            .on_action(cx.listener(|this, _: &Find, _window, cx| {
                this.toggle_search_bar(cx);
            }))
            .on_action(cx.listener(|this, _: &FindNext, _window, cx| {
                this.search(true, cx);
            }))
            .on_action(cx.listener(|this, _: &FindPrev, _window, cx| {
                this.search(false, cx);
            }))
            .on_action(cx.listener(|this, _: &CopySelection, _window, cx| {
                this.copy_selection(cx);
            }))
            .on_action(cx.listener(|this, _: &AddFilter, _window, cx| {
                this.open_add_filter_modal(cx);
            }))
            .on_action(cx.listener(|this, _: &SaveFilters, _window, cx| {
                this.prompt_save_filters(cx);
            }))
            .on_action(cx.listener(|this, _: &SaveFiltersAs, _window, cx| {
                this.prompt_save_filters(cx);
            }))
            .on_action(cx.listener(|this, _: &ClearLogs, _window, cx| {
                this.clear_logs(cx);
            }))
            .on_action(cx.listener(|this, _: &AddFilterTab, _window, cx| {
                this.add_filter_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &DeleteFilterTab, _window, cx| {
                this.delete_current_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &RenameFilterTab, _window, cx| {
                this.rename_current_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleTheme, _window, cx| {
                this.toggle_theme(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFullLineDisplay, _window, cx| {
                this.toggle_full_line_display(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleAutoScroll, _window, cx| {
                this.toggle_auto_scroll(cx);
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _window, cx| {
                this.zoom(1.0, cx);
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _window, cx| {
                this.zoom(-1.0, cx);
            }))
            .on_action(cx.listener(|this, _: &TogglePauseStream, _window, cx| {
                this.toggle_pause_stream(cx);
            }))
            .on_action(cx.listener(|this, _: &AddIncludeFromSelection, _window, cx| {
                this.add_quick_filter_from_selection(false, cx);
            }))
            .on_action(cx.listener(|this, _: &AddExcludeFromSelection, _window, cx| {
                this.add_quick_filter_from_selection(true, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFilterPanelPosition, _window, cx| {
                this.toggle_filter_panel_position(cx);
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_dark)
            .text_color(theme.text_primary)
            // ================================================================
            // 1. TOP TOOLBAR (3-Section Grouped & Segmented Layout)
            // ================================================================
            .child(
                div()
                    .h(px(44.0))
                    .w_full()
                    .bg(theme.bg_header)
                    .border_b_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(14.0))
                    // Left Section: Brand + File Actions + Active File Pill
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            // App Brand Pill
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .pr(px(6.0))
                                    .child(
                                        div()
                                            .w(px(8.0))
                                            .h(px(8.0))
                                            .rounded_full()
                                            .bg(theme.accent),
                                    )
                                    .child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .text_size(px(13.0))
                                            .child("LogAnalysis"),
                                    ),
                            )
                            // File Action Group
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .p(px(2.0))
                                    .rounded_md()
                                    .bg(theme.bg_panel)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .id("btn-toolbar-open")
                                            .px(px(8.0))
                                            .py(px(4.0))
                                            .rounded_sm()
                                            .bg(theme.accent)
                                            .text_color(rgb(0xffffff))
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.prompt_open_file(cx);
                                            }))
                                            .child("Open"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-toolbar-save-filters")
                                            .px(px(8.0))
                                            .py(px(4.0))
                                            .rounded_sm()
                                            .hover(move |s| s.bg(theme.btn_hover))
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.prompt_save_filters(cx);
                                            }))
                                            .child("Save Profile"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-toolbar-clear-logs")
                                            .px(px(8.0))
                                            .py(px(4.0))
                                            .rounded_sm()
                                            .hover(move |s| s.bg(theme.btn_hover))
                                            .text_size(px(11.0))
                                            .text_color(theme.text_muted)
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.clear_logs(cx);
                                            }))
                                            .child("Clear"),
                                    ),
                            )
                            // Active File Pill Badge
                            .child(
                                div()
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .rounded_full()
                                    .bg(theme.bg_badge)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .text_size(px(11.0))
                                    .font_family("monospace")
                                    .text_color(if self.active_file_name.is_some() {
                                        theme.text_primary
                                    } else {
                                        theme.text_muted
                                    })
                                    .child(if let Some(name) = &self.active_file_name {
                                        format!("📄 {}", name)
                                    } else {
                                        "No active log".into()
                                    }),
                            ),
                    )
                    // Center Section: Segmented View Toggles & Selection Actions
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            // Segmented Control: All vs Filtered Only
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .p(px(2.0))
                                    .rounded_md()
                                    .bg(theme.bg_panel)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .id("seg-all-lines")
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded_sm()
                                            .bg(if !self.show_only_filtered {
                                                theme.accent
                                            } else {
                                                rgba(0x00000000)
                                            })
                                            .text_color(if !self.show_only_filtered {
                                                rgb(0xffffff)
                                            } else {
                                                theme.text_muted
                                            })
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                if this.show_only_filtered {
                                                    this.toggle_filtered_only(cx);
                                                }
                                            }))
                                            .child("All Lines"),
                                    )
                                    .child(
                                        div()
                                            .id("seg-filtered-only")
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded_sm()
                                            .bg(if self.show_only_filtered {
                                                theme.accent
                                            } else {
                                                rgba(0x00000000)
                                            })
                                            .text_color(if self.show_only_filtered {
                                                rgb(0xffffff)
                                            } else {
                                                theme.text_muted
                                            })
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                if !this.show_only_filtered {
                                                    this.toggle_filtered_only(cx);
                                                }
                                            }))
                                            .child("Filtered Only"),
                                    ),
                            )
                            // View Options Pill Group: Full Width & Auto-Scroll
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .p(px(2.0))
                                    .rounded_md()
                                    .bg(theme.bg_panel)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .id("btn-toolbar-full-line")
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded_sm()
                                            .bg(if self.full_line_display {
                                                theme.accent_muted
                                            } else {
                                                rgba(0x00000000)
                                            })
                                            .text_color(if self.full_line_display {
                                                theme.accent
                                            } else {
                                                theme.text_muted
                                            })
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.toggle_full_line_display(cx);
                                            }))
                                            .child(if self.full_line_display {
                                                "↔ H-Scroll"
                                            } else {
                                                "⇥ Clip"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("btn-toolbar-auto-scroll")
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded_sm()
                                            .bg(if self.auto_scroll {
                                                theme.accent_muted
                                            } else {
                                                rgba(0x00000000)
                                            })
                                            .text_color(if self.auto_scroll {
                                                theme.accent
                                            } else {
                                                theme.text_muted
                                            })
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.toggle_auto_scroll(cx);
                                            }))
                                            .child("⬇ Tail"),
                                    )
                                    // Filter Dock Position Toggle (Left vs Bottom)
                                    .child(
                                        div()
                                            .id("btn-toolbar-filter-dock")
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded_sm()
                                            .bg(if self.filter_panel_position == FilterPanelPosition::Bottom {
                                                theme.accent_muted
                                            } else {
                                                rgba(0x00000000)
                                            })
                                            .text_color(if self.filter_panel_position == FilterPanelPosition::Bottom {
                                                theme.accent
                                            } else {
                                                theme.text_muted
                                            })
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.toggle_filter_panel_position(cx);
                                            }))
                                            .child(if self.filter_panel_position == FilterPanelPosition::Bottom {
                                                "⬒ Bottom Dock"
                                            } else {
                                                "◨ Left Dock"
                                            }),
                                    ),
                            )
                            // Selection Quick Actions
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .p(px(2.0))
                                    .rounded_md()
                                    .bg(theme.bg_panel)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .id("btn-toolbar-copy-line")
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded_sm()
                                            .hover(move |s| s.bg(theme.btn_hover))
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.copy_selection(cx);
                                            }))
                                            .child("Copy Line"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-toolbar-quick-filter")
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded_sm()
                                            .hover(move |s| s.bg(theme.btn_hover))
                                            .text_size(px(11.0))
                                            .text_color(theme.accent)
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.add_quick_filter_from_selection(false, cx);
                                            }))
                                            .child("+ Filter Sel"),
                                    ),
                            ),
                    )
                    // Right Section: Search, Theme Toggle, and ADB Live Stream Pill
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            // Find Toggle Button
                            .child(
                                div()
                                    .id("btn-toolbar-find")
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .rounded_md()
                                    .border_1()
                                    .border_color(if self.is_search_visible {
                                        theme.accent
                                    } else {
                                        theme.border_subtle
                                    })
                                    .bg(if self.is_search_visible {
                                        theme.accent_muted
                                    } else {
                                        theme.bg_panel
                                    })
                                    .text_color(if self.is_search_visible {
                                        theme.accent
                                    } else {
                                        theme.text_primary
                                    })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.toggle_search_bar(cx);
                                    }))
                                    .child("🔍 Find"),
                            )
                            // Theme Switcher
                            .child(
                                div()
                                    .id("btn-toolbar-theme-toggle")
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded_md()
                                    .bg(theme.bg_panel)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .hover(move |s| s.bg(theme.btn_hover))
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.toggle_theme(cx);
                                    }))
                                    .child(if self.is_dark_theme { "☀ Light" } else { "🌙 Dark" }),
                            )
                            // Pause Stream Button (when streaming)
                            .children(if self.is_streaming {
                                Some(
                                    div()
                                        .id("btn-toolbar-pause-adb")
                                        .px(px(8.0))
                                        .py(px(4.0))
                                        .rounded_md()
                                        .bg(if self.is_stream_paused {
                                            theme.warning_amber
                                        } else {
                                            theme.bg_panel
                                        })
                                        .text_color(if self.is_stream_paused {
                                            rgb(0x000000)
                                        } else {
                                            theme.text_primary
                                        })
                                        .border_1()
                                        .border_color(theme.border_subtle)
                                        .text_size(px(11.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _, _window, cx| {
                                            this.toggle_pause_stream(cx);
                                        }))
                                        .child(if self.is_stream_paused {
                                            "▶ Resume"
                                        } else {
                                            "⏸ Pause"
                                        }),
                                )
                            } else {
                                None
                            })
                            // Start / Stop ADB Stream Pill
                            .child(
                                div()
                                    .id("btn-toolbar-adb")
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .rounded_md()
                                    .bg(if self.is_streaming {
                                        theme.error_red
                                    } else {
                                        theme.success_green
                                    })
                                    .text_color(rgb(0xffffff))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        if this.is_streaming {
                                            this.stop_adb_stream(cx);
                                        } else {
                                            this.start_adb_stream(cx);
                                        }
                                    }))
                                    .child(if self.is_streaming {
                                        "⏹ Stop ADB"
                                    } else {
                                        "⚡ Live ADB"
                                    }),
                            ),
                    ),
            )
            // ================================================================
            // 2. MAIN WORKSPACE BODY (Configurable Left or Bottom Filter Dock)
            // ================================================================
            .child({
                let is_bottom_dock = self.filter_panel_position == FilterPanelPosition::Bottom;

                // --- 2A. FILTER PANEL (Left Sidebar or Bottom Dock) ---
                let filter_panel_el = div()
                    .id("filter-panel-container")
                    .bg(theme.bg_panel)
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .when(!is_bottom_dock, |s| {
                        s.w(px(310.0)).h_full().border_r_1().border_color(theme.border)
                    })
                    .when(is_bottom_dock, |s| {
                        s.w_full().h(px(220.0)).border_t_1().border_color(theme.border)
                    })
                    // Tab Bar Header
                    .child(
                        div()
                            .h(px(36.0))
                            .w_full()
                            .bg(theme.bg_header)
                            .border_b_1()
                            .border_color(theme.border)
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(8.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .overflow_hidden()
                                    .children(
                                        self.filter_tabs
                                            .iter()
                                            .enumerate()
                                            .map(|(t_idx, tab)| {
                                                let is_active_tab = t_idx == self.active_tab_index;
                                                div()
                                                    .id(ElementId::NamedInteger(
                                                        "tab-header-btn".into(),
                                                        t_idx as u64,
                                                    ))
                                                    .px(px(8.0))
                                                    .py(px(3.0))
                                                    .rounded_md()
                                                    .bg(if is_active_tab {
                                                        theme.accent
                                                    } else {
                                                        theme.bg_card
                                                    })
                                                    .text_color(if is_active_tab {
                                                        rgb(0xffffff)
                                                    } else {
                                                        theme.text_muted
                                                    })
                                                    .text_size(px(11.0))
                                                    .font_weight(if is_active_tab {
                                                        FontWeight::SEMIBOLD
                                                    } else {
                                                        FontWeight::NORMAL
                                                    })
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(
                                                        move |this, _, _window, cx| {
                                                            this.select_tab(t_idx, cx);
                                                        },
                                                    ))
                                                    .child(tab.name.clone())
                                            })
                                            .collect::<Vec<_>>(),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .id("btn-add-tab")
                                            .px(px(6.0))
                                            .py(px(2.0))
                                            .rounded_sm()
                                            .bg(theme.bg_card)
                                            .hover(move |s| s.bg(theme.btn_hover))
                                            .text_size(px(10.0))
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.add_filter_tab(cx);
                                            }))
                                            .child("+ Tab"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-del-tab")
                                            .px(px(5.0))
                                            .py(px(2.0))
                                            .rounded_sm()
                                            .bg(theme.bg_card)
                                            .hover(move |s| s.bg(theme.btn_hover))
                                            .text_size(px(10.0))
                                            .text_color(theme.error_red)
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.delete_current_tab(cx);
                                            }))
                                            .child("✕"),
                                    ),
                            ),
                    )
                    // Section Header + Inline/Dedicated Search Bar + Add Rule CTA
                    .child(
                        div()
                            .h(px(34.0))
                            .w_full()
                            .px(px(10.0))
                            .border_b_1()
                            .border_color(theme.border_subtle)
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_muted)
                                            .child("FILTER RULES"),
                                    )
                                    .children(if is_bottom_dock {
                                        Some(
                                            div()
                                                .id("input-filter-rule-search-inline")
                                                .h(px(24.0))
                                                .w(px(220.0))
                                                .px(px(8.0))
                                                .rounded_md()
                                                .bg(theme.bg_dark)
                                                .border_1()
                                                .border_color(
                                                    if input_focus == InputFocusTarget::FilterRuleSearch {
                                                        theme.accent
                                                    } else {
                                                        theme.border_subtle
                                                    },
                                                )
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.input_focus = InputFocusTarget::FilterRuleSearch;
                                                    window.focus(&this.focus_handle);
                                                    cx.notify();
                                                }))
                                                .child(
                                                    div()
                                                        .text_size(px(10.5))
                                                        .text_color(if self.filter_search_query.is_empty() {
                                                            theme.text_muted
                                                        } else {
                                                            theme.text_primary
                                                        })
                                                        .child(if self.filter_search_query.is_empty() {
                                                            if input_focus == InputFocusTarget::FilterRuleSearch {
                                                                "🔍 | Type to filter...".to_string()
                                                            } else {
                                                                "🔍 Filter rules...".to_string()
                                                            }
                                                        } else if input_focus == InputFocusTarget::FilterRuleSearch {
                                                            format!("🔍 {}|", self.filter_search_query)
                                                        } else {
                                                            format!("🔍 {}", self.filter_search_query)
                                                        }),
                                                )
                                                .children(if !self.filter_search_query.is_empty() {
                                                    Some(
                                                        div()
                                                            .id("btn-clear-filter-search-inline")
                                                            .px(px(4.0))
                                                            .cursor_pointer()
                                                            .text_size(px(10.0))
                                                            .text_color(theme.text_muted)
                                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                                this.filter_search_query.clear();
                                                                cx.notify();
                                                            }))
                                                            .child("✕"),
                                                    )
                                                } else {
                                                    None
                                                }),
                                        )
                                    } else {
                                        None
                                    }),
                            )
                            .child(
                                div()
                                    .id("btn-add-filter-rule")
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .rounded_md()
                                    .bg(theme.accent)
                                    .text_color(rgb(0xffffff))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_add_filter_modal(cx);
                                        window.focus(&this.focus_handle);
                                    }))
                                    .child("+ New Rule"),
                            ),
                    )
                    // In Left Sidebar mode, display the full-width dedicated search box
                    .children(if !is_bottom_dock {
                        Some(
                            div()
                                .p(px(8.0))
                                .border_b_1()
                                .border_color(theme.border_subtle)
                                .child(
                                    div()
                                        .id("input-filter-rule-search")
                                        .h(px(28.0))
                                        .w_full()
                                        .px(px(8.0))
                                        .rounded_md()
                                        .bg(theme.bg_dark)
                                        .border_1()
                                        .border_color(
                                            if input_focus == InputFocusTarget::FilterRuleSearch {
                                                theme.accent
                                            } else {
                                                theme.border_subtle
                                            },
                                        )
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.input_focus = InputFocusTarget::FilterRuleSearch;
                                            window.focus(&this.focus_handle);
                                            cx.notify();
                                        }))
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(if self.filter_search_query.is_empty() {
                                                    theme.text_muted
                                                } else {
                                                    theme.text_primary
                                                })
                                                .child(if self.filter_search_query.is_empty() {
                                                    if input_focus == InputFocusTarget::FilterRuleSearch {
                                                        "🔍 | Type to filter rules...".to_string()
                                                    } else {
                                                        "🔍 Filter rules in tab...".to_string()
                                                    }
                                                } else if input_focus == InputFocusTarget::FilterRuleSearch {
                                                    format!("🔍 {}|", self.filter_search_query)
                                                } else {
                                                    format!("🔍 {}", self.filter_search_query)
                                                }),
                                        )
                                        .children(if !self.filter_search_query.is_empty() {
                                            Some(
                                                div()
                                                    .id("btn-clear-filter-search")
                                                    .px(px(4.0))
                                                    .cursor_pointer()
                                                    .text_size(px(10.0))
                                                    .text_color(theme.text_muted)
                                                    .on_click(cx.listener(|this, _, _window, cx| {
                                                        this.filter_search_query.clear();
                                                        cx.notify();
                                                    }))
                                                    .child("✕"),
                                            )
                                        } else {
                                            None
                                        }),
                                ),
                        )
                    } else {
                        None
                    })
                    // Scrollable Color-Coded Filter Rule Cards (Vertical List format)
                    .child(
                        div()
                            .id("filter-rules-scroll")
                            .flex_1()
                            .overflow_y_scroll()
                            .p(px(8.0))
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .children(
                                self.filter_tabs
                                    .get(self.active_tab_index)
                                    .map(|tab| {
                                        let counts = self
                                            .filter_result
                                            .as_ref()
                                            .map(|r| r.filter_counts.clone())
                                            .unwrap_or_default();
                                        let search_q = self.filter_search_query.to_lowercase();
                                        tab.filters
                                            .iter()
                                            .enumerate()
                                            .filter(|(_, r)| {
                                                if search_q.is_empty() {
                                                    true
                                                } else {
                                                    r.text.to_lowercase().contains(&search_q)
                                                }
                                            })
                                            .map(|(r_idx, rule)| {
                                                let count = counts.get(r_idx).copied().unwrap_or(0);
                                                let is_active = rule.active;
                                                let is_exclude = rule.exclude;
                                                let is_regex = rule.regex;
                                                let swatch_rgb = loganalysis_core::parse_color_spec(
                                                    &rule.bg_color,
                                                    true,
                                                )
                                                .map(rgb)
                                                .unwrap_or(if is_exclude {
                                                    theme.error_red
                                                } else {
                                                    theme.accent
                                                });

                                                div()
                                                    .id(ElementId::NamedInteger(
                                                        "filter-row".into(),
                                                        r_idx as u64,
                                                    ))
                                                    .w_full()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .px(px(8.0))
                                                    .py(px(6.0))
                                                    .rounded_md()
                                                    .bg(if is_active {
                                                        theme.bg_card
                                                    } else {
                                                        theme.bg_panel
                                                    })
                                                    .border_1()
                                                    .border_color(if is_active {
                                                        theme.border
                                                    } else {
                                                        theme.border_subtle
                                                    })
                                                    .hover(move |s| s.bg(theme.btn_hover))
                                                    // Left side: Color swatch + Checkbox + Pattern Text
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_2()
                                                            .overflow_hidden()
                                                            // Vertical Color Swatch Bar
                                                            .child(
                                                                div()
                                                                    .w(px(4.0))
                                                                    .h(px(18.0))
                                                                    .rounded_full()
                                                                    .flex_shrink_0()
                                                                    .bg(swatch_rgb),
                                                            )
                                                            // Active Checkbox
                                                            .child(
                                                                div()
                                                                    .id(ElementId::NamedInteger(
                                                                        "chk-rule".into(),
                                                                        r_idx as u64,
                                                                    ))
                                                                    .w(px(15.0))
                                                                    .h(px(15.0))
                                                                    .flex_shrink_0()
                                                                    .rounded_sm()
                                                                    .border_1()
                                                                    .border_color(if is_active {
                                                                        theme.accent
                                                                    } else {
                                                                        theme.border
                                                                    })
                                                                    .bg(if is_active {
                                                                        theme.accent
                                                                    } else {
                                                                        theme.bg_dark
                                                                    })
                                                                    .cursor_pointer()
                                                                    .on_click(cx.listener(
                                                                        move |this, _, _window, cx| {
                                                                            this.toggle_rule_active(
                                                                                r_idx, cx,
                                                                            );
                                                                        },
                                                                    ))
                                                                    .flex()
                                                                    .items_center()
                                                                    .justify_center()
                                                                    .text_size(px(10.0))
                                                                    .text_color(rgb(0xffffff))
                                                                    .child(if is_active {
                                                                        "✓"
                                                                    } else {
                                                                        ""
                                                                    }),
                                                            )
                                                            // Pattern String (Monospace)
                                                            .child(
                                                                div()
                                                                    .font_family("monospace")
                                                                    .text_size(px(11.5))
                                                                    .whitespace_nowrap()
                                                                    .overflow_hidden()
                                                                    .text_color(if is_active {
                                                                        theme.text_primary
                                                                    } else {
                                                                        theme.text_muted
                                                                    })
                                                                    .child(rule.text.clone()),
                                                            ),
                                                    )
                                                    // Right side: Micro Badges + Match Count Pill + Edit + Delete
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_1()
                                                            .flex_shrink_0()
                                                            .children(if is_regex {
                                                                Some(
                                                                    div()
                                                                        .px(px(4.0))
                                                                        .py(px(1.0))
                                                                        .rounded_sm()
                                                                        .bg(theme.bg_badge)
                                                                        .text_size(px(9.0))
                                                                        .text_color(theme.accent)
                                                                        .child(".*"),
                                                                )
                                                            } else {
                                                                None
                                                            })
                                                            .children(if is_exclude {
                                                                Some(
                                                                    div()
                                                                        .px(px(4.0))
                                                                        .py(px(1.0))
                                                                        .rounded_sm()
                                                                        .bg(theme.bg_badge)
                                                                        .text_size(px(9.0))
                                                                        .font_weight(FontWeight::BOLD)
                                                                        .text_color(theme.error_red)
                                                                        .child("NOT"),
                                                                )
                                                            } else {
                                                                None
                                                            })
                                                            // Match Count Pill
                                                            .child(
                                                                div()
                                                                    .px(px(6.0))
                                                                    .py(px(1.0))
                                                                    .rounded_full()
                                                                    .bg(theme.bg_badge)
                                                                    .font_family("monospace")
                                                                    .text_size(px(10.0))
                                                                    .text_color(if count > 0 {
                                                                        theme.text_primary
                                                                    } else {
                                                                        theme.text_muted
                                                                    })
                                                                    .child(format!("{}", count)),
                                                            )
                                                            // Edit Rule Button
                                                            .child(
                                                                div()
                                                                    .id(ElementId::NamedInteger(
                                                                        "edit-rule".into(),
                                                                        r_idx as u64,
                                                                    ))
                                                                    .px(px(4.0))
                                                                    .cursor_pointer()
                                                                    .text_size(px(11.0))
                                                                    .text_color(theme.text_muted)
                                                                    .on_click(cx.listener(
                                                                        move |this, _, _window, cx| {
                                                                            this.open_edit_filter_modal(
                                                                                r_idx, cx,
                                                                            );
                                                                        },
                                                                    ))
                                                                    .child("✎"),
                                                            )
                                                            // Delete Rule Button
                                                            .child(
                                                                div()
                                                                    .id(ElementId::NamedInteger(
                                                                        "del-rule".into(),
                                                                        r_idx as u64,
                                                                    ))
                                                                    .px(px(4.0))
                                                                    .cursor_pointer()
                                                                    .text_size(px(11.0))
                                                                    .text_color(theme.text_muted)
                                                                    .on_click(cx.listener(
                                                                        move |this, _, _window, cx| {
                                                                            this.delete_rule(r_idx, cx);
                                                                        },
                                                                    ))
                                                                    .child("✕"),
                                                            ),
                                                    )
                                            })
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default(),
                            ),
                    );

                // --- 2B. LOG VIEWPORT COMPONENT ---
                let log_viewport_el = div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(theme.bg_dark)
                    .overflow_hidden()
                    // Interactive Find Bar
                    .children(if self.is_search_visible {
                        let is_search_focused = input_focus == InputFocusTarget::SearchBar;
                        Some(
                            div()
                                .h(px(38.0))
                                .w_full()
                                .bg(theme.bg_header)
                                .border_b_1()
                                .border_color(theme.border)
                                .flex()
                                .items_center()
                                .justify_between()
                                .px(px(12.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .flex_1()
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme.text_muted)
                                                .child("FIND"),
                                        )
                                        // Clickable Text Input Field
                                        .child(
                                            div()
                                                .id("input-find-bar")
                                                .w(px(320.0))
                                                .h(px(26.0))
                                                .px(px(8.0))
                                                .rounded_md()
                                                .bg(theme.bg_dark)
                                                .border_1()
                                                .border_color(if is_search_focused {
                                                    theme.accent
                                                } else {
                                                    theme.border_subtle
                                                })
                                                .flex()
                                                .items_center()
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.input_focus = InputFocusTarget::SearchBar;
                                                    window.focus(&this.focus_handle);
                                                    cx.notify();
                                                }))
                                                .font_family("monospace")
                                                .text_size(px(11.5))
                                                .text_color(if self.search_query.is_empty() {
                                                    theme.text_muted
                                                } else {
                                                    theme.text_primary
                                                })
                                                .child(if self.search_query.is_empty() {
                                                    if is_search_focused {
                                                        "| Type search query & press Enter...".to_string()
                                                    } else {
                                                        "Click to type search query...".to_string()
                                                    }
                                                } else if is_search_focused {
                                                    format!("{}|", self.search_query)
                                                } else {
                                                    self.search_query.clone()
                                                }),
                                        )
                                        // Regex & Case Toggles
                                        .child(
                                            div()
                                                .id("btn-find-regex")
                                                .px(px(6.0))
                                                .py(px(2.0))
                                                .rounded_sm()
                                                .bg(if self.is_search_regex {
                                                    theme.accent
                                                } else {
                                                    theme.bg_card
                                                })
                                                .text_color(if self.is_search_regex {
                                                    rgb(0xffffff)
                                                } else {
                                                    theme.text_muted
                                                })
                                                .text_size(px(10.0))
                                                .font_family("monospace")
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.is_search_regex = !this.is_search_regex;
                                                    cx.notify();
                                                }))
                                                .child(".* Regex"),
                                        )
                                        .child(
                                            div()
                                                .id("btn-find-case")
                                                .px(px(6.0))
                                                .py(px(2.0))
                                                .rounded_sm()
                                                .bg(if self.is_search_case_sensitive {
                                                    theme.accent
                                                } else {
                                                    theme.bg_card
                                                })
                                                .text_color(if self.is_search_case_sensitive {
                                                    rgb(0xffffff)
                                                } else {
                                                    theme.text_muted
                                                })
                                                .text_size(px(10.0))
                                                .font_family("monospace")
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.is_search_case_sensitive =
                                                        !this.is_search_case_sensitive;
                                                    cx.notify();
                                                }))
                                                .child("Aa Case"),
                                        ),
                                )
                                // Prev / Next / Close Controls
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(
                                            div()
                                                .id("btn-find-prev")
                                                .px(px(8.0))
                                                .py(px(3.0))
                                                .rounded_sm()
                                                .bg(theme.bg_card)
                                                .hover(move |s| s.bg(theme.btn_hover))
                                                .text_size(px(11.0))
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.search(false, cx);
                                                }))
                                                .child("↑ Prev"),
                                        )
                                        .child(
                                            div()
                                                .id("btn-find-next")
                                                .px(px(8.0))
                                                .py(px(3.0))
                                                .rounded_sm()
                                                .bg(theme.accent)
                                                .text_color(rgb(0xffffff))
                                                .text_size(px(11.0))
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.search(true, cx);
                                                }))
                                                .child("↓ Next"),
                                        )
                                        .child(
                                            div()
                                                .id("btn-find-close")
                                                .px(px(6.0))
                                                .py(px(3.0))
                                                .rounded_sm()
                                                .cursor_pointer()
                                                .text_size(px(11.0))
                                                .text_color(theme.text_muted)
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.toggle_search_bar(cx);
                                                }))
                                                .child("✕"),
                                        ),
                                ),
                        )
                    } else {
                        None
                    })
                    // Virtualized Log Rows or Polished Empty State
                    .child(if has_log_source {
                        let mut list =
                            uniform_list("log-view", row_count, move |range, _window, _cx| {
                                range
                                    .map(|idx| {
                                        let (real_idx, text) = if let (Some(res), Some(src)) =
                                            (&filter_res, &source)
                                        {
                                            let r_idx =
                                                res.visible_indices.get(idx).copied().unwrap_or(0);
                                            let t = src
                                                .get_line(r_idx)
                                                .unwrap_or_default()
                                                .to_string();
                                            (r_idx, t)
                                        } else {
                                            (0, String::new())
                                        };

                                        let (has_custom_bg, bg_color, fg_color) =
                                            if let Some(res) = &filter_res {
                                                if let Some(style) = res.visible_styles.get(idx) {
                                                    (
                                                        style.bg_hex.is_some(),
                                                        style.bg_hex.map(rgb).unwrap_or(theme.bg_dark),
                                                        style
                                                            .fg_hex
                                                            .map(rgb)
                                                            .unwrap_or(theme.text_primary),
                                                    )
                                                } else {
                                                    (false, theme.bg_dark, theme.text_primary)
                                                }
                                            } else {
                                                (false, theme.bg_dark, theme.text_primary)
                                            };

                                        let is_selected = selected == Some(idx);
                                        let final_bg = if is_selected {
                                            theme.selection_bg
                                        } else {
                                            bg_color
                                        };
                                        let final_fg = if is_selected && !has_custom_bg {
                                            rgb(0xffffff)
                                        } else {
                                            fg_color
                                        };
                                        let entity_handle = weak_self.clone();

                                        div()
                                            .id(ElementId::NamedInteger(
                                                "log-row".into(),
                                                idx as u64,
                                            ))
                                            .h(row_height)
                                            .min_h(row_height)
                                            .max_h(row_height)
                                            .w_full()
                                            .bg(final_bg)
                                            .hover(move |s| {
                                                if !is_selected && !has_custom_bg {
                                                    s.bg(theme.row_hover)
                                                } else {
                                                    s
                                                }
                                            })
                                            .text_color(final_fg)
                                            .font_family("monospace")
                                            .text_size(font_size)
                                            .line_height(row_height)
                                            .flex()
                                            .items_center()
                                            .flex_shrink_0()
                                            .overflow_hidden()
                                            .cursor_pointer()
                                            .on_click(move |_, _window, cx| {
                                                let _ = entity_handle.update(cx, |this, cx| {
                                                    this.selection.select(idx, real_idx);
                                                    this.input_focus = InputFocusTarget::None;
                                                    cx.notify();
                                                });
                                            })
                                            // Line Number Gutter (Fixed 64px + Subtle Right Border)
                                            .child(
                                                div()
                                                    .w(px(64.0))
                                                    .h_full()
                                                    .flex_shrink_0()
                                                    .bg(if is_selected {
                                                        theme.accent_muted
                                                    } else {
                                                        theme.bg_gutter
                                                    })
                                                    .border_r_1()
                                                    .border_color(theme.border_subtle)
                                                    .text_color(if is_selected {
                                                        theme.accent
                                                    } else {
                                                        theme.text_gutter
                                                    })
                                                    .font_weight(if is_selected {
                                                        FontWeight::BOLD
                                                    } else {
                                                        FontWeight::NORMAL
                                                    })
                                                    .flex()
                                                    .items_center()
                                                    .justify_end()
                                                    .pr(px(8.0))
                                                    .whitespace_nowrap()
                                                    .overflow_hidden()
                                                    .child(format!("{}", real_idx + 1)),
                                            )
                                            // Monospace Log Content
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .px(px(10.0))
                                                    .whitespace_nowrap()
                                                    .child(text),
                                            )
                                    })
                                    .collect::<Vec<_>>()
                            });

                        if self.full_line_display {
                            list = list.with_horizontal_sizing_behavior(
                                ListHorizontalSizingBehavior::Unconstrained,
                            );
                        }

                        list.track_scroll(self.scroll_handle.clone())
                            .flex_1()
                            .into_any_element()
                    } else {
                        // Modern Onboarding Empty State Card
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .w(px(460.0))
                                    .p(px(28.0))
                                    .rounded_xl()
                                    .bg(theme.bg_panel)
                                    .border_1()
                                    .border_color(theme.border)
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap_4()
                                    .child(
                                        div()
                                            .text_size(px(18.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.text_primary)
                                            .child("LogAnalysis Studio"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme.text_muted)
                                            .child(
                                                "Drop any log or filter JSON file here, or start a live ADB logcat stream.",
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_3()
                                            .pt(px(4.0))
                                            .child(
                                                div()
                                                    .id("btn-empty-state-open")
                                                    .px(px(16.0))
                                                    .py(px(8.0))
                                                    .rounded_md()
                                                    .bg(theme.accent)
                                                    .text_color(rgb(0xffffff))
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(
                                                        |this, _, _window, cx| {
                                                            this.prompt_open_file(cx);
                                                        },
                                                    ))
                                                    .child("📂 Open Log File (Ctrl+O)"),
                                            )
                                            .child(
                                                div()
                                                    .id("btn-empty-state-adb")
                                                    .px(px(16.0))
                                                    .py(px(8.0))
                                                    .rounded_md()
                                                    .bg(theme.bg_card)
                                                    .border_1()
                                                    .border_color(theme.border)
                                                    .hover(move |s| s.bg(theme.btn_hover))
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(
                                                        |this, _, _window, cx| {
                                                            this.start_adb_stream(cx);
                                                        },
                                                    ))
                                                    .child("⚡ Start ADB Stream"),
                                            ),
                                    )
                                    // Shortcut Cheat Sheet
                                    .child(
                                        div()
                                            .w_full()
                                            .mt(px(8.0))
                                            .pt(px(12.0))
                                            .border_t_1()
                                            .border_color(theme.border_subtle)
                                            .flex()
                                            .flex_col()
                                            .gap_1p5()
                                            .text_size(px(11.0))
                                            .font_family("monospace")
                                            .text_color(theme.text_muted)
                                            .child("Ctrl+F       Search in logs (F3 / Shift+F3)")
                                            .child("Ctrl+Shift+F Add new highlight/exclude rule")
                                            .child("Ctrl+H       Toggle Filtered Only view")
                                            .child("Ctrl+Shift+L Toggle Filter Panel (Left / Bottom)")
                                            .child("Ctrl+T / D   New / Delete filter tab"),
                                    ),
                            )
                            .into_any_element()
                    });

                // Assemble Container based on FilterPanelPosition
                let mut container = div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .overflow_hidden();

                if !is_bottom_dock {
                    container = container.flex_row().child(filter_panel_el).child(log_viewport_el);
                } else {
                    container = container.flex_col().child(log_viewport_el).child(filter_panel_el);
                }

                container
            })
            // ================================================================
            // 3. BOTTOM DASHBOARD STATUS BAR
            // ================================================================
            .child(
                div()
                    .h(px(28.0))
                    .w_full()
                    .bg(theme.bg_header)
                    .border_t_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(12.0))
                    // Left: Live Status Indicator Dot + Status Message
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(7.0))
                                    .h(px(7.0))
                                    .rounded_full()
                                    .bg(if self.is_streaming {
                                        if self.is_stream_paused {
                                            theme.warning_amber
                                        } else {
                                            theme.success_green
                                        }
                                    } else {
                                        theme.accent
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child(self.status_text.clone()),
                            ),
                    )
                    // Right: Telemetry Metrics & Interactive Font Zoom
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .font_family("monospace")
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(format!(
                                "Visible: {} / {} ({:.1}%)",
                                row_count, total_lines, visible_pct
                            ))
                            .child(format!("Matched: {}", total_matched))
                            .child(if let Some(line_idx) = selected_real_line {
                                format!("Line: {}", line_idx + 1)
                            } else {
                                "Line: -".to_string()
                            })
                            // Interactive Zoom Controls
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .id("btn-status-zoom-out")
                                            .px(px(5.0))
                                            .rounded_sm()
                                            .bg(theme.bg_card)
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.zoom(-1.0, cx);
                                            }))
                                            .child("-"),
                                    )
                                    .child(div().px(px(4.0)).child(format!("{:.0}px", font_size_f32)))
                                    .child(
                                        div()
                                            .id("btn-status-zoom-in")
                                            .px(px(5.0))
                                            .rounded_sm()
                                            .bg(theme.bg_card)
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.zoom(1.0, cx);
                                            }))
                                            .child("+"),
                                    ),
                            ),
                    ),
            )
            // ================================================================
            // 4. MODAL FILTER ADD / EDIT OVERLAY (With Interactive Keyboard Input)
            // ================================================================
            .children(if self.filter_edit_modal.is_open {
                let is_edit = self.filter_edit_modal.editing_index.is_some();
                let modal_pattern = self.filter_edit_modal.pattern.clone();
                let is_reg = self.filter_edit_modal.is_regex;
                let is_cs = self.filter_edit_modal.is_case_sensitive;
                let is_ex = self.filter_edit_modal.is_exclude;
                let bg_name = self.filter_edit_modal.bg_color_name.clone();
                let is_modal_focused = input_focus == InputFocusTarget::ModalPattern;

                Some(
                    div()
                        .id("modal-overlay-filter-edit")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x000000aa))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .id("modal-card-filter-edit")
                                .w(px(480.0))
                                .bg(theme.bg_panel)
                                .border_1()
                                .border_color(theme.accent)
                                .rounded_xl()
                                .shadow_lg()
                                .p(px(20.0))
                                .flex()
                                .flex_col()
                                .gap_3()
                                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                                    this.handle_key_down(event, cx);
                                }))
                                // Modal Header
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .font_weight(FontWeight::BOLD)
                                                .text_size(px(15.0))
                                                .child(if is_edit {
                                                    "Edit Filter Rule"
                                                } else {
                                                    "New Filter Rule"
                                                }),
                                        )
                                        .child(
                                            div()
                                                .id("btn-modal-close-x")
                                                .cursor_pointer()
                                                .text_size(px(14.0))
                                                .text_color(theme.text_muted)
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.close_filter_modal(cx);
                                                }))
                                                .child("✕"),
                                        ),
                                )
                                // Interactive Pattern Input Field
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_muted)
                                                .child("Pattern (type directly or pick a preset below):"),
                                        )
                                        .child(
                                            div()
                                                .id("input-modal-pattern")
                                                .px(px(10.0))
                                                .py(px(8.0))
                                                .bg(theme.bg_dark)
                                                .border_1()
                                                .border_color(if is_modal_focused {
                                                    theme.accent
                                                } else {
                                                    theme.border
                                                })
                                                .rounded_md()
                                                .font_family("monospace")
                                                .text_size(px(12.5))
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.input_focus = InputFocusTarget::ModalPattern;
                                                    window.focus(&this.focus_handle);
                                                    cx.notify();
                                                }))
                                                .text_color(if modal_pattern.is_empty() {
                                                    theme.text_muted
                                                } else {
                                                    theme.text_primary
                                                })
                                                .child(if modal_pattern.is_empty() {
                                                    if is_modal_focused {
                                                        "| Type pattern here...".to_string()
                                                    } else {
                                                        "Click to type pattern...".to_string()
                                                    }
                                                } else if is_modal_focused {
                                                    format!("{}|", modal_pattern)
                                                } else {
                                                    modal_pattern
                                                }),
                                        ),
                                )
                                // Rule Flags: Exclude, Regex, Case Sensitive
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_4()
                                        .py(px(4.0))
                                        .child(
                                            div()
                                                .id("chk-modal-exclude")
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.filter_edit_modal.is_exclude =
                                                        !this.filter_edit_modal.is_exclude;
                                                    cx.notify();
                                                }))
                                                .child(
                                                    div()
                                                        .w(px(15.0))
                                                        .h(px(15.0))
                                                        .rounded_sm()
                                                        .border_1()
                                                        .border_color(theme.border)
                                                        .bg(if is_ex {
                                                            theme.error_red
                                                        } else {
                                                            theme.bg_dark
                                                        })
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(0xffffff))
                                                        .child(if is_ex { "✓" } else { "" }),
                                                )
                                                .child(div().text_size(px(11.5)).child("Exclude (NOT)")),
                                        )
                                        .child(
                                            div()
                                                .id("chk-modal-regex")
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.filter_edit_modal.is_regex =
                                                        !this.filter_edit_modal.is_regex;
                                                    cx.notify();
                                                }))
                                                .child(
                                                    div()
                                                        .w(px(15.0))
                                                        .h(px(15.0))
                                                        .rounded_sm()
                                                        .border_1()
                                                        .border_color(theme.border)
                                                        .bg(if is_reg {
                                                            theme.accent
                                                        } else {
                                                            theme.bg_dark
                                                        })
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(0xffffff))
                                                        .child(if is_reg { "✓" } else { "" }),
                                                )
                                                .child(div().text_size(px(11.5)).child("Regex")),
                                        )
                                        .child(
                                            div()
                                                .id("chk-modal-case")
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.filter_edit_modal.is_case_sensitive =
                                                        !this.filter_edit_modal.is_case_sensitive;
                                                    cx.notify();
                                                }))
                                                .child(
                                                    div()
                                                        .w(px(15.0))
                                                        .h(px(15.0))
                                                        .rounded_sm()
                                                        .border_1()
                                                        .border_color(theme.border)
                                                        .bg(if is_cs {
                                                            theme.accent
                                                        } else {
                                                            theme.bg_dark
                                                        })
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(0xffffff))
                                                        .child(if is_cs { "✓" } else { "" }),
                                                )
                                                .child(div().text_size(px(11.5)).child("Case Sensitive")),
                                        ),
                                )
                                // Highlight Color Swatches
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_muted)
                                                .child("Highlight Color:"),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2p5()
                                                .children(
                                                    [
                                                        ("Red", 0xFFB6B6, 0x000000),
                                                        ("Yellow", 0xFFFF00, 0x000000),
                                                        ("Green", 0x90EE90, 0x000000),
                                                        ("Cyan", 0x00FFFF, 0x000000),
                                                        ("Blue", 0xB6D0FF, 0x000000),
                                                        ("Orange", 0xFFD580, 0x000000),
                                                        ("Purple", 0xE6E6FA, 0x000000),
                                                        ("White", 0xFFFFFF, 0x000000),
                                                    ]
                                                    .into_iter()
                                                    .map(|(name, bg_val, _fg_val)| {
                                                        let is_curr = bg_name == name;
                                                        div()
                                                            .id(ElementId::NamedInteger(
                                                                "palette-color".into(),
                                                                bg_val as u64,
                                                            ))
                                                            .w(px(26.0))
                                                            .h(px(26.0))
                                                            .rounded_full()
                                                            .bg(rgb(bg_val))
                                                            .border_2()
                                                            .border_color(if is_curr {
                                                                theme.accent
                                                            } else {
                                                                theme.border
                                                            })
                                                            .cursor_pointer()
                                                            .on_click(cx.listener(
                                                                move |this, _, _window, cx| {
                                                                    this.filter_edit_modal
                                                                        .bg_color_name =
                                                                        name.to_string();
                                                                    this.filter_edit_modal
                                                                        .fg_color_name =
                                                                        "Black".to_string();
                                                                    cx.notify();
                                                                },
                                                            ))
                                                    })
                                                    .collect::<Vec<_>>(),
                                                ),
                                        ),
                                )
                                // Quick Pattern Presets
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_muted)
                                                .child("Quick Presets:"),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .gap_1p5()
                                                .children(
                                                    [
                                                        ("Fatal", "Red"),
                                                        ("Exception", "Yellow"),
                                                        ("Error", "Red"),
                                                        ("Warning", "Yellow"),
                                                        ("ActivityTaskManager", "Cyan"),
                                                        ("AndroidRuntime", "Red"),
                                                    ]
                                                    .into_iter()
                                                    .enumerate()
                                                    .map(|(p_idx, (pat, col))| {
                                                        div()
                                                            .id(ElementId::NamedInteger(
                                                                "quick-preset".into(),
                                                                p_idx as u64,
                                                            ))
                                                            .px(px(8.0))
                                                            .py(px(3.0))
                                                            .rounded_md()
                                                            .bg(theme.bg_card)
                                                            .border_1()
                                                            .border_color(theme.border_subtle)
                                                            .hover(move |s| s.bg(theme.btn_hover))
                                                            .font_family("monospace")
                                                            .text_size(px(10.5))
                                                            .cursor_pointer()
                                                            .on_click(cx.listener(
                                                                move |this, _, _window, cx| {
                                                                    this.filter_edit_modal.pattern =
                                                                        pat.to_string();
                                                                    this.filter_edit_modal
                                                                        .bg_color_name =
                                                                        col.to_string();
                                                                    cx.notify();
                                                                },
                                                            ))
                                                            .child(pat)
                                                    })
                                                    .collect::<Vec<_>>(),
                                                ),
                                        ),
                                )
                                // Action Buttons (Cancel / Save)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_end()
                                        .gap_2()
                                        .pt(px(8.0))
                                        .child(
                                            div()
                                                .id("btn-modal-cancel")
                                                .px(px(14.0))
                                                .py(px(6.0))
                                                .rounded_md()
                                                .bg(theme.bg_card)
                                                .hover(move |s| s.bg(theme.btn_hover))
                                                .text_size(px(11.5))
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.close_filter_modal(cx);
                                                }))
                                                .child("Cancel (Esc)"),
                                        )
                                        .child(
                                            div()
                                                .id("btn-modal-save")
                                                .px(px(18.0))
                                                .py(px(6.0))
                                                .rounded_md()
                                                .bg(theme.accent)
                                                .text_color(rgb(0xffffff))
                                                .text_size(px(11.5))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _window, cx| {
                                                    this.save_modal_filter(cx);
                                                }))
                                                .child(if is_edit {
                                                    "Update Rule (Enter)"
                                                } else {
                                                    "Save Rule (Enter)"
                                                }),
                                        ),
                                ),
                        ),
                )
            } else {
                None
            })
    }
}

fn main() {
    let cli_path = std::env::args().nth(1).map(std::path::PathBuf::from);

    Application::new().run(move |cx: &mut App| {
        // Register Global Key Bindings
        cx.bind_keys([
            KeyBinding::new("ctrl-o", OpenFile, None),
            KeyBinding::new("ctrl-h", ToggleFilteredOnly, None),
            KeyBinding::new("ctrl-f", Find, None),
            KeyBinding::new("f3", FindNext, None),
            KeyBinding::new("shift-f3", FindPrev, None),
            KeyBinding::new("ctrl-c", CopySelection, None),
            KeyBinding::new("ctrl-shift-f", AddFilter, None),
            KeyBinding::new("ctrl-s", SaveFilters, None),
            KeyBinding::new("ctrl-shift-s", SaveFiltersAs, None),
            KeyBinding::new("ctrl-k", ClearLogs, None),
            KeyBinding::new("ctrl-=", ZoomIn, None),
            KeyBinding::new("ctrl-+", ZoomIn, None),
            KeyBinding::new("ctrl--", ZoomOut, None),
            KeyBinding::new("ctrl-space", TogglePauseStream, None),
            KeyBinding::new("ctrl-t", AddFilterTab, None),
            KeyBinding::new("ctrl-d", DeleteFilterTab, None),
            KeyBinding::new("ctrl-r", RenameFilterTab, None),
            KeyBinding::new("ctrl-shift-l", ToggleFilterPanelPosition, None),
        ]);

        let bounds = Bounds::centered(None, size(px(1200.0), px(850.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("LogAnalysis (High Performance)".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        };

        let initial_path = cli_path.clone();
        if let Err(e) = cx.open_window(options, move |window, cx| {
            let entity = cx.new(LogAnalysisWorkspace::new);
            entity.update(cx, |workspace, _cx| {
                window.focus(&workspace.focus_handle);
            });
            if let Some(path) = initial_path {
                entity.update(cx, |workspace, cx| {
                    workspace.open_file(&path, cx);
                });
            }
            entity
        }) {
            eprintln!("Failed to open LogAnalysis window: {:?}", e);
        }
    });
}

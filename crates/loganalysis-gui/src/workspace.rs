use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use crossbeam_channel::{bounded, TryRecvError};
use gpui::*;
use loganalysis_core::*;

use crate::components::text_input::{TextInput, TextInputEvent};
use crate::selection::SelectionModel;
use crate::theme::Theme;
use crate::types::{FilterEditState, FilterPanelPosition, InputFocusTarget};

pub struct LogAnalysisWorkspace {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) log_source: Option<Arc<ActiveLogSource>>,
    pub(crate) filter_result: Option<Arc<FilterResult>>,
    pub(crate) scroll_handle: UniformListScrollHandle,
    pub(crate) selection: SelectionModel,
    pub(crate) theme: Theme,
    pub(crate) input_focus: InputFocusTarget,

    // Filter state
    pub(crate) filter_tabs: Vec<FilterTabData>,
    pub(crate) active_tab_index: usize,
    pub(crate) show_only_filtered: bool,
    pub(crate) filter_cancel_gen: Arc<AtomicU64>,
    pub(crate) filter_edit_modal: FilterEditState,
    pub(crate) modal_pattern_input: Entity<TextInput>,
    pub(crate) filter_search_query: String,

    // Search state
    pub(crate) search_query: String,
    pub(crate) is_search_visible: bool,
    pub(crate) is_search_regex: bool,
    pub(crate) is_search_case_sensitive: bool,
    pub(crate) cached_search_matcher: Option<Arc<loganalysis_core::CompiledMatcher>>,
    pub(crate) search_cancel_gen: Arc<AtomicU64>,
    pub(crate) find_input: Entity<TextInput>,

    // ADB state
    pub(crate) adb_devices: Vec<String>,
    pub(crate) selected_device: Option<String>,
    pub(crate) adb_stream_handle: Option<LogcatStreamHandle>,
    pub(crate) is_streaming: bool,
    pub(crate) is_stream_paused: bool,
    pub(crate) auto_scroll: bool,
    pub(crate) paused_stream_buffer: VecDeque<Vec<String>>,
    pub(crate) paused_stream_line_count: usize,

    // Display & Status
    pub(crate) font_size: Pixels,
    pub(crate) status_text: SharedString,
    pub(crate) active_file_name: Option<SharedString>,
    pub(crate) is_dark_theme: bool,
    pub(crate) full_line_display: bool,
    pub(crate) filter_panel_position: FilterPanelPosition,
    pub(crate) is_dragging_scrollbar: bool,
}

fn delete_previous_word(s: &mut String) {
    while let Some(c) = s.chars().last() {
        if c.is_whitespace() {
            s.pop();
        } else {
            break;
        }
    }
    while let Some(c) = s.chars().last() {
        if !c.is_whitespace() {
            s.pop();
        } else {
            break;
        }
    }
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

        let modal_pattern_input = cx.new(|cx| TextInput::new("Type filter pattern...", cx));

        cx.subscribe(
            &modal_pattern_input,
            |this, _input, event: &TextInputEvent, cx| match event {
                TextInputEvent::Submit => {
                    this.save_modal_filter(cx);
                }
                TextInputEvent::Cancel => {
                    this.close_filter_modal(cx);
                }
                TextInputEvent::Change => {}
            },
        )
        .detach();

        let find_input = cx.new(|cx| TextInput::new("Type search query & press Enter...", cx));

        cx.subscribe(
            &find_input,
            |this, input, event: &TextInputEvent, cx| match event {
                TextInputEvent::Submit => {
                    this.search_query = input.read(cx).text().to_string();
                    this.update_cached_search_matcher();
                    this.search(true, cx);
                }
                TextInputEvent::Cancel => {
                    this.is_search_visible = false;
                    this.input_focus = InputFocusTarget::None;
                    this.update_cached_search_matcher();
                    cx.notify();
                }
                TextInputEvent::Change => {
                    this.search_query = input.read(cx).text().to_string();
                    this.update_cached_search_matcher();
                    cx.notify();
                }
            },
        )
        .detach();

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
            filter_cancel_gen: Arc::new(AtomicU64::new(0)),
            filter_edit_modal: FilterEditState::default(),
            modal_pattern_input,
            filter_search_query: String::new(),

            search_query: String::new(),
            is_search_visible: false,
            is_search_regex: false,
            is_search_case_sensitive: false,
            cached_search_matcher: None,
            search_cancel_gen: Arc::new(AtomicU64::new(0)),
            find_input,

            adb_devices: Vec::new(),
            selected_device: None,
            adb_stream_handle: None,
            is_streaming: false,
            is_stream_paused: false,
            auto_scroll: true,
            paused_stream_buffer: VecDeque::new(),
            paused_stream_line_count: 0,

            font_size: px(12.0),
            status_text: "Ready | Open a file or start ADB streaming".into(),
            active_file_name: None,
            is_dark_theme: false,
            full_line_display: false,
            filter_panel_position: FilterPanelPosition::Bottom,
            is_dragging_scrollbar: false,
        }
    }

    /// Prompts user to select a log file using native OS file dialog with filters
    pub fn prompt_open_file(&mut self, cx: &mut Context<Self>) {
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut async_cx = cx.clone();
            async move {
                let file = rfd::AsyncFileDialog::new()
                    .set_title("Open Log File")
                    .add_filter("Log Files (*.log, *.txt)", &["log", "txt"])
                    .add_filter("All Files (*.*)", &["*"])
                    .pick_file()
                    .await;

                if let Some(handle) = file {
                    let path = handle.path().to_path_buf();
                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        workspace.open_file(&path, cx);
                    });
                }
            }
        })
        .detach();
    }

    /// Loads a log file using FileLogBuffer asynchronously on the background executor and executes filtering
    pub fn open_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        if self.is_streaming {
            self.stop_adb_stream(cx);
        }
        let path_buf = path.to_path_buf();
        let file_name: SharedString = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
            .into();

        self.status_text = format!("Opening log file {:?}...", file_name).into();
        self.selection.clear();
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
                            workspace.log_source = Some(arc_buf.clone());
                            workspace.active_file_name = Some(file_name);
                            workspace.status_text =
                                format!("Loaded {} lines from {:?}", arc_buf.line_count(), path_buf)
                                    .into();
                            workspace.reapply_filters(cx);
                        }
                        Err(e) => {
                            workspace.status_text = format!("Failed to open file: {}", e).into();
                            cx.notify();
                        }
                    }
                });
            }
        })
        .detach();
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

        let generation = self.filter_cancel_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let token = self.filter_cancel_gen.clone();
        let show_only = self.show_only_filtered;

        let source_for_bg = source.clone();
        let source_for_ui = source.clone();

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
                            Some(&token),
                            generation,
                        )
                    })
                    .await;

                if let Some(res) = result {
                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        // Check if a newer generation superseded this task
                        if workspace.filter_cancel_gen.load(Ordering::SeqCst) != generation {
                            return;
                        }

                        let prev_anchor = workspace.selection.anchor_source_idx;
                        workspace.filter_result = Some(Arc::new(res));

                        // Viewport anchor preservation
                        if let (Some(anchor), Some(res)) = (prev_anchor, &workspace.filter_result) {
                            if let Some(new_row) =
                                find_nearest_anchor_row(&res.visible_indices, anchor)
                            {
                                workspace.selection.selected_row = Some(new_row);
                                workspace
                                    .scroll_handle
                                    .scroll_to_item(new_row, ScrollStrategy::Center);
                            }
                        }

                        let visible_count = workspace
                            .filter_result
                            .as_ref()
                            .map(|r| r.visible_indices.len())
                            .unwrap_or(0);
                        let total_count = source_for_ui.line_count();
                        workspace.status_text = format!(
                            "Visible: {} / {} lines ({} matched)",
                            visible_count,
                            total_count,
                            workspace
                                .filter_result
                                .as_ref()
                                .map(|r| r.total_matched_lines)
                                .unwrap_or(0)
                        )
                        .into();
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Toggles "Show Only Filtered" mode (Ctrl+H) with anchor preservation
    pub fn toggle_filtered_only(&mut self, cx: &mut Context<Self>) {
        self.show_only_filtered = !self.show_only_filtered;
        self.reapply_filters(cx);
    }

    /// Scans for connected ADB devices asynchronously
    pub fn refresh_adb_devices(&mut self, cx: &mut Context<Self>) {
        self.status_text = "Scanning ADB devices...".into();
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();

            async move {
                let res = bg_executor
                    .spawn(async move { scan_adb_devices(Duration::from_secs(2)) })
                    .await;

                let _ = this.update(&mut async_cx, |workspace, cx| {
                    match res {
                        Ok(devices) => {
                            workspace.selected_device = devices.first().cloned();
                            workspace.adb_devices = devices;
                            workspace.status_text =
                                format!("Found {} ADB devices", workspace.adb_devices.len()).into();
                        }
                        Err(e) => {
                            workspace.status_text = format!("ADB scan error: {}", e).into();
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Starts streaming logcat from selected ADB device with zero-copy chunk ingestion
    pub fn start_adb_stream(&mut self, cx: &mut Context<Self>) {
        if self.is_streaming {
            return;
        }

        // Bounded ingestion channel (256 chunks) prevents unbounded memory blowup on bursts
        let (tx, rx) = bounded(256);
        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let serial = self.selected_device.as_deref();

        match start_logcat_stream(serial, tx, stop_flag.clone()) {
            Ok(handle) => {
                self.adb_stream_handle = Some(handle);
                self.is_streaming = true;

                let stream_buf = StreamLogBuffer::default();
                self.log_source = Some(Arc::new(ActiveLogSource::Stream(stream_buf)));
                self.status_text = "Streaming adb logcat...".into();

                // Non-blocking async loop with batching, throttling & bounded buffers to avoid starving GPUI executor
                cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                    let mut async_cx = cx.clone();
                    let timer_executor = cx.background_executor().clone();
                    async move {
                        let mut last_reapply = std::time::Instant::now();
                        let mut pending_filter_reapply = false;
                        let mut accumulated_trimmed = 0usize;

                        loop {
                            let mut batch = Vec::new();
                            let mut is_disconnected = false;

                            // Drain available chunks up to 5000 lines
                            loop {
                                match rx.try_recv() {
                                    Ok(chunk) => {
                                        batch.extend(chunk);
                                        if batch.len() >= 5000 {
                                            break;
                                        }
                                    }
                                    Err(TryRecvError::Empty) => break,
                                    Err(TryRecvError::Disconnected) => {
                                        is_disconnected = true;
                                        break;
                                    }
                                }
                            }

                            if !batch.is_empty() {
                                let (should_continue, tr) = this
                                    .update(&mut async_cx, |workspace, cx| {
                                        let mut trimmed_count = 0;
                                        if workspace.is_stream_paused {
                                            // Buffer chunks with bounded capacity to prevent OOM
                                            let chunk_len = batch.len();
                                            workspace.paused_stream_line_count += chunk_len;
                                            workspace.paused_stream_buffer.push_back(batch);

                                            while workspace.paused_stream_line_count > loganalysis_core::MAX_MONITOR_LINES {
                                                if let Some(front) = workspace.paused_stream_buffer.pop_front() {
                                                    workspace.paused_stream_line_count =
                                                        workspace.paused_stream_line_count.saturating_sub(front.len());
                                                } else {
                                                    break;
                                                }
                                            }
                                        } else if let Some(src) = &mut workspace.log_source {
                                            let batch_len = batch.len();
                                            let (new_total_len, new_lines_range, trimmed) = {
                                                // Zero-copy in-place mutation when unique; fallback to clone only if filter task holds Arc
                                                let (len, tr) = if let Some(active_src) = Arc::get_mut(src) {
                                                    if let ActiveLogSource::Stream(ref mut stream) = active_src {
                                                        stream.append_chunk(batch);
                                                        let tr = stream.trim_if_needed().unwrap_or(0);
                                                        (stream.line_count(), tr)
                                                    } else {
                                                        (0, 0)
                                                    }
                                                } else if let ActiveLogSource::Stream(ref stream) = **src {
                                                    let mut new_stream = stream.clone();
                                                    new_stream.append_chunk(batch);
                                                    let tr = new_stream.trim_if_needed().unwrap_or(0);
                                                    let len = new_stream.line_count();
                                                    *src = Arc::new(ActiveLogSource::Stream(new_stream));
                                                    (len, tr)
                                                } else {
                                                    (0, 0)
                                                };
                                                let start = len.saturating_sub(batch_len);
                                                (len, start..len, tr)
                                            };
                                            trimmed_count = trimmed;

                                            // Incremental filtering: evaluate only newly arrived lines without full buffer Rayon pass
                                            if let (Some(filter_res), Some(src_ref)) = (&mut workspace.filter_result, &workspace.log_source) {
                                                let show_only = workspace.show_only_filtered;
                                                let mut active_filters = Vec::new();
                                                for tab in &workspace.filter_tabs {
                                                    if tab.enabled {
                                                        for rule in &tab.filters {
                                                            if rule.active {
                                                                active_filters.push(rule.clone());
                                                            }
                                                        }
                                                    }
                                                }

                                                let res_mut = Arc::make_mut(filter_res);
                                                append_streaming_batch(
                                                    res_mut,
                                                    &**src_ref,
                                                    new_lines_range,
                                                    trimmed,
                                                    &active_filters,
                                                    show_only,
                                                );

                                                let visible_count = res_mut.visible_indices.len();
                                                workspace.status_text = format!(
                                                    "Visible: {} / {} lines ({} matched)",
                                                    visible_count,
                                                    new_total_len,
                                                    res_mut.total_matched_lines
                                                ).into();
                                            } else {
                                                // Initial pass if filter_result was not yet computed
                                                workspace.reapply_filters(cx);
                                            }

                                            // Smart Auto-Scroll to bottom if enabled
                                            if workspace.auto_scroll {
                                                if let Some(res) = &workspace.filter_result {
                                                    let count = res.visible_indices.len();
                                                    if count > 0 {
                                                        workspace.scroll_handle.scroll_to_item(
                                                            count - 1,
                                                            ScrollStrategy::Bottom,
                                                        );
                                                    }
                                                }
                                            }
                                        }

                                        if is_disconnected {
                                            workspace.is_streaming = false;
                                            workspace.status_text = "ADB stream disconnected".into();
                                        }

                                        cx.notify();
                                        (workspace.is_streaming, trimmed_count)
                                    })
                                    .unwrap_or((false, 0));

                                if tr > 0 {
                                    accumulated_trimmed += tr;
                                    if accumulated_trimmed >= 10_000 || last_reapply.elapsed() >= Duration::from_secs(5) {
                                        pending_filter_reapply = true;
                                    }
                                }

                                if !should_continue || is_disconnected {
                                    break;
                                }
                            } else if is_disconnected {
                                // Subprocess closed pipe: cleanly exit streaming state and terminate async loop
                                let _ = this.update(&mut async_cx, |workspace, cx| {
                                    workspace.is_streaming = false;
                                    workspace.status_text = "ADB stream disconnected".into();
                                    cx.notify();
                                });
                                break;
                            } else {
                                // Flush any pending throttled filter reapply when channel is empty
                                if pending_filter_reapply && last_reapply.elapsed() >= Duration::from_millis(60) {
                                    let _ = this.update(&mut async_cx, |workspace, cx| {
                                        if !workspace.is_stream_paused {
                                            workspace.reapply_filters(cx);
                                        }
                                    });
                                    last_reapply = std::time::Instant::now();
                                    pending_filter_reapply = false;
                                    accumulated_trimmed = 0;
                                }

                                // Yield execution cooperatively to GPUI async executor
                                timer_executor.timer(Duration::from_millis(16)).await;
                                let still_streaming = this
                                    .update(&mut async_cx, |workspace, _| workspace.is_streaming)
                                    .unwrap_or(false);
                                if !still_streaming {
                                    break;
                                }
                            }
                        }
                    }
                })
                .detach();
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

    /// Cyclic search forward or backward (with async dispatch for large logs)
    pub fn search(&mut self, forward: bool, cx: &mut Context<Self>) {
        let input_text = self.find_input.read(cx).text().trim().to_string();
        if !input_text.is_empty() {
            self.search_query = input_text;
        }
        self.update_cached_search_matcher();

        if self.search_query.is_empty() {
            return;
        }

        let Some(source) = self.log_source.clone() else {
            return;
        };
        let Some(res) = self.filter_result.clone() else {
            return;
        };

        let query = SearchQuery {
            pattern: self.search_query.clone(),
            case_sensitive: self.is_search_case_sensitive,
            is_regex: self.is_search_regex,
            forward,
        };

        let start_row = self.selection.selected_row;

        // If dataset is large (> ASYNC_SEARCH_THRESHOLD), dispatch to background executor
        if res.visible_indices.len() > ASYNC_SEARCH_THRESHOLD {
            self.status_text = "Searching in background...".into();
            cx.notify();

            // Advance generation to invalidate and cancel any previous in-flight background searches
            let generation = self.search_cancel_gen.fetch_add(1, Ordering::SeqCst) + 1;
            let cancel_token = self.search_cancel_gen.clone();

            cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let bg_executor = cx.background_executor().clone();
                let mut async_cx = cx.clone();

                async move {
                    let search_res = bg_executor
                        .spawn(async move {
                            search_in_log(
                                &*source,
                                &res.visible_indices,
                                &query,
                                start_row,
                                Some(&cancel_token),
                                generation,
                                None,
                            )
                        })
                        .await;

                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        // Discard results if superseded by a newer search
                        if workspace.search_cancel_gen.load(Ordering::SeqCst) != generation {
                            return;
                        }

                        if let Ok(Some(found)) = search_res {
                            workspace.selection.select(found.visible_row, found.real_idx);
                            workspace
                                .scroll_handle
                                .scroll_to_item(found.visible_row, ScrollStrategy::Center);
                            workspace.status_text =
                                format!("Found match at line {}", found.real_idx + 1).into();
                        } else {
                            workspace.status_text = "Search pattern not found".into();
                        }
                        cx.notify();
                    });
                }
            })
            .detach();
        } else if let Ok(Some(found)) =
            search_in_log(&*source, &res.visible_indices, &query, start_row, None, 0, None)
        {
            self.selection.select(found.visible_row, found.real_idx);
            self.scroll_handle
                .scroll_to_item(found.visible_row, ScrollStrategy::Center);
            self.status_text = format!("Found match at line {}", found.real_idx + 1).into();
            cx.notify();
        } else {
            self.status_text = "Search pattern not found".into();
            cx.notify();
        }
    }

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

    /// Updates the pre-compiled search matcher cached outside the render loop
    pub fn update_cached_search_matcher(&mut self) {
        self.cached_search_matcher = if self.is_search_visible && !self.search_query.trim().is_empty() {
            loganalysis_core::CompiledMatcher::new(
                &self.search_query,
                self.is_search_case_sensitive,
                self.is_search_regex,
            )
            .ok()
            .map(Arc::new)
        } else {
            None
        };
    }

    /// Toggles regex mode for the find bar
    pub fn toggle_search_regex(&mut self, cx: &mut Context<Self>) {
        self.is_search_regex = !self.is_search_regex;
        self.update_cached_search_matcher();
        cx.notify();
    }

    /// Toggles case-sensitivity for the find bar
    pub fn toggle_search_case_sensitive(&mut self, cx: &mut Context<Self>) {
        self.is_search_case_sensitive = !self.is_search_case_sensitive;
        self.update_cached_search_matcher();
        cx.notify();
    }

    /// Toggles the floating find bar
    pub fn toggle_search_bar(&mut self, cx: &mut Context<Self>) {
        self.is_search_visible = !self.is_search_visible;
        if self.is_search_visible {
            self.input_focus = InputFocusTarget::SearchBar;
        } else if self.input_focus == InputFocusTarget::SearchBar {
            self.input_focus = InputFocusTarget::None;
        }
        self.update_cached_search_matcher();
        cx.notify();
    }

    /// Handles keyboard input for active inline text boxes
    pub fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();

        // If the filter modal is open, handle Escape fallback when focus is on non-input elements
        if self.filter_edit_modal.is_open && key == "escape" {
            self.close_filter_modal(cx);
            return;
        }

        if self.input_focus == InputFocusTarget::None {
            return;
        }

        let m = &event.keystroke.modifiers;

        // Support common text box shortcuts (Ctrl+V paste, Ctrl+Backspace delete word, Ctrl+U / Ctrl+A clear)
        if m.control && !m.alt && !m.platform {
            if key == "backspace" || key == "delete" {
                match self.input_focus {
                    InputFocusTarget::SearchBar => {
                        delete_previous_word(&mut self.search_query);
                        self.update_cached_search_matcher();
                        cx.notify();
                    }
                    InputFocusTarget::FilterRuleSearch => {
                        delete_previous_word(&mut self.filter_search_query);
                        cx.notify();
                    }
                    _ => {}
                }
                return;
            } else if key == "v" || key == "V" {
                if let Some(clipboard) = cx.read_from_clipboard() {
                    if let Some(text) = clipboard.text() {
                        let clean = text.replace(['\r', '\n'], "");
                        match self.input_focus {
                            InputFocusTarget::SearchBar => {
                                self.search_query.push_str(&clean);
                                self.update_cached_search_matcher();
                                cx.notify();
                            }
                            InputFocusTarget::FilterRuleSearch => {
                                self.filter_search_query.push_str(&clean);
                                cx.notify();
                            }
                            _ => {}
                        }
                    }
                }
                return;
            } else if key == "u" || key == "U" || key == "a" || key == "A" {
                match self.input_focus {
                    InputFocusTarget::SearchBar => {
                        self.search_query.clear();
                        self.update_cached_search_matcher();
                        cx.notify();
                    }
                    InputFocusTarget::FilterRuleSearch => {
                        self.filter_search_query.clear();
                        cx.notify();
                    }
                    _ => {}
                }
                return;
            }
        }

        // Ignore other shortcut combinations (Ctrl/Alt/Platform) except Shift
        if m.control || m.alt || m.platform {
            return;
        }

        match key {
            "escape" => match self.input_focus {
                InputFocusTarget::SearchBar => {
                    self.is_search_visible = false;
                    self.input_focus = InputFocusTarget::None;
                    self.update_cached_search_matcher();
                    cx.notify();
                }
                InputFocusTarget::FilterRuleSearch => {
                    self.filter_search_query.clear();
                    self.input_focus = InputFocusTarget::None;
                    cx.notify();
                }
                _ => {}
            },
            "enter" => match self.input_focus {
                InputFocusTarget::SearchBar => {
                    self.search(true, cx);
                }
                InputFocusTarget::FilterRuleSearch => {
                    self.input_focus = InputFocusTarget::None;
                    cx.notify();
                }
                _ => {}
            },
            "backspace" => match self.input_focus {
                InputFocusTarget::SearchBar => {
                    self.search_query.pop();
                    self.update_cached_search_matcher();
                    cx.notify();
                }
                InputFocusTarget::FilterRuleSearch => {
                    self.filter_search_query.pop();
                    cx.notify();
                }
                _ => {}
            },
            "delete" => match self.input_focus {
                InputFocusTarget::SearchBar => {
                    self.search_query.pop();
                    self.update_cached_search_matcher();
                    cx.notify();
                }
                InputFocusTarget::FilterRuleSearch => {
                    self.filter_search_query.pop();
                    cx.notify();
                }
                _ => {}
            },
            "space" => match self.input_focus {
                InputFocusTarget::SearchBar => {
                    self.search_query.push(' ');
                    self.update_cached_search_matcher();
                    cx.notify();
                }
                InputFocusTarget::FilterRuleSearch => {
                    self.filter_search_query.push(' ');
                    cx.notify();
                }
                _ => {}
            },
            _ => {
                if let Some(ch) = &event.keystroke.key_char {
                    if !ch.is_empty() && !ch.chars().any(|c| c.is_control()) {
                        match self.input_focus {
                            InputFocusTarget::SearchBar => {
                                self.search_query.push_str(ch);
                                cx.notify();
                            }
                            InputFocusTarget::FilterRuleSearch => {
                                self.filter_search_query.push_str(ch);
                                cx.notify();
                            }
                            _ => {}
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
                        InputFocusTarget::SearchBar => {
                            self.search_query.push_str(&c);
                            self.update_cached_search_matcher();
                            cx.notify();
                        }
                        InputFocusTarget::FilterRuleSearch => {
                            self.filter_search_query.push_str(&c);
                            cx.notify();
                        }
                        _ => {}
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
            if let Some(rule) = tab.filters.get(rule_idx).cloned() {
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
            cx.notify();
            self.reapply_filters(cx);
        }
    }

    /// Refreshes the status line text based on current selection or system state
    pub fn refresh_status_line(&mut self, cx: &mut Context<Self>) {
        if let Some(source_idx) = self.selection.anchor_source_idx {
            if let (Some(src), Some(tab)) = (&self.log_source, self.filter_tabs.get(self.active_tab_index)) {
                if let Some(line) = src.get_line(source_idx) {
                    let mut matched_info = None;
                    for rule in &tab.filters {
                        if !rule.active {
                            continue;
                        }
                        if let Ok(matcher) = loganalysis_core::CompiledMatcher::new(&rule.text, rule.case_sensitive, rule.regex) {
                            if matcher.matches(line.as_bytes()) {
                                let count = self.filter_result.as_ref()
                                    .map(|r| r.count_for_rule(&rule.id))
                                    .unwrap_or(0);
                                matched_info = Some((rule.text.clone(), rule.bg_color.clone(), count));
                                break;
                            }
                        }
                    }
                    if let Some((rule_text, color, count)) = matched_info {
                        self.status_text = format!(
                            "Line {} | Matched Rule: '{}' ({}) | {} matches in tab",
                            source_idx + 1,
                            rule_text,
                            color,
                            count
                        )
                        .into();
                        cx.notify();
                        return;
                    } else {
                        self.status_text = format!("Line {} | No filter rule match", source_idx + 1).into();
                        cx.notify();
                        return;
                    }
                }
            }
        }

        if let Some(fname) = &self.active_file_name {
            self.status_text = format!("Loaded {}", fname).into();
        } else if self.is_streaming {
            if self.is_stream_paused {
                self.status_text = "ADB streaming PAUSED".into();
            } else {
                self.status_text = "Streaming live ADB logcat...".into();
            }
        } else {
            self.status_text = "Ready".into();
        }
        cx.notify();
    }

    /// Selects a log row and updates status line attribution
    pub fn select_log_row(&mut self, visible_idx: usize, source_idx: usize, cx: &mut Context<Self>) {
        self.selection.select(visible_idx, source_idx);
        self.input_focus = InputFocusTarget::None;
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

    /// Prompts user to save filter profiles to a JSON file atomically with filters
    pub fn prompt_save_filters(&mut self, cx: &mut Context<Self>) {
        let Ok(json_str) = self.export_filters_json() else {
            self.status_text = "Failed to serialize filters".into();
            cx.notify();
            return;
        };

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut async_cx = cx.clone();
            async move {
                let file = rfd::AsyncFileDialog::new()
                    .set_title("Save Filter Profile")
                    .set_file_name("filters.json")
                    .add_filter("JSON Profile (*.json)", &["json"])
                    .add_filter("All Files (*.*)", &["*"])
                    .save_file()
                    .await;

                if let Some(handle) = file {
                    let path = handle.path().to_path_buf();
                    let tmp_path = path.with_extension("tmp");
                    let write_res = std::fs::write(&tmp_path, json_str.as_bytes())
                        .and_then(|_| std::fs::rename(&tmp_path, &path));

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
                    .spawn(async move { std::fs::read_to_string(&path_clone) })
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

    /// Prompts user to select and load a filter profile JSON file with JSON and All Files filters
    pub fn prompt_load_filters(&mut self, cx: &mut Context<Self>) {
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut async_cx = cx.clone();
            async move {
                let file = rfd::AsyncFileDialog::new()
                    .set_title("Load Filter Profile")
                    .add_filter("JSON Profile (*.json)", &["json"])
                    .add_filter("All Files (*.*)", &["*"])
                    .pick_file()
                    .await;

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

    /// Clears currently loaded logs (Ctrl+K)
    pub fn clear_logs(&mut self, cx: &mut Context<Self>) {
        if self.is_streaming {
            self.stop_adb_stream(cx);
        }
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

    /// Scrolls the log viewport based on mouse vertical position in the scrollbar track
    pub fn scroll_to_ratio_from_mouse_y(&mut self, mouse_y: Pixels, cx: &mut Context<Self>) {
        let row_count = self
            .filter_result
            .as_ref()
            .map(|r| r.visible_indices.len())
            .unwrap_or(0);

        if row_count == 0 {
            return;
        }

        let bounds = self.scroll_handle.0.borrow().base_handle.bounds();
        let track_top: f32 = bounds.origin.y.into();
        let track_height: f32 = bounds.size.height.into();
        let mouse_y_f32: f32 = mouse_y.into();

        if track_height <= 0.0 {
            return;
        }

        let relative_y = (mouse_y_f32 - track_top).clamp(0.0, track_height);
        let ratio = (relative_y / track_height).clamp(0.0, 1.0);
        let max_idx = row_count.saturating_sub(1);
        let target_idx = ((ratio * max_idx as f32).round() as usize).min(max_idx);

        if ratio >= 0.98 {
            self.scroll_handle.scroll_to_item_strict(target_idx, ScrollStrategy::Bottom);
        } else {
            self.scroll_handle.scroll_to_item_strict(target_idx, ScrollStrategy::Top);
        }
        cx.notify();
    }

    /// Toggles ADB stream pause and flushes buffered chunks when resumed
    pub fn toggle_pause_stream(&mut self, cx: &mut Context<Self>) {
        if self.input_focus != InputFocusTarget::None {
            return;
        }
        if self.is_streaming {
            self.is_stream_paused = !self.is_stream_paused;
            if !self.is_stream_paused {
                // Flush buffered chunks
                let buffered = std::mem::take(&mut self.paused_stream_buffer);
                self.paused_stream_line_count = 0;
                if !buffered.is_empty() {
                    if let Some(src) = &mut self.log_source {
                        if let Some(active_src) = Arc::get_mut(src) {
                            if let ActiveLogSource::Stream(ref mut stream) = active_src {
                                for chunk in buffered {
                                    stream.append_chunk(chunk);
                                }
                                stream.trim_if_needed();
                            }
                        } else if let ActiveLogSource::Stream(ref stream) = **src {
                            let mut new_stream = stream.clone();
                            for chunk in buffered {
                                new_stream.append_chunk(chunk);
                            }
                            new_stream.trim_if_needed();
                            *src = Arc::new(ActiveLogSource::Stream(new_stream));
                        }
                        self.reapply_filters(cx);
                    }
                }
                self.status_text = "ADB streaming RESUMED".into();
            } else {
                self.status_text = "ADB streaming PAUSED (Press Ctrl+Space to resume)".into();
            }
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


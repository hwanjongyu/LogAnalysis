mod actions;
mod selection;
mod theme;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use crossbeam_channel::unbounded;
use gpui::*;
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

pub struct LogAnalysisWorkspace {
    focus_handle: FocusHandle,
    log_source: Option<Arc<ActiveLogSource>>,
    filter_result: Option<Arc<FilterResult>>,
    scroll_handle: UniformListScrollHandle,
    selection: SelectionModel,
    theme: Theme,

    // Filter state
    filter_tabs: Vec<FilterTabData>,
    active_tab_index: usize,
    show_only_filtered: bool,
    cancel_generation: Arc<AtomicU64>,

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

    // Display & Status
    font_size: Pixels,
    status_text: SharedString,
    active_file_name: Option<SharedString>,
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

            filter_tabs: vec![default_tab],
            active_tab_index: 0,
            show_only_filtered: false,
            cancel_generation: Arc::new(AtomicU64::new(0)),

            search_query: String::new(),
            is_search_visible: false,
            is_search_regex: false,
            is_search_case_sensitive: false,

            adb_devices: Vec::new(),
            selected_device: None,
            adb_stream_handle: None,
            is_streaming: false,

            font_size: px(12.0),
            status_text: "Ready | Open a file or start ADB streaming".into(),
            active_file_name: None,
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
                                if let Some(src) = &workspace.log_source {
                                    if let ActiveLogSource::Stream(ref stream) = **src {
                                        let mut new_stream = stream.clone();
                                        new_stream.append_chunk(chunk);
                                        new_stream.trim_if_needed();
                                        workspace.log_source = Some(Arc::new(ActiveLogSource::Stream(new_stream)));
                                        workspace.reapply_filters(cx);
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
        cx.notify();
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
}

impl Render for LogAnalysisWorkspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let row_count = self.filter_result.as_ref().map(|r| r.visible_indices.len()).unwrap_or(0);
        let font_size = self.font_size;
        let row_height = font_size * 1.5;

        let filter_res = self.filter_result.clone();
        let source = self.log_source.clone();
        let selected = self.selection.selected_row;
        let has_log_source = self.log_source.is_some();

        div()
            .id("workspace-container")
            .track_focus(&self.focus_handle)
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
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_dark)
            .text_color(theme.text_primary)
            // Top Toolbar
            .child(
                div()
                    .h(px(40.0))
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
                            .gap_3()
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(14.0))
                                    .child("LogAnalysis"),
                            )
                            // Open File Button in Toolbar
                            .child(
                                div()
                                    .id("btn-toolbar-open")
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded_md()
                                    .bg(theme.accent)
                                    .text_color(rgb(0xffffff))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.prompt_open_file(cx);
                                    }))
                                    .child("📂 Open File (Ctrl+O)"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_muted)
                                    .child(if let Some(name) = &self.active_file_name {
                                        name.clone()
                                    } else {
                                        "No file open".into()
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            // Find Button
                            .child(
                                div()
                                    .id("btn-toolbar-find")
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded_md()
                                    .bg(if self.is_search_visible { theme.accent } else { theme.btn_hover })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.toggle_search_bar(cx);
                                    }))
                                    .child("Find (Ctrl+F)"),
                            )
                            // Show Only Filtered Button
                            .child(
                                div()
                                    .id("btn-toolbar-filter-only")
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded_md()
                                    .bg(if self.show_only_filtered { theme.accent } else { theme.btn_hover })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.toggle_filtered_only(cx);
                                    }))
                                    .child(if self.show_only_filtered { "Filtered Only (ON)" } else { "Filtered Only (OFF)" }),
                            )
                            // ADB Stream Button
                            .child(
                                div()
                                    .id("btn-toolbar-adb")
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded_md()
                                    .bg(if self.is_streaming { theme.error_red } else { theme.success_green })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        if this.is_streaming {
                                            this.stop_adb_stream(cx);
                                        } else {
                                            this.start_adb_stream(cx);
                                        }
                                    }))
                                    .child(if self.is_streaming { "Stop ADB" } else { "Start ADB" }),
                            ),
                    ),
            )
            // Main Body: Split Filter Sidebar & Log Viewport
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    // Filter Sidebar (Left Panel)
                    .child(
                        div()
                            .w(px(260.0))
                            .h_full()
                            .bg(theme.bg_panel)
                            .border_r_1()
                            .border_color(theme.border)
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(32.0))
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
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_size(px(12.0))
                                            .child("Filter Rules"),
                                    ),
                            )
                            // Filter Rules List
                            .child(
                                div()
                                    .id("filter-rules-scroll")
                                    .flex_1()
                                    .overflow_y_scroll()
                                    .p_2()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .children(
                                        self.filter_tabs.get(self.active_tab_index).map(|tab| {
                                            tab.filters.iter().map(|rule| {
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .px(px(6.0))
                                                    .py(px(4.0))
                                                    .rounded_sm()
                                                    .bg(theme.bg_header)
                                                    .border_1()
                                                    .border_color(theme.border)
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .child(rule.text.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .text_color(if rule.exclude { theme.error_red } else { theme.text_muted })
                                                            .child(if rule.exclude { "EXCLUDE" } else { "MATCH" }),
                                                    )
                                            }).collect::<Vec<_>>()
                                        }).unwrap_or_default()
                                    ),
                            ),
                    )
                    // Virtualized Log Viewport (Center)
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .flex_col()
                            .bg(theme.bg_dark)
                            .children(if self.is_search_visible {
                                Some(
                                    div()
                                        .h(px(32.0))
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
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(theme.text_muted)
                                                        .child("Find:"),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .child(if self.search_query.is_empty() { "Press F3 for Next, Shift+F3 for Prev".to_string() } else { self.search_query.clone() }),
                                                ),
                                        ),
                                )
                            } else {
                                None
                            })
                            .child(if has_log_source {
                                uniform_list("log-view", row_count, move |range, _window, _cx| {
                                    range.map(|idx| {
                                        let (real_idx, text) = if let (Some(res), Some(src)) = (&filter_res, &source) {
                                            let r_idx = res.visible_indices.get(idx).copied().unwrap_or(0);
                                            let t = src.get_line(r_idx).unwrap_or_default().to_string();
                                            (r_idx, t)
                                        } else {
                                            (0, String::new())
                                        };

                                        let (bg_color, fg_color) = if let Some(res) = &filter_res {
                                            if let Some(style) = res.visible_styles.get(idx) {
                                                (
                                                    style.bg_hex.map(rgb).unwrap_or(theme.bg_dark),
                                                    style.fg_hex.map(rgb).unwrap_or(theme.text_primary),
                                                )
                                            } else {
                                                (theme.bg_dark, theme.text_primary)
                                            }
                                        } else {
                                            (theme.bg_dark, theme.text_primary)
                                        };

                                        let is_selected = selected == Some(idx);
                                        let final_bg = if is_selected { theme.selection_bg } else { bg_color };

                                        div()
                                            .h(row_height)
                                            .min_h(row_height)
                                            .max_h(row_height)
                                            .w_full()
                                            .bg(final_bg)
                                            .text_color(fg_color)
                                            .text_size(font_size)
                                            .line_height(row_height)
                                            .flex()
                                            .items_center()
                                            .flex_shrink_0()
                                            .overflow_hidden()
                                            // Line number gutter
                                            .child(
                                                div()
                                                    .w(px(54.0))
                                                    .h_full()
                                                    .flex_shrink_0()
                                                    .bg(theme.bg_gutter)
                                                    .text_color(theme.text_gutter)
                                                    .flex()
                                                    .items_center()
                                                    .justify_end()
                                                    .pr(px(6.0))
                                                    .whitespace_nowrap()
                                                    .overflow_hidden()
                                                    .child(format!("{}", real_idx + 1)),
                                            )
                                            // Log Line Text
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .px(px(8.0))
                                                    .whitespace_nowrap()
                                                    .overflow_hidden()
                                                    .child(text),
                                            )
                                    }).collect::<Vec<_>>()
                                })
                                .track_scroll(self.scroll_handle.clone())
                                .flex_1()
                                .into_any_element()
                            } else {
                                // Empty state when no file is open
                                div()
                                    .flex_1()
                                    .h_full()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .gap_4()
                                    .child(
                                        div()
                                            .text_size(px(16.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.text_primary)
                                            .child("📂 No log file opened"),
                                    )
                                    .child(
                                        div()
                                            .id("btn-empty-state-open")
                                            .px(px(16.0))
                                            .py(px(8.0))
                                            .rounded_md()
                                            .bg(theme.accent)
                                            .text_color(rgb(0xffffff))
                                            .font_weight(FontWeight::MEDIUM)
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.prompt_open_file(cx);
                                            }))
                                            .child("Open Log File (Ctrl+O)"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme.text_muted)
                                            .child("Or launch: loganalysis <path_to_log_file>"),
                                    )
                                    .into_any_element()
                            }),
                    ),
            )
            // Bottom Status Bar
            .child(
                div()
                    .h(px(24.0))
                    .w_full()
                    .bg(theme.bg_header)
                    .border_t_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(8.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(self.status_text.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child("120 FPS GPU Virtualized (GPUI)"),
                    ),
            )
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
        if let Err(e) = cx.open_window(options, move |_window, cx| {
            let entity = cx.new(LogAnalysisWorkspace::new);
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

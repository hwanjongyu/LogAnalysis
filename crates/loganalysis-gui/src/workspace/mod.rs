mod adb;
mod bookmarks;
mod command_palette;
mod dock;
mod filters;
mod merged;
mod recent_files;
mod search;
mod selection;
mod split_view;

#[allow(unused_imports)]
pub use split_view::probe_parent_timestamp;
#[allow(unused_imports)]
pub(crate) use recent_files::{get_recent_files_path, load_recent_files, save_recent_files};


use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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
    pub(crate) heatmap_data: Option<Arc<HeatmapData>>,
    pub(crate) timeline_data: Option<Arc<EpsTimelineData>>,
    pub(crate) is_timeline_visible: bool,
    pub(crate) analytics_seq: u64,
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
    pub(crate) filter_rule_input: Entity<TextInput>,

    // Search state
    pub(crate) search_query: String,
    pub(crate) is_search_visible: bool,
    pub(crate) is_search_regex: bool,
    pub(crate) is_search_case_sensitive: bool,
    pub(crate) cached_search_matcher: Option<Arc<loganalysis_core::CompiledMatcher>>,
    pub(crate) search_cancel_gen: Arc<AtomicU64>,
    pub(crate) find_input: Entity<TextInput>,
    pub(crate) search_match_status: Option<String>,

    // ADB state
    pub(crate) adb_devices: Vec<String>,
    pub(crate) adb_device_details: Vec<loganalysis_core::AdbDeviceInfo>,
    pub(crate) selected_device: Option<String>,
    pub(crate) is_device_selector_open: bool,
    pub(crate) is_scanning_adb: bool,
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
    pub(crate) recent_files: Vec<std::path::PathBuf>,
    pub(crate) is_dark_theme: bool,
    pub(crate) full_line_display: bool,
    pub(crate) filter_panel_position: FilterPanelPosition,
    pub(crate) is_dragging_scrollbar: bool,
    pub(crate) is_dragging_secondary_scrollbar: bool,

    // Command Palette state
    pub(crate) is_command_palette_open: bool,
    pub(crate) palette_input: Entity<TextInput>,
    pub(crate) palette_selected_index: usize,

    // Bookmark state
    pub(crate) bookmarks: BookmarkManager,
    pub(crate) is_bookmark_drawer_open: bool,
    pub(crate) bookmark_drawer_selected_index: usize,
    pub(crate) is_note_modal_open: bool,
    pub(crate) note_editing_source_idx: Option<usize>,
    pub(crate) note_input: Entity<TextInput>,

    // Split View state
    pub(crate) is_split_view: bool,
    pub(crate) is_split_vertical: bool,
    pub(crate) is_scroll_synced: bool,
    pub(crate) active_split_pane_is_secondary: bool,
    pub(crate) secondary_log_source: Option<Arc<ActiveLogSource>>,
    pub(crate) secondary_file_name: Option<SharedString>,
    pub(crate) secondary_filter_result: Option<Arc<FilterResult>>,
    pub(crate) secondary_scroll_handle: UniformListScrollHandle,
    pub(crate) secondary_selection: SelectionModel,

    // Column View state
    pub(crate) is_column_view: bool,

    // Workstation & Facets state
    pub(crate) is_filter_dock_visible: bool,
    pub(crate) is_facets_visible: bool,
    pub(crate) facet_tags: Vec<(String, usize)>,

    // Resizable Filter Dock state
    pub(crate) filter_dock_width: f32,
    pub(crate) filter_dock_height: f32,
    pub(crate) is_resizing_filter_dock: bool,
    pub(crate) filter_dock_drag_start_pos: Option<Point<Pixels>>,
    pub(crate) filter_dock_drag_start_size: f32,
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
                    if this.search_query.trim().is_empty() {
                        this.search_match_status = None;
                    }
                    this.update_cached_search_matcher();
                    cx.notify();
                }
            },
        )
        .detach();

        let filter_rule_input = cx.new(|cx| TextInput::new("Filter rules in tab...", cx));

        cx.subscribe(
            &filter_rule_input,
            |this, input, event: &TextInputEvent, cx| match event {
                TextInputEvent::Submit => {
                    this.input_focus = InputFocusTarget::None;
                    cx.notify();
                }
                TextInputEvent::Cancel => {
                    input.update(cx, |inp, cx| inp.set_text("", cx));
                    this.filter_search_query.clear();
                    this.input_focus = InputFocusTarget::None;
                    cx.notify();
                }
                TextInputEvent::Change => {
                    this.filter_search_query = input.read(cx).text().to_string();
                    cx.notify();
                }
            },
        )
        .detach();

        let palette_input = cx.new(|cx| TextInput::new("Type a command or search (Esc to close)...", cx));

        cx.subscribe(
            &palette_input,
            |this, _input, event: &TextInputEvent, cx| match event {
                TextInputEvent::Submit => {
                    this.submit_command_palette(None, cx);
                }
                TextInputEvent::Cancel => {
                    this.close_command_palette(cx);
                }
                TextInputEvent::Change => {
                    this.palette_selected_index = 0;
                    cx.notify();
                }
            },
        )
        .detach();

        let note_input = cx.new(|cx| TextInput::new("Add triage note or root cause...", cx));

        cx.subscribe(
            &note_input,
            |this, _input, event: &TextInputEvent, cx| match event {
                TextInputEvent::Submit => {
                    this.save_bookmark_note(cx);
                }
                TextInputEvent::Cancel => {
                    this.close_note_modal(cx);
                }
                TextInputEvent::Change => {}
            },
        )
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            log_source: None,
            filter_result: None,
            heatmap_data: None,
            timeline_data: None,
            is_timeline_visible: true,
            analytics_seq: 0,
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
            filter_rule_input,

            search_query: String::new(),
            is_search_visible: false,
            is_search_regex: false,
            is_search_case_sensitive: false,
            cached_search_matcher: None,
            search_cancel_gen: Arc::new(AtomicU64::new(0)),
            find_input,
            search_match_status: None,

            adb_devices: Vec::new(),
            adb_device_details: Vec::new(),
            selected_device: None,
            is_device_selector_open: false,
            is_scanning_adb: false,
            adb_stream_handle: None,
            is_streaming: false,
            is_stream_paused: false,
            auto_scroll: true,
            paused_stream_buffer: VecDeque::new(),
            paused_stream_line_count: 0,

            font_size: px(12.0),
            status_text: "Ready | Open a file or start ADB streaming".into(),
            active_file_name: None,
            recent_files: load_recent_files(),
            is_dark_theme: false,
            full_line_display: false,
            filter_panel_position: FilterPanelPosition::Bottom,
            is_dragging_scrollbar: false,
            is_dragging_secondary_scrollbar: false,
            is_command_palette_open: false,
            palette_input,
            palette_selected_index: 0,
            bookmarks: BookmarkManager::new(),
            is_bookmark_drawer_open: false,
            bookmark_drawer_selected_index: 0,
            is_note_modal_open: false,
            note_editing_source_idx: None,
            note_input,

            is_split_view: false,
            is_split_vertical: true,
            is_scroll_synced: true,
            active_split_pane_is_secondary: false,
            secondary_log_source: None,
            secondary_file_name: None,
            secondary_filter_result: None,
            secondary_scroll_handle: UniformListScrollHandle::new(),
            secondary_selection: SelectionModel::new(),

            is_column_view: false,
            is_filter_dock_visible: true,
            is_facets_visible: true,
            facet_tags: Vec::new(),
            filter_dock_width: 310.0,
            filter_dock_height: 220.0,
            is_resizing_filter_dock: false,
            filter_dock_drag_start_pos: None,
            filter_dock_drag_start_size: 0.0,
        }
    }

    /// Prompts user to select a log file using native OS file dialog attached to window
    pub fn prompt_open_file(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
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
                            workspace.bookmarks.clear();
                            workspace.add_recent_file(path_buf.clone());
                            workspace.refresh_facet_tags();
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
        if self.is_split_view && self.secondary_log_source.is_some() {
            self.reapply_secondary_filters(cx);
        }

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
                        let res = execute_filtering(
                            &*source_for_bg,
                            &active_filters,
                            show_only,
                            Some(&token),
                            generation,
                        )?;
                        let heatmap = HeatmapData::compute(
                            &*source_for_bg,
                            &res.visible_indices,
                            &res.visible_styles,
                            200,
                        );
                        let timeline = EpsTimelineData::compute(
                            &*source_for_bg,
                            &res.visible_indices,
                            &res.visible_styles,
                            80,
                        );
                        Some((res, heatmap, timeline))
                    })
                    .await;

                if let Some((res, heatmap, timeline)) = result {
                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        // Check if a newer generation superseded this task
                        if workspace.filter_cancel_gen.load(Ordering::SeqCst) != generation {
                            return;
                        }

                        let prev_anchor = workspace.selection.anchor_source_idx;
                        workspace.filter_result = Some(Arc::new(res));
                        workspace.heatmap_data = Some(Arc::new(heatmap));
                        workspace.timeline_data = Some(Arc::new(timeline));

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
    /// Imports filter profile JSON supporting all 3 legacy Python formats
    pub fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();

        // If the filter modal is open, handle Escape and Enter fallback when focus is on non-input elements
        if self.filter_edit_modal.is_open {
            if key == "escape" {
                self.close_filter_modal(cx);
                return;
            } else if key == "enter" {
                self.save_modal_filter(cx);
                return;
            }
        }

        // If the note modal is open, handle Escape and Enter
        if self.is_note_modal_open {
            if key == "escape" {
                self.close_note_modal(cx);
                return;
            } else if key == "enter" {
                self.save_bookmark_note(cx);
                return;
            }
        }

        // If the bookmark drawer is open, handle navigation keys
        if self.is_bookmark_drawer_open {
            if key == "escape" {
                self.close_bookmark_drawer(cx);
                return;
            } else if key == "up" {
                if self.bookmark_drawer_selected_index > 0 {
                    self.bookmark_drawer_selected_index -= 1;
                    cx.notify();
                }
                return;
            } else if key == "down" {
                if self.bookmark_drawer_selected_index + 1 < self.bookmarks.len() {
                    self.bookmark_drawer_selected_index += 1;
                    cx.notify();
                }
                return;
            } else if key == "enter" {
                if let Some(b) = self.bookmarks.items().get(self.bookmark_drawer_selected_index) {
                    let src_idx = b.source_idx;
                    self.jump_to_bookmark_source_idx(src_idx, cx);
                    self.close_bookmark_drawer(cx);
                }
                return;
            } else if key == "delete" || key == "backspace" {
                if let Some(b) = self.bookmarks.items().get(self.bookmark_drawer_selected_index) {
                    let src_idx = b.source_idx;
                    let line_num = b.line_number;
                    self.bookmarks.remove(src_idx);
                    self.status_text = format!("Removed bookmark on line {line_num}").into();
                    cx.notify();
                }
                return;
            }
        }

        // Navigate log lines with Up and Down arrows
        if self.input_focus == InputFocusTarget::None
            && !self.filter_edit_modal.is_open
            && !self.is_note_modal_open
            && !self.is_bookmark_drawer_open
            && !self.is_command_palette_open
        {
            let is_sec = self.is_split_view && self.active_split_pane_is_secondary;
            if key == "up" {
                if is_sec {
                    if let Some(res) = &self.secondary_filter_result {
                        let total_rows = res.visible_indices.len();
                        if total_rows > 0 {
                            let cur_row = self.secondary_selection.selected_row.unwrap_or(0);
                            let new_row = cur_row.saturating_sub(1);
                            if let Some(&src_idx) = res.visible_indices.get(new_row) {
                                self.select_secondary_log_row(new_row, src_idx, cx);
                                self.secondary_scroll_handle.scroll_to_item_strict(new_row, ScrollStrategy::Center);
                            }
                        }
                    }
                } else if let Some(res) = &self.filter_result {
                    let total_rows = res.visible_indices.len();
                    if total_rows > 0 {
                        let cur_row = self.selection.selected_row.unwrap_or(0);
                        let new_row = cur_row.saturating_sub(1);
                        if let Some(&src_idx) = res.visible_indices.get(new_row) {
                            self.select_log_row(new_row, src_idx, cx);
                            self.scroll_handle.scroll_to_item_strict(new_row, ScrollStrategy::Center);
                        }
                    }
                }
            } else if key == "down" {
                if is_sec {
                    if let Some(res) = &self.secondary_filter_result {
                        let total_rows = res.visible_indices.len();
                        if total_rows > 0 {
                            let cur_row = self.secondary_selection.selected_row.map(|r| r + 1).unwrap_or(0);
                            let new_row = cur_row.min(total_rows.saturating_sub(1));
                            if let Some(&src_idx) = res.visible_indices.get(new_row) {
                                self.select_secondary_log_row(new_row, src_idx, cx);
                                self.secondary_scroll_handle.scroll_to_item_strict(new_row, ScrollStrategy::Center);
                            }
                        }
                    }
                } else if let Some(res) = &self.filter_result {
                    let total_rows = res.visible_indices.len();
                    if total_rows > 0 {
                        let cur_row = self.selection.selected_row.map(|r| r + 1).unwrap_or(0);
                        let new_row = cur_row.min(total_rows.saturating_sub(1));
                        if let Some(&src_idx) = res.visible_indices.get(new_row) {
                            self.select_log_row(new_row, src_idx, cx);
                            self.scroll_handle.scroll_to_item_strict(new_row, ScrollStrategy::Center);
                        }
                    }
                }
            }
        }
    }

    /// Adds a new filter rule to the active tab
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

    /// Clears currently loaded logs (Ctrl+K)
    pub fn clear_logs(&mut self, cx: &mut Context<Self>) {
        if self.is_streaming {
            self.stop_adb_stream(cx);
        }
        self.log_source = None;
        self.filter_result = None;
        self.heatmap_data = None;
        self.timeline_data = None;
        self.selection.clear();
        self.bookmarks.clear();
        self.facet_tags.clear();
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
        let font_size_f32: f32 = self.font_size.into();
        let row_height_f32 = font_size_f32 * 1.55;
        let visible_items = (track_height / row_height_f32).floor() as usize;
        let max_top_idx = row_count.saturating_sub(visible_items.max(1));
        let target_top_idx = ((ratio * max_top_idx as f32).round() as usize).min(max_top_idx);

        self.scroll_handle.scroll_to_item_strict(target_top_idx, ScrollStrategy::Top);
        cx.notify();
    }

    /// Scrolls the secondary log viewport based on mouse vertical position in the scrollbar track
    pub fn scroll_secondary_to_ratio_from_mouse_y(&mut self, mouse_y: Pixels, cx: &mut Context<Self>) {
        let row_count = self
            .secondary_filter_result
            .as_ref()
            .map(|r| r.visible_indices.len())
            .unwrap_or(0);

        if row_count == 0 {
            return;
        }

        let bounds = self.secondary_scroll_handle.0.borrow().base_handle.bounds();
        let track_top: f32 = bounds.origin.y.into();
        let track_height: f32 = bounds.size.height.into();
        let mouse_y_f32: f32 = mouse_y.into();

        if track_height <= 0.0 {
            return;
        }

        let relative_y = (mouse_y_f32 - track_top).clamp(0.0, track_height);
        let ratio = (relative_y / track_height).clamp(0.0, 1.0);
        let font_size_f32: f32 = self.font_size.into();
        let row_height_f32 = font_size_f32 * 1.55;
        let visible_items = (track_height / row_height_f32).floor() as usize;
        let max_top_idx = row_count.saturating_sub(visible_items.max(1));
        let target_top_idx = ((ratio * max_top_idx as f32).round() as usize).min(max_top_idx);

        self.secondary_scroll_handle.scroll_to_item_strict(target_top_idx, ScrollStrategy::Top);
        cx.notify();
    }

    /// Adds a new filter tab (Ctrl+T)
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

    /// Toggles filter dock visibility between shown and hidden
    /// Returns human-readable buffer memory footprint and storage mechanism
    pub fn get_buffer_memory_display(&self) -> (String, &'static str) {
        if let Some(source) = &self.log_source {
            match &**source {
                ActiveLogSource::File(fb) => {
                    let bytes = fb.byte_size();
                    if bytes < 1024 * 1024 {
                        (format!("{:.1} KB", bytes as f64 / 1024.0), "MMap")
                    } else if bytes < 1024 * 1024 * 1024 {
                        (format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)), "MMap")
                    } else {
                        (format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0)), "MMap")
                    }
                }
                ActiveLogSource::Stream(sb) => {
                    let lines = sb.line_count();
                    let approx_bytes = lines * 128;
                    if approx_bytes < 1024 * 1024 {
                        (format!("{:.1} KB", approx_bytes as f64 / 1024.0), "Heap")
                    } else {
                        (format!("{:.1} MB", approx_bytes as f64 / (1024.0 * 1024.0)), "Heap")
                    }
                }
                ActiveLogSource::Merged(mb) => {
                    let lines = mb.line_count();
                    let approx_bytes = lines * 128;
                    (format!("{:.1} MB", approx_bytes as f64 / (1024.0 * 1024.0)), "Merged")
                }
            }
        } else {
            ("0 MB".to_string(), "Idle")
        }
    }
    /// Recomputes heatmap density and EPS timeline data for the active filter result.
    pub(crate) fn update_analytics_data(&mut self) {
        if let (Some(filter_res), Some(src_ref)) = (&self.filter_result, &self.log_source) {
            self.heatmap_data = Some(Arc::new(HeatmapData::compute(
                &**src_ref,
                &filter_res.visible_indices,
                &filter_res.visible_styles,
                200,
            )));
            if self.is_timeline_visible {
                self.timeline_data = Some(Arc::new(EpsTimelineData::compute(
                    &**src_ref,
                    &filter_res.visible_indices,
                    &filter_res.visible_styles,
                    80,
                )));
            }
        }
    }

    /// Toggles the Throughput & EPS Timeline Sparkline ribbon (Ctrl+Shift+T)
    pub fn toggle_eps_timeline(&mut self, cx: &mut Context<Self>) {
        self.is_timeline_visible = !self.is_timeline_visible;
        if self.is_timeline_visible && self.timeline_data.is_none() {
            if let (Some(src_ref), Some(filter_res)) = (&self.log_source, &self.filter_result) {
                self.timeline_data = Some(Arc::new(EpsTimelineData::compute(
                    &**src_ref,
                    &filter_res.visible_indices,
                    &filter_res.visible_styles,
                    80,
                )));
            }
        }
        self.status_text = if self.is_timeline_visible {
            "Throughput Timeline Ribbon: ON".into()
        } else {
            "Throughput Timeline Ribbon: OFF".into()
        };
        cx.notify();
    }
    /// Toggles Structured Column Grid View vs Raw Monospace View (Ctrl+G / Cmd+G)
    pub fn toggle_column_view(&mut self, cx: &mut Context<Self>) {
        self.is_column_view = !self.is_column_view;
        self.status_text = if self.is_column_view {
            "Structured Column Grid View: ON".into()
        } else {
            "Structured Column Grid View: OFF (Raw Monospace)".into()
        };
        cx.notify();
    }

    /// Adds a quick filter rule for a specific tag
    pub fn add_quick_tag_filter(&mut self, tag: &str, cx: &mut Context<Self>) {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return;
        }
        let rule = FilterRule {
            text: trimmed.to_string(),
            regex: false,
            case_sensitive: true,
            exclude: false,
            bg_color: "Cyan".to_string(),
            text_color: "Black".to_string(),
            active: true,
            ..Default::default()
        };
        self.add_filter_rule(rule, cx);
        self.status_text = format!("Added quick filter for tag: {}", trimmed).into();
    }

}

mod adb;
mod bookmarks;
mod command_palette;
mod dock;
mod filters;
mod merged;
mod pattern_history;
mod recent_files;
mod search;
mod selection;
mod split_view;

pub use dock::{
    apply_facet_filter_to_tab, clamp_filter_dock_height, clamp_filter_dock_width,
    compute_facet_tags, default_filter_dock_size, DEFAULT_FILTER_DOCK_HEIGHT,
    DEFAULT_FILTER_DOCK_WIDTH, MAX_FILTER_DOCK_HEIGHT, MAX_FILTER_DOCK_WIDTH,
    MIN_FILTER_DOCK_HEIGHT, MIN_FILTER_DOCK_WIDTH,
};
pub use filters::{
    compute_drop_target_index, remap_editing_index_on_delete, remap_editing_index_on_duplicate,
    remap_index_after_move,
};
pub use pattern_history::{PatternHistory, PatternHistoryEntry};
pub use recent_files::{load_recent_files_from_path, push_recent_path, save_recent_files_to_path};
pub use selection::ActiveSelectedLine;
pub use split_view::probe_parent_timestamp;

/// Default height in logical pixels of the collapsible Line Detail inspector panel.
pub const DEFAULT_LINE_DETAIL_HEIGHT: f32 = 160.0;
/// Minimum allowed height in logical pixels of the collapsible Line Detail inspector panel.
pub const MIN_LINE_DETAIL_HEIGHT: f32 = 90.0;
/// Maximum allowed height in logical pixels of the collapsible Line Detail inspector panel.
pub const MAX_LINE_DETAIL_HEIGHT: f32 = 420.0;
/// Horizontal keyboard scroll step in logical pixels for Left/Right arrow keys.
pub const H_SCROLL_STEP_PX: f32 = 40.0;
/// Accelerated horizontal keyboard scroll step in logical pixels when holding Shift.
pub const H_SCROLL_FAST_STEP_PX: f32 = 160.0;

/// Clamps Line Detail inspector panel height adjustment from `start_height + delta` to `[90.0, 420.0]`.
#[inline]
pub fn clamp_line_detail_height(start_height: f32, delta: f32) -> f32 {
    let sum = start_height + delta;
    if !sum.is_finite() {
        return DEFAULT_LINE_DETAIL_HEIGHT;
    }
    sum.clamp(MIN_LINE_DETAIL_HEIGHT, MAX_LINE_DETAIL_HEIGHT)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineDetailCacheKey {
    pub is_secondary: bool,
    pub abs_source_idx: usize,
    pub pretty_json: bool,
    pub line_len: usize,
}

use gpui::*;
use loganalysis_core::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::components::text_input::{TextInput, TextInputEvent};
use crate::controllers::{
    AdbController, BookmarkController, SearchController, SplitViewController,
};
use crate::selection::SelectionModel;
use crate::theme::Theme;
use crate::types::{
    AdbStreamState, BookmarkDrawerState, FilterDropTarget, FilterEditState, FilterPanelPosition,
    InputFocusTarget, SearchState, SplitViewState,
};

/// Default workspace visibility/mode flags applied on workspace initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceDefaults {
    pub is_timeline_visible: bool,
    pub is_zen_mode: bool,
    pub full_line_display: bool,
}

/// Canonical default flags for newly initialized workspaces.
pub const WORKSPACE_DEFAULTS: WorkspaceDefaults = WorkspaceDefaults {
    is_timeline_visible: false,
    is_zen_mode: false,
    full_line_display: true,
};

pub struct LogAnalysisWorkspace {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) log_source: Option<Arc<ActiveLogSource>>,
    pub(crate) primary_source_gen: u64,
    pub(crate) filter_result: Option<Arc<FilterResult>>,
    pub(crate) heatmap_data: Option<Arc<HeatmapData>>,
    pub(crate) timeline_data: Option<Arc<EpsTimelineData>>,
    pub(crate) is_timeline_visible: bool,
    pub(crate) analytics_cancel_gen: Arc<AtomicU64>,
    pub(crate) scroll_handle: UniformListScrollHandle,
    pub(crate) selection: SelectionModel,
    pub(crate) theme: Theme,
    pub(crate) input_focus: InputFocusTarget,

    // Filter state
    pub(crate) filter_tabs: Vec<FilterTabData>,
    pub(crate) active_tab_index: usize,
    pub(crate) active_filter_profile_path: Option<std::path::PathBuf>,
    pub(crate) show_only_filtered: bool,
    pub(crate) filter_cancel_gen: Arc<AtomicU64>,
    pub(crate) cached_stream_filters: Option<Arc<CompiledStreamFilters>>,
    pub(crate) filter_edit_modal: FilterEditState,
    pub(crate) modal_pattern_input: Entity<TextInput>,
    pub(crate) filter_search_query: String,
    pub(crate) filter_rule_input: Entity<TextInput>,
    pub(crate) filter_rules_scroll_handle: ScrollHandle,
    pub(crate) filter_drag_drop_target: Option<FilterDropTarget>,
    pub(crate) dragging_filter_rule_index: Option<usize>,

    // Search state
    pub(crate) search_ctrl: SearchController,
    pub(crate) find_input: Entity<TextInput>,

    // ADB state
    pub(crate) adb: AdbController,

    // Display & Status
    pub(crate) font_size: Pixels,
    pub(crate) status_text: SharedString,
    pub(crate) active_file_name: Option<SharedString>,
    pub(crate) recent_files: Vec<std::path::PathBuf>,
    pub(crate) recent_files_gen: u64,
    pub(crate) pattern_history: PatternHistory,
    pub(crate) is_dark_theme: bool,
    pub(crate) full_line_display: bool,
    pub(crate) filter_panel_position: FilterPanelPosition,
    pub(crate) is_dragging_scrollbar: bool,
    pub(crate) is_dragging_secondary_scrollbar: bool,
    pub(crate) is_dragging_h_scrollbar: bool,
    pub(crate) is_dragging_secondary_h_scrollbar: bool,
    pub(crate) h_scrollbar_drag_grab_offset: Option<f32>,
    pub(crate) h_scrollbar_track_bounds: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    pub(crate) secondary_h_scrollbar_track_bounds: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    pub is_overflow_menu_open: bool,
    pub is_zen_mode: bool,

    // Line Detail Inspector state
    pub(crate) is_line_detail_open: bool,
    pub(crate) line_detail_height: f32,
    pub(crate) is_resizing_line_detail: bool,
    pub(crate) line_detail_drag_start_pos: Option<Point<Pixels>>,
    pub(crate) line_detail_drag_start_height: f32,
    pub(crate) line_detail_scroll_handle: ScrollHandle,
    pub(crate) is_line_detail_json_pretty: bool,
    pub(crate) line_detail_inspected_key: Option<(bool, usize)>,
    pub(crate) cached_line_detail: Option<(
        LineDetailCacheKey,
        crate::components::log_view::LineDetailFormatted,
    )>,

    // Command Palette state
    pub(crate) is_command_palette_open: bool,
    pub(crate) palette_input: Entity<TextInput>,
    pub(crate) palette_selected_index: usize,

    // Bookmark state
    pub(crate) bookmarks: BookmarkController,
    pub(crate) note_input: Entity<TextInput>,

    // Split View state
    pub(crate) split_view: SplitViewController,
    pub(crate) secondary_filter_cancel_gen: Arc<AtomicU64>,
    pub(crate) secondary_scroll_handle: UniformListScrollHandle,
    pub(crate) secondary_selection: SelectionModel,

    // Column View state
    pub(crate) is_column_view: bool,

    // Workstation & Facets state
    pub(crate) is_filter_dock_visible: bool,
    pub(crate) is_facets_visible: bool,
    pub(crate) facet_tags: Vec<(String, usize)>,
    pub(crate) has_auto_sampled_facets: bool,

    // Resizable Filter Dock state
    pub(crate) filter_dock_width: f32,
    pub(crate) filter_dock_height: f32,
    pub(crate) is_resizing_filter_dock: bool,
    pub(crate) filter_dock_drag_start_pos: Option<Point<Pixels>>,
    pub(crate) filter_dock_drag_start_size: f32,
}

/// Priority action to execute when Escape key is pressed in the application,
/// evaluated in strict reverse z-order:
/// 1. Floating ephemeral menus (Overflow menu -> ADB device selector)
/// 2. Modals and drawers (Filter modal -> Note modal -> Bookmark drawer -> Command palette)
/// 3. Collapsible bottom inspector (Line Detail)
/// 4. Active log row selection (Primary / Secondary)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeAction {
    CloseOverflowMenu,
    CloseDeviceSelector,
    CloseFilterModal,
    CloseNoteModal,
    CloseBookmarkDrawer,
    CloseCommandPalette,
    CloseLineDetail,
    ClearSelection,
    None,
}

/// Evaluates reverse z-order Escape dismissal priority across application UI elements
#[allow(clippy::too_many_arguments)]
pub fn determine_escape_action(
    is_overflow_menu_open: bool,
    is_device_selector_open: bool,
    is_filter_modal_open: bool,
    is_note_modal_open: bool,
    is_bookmark_drawer_open: bool,
    is_command_palette_open: bool,
    is_line_detail_open: bool,
    has_selection: bool,
) -> EscapeAction {
    if is_overflow_menu_open {
        EscapeAction::CloseOverflowMenu
    } else if is_device_selector_open {
        EscapeAction::CloseDeviceSelector
    } else if is_filter_modal_open {
        EscapeAction::CloseFilterModal
    } else if is_note_modal_open {
        EscapeAction::CloseNoteModal
    } else if is_bookmark_drawer_open {
        EscapeAction::CloseBookmarkDrawer
    } else if is_command_palette_open {
        EscapeAction::CloseCommandPalette
    } else if is_line_detail_open {
        EscapeAction::CloseLineDetail
    } else if has_selection {
        EscapeAction::ClearSelection
    } else {
        EscapeAction::None
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
                    if !this.handle_suggestion_key("enter", false, cx) {
                        this.save_modal_filter(cx);
                    }
                }
                TextInputEvent::Cancel => {
                    if !this.handle_suggestion_key("escape", false, cx) {
                        this.close_filter_modal(cx);
                    }
                }
                TextInputEvent::Change => this.on_modal_pattern_changed(cx),
            },
        )
        .detach();

        let find_input = cx.new(|cx| TextInput::new("Type search query & press Enter...", cx));

        cx.subscribe(
            &find_input,
            |this, input, event: &TextInputEvent, cx| match event {
                TextInputEvent::Submit => {
                    this.search_ctrl.query = input.read(cx).text().to_string();
                    this.update_cached_search_matcher();
                    this.search(true, cx);
                }
                TextInputEvent::Cancel => {
                    this.close_search_bar(cx);
                }
                TextInputEvent::Change => {
                    this.search_ctrl.query = input.read(cx).text().to_string();
                    if this.search_ctrl.query.trim().is_empty() {
                        this.search_ctrl.match_status = None;
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
                    this.clear_filter_drag_state_fields();
                    this.input_focus = InputFocusTarget::None;
                    cx.notify();
                }
                TextInputEvent::Change => {
                    this.filter_search_query = input.read(cx).text().to_string();
                    this.clear_filter_drag_state_fields();
                    cx.notify();
                }
            },
        )
        .detach();

        let palette_input =
            cx.new(|cx| TextInput::new("Type a command or search (Esc to close)...", cx));

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

        let mut split_view = SplitViewController::new();
        split_view.is_split_vertical = true;
        let (default_dock_width, default_dock_height) = default_filter_dock_size();

        let mut workspace = Self {
            focus_handle: cx.focus_handle(),
            log_source: None,
            primary_source_gen: 0,
            filter_result: None,
            heatmap_data: None,
            timeline_data: None,
            is_timeline_visible: WORKSPACE_DEFAULTS.is_timeline_visible,
            analytics_cancel_gen: Arc::new(AtomicU64::new(0)),
            scroll_handle: UniformListScrollHandle::new(),
            selection: SelectionModel::new(),
            theme: Theme::default(),
            input_focus: InputFocusTarget::None,

            filter_tabs: vec![default_tab],
            active_tab_index: 0,
            active_filter_profile_path: None,
            show_only_filtered: false,
            filter_cancel_gen: Arc::new(AtomicU64::new(0)),
            cached_stream_filters: None,
            filter_edit_modal: FilterEditState::default(),
            modal_pattern_input,
            filter_search_query: String::new(),
            filter_rule_input,
            filter_rules_scroll_handle: ScrollHandle::new(),
            filter_drag_drop_target: None,
            dragging_filter_rule_index: None,

            search_ctrl: SearchController::new(),
            find_input,

            adb: AdbController::new(),

            font_size: px(12.0),
            status_text: "Ready | Open a file or start ADB streaming".into(),
            active_file_name: None,
            recent_files: Vec::new(),
            recent_files_gen: 0,
            pattern_history: PatternHistory::default(),
            is_dark_theme: false,
            full_line_display: WORKSPACE_DEFAULTS.full_line_display,
            filter_panel_position: FilterPanelPosition::Bottom,
            is_dragging_scrollbar: false,
            is_dragging_secondary_scrollbar: false,
            is_dragging_h_scrollbar: false,
            is_dragging_secondary_h_scrollbar: false,
            h_scrollbar_drag_grab_offset: None,
            h_scrollbar_track_bounds: std::rc::Rc::new(std::cell::Cell::new(Bounds::default())),
            secondary_h_scrollbar_track_bounds: std::rc::Rc::new(std::cell::Cell::new(
                Bounds::default(),
            )),
            is_overflow_menu_open: false,
            is_zen_mode: WORKSPACE_DEFAULTS.is_zen_mode,
            is_line_detail_open: false,
            line_detail_height: DEFAULT_LINE_DETAIL_HEIGHT,
            is_resizing_line_detail: false,
            line_detail_drag_start_pos: None,
            line_detail_drag_start_height: DEFAULT_LINE_DETAIL_HEIGHT,
            line_detail_scroll_handle: ScrollHandle::new(),
            is_line_detail_json_pretty: true,
            line_detail_inspected_key: None,
            cached_line_detail: None,
            is_command_palette_open: false,
            palette_input,
            palette_selected_index: 0,
            bookmarks: BookmarkController::new(),
            note_input,

            split_view,
            secondary_filter_cancel_gen: Arc::new(AtomicU64::new(0)),
            secondary_scroll_handle: UniformListScrollHandle::new(),
            secondary_selection: SelectionModel::new(),

            is_column_view: false,
            is_filter_dock_visible: true,
            is_facets_visible: true,
            facet_tags: Vec::new(),
            has_auto_sampled_facets: false,
            filter_dock_width: default_dock_width,
            filter_dock_height: default_dock_height,
            is_resizing_filter_dock: false,
            filter_dock_drag_start_pos: None,
            filter_dock_drag_start_size: 0.0,
        };
        workspace.load_recent_files_async(cx);
        workspace.load_pattern_history_async(cx);
        workspace
    }

    /// Returns a snapshot of ADB controller state.
    pub fn adb_state(&self) -> AdbStreamState {
        self.adb.state()
    }

    /// Returns a snapshot of bookmark drawer & note modal state.
    pub fn bookmark_drawer_state(&self) -> BookmarkDrawerState {
        self.bookmarks.state()
    }

    /// Returns a snapshot of search bar controller state.
    pub fn search_state(&self) -> SearchState {
        self.search_ctrl.state()
    }

    /// Returns a snapshot of dual-pane split-view state.
    pub fn split_view_state(&self) -> SplitViewState {
        self.split_view.state()
    }

    /// Collects all active filter rules across enabled filter tabs.
    pub(crate) fn collect_active_flat_filters(&self) -> Vec<FilterRule> {
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
        active_filters
    }

    /// Rebuilds and caches pre-compiled stream filters from active rules.
    pub(crate) fn rebuild_cached_stream_filters(&mut self) -> Arc<CompiledStreamFilters> {
        let active_filters = self.collect_active_flat_filters();
        let compiled = Arc::new(CompiledStreamFilters::new(&active_filters));
        self.cached_stream_filters = Some(Arc::clone(&compiled));
        compiled
    }

    /// Returns the cached compiled stream filters, building them on first demand.
    pub(crate) fn ensure_cached_stream_filters(&mut self) -> Arc<CompiledStreamFilters> {
        if let Some(existing) = &self.cached_stream_filters {
            return Arc::clone(existing);
        }
        self.rebuild_cached_stream_filters()
    }

    /// Prompts user to select a log file using native OS file dialog attached to window
    pub fn prompt_open_file(&mut self, parent: Option<&Window>, cx: &mut Context<Self>) {
        let base_dialog = crate::dialogs::build_open_log_dialog_base();
        let dialog = crate::dialogs::prepare_dialog_with_parent(base_dialog, parent);

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();
            async move {
                let dialog = crate::dialogs::resolve_dialog_for_display(dialog, &bg_executor).await;

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

    /// Advances and returns the monotonic primary source generation token.
    pub(crate) fn advance_primary_source_gen(&mut self) -> u64 {
        next_source_generation(&mut self.primary_source_gen)
    }

    /// Stops any live ADB stream (`[INV-06]`), advances `primary_source_gen`, clears primary source
    /// metadata, and synchronously resets derived analytics/search state before spawning an async open.
    pub(crate) fn begin_primary_source_transition(&mut self, cx: &mut Context<Self>) -> u64 {
        if self.adb.is_streaming {
            self.stop_adb_stream(cx);
        }
        let gen_id = self.advance_primary_source_gen();
        self.log_source = None;
        self.active_file_name = None;
        self.reset_derived_log_state();
        gen_id
    }

    /// Loads a log file using FileLogBuffer asynchronously on the background executor and executes filtering
    pub fn open_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let open_gen = self.begin_primary_source_transition(cx);
        let path_buf = path.to_path_buf();
        let file_name: SharedString = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
            .into();

        self.status_text = format!("Opening log file {:?}...", file_name).into();
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
                    if !is_source_generation_current(workspace.primary_source_gen, open_gen) {
                        return;
                    }
                    match open_result {
                        Ok(buf) => {
                            let arc_buf = Arc::new(ActiveLogSource::File(buf));
                            workspace.log_source = Some(arc_buf.clone());
                            workspace.active_file_name = Some(file_name);
                            workspace.add_recent_file_async(path_buf.clone(), cx);
                            workspace.refresh_facet_tags_async(cx);
                            workspace.status_text = format!(
                                "Loaded {} lines from {:?}",
                                arc_buf.line_count(),
                                path_buf
                            )
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
        if self.split_view.is_split_view && self.split_view.secondary_log_source.is_some() {
            self.reapply_secondary_filters(cx);
        }

        let compiled = self.rebuild_cached_stream_filters();

        let Some(source) = self.log_source.clone() else {
            return;
        };

        let active_filters = compiled.rules().to_vec();
        let generation = self.filter_cancel_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let token = self.filter_cancel_gen.clone();
        let show_only = self.show_only_filtered;

        let snapshot_len = source.line_count();
        let snapshot_trimmed = source.total_trimmed();
        let compiled_for_ui = Arc::clone(&compiled);

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

                        let mut final_res = res;
                        let Some(src) = &workspace.log_source else {
                            return;
                        };
                        let caught_up = reconcile_snapshot_filter_result(
                            &mut final_res,
                            src,
                            snapshot_len,
                            snapshot_trimmed,
                            &compiled_for_ui,
                            show_only,
                        );

                        workspace
                            .analytics_cancel_gen
                            .fetch_add(1, Ordering::SeqCst);
                        workspace.filter_result = Some(Arc::new(final_res));
                        workspace.heatmap_data = Some(Arc::new(heatmap));
                        workspace.timeline_data = Some(Arc::new(timeline));
                        if caught_up {
                            workspace.update_analytics_data_async(cx);
                        }

                        // Viewport anchor preservation
                        preserve_selection_after_filter(
                            &mut workspace.selection,
                            workspace.filter_result.as_deref(),
                            &workspace.scroll_handle,
                        );
                        workspace.reset_line_detail_scroll_if_line_changed();

                        let visible_count = workspace
                            .filter_result
                            .as_ref()
                            .map(|r| r.visible_indices.len())
                            .unwrap_or(0);
                        let total_count = workspace
                            .log_source
                            .as_ref()
                            .map(|s| s.line_count())
                            .unwrap_or_else(|| source_for_ui.line_count());
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
    /// Handles Escape key dismissal using reverse z-order priority.
    /// Returns `true` if the Escape key was consumed.
    pub fn handle_escape(&mut self, cx: &mut Context<Self>) -> bool {
        // An open suggestion dropdown consumes Escape before the modal closes.
        if self.handle_suggestion_key("escape", false, cx) {
            return true;
        }
        let has_selection = self.selection.selected_row.is_some()
            || (self.split_view.is_split_view && self.secondary_selection.selected_row.is_some());
        match determine_escape_action(
            self.is_overflow_menu_open,
            self.adb.is_device_selector_open,
            self.filter_edit_modal.is_open,
            self.bookmarks.is_note_modal_open,
            self.bookmarks.is_drawer_open,
            self.is_command_palette_open,
            self.is_line_detail_open,
            has_selection,
        ) {
            EscapeAction::CloseOverflowMenu => {
                self.close_overflow_menu(cx);
                true
            }
            EscapeAction::CloseDeviceSelector => {
                self.close_device_selector(cx);
                true
            }
            EscapeAction::CloseFilterModal => {
                self.close_filter_modal(cx);
                true
            }
            EscapeAction::CloseNoteModal => {
                self.close_note_modal(cx);
                true
            }
            EscapeAction::CloseBookmarkDrawer => {
                self.close_bookmark_drawer(cx);
                true
            }
            EscapeAction::CloseCommandPalette => {
                self.close_command_palette(cx);
                true
            }
            EscapeAction::CloseLineDetail => {
                self.close_line_detail(cx);
                true
            }
            EscapeAction::ClearSelection => {
                self.clear_selection(cx);
                true
            }
            EscapeAction::None => {
                if self.is_zen_mode {
                    self.toggle_zen_mode(cx);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Returns `true` when any workspace `TextInput` currently holds window focus.
    pub(crate) fn has_any_text_input_focus(&self, window: &Window, cx: &App) -> bool {
        self.modal_pattern_input
            .read(cx)
            .focus_handle
            .is_focused(window)
            || self.note_input.read(cx).focus_handle.is_focused(window)
            || self.find_input.read(cx).focus_handle.is_focused(window)
            || self.palette_input.read(cx).focus_handle.is_focused(window)
            || self
                .filter_rule_input
                .read(cx)
                .focus_handle
                .is_focused(window)
    }

    /// Handles keyboard events across application components
    pub fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();

        // Handle Escape key dismissal using reverse z-order priority
        if key == "escape" && self.handle_escape(cx) {
            return;
        }

        // Suggestion dropdown gets first claim on navigation/selection keys.
        if self.filter_edit_modal.is_open
            && self.handle_suggestion_key(key, event.keystroke.modifiers.shift, cx)
        {
            return;
        }

        // If the filter modal is open, handle Enter fallback when focus is on non-input elements
        if self.filter_edit_modal.is_open && key == "enter" {
            self.save_modal_filter(cx);
            return;
        }

        // If the note modal is open, handle Enter
        if self.bookmarks.is_note_modal_open && key == "enter" {
            self.save_bookmark_note(cx);
            return;
        }

        // If the bookmark drawer is open, handle navigation keys
        if self.bookmarks.is_drawer_open {
            if key == "up" {
                if self.bookmarks.drawer_selected_index > 0 {
                    self.bookmarks.drawer_selected_index -= 1;
                    cx.notify();
                }
                return;
            } else if key == "down" {
                if self.bookmarks.drawer_selected_index + 1 < self.bookmarks.len() {
                    self.bookmarks.drawer_selected_index += 1;
                    cx.notify();
                }
                return;
            } else if key == "enter" {
                if let Some(b) = self
                    .bookmarks
                    .items()
                    .get(self.bookmarks.drawer_selected_index)
                {
                    let src_idx = b.source_idx;
                    self.jump_to_bookmark_source_idx(src_idx, cx);
                    self.close_bookmark_drawer(cx);
                }
                return;
            } else if key == "delete" || key == "backspace" {
                if let Some(b) = self
                    .bookmarks
                    .items()
                    .get(self.bookmarks.drawer_selected_index)
                {
                    let src_idx = b.source_idx;
                    let line_num = b.line_number;
                    self.bookmarks.remove(src_idx);
                    self.bookmarks.drawer_selected_index =
                        crate::modals::bookmark_drawer::clamp_drawer_selected_index(
                            self.bookmarks.drawer_selected_index,
                            self.bookmarks.len(),
                        );
                    self.status_text = format!("Removed bookmark on line {line_num}").into();
                    cx.notify();
                }
                return;
            }
        }

        // Navigate log lines with Up/Down arrows and scroll horizontally with Left/Right/Home/End
        if self.input_focus == InputFocusTarget::None
            && !self.has_any_text_input_focus(window, cx)
            && !self.filter_edit_modal.is_open
            && !self.bookmarks.is_note_modal_open
            && !self.bookmarks.is_drawer_open
            && !self.is_command_palette_open
        {
            let is_sec = self.split_view.is_split_view && self.split_view.active_pane_is_secondary;
            if key == "up" {
                if is_sec {
                    if let Some(res) = &self.split_view.secondary_filter_result {
                        let total_rows = res.visible_indices.len();
                        if total_rows > 0 {
                            let cur_row = self.secondary_selection.selected_row.unwrap_or(0);
                            let new_row = cur_row.saturating_sub(1);
                            if let Some(&src_idx) = res.visible_indices.get(new_row) {
                                self.select_secondary_log_row(new_row, src_idx, cx);
                                self.secondary_scroll_handle
                                    .scroll_to_item_strict(new_row, ScrollStrategy::Center);
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
                            self.scroll_handle
                                .scroll_to_item_strict(new_row, ScrollStrategy::Center);
                        }
                    }
                }
            } else if key == "down" {
                if is_sec {
                    if let Some(res) = &self.split_view.secondary_filter_result {
                        let total_rows = res.visible_indices.len();
                        if total_rows > 0 {
                            let cur_row = self
                                .secondary_selection
                                .selected_row
                                .map(|r| r + 1)
                                .unwrap_or(0);
                            let new_row = cur_row.min(total_rows.saturating_sub(1));
                            if let Some(&src_idx) = res.visible_indices.get(new_row) {
                                self.select_secondary_log_row(new_row, src_idx, cx);
                                self.secondary_scroll_handle
                                    .scroll_to_item_strict(new_row, ScrollStrategy::Center);
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
                            self.scroll_handle
                                .scroll_to_item_strict(new_row, ScrollStrategy::Center);
                        }
                    }
                }
            } else if self.full_line_display
                && !event.keystroke.modifiers.control
                && !event.keystroke.modifiers.platform
                && !event.keystroke.modifiers.alt
                && matches!(key, "left" | "right" | "home" | "end")
            {
                let handle = if is_sec {
                    &self.secondary_scroll_handle
                } else {
                    &self.scroll_handle
                };
                let step = if event.keystroke.modifiers.shift {
                    H_SCROLL_FAST_STEP_PX
                } else {
                    H_SCROLL_STEP_PX
                };
                let state = handle.0.borrow();
                let max_x: f32 = state.base_handle.max_offset().width.into();
                let cur = state.base_handle.offset();
                let cur_x: f32 = cur.x.into();
                let next_x = match key {
                    "left" => {
                        crate::components::log_view::clamp_horizontal_scroll_x(cur_x + step, max_x)
                    }
                    "right" => {
                        crate::components::log_view::clamp_horizontal_scroll_x(cur_x - step, max_x)
                    }
                    "home" => 0.0,
                    "end" => crate::components::log_view::clamp_horizontal_scroll_x(-max_x, max_x),
                    _ => cur_x,
                };
                let changed = (next_x - cur_x).abs() > 0.01;
                if changed {
                    state.base_handle.set_offset(point(px(next_x), cur.y));
                }
                drop(state);
                if changed {
                    cx.notify();
                }
            }
        }
    }

    /// Refreshes the status line text based on current selection or system state
    pub fn refresh_status_line(&mut self, cx: &mut Context<Self>) {
        if let Some(source_idx) = self.selection.anchor_source_idx {
            if self.log_source.is_some() {
                let compiled = self.ensure_cached_stream_filters();
                if let Some(src) = &self.log_source {
                    if src.get_raw_line(source_idx).is_some() {
                        self.status_text = resolve_status_line_for_selection(
                            source_idx,
                            self.selection.selected_row,
                            src,
                            self.filter_result.as_deref(),
                            &compiled,
                        )
                        .into();
                        cx.notify();
                        return;
                    }
                }
            }
        }

        if let Some(fname) = &self.active_file_name {
            self.status_text = format!("Loaded {}", fname).into();
        } else if self.adb.is_streaming {
            if self.adb.is_paused {
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
        self.begin_primary_source_transition(cx);
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

    /// Resets font size back to default 12px (Ctrl+0 / Cmd+0)
    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.font_size = px(12.0);
        self.status_text = "Font size reset to default (12px)".into();
        cx.notify();
    }

    /// Scrolls the log viewport based on mouse vertical position in the scrollbar track
    pub fn scroll_to_ratio_from_mouse_y(&mut self, mouse_y: Pixels, cx: &mut Context<Self>) {
        let row_count = self
            .filter_result
            .as_ref()
            .map(|r| r.visible_indices.len())
            .unwrap_or(0);

        if scroll_handle_to_mouse_y(&self.scroll_handle, row_count, self.font_size, mouse_y) {
            cx.notify();
        }
    }

    /// Scrolls the secondary log viewport based on mouse vertical position in the scrollbar track
    pub fn scroll_secondary_to_ratio_from_mouse_y(
        &mut self,
        mouse_y: Pixels,
        cx: &mut Context<Self>,
    ) {
        let row_count = self
            .split_view
            .secondary_filter_result
            .as_ref()
            .map(|r| r.visible_indices.len())
            .unwrap_or(0);

        if scroll_handle_to_mouse_y(
            &self.secondary_scroll_handle,
            row_count,
            self.font_size,
            mouse_y,
        ) {
            cx.notify();
        }
    }

    /// Scrolls the primary or secondary log viewport horizontally based on pointer X coordinate.
    pub(crate) fn scroll_h_to_mouse_x(
        &self,
        is_primary: bool,
        mouse_x: Pixels,
        cx: &mut Context<Self>,
    ) {
        if !self.full_line_display {
            return;
        }
        let (handle, track_cell) = if is_primary {
            (&self.scroll_handle, &self.h_scrollbar_track_bounds)
        } else {
            (
                &self.secondary_scroll_handle,
                &self.secondary_h_scrollbar_track_bounds,
            )
        };

        let snapshot = crate::components::log_view::read_h_scroll_render_snapshot(handle, true);
        let Some(metrics) = crate::components::log_view::compute_horizontal_scrollbar_metrics(
            snapshot.viewport_width,
            snapshot.content_width,
            snapshot.clamped_scroll_x,
        ) else {
            return;
        };

        let captured_bounds = track_cell.get();
        let state = handle.0.borrow();
        let effective_bounds = if captured_bounds.size.width > px(0.0) {
            captured_bounds
        } else {
            state.base_handle.bounds()
        };
        let track_left: f32 = effective_bounds.origin.x.into();
        let track_width: f32 = effective_bounds.size.width.into();
        if track_width <= 0.0 {
            drop(state);
            return;
        }

        let next_x = crate::components::log_view::scroll_x_from_mouse_x(
            mouse_x.into(),
            track_left,
            track_width,
            metrics.thumb_width,
            self.h_scrollbar_drag_grab_offset,
            metrics.max_scroll_x,
        );
        let cur = state.base_handle.offset();
        let cur_x: f32 = cur.x.into();
        let changed = (next_x - cur_x).abs() > 0.01;
        if changed {
            state.base_handle.set_offset(point(px(next_x), cur.y));
        }
        drop(state);
        if changed {
            cx.notify();
        }
    }

    /// Clears all active vertical/horizontal scrollbar drag flags, returning `true` if any changed.
    pub(crate) fn clear_scrollbar_drag_flags(&mut self) -> bool {
        let had_drag = self.is_dragging_scrollbar
            || self.is_dragging_secondary_scrollbar
            || self.is_dragging_h_scrollbar
            || self.is_dragging_secondary_h_scrollbar
            || self.h_scrollbar_drag_grab_offset.is_some();
        if had_drag {
            self.is_dragging_scrollbar = false;
            self.is_dragging_secondary_scrollbar = false;
            self.is_dragging_h_scrollbar = false;
            self.is_dragging_secondary_h_scrollbar = false;
            self.h_scrollbar_drag_grab_offset = None;
        }
        had_drag
    }

    /// Toggles between Light and Dark theme (Ctrl+U / F10)
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
        if !self.full_line_display {
            reset_horizontal_scroll_offset(&self.scroll_handle);
            reset_horizontal_scroll_offset(&self.secondary_scroll_handle);
            self.is_dragging_h_scrollbar = false;
            self.is_dragging_secondary_h_scrollbar = false;
            self.h_scrollbar_drag_grab_offset = None;
        }
        self.status_text = if self.full_line_display {
            "Full Line Display: ON (Horizontal scroll enabled)".into()
        } else {
            "Full Line Display: OFF (Truncated)".into()
        };
        cx.notify();
    }

    /// Opens the collapsible Line Detail inspector panel (idempotent).
    pub fn open_line_detail(&mut self, cx: &mut Context<Self>) {
        if !self.is_line_detail_open {
            self.is_line_detail_open = true;
            self.reset_line_detail_scroll_if_line_changed();
            cx.notify();
        }
    }

    /// Closes the collapsible Line Detail inspector panel while preserving selection.
    pub fn close_line_detail(&mut self, cx: &mut Context<Self>) {
        if self.is_line_detail_open || self.is_resizing_line_detail {
            self.is_line_detail_open = false;
            self.is_resizing_line_detail = false;
            self.line_detail_drag_start_pos = None;
            cx.notify();
        }
    }

    /// Toggles the collapsible Line Detail inspector panel (`Ctrl+Shift+D`).
    pub fn toggle_line_detail(&mut self, cx: &mut Context<Self>) {
        if self.is_line_detail_open {
            self.close_line_detail(cx);
            self.status_text = "Line Detail Inspector: Closed".into();
        } else {
            self.is_line_detail_open = true;
            self.reset_line_detail_scroll_if_line_changed();
            self.status_text = "Line Detail Inspector: Open".into();
        }
        cx.notify();
    }

    /// Toggles JSON pretty-printing inside the Line Detail inspector and resets scroll to top.
    pub fn toggle_line_detail_json_pretty(&mut self, cx: &mut Context<Self>) {
        self.is_line_detail_json_pretty = !self.is_line_detail_json_pretty;
        self.cached_line_detail = None;
        self.line_detail_scroll_handle.set_offset(Point::default());
        self.ensure_line_detail_cache();
        cx.notify();
    }

    /// Starts interactive height resizing of the Line Detail panel from mouse position.
    pub fn start_resizing_line_detail(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        self.is_resizing_line_detail = true;
        self.line_detail_drag_start_pos = Some(pos);
        self.line_detail_drag_start_height = self.line_detail_height;
        cx.notify();
    }

    /// Updates Line Detail panel height during vertical splitter mouse drag.
    pub fn update_resizing_line_detail(
        &mut self,
        current_pos: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !self.is_resizing_line_detail {
            return;
        }
        let Some(start_pos) = self.line_detail_drag_start_pos else {
            return;
        };
        let start_y: f32 = start_pos.y.into();
        let current_y: f32 = current_pos.y.into();
        let delta = start_y - current_y;
        let new_height = clamp_line_detail_height(self.line_detail_drag_start_height, delta);
        if (self.line_detail_height - new_height).abs() > 0.5 {
            self.line_detail_height = new_height;
            cx.notify();
        }
    }

    /// Finishes interactive height resizing of the Line Detail panel.
    pub fn stop_resizing_line_detail(&mut self, cx: &mut Context<Self>) {
        if self.is_resizing_line_detail {
            self.is_resizing_line_detail = false;
            self.line_detail_drag_start_pos = None;
            cx.notify();
        }
    }

    /// Resets the Line Detail panel height to the default `160.0px`.
    pub fn reset_line_detail_height(&mut self, cx: &mut Context<Self>) {
        self.line_detail_height = DEFAULT_LINE_DETAIL_HEIGHT;
        cx.notify();
    }

    /// Resets the Line Detail scroll position and cache when the inspected physical line changes.
    pub(crate) fn reset_line_detail_scroll_if_line_changed(&mut self) {
        let next_key = self
            .resolve_active_selected_line()
            .map(|sel| (sel.is_secondary_pane, sel.abs_source_idx));
        if self.line_detail_inspected_key != next_key {
            self.line_detail_inspected_key = next_key;
            self.cached_line_detail = None;
            self.line_detail_scroll_handle.set_offset(Point::default());
        }
    }

    /// Ensures the cached formatted Line Detail payload matches the currently selected line and JSON mode.
    pub(crate) fn ensure_line_detail_cache(&mut self) {
        let Some(active) = self.resolve_active_selected_line() else {
            self.cached_line_detail = None;
            return;
        };
        let key = LineDetailCacheKey {
            is_secondary: active.is_secondary_pane,
            abs_source_idx: active.abs_source_idx,
            pretty_json: self.is_line_detail_json_pretty,
            line_len: active.line_text.len(),
        };
        if self
            .cached_line_detail
            .as_ref()
            .is_some_and(|(cached_key, _)| *cached_key == key)
        {
            return;
        }
        let formatted = crate::components::log_view::format_line_detail_text(
            active.line_text.as_ref(),
            self.is_line_detail_json_pretty,
            crate::components::log_view::LINE_DETAIL_RENDER_MAX_BYTES,
        );
        self.cached_line_detail = Some((key, formatted));
    }

    /// Toggles smart auto-scroll for streaming
    pub fn toggle_auto_scroll(&mut self, cx: &mut Context<Self>) {
        self.adb.auto_scroll = !self.adb.auto_scroll;
        self.status_text = if self.adb.auto_scroll {
            "Auto-scroll: ON".into()
        } else {
            "Auto-scroll: OFF".into()
        };
        cx.notify();
    }

    /// Toggles filter panel dock position between Left and Bottom
    pub fn toggle_filter_panel_position(&mut self, cx: &mut Context<Self>) {
        self.clear_filter_drag_state_fields();
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

    /// Toggles the floating overflow menu
    pub fn toggle_overflow_menu(&mut self, cx: &mut Context<Self>) {
        self.is_overflow_menu_open = !self.is_overflow_menu_open;
        if self.is_overflow_menu_open {
            self.adb.is_device_selector_open = false;
        }
        cx.notify();
    }

    /// Closes the floating overflow menu if open
    pub fn close_overflow_menu(&mut self, cx: &mut Context<Self>) {
        if self.is_overflow_menu_open {
            self.is_overflow_menu_open = false;
            cx.notify();
        }
    }

    /// Returns human-readable buffer memory footprint and storage mechanism
    pub fn get_buffer_memory_display(&self) -> (String, &'static str) {
        format_source_memory_display(self.log_source.as_deref())
    }
    /// Clears derived analytics, selection, bookmarks, facets, and cancels in-flight background work when switching primary log sources.
    pub(crate) fn reset_derived_log_state(&mut self) {
        reset_derived_controllers_and_tokens(
            &self.filter_cancel_gen,
            &self.analytics_cancel_gen,
            &mut self.search_ctrl,
            &mut self.selection,
            &mut self.bookmarks,
            &mut self.adb,
            &mut self.has_auto_sampled_facets,
        );
        self.filter_result = None;
        self.heatmap_data = None;
        self.timeline_data = None;
        self.facet_tags.clear();
        self.scroll_handle
            .0
            .borrow()
            .base_handle
            .set_offset(Point::default());
        self.is_dragging_scrollbar = false;
        self.is_dragging_h_scrollbar = false;
        self.h_scrollbar_drag_grab_offset = None;
        self.line_detail_inspected_key = None;
        self.cached_line_detail = None;
        self.line_detail_scroll_handle.set_offset(Point::default());
    }

    /// Asynchronously recomputes heatmap density and EPS timeline data off the UI thread.
    pub(crate) fn update_analytics_data_async(&mut self, cx: &mut Context<Self>) {
        self.spawn_and_apply_stream_analytics(cx);
    }

    /// Toggles the Throughput & EPS Timeline Sparkline ribbon (Ctrl+Shift+T)
    pub fn toggle_eps_timeline(&mut self, cx: &mut Context<Self>) {
        self.is_timeline_visible = !self.is_timeline_visible;
        if self.is_timeline_visible && self.timeline_data.is_none() {
            if let (Some(src_ref), Some(filter_res)) =
                (self.log_source.clone(), self.filter_result.clone())
            {
                let filter_gen = self.filter_cancel_gen.load(Ordering::Relaxed);
                let src_check = src_ref.clone();
                let filter_check = filter_res.clone();

                cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                    let bg_executor = cx.background_executor().clone();
                    let mut async_cx = cx.clone();

                    async move {
                        let timeline = bg_executor
                            .spawn(async move {
                                Arc::new(EpsTimelineData::compute(
                                    &*src_ref,
                                    &filter_res.visible_indices,
                                    &filter_res.visible_styles,
                                    80,
                                ))
                            })
                            .await;

                        let _ = this.update(&mut async_cx, |workspace, cx| {
                            if workspace.is_timeline_visible
                                && workspace.filter_cancel_gen.load(Ordering::Relaxed) == filter_gen
                                && workspace
                                    .log_source
                                    .as_ref()
                                    .is_some_and(|s| Arc::ptr_eq(s, &src_check))
                                && workspace
                                    .filter_result
                                    .as_ref()
                                    .is_some_and(|f| Arc::ptr_eq(f, &filter_check))
                            {
                                workspace.timeline_data = Some(timeline);
                                cx.notify();
                            }
                        });
                    }
                })
                .detach();
            }
        }
        self.status_text = if self.is_timeline_visible {
            "Throughput Timeline Ribbon: ON".into()
        } else {
            "Throughput Timeline Ribbon: OFF".into()
        };
        cx.notify();
    }

    /// Alias for toggle_eps_timeline (Ctrl+Shift+T)
    pub fn toggle_timeline(&mut self, cx: &mut Context<Self>) {
        self.toggle_eps_timeline(cx);
    }

    /// Toggles Zen Focus Mode (F11 / Ctrl+Shift+Z / Cmd+Shift+Z)
    pub fn toggle_zen_mode(&mut self, cx: &mut Context<Self>) {
        self.clear_filter_drag_state_fields();
        self.is_zen_mode = !self.is_zen_mode;
        self.status_text = if self.is_zen_mode {
            "Zen Focus Mode: ON (F11 to exit)".into()
        } else {
            "Zen Focus Mode: OFF".into()
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

/// Formats a byte count uniformly into human-readable `KB`, `MB`, or `GB`.
pub fn format_byte_size(bytes: usize) -> String {
    const MB: usize = 1024 * 1024;
    const GB: usize = 1024 * 1024 * 1024;
    if bytes < MB {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    }
}

/// Formats human-readable buffer memory footprint and storage backend label for an optional [`ActiveLogSource`].
pub fn format_source_memory_display(source: Option<&ActiveLogSource>) -> (String, &'static str) {
    match source {
        Some(ActiveLogSource::File(fb)) => (format_byte_size(fb.byte_size()), "MMap"),
        Some(ActiveLogSource::Stream(sb)) => (format_byte_size(sb.line_count() * 128), "Heap"),
        Some(ActiveLogSource::Merged(mb)) => (format_byte_size(mb.line_count() * 128), "Merged"),
        None => ("0 MB".to_string(), "Idle"),
    }
}

fn scroll_handle_to_mouse_y(
    handle: &UniformListScrollHandle,
    row_count: usize,
    font_size: Pixels,
    mouse_y: Pixels,
) -> bool {
    if row_count == 0 {
        return false;
    }

    let bounds = handle.0.borrow().base_handle.bounds();
    let track_top: f32 = bounds.origin.y.into();
    let track_height: f32 = bounds.size.height.into();
    let mouse_y_f32: f32 = mouse_y.into();

    if track_height <= 0.0 {
        return false;
    }

    let relative_y = (mouse_y_f32 - track_top).clamp(0.0, track_height);
    let ratio = (relative_y / track_height).clamp(0.0, 1.0);
    let font_size_f32: f32 = font_size.into();
    let row_height_f32 = crate::components::log_view::log_row_height_px(font_size_f32);
    let visible_items = (track_height / row_height_f32).floor() as usize;
    let max_top_idx = row_count.saturating_sub(visible_items.max(1));
    let target_top_idx = ((ratio * max_top_idx as f32).round() as usize).min(max_top_idx);

    handle.scroll_to_item_strict(target_top_idx, ScrollStrategy::Top);
    true
}

/// Resets horizontal scroll offset (`offset.x = 0.0`) while preserving vertical scroll offset.
pub(crate) fn reset_horizontal_scroll_offset(handle: &UniformListScrollHandle) {
    let state = handle.0.borrow();
    let cur = state.base_handle.offset();
    if !cur.x.is_zero() {
        state.base_handle.set_offset(point(px(0.0), cur.y));
    }
    drop(state);
}

/// Preserves or clears `selection` using nearest-anchor lookup after a background filter pass completes.
pub(crate) fn preserve_selection_after_filter(
    selection: &mut SelectionModel,
    filter_result: Option<&FilterResult>,
    scroll_handle: &UniformListScrollHandle,
) {
    let Some(anchor) = selection.anchor_source_idx else {
        return;
    };
    let Some(res) = filter_result else {
        selection.clear();
        return;
    };
    if let Some(new_row) = find_nearest_anchor_row(&res.visible_indices, anchor) {
        selection.selected_row = Some(new_row);
        scroll_handle.scroll_to_item(new_row, ScrollStrategy::Center);
    } else {
        selection.clear();
    }
}

/// Advances and returns a monotonic async load generation token.
#[inline]
pub(crate) fn next_source_generation(slot: &mut u64) -> u64 {
    *slot = slot.wrapping_add(1);
    *slot
}

/// Returns `true` iff an async load completion's generation still matches the active slot.
#[inline]
pub(crate) fn is_source_generation_current(current: u64, expected: u64) -> bool {
    current == expected
}

/// Returns `true` when an ADB stream should perform its one-shot initial facet tag scan.
#[inline]
pub(crate) fn should_auto_sample_facets(
    has_auto_sampled_facets: bool,
    facet_tags_empty: bool,
    line_count: usize,
) -> bool {
    !has_auto_sampled_facets && facet_tags_empty && line_count >= 100
}

/// Resets derived controllers, selection, bookmarks, facet scan flag, and cancellation tokens
/// when switching primary log sources.
pub(crate) fn reset_derived_controllers_and_tokens(
    filter_cancel_gen: &AtomicU64,
    analytics_cancel_gen: &AtomicU64,
    search_ctrl: &mut SearchController,
    selection: &mut SelectionModel,
    bookmarks: &mut BookmarkController,
    adb: &mut AdbController,
    has_auto_sampled_facets: &mut bool,
) {
    filter_cancel_gen.fetch_add(1, Ordering::Relaxed);
    search_ctrl.abort_ongoing_searches();
    analytics_cancel_gen.fetch_add(1, Ordering::SeqCst);
    selection.clear();
    search_ctrl.match_status = None;
    bookmarks.clear();
    bookmarks.close_drawer();
    bookmarks.close_note_modal();
    *has_auto_sampled_facets = false;
    adb.is_paused = false;
    adb.clear_paused_buffer();
}

/// Resolves the status bar description for the currently selected line using precompiled filters
/// and last-match rule precedence without compiling matchers on the UI thread.
pub(crate) fn resolve_status_line_for_selection(
    source_idx: usize,
    selected_row: Option<usize>,
    source: &ActiveLogSource,
    filter_result: Option<&FilterResult>,
    compiled: &CompiledStreamFilters,
) -> String {
    let last_flat_idx = if let (Some(sel_row), Some(filter_res)) = (selected_row, filter_result) {
        if filter_res.visible_indices.get(sel_row) == Some(&source_idx) {
            filter_res
                .visible_filter_matches
                .get(sel_row)
                .and_then(|matches| matches.last().copied())
        } else {
            resolve_last_flat_match(source, source_idx, compiled)
        }
    } else {
        resolve_last_flat_match(source, source_idx, compiled)
    };

    if let Some(rule) = last_flat_idx.and_then(|idx| compiled.rules().get(idx)) {
        let count = filter_result
            .map(|r| r.count_for_rule(&rule.id))
            .unwrap_or(0);
        format!(
            "Line {} | Matched Rule: '{}' ({}) | {} matches in tab",
            source_idx + 1,
            rule.text,
            rule.bg_color,
            count
        )
    } else {
        format!("Line {} | No filter rule match", source_idx + 1)
    }
}

fn resolve_last_flat_match(
    source: &ActiveLogSource,
    source_idx: usize,
    compiled: &CompiledStreamFilters,
) -> Option<usize> {
    let raw_line = source.get_raw_line(source_idx)?;
    let mut matched_indices = smallvec::SmallVec::<[usize; 8]>::new();
    compiled
        .matcher()
        .find_matches(raw_line, compiled.prepared(), &mut matched_indices);
    matched_indices
        .last()
        .map(|&p_idx| compiled.prepared()[p_idx].flat_index)
}

/// Reconciles a background-filtered snapshot result against the live `ActiveLogSource`
/// so lines appended or ring-buffer-trimmed while background filtering ran are reflected accurately.
fn reconcile_snapshot_filter_result(
    final_res: &mut FilterResult,
    live_src: &ActiveLogSource,
    snapshot_len: usize,
    snapshot_trimmed: usize,
    compiled: &CompiledStreamFilters,
    show_only: bool,
) -> bool {
    let current_len = live_src.line_count();
    let delta_trimmed = live_src.head_trimmed_since(snapshot_trimmed);
    let catchup_start = snapshot_len.saturating_sub(delta_trimmed);
    if current_len > catchup_start || delta_trimmed > 0 {
        compiled.append_streaming_batch(
            final_res,
            live_src,
            catchup_start..current_len,
            delta_trimmed,
            show_only,
        );
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::reconcile_snapshot_filter_result;
    use loganalysis_core::{
        execute_filtering, ActiveLogSource, CompiledStreamFilters, FilterRule, LogSource,
        StreamLogBuffer,
    };

    #[test]
    fn test_reconcile_snapshot_filter_result_with_append_and_trim() {
        let mut stream = StreamLogBuffer::new(5, 2);
        stream.append_chunk(vec![
            "01-01 00:00:00.000 1 1 E Tag: snap_0".to_string(),
            "01-01 00:00:01.000 1 1 I Tag: snap_1".to_string(),
            "01-01 00:00:02.000 1 1 E Tag: snap_2".to_string(),
            "01-01 00:00:03.000 1 1 I Tag: snap_3".to_string(),
        ]);
        assert_eq!(stream.total_trimmed(), 0);

        let filters = vec![FilterRule {
            text: "E Tag:".to_string(),
            regex: false,
            case_sensitive: true,
            exclude: false,
            active: true,
            ..Default::default()
        }];
        let compiled = CompiledStreamFilters::new(&filters);

        // Snapshot before streaming mutations
        let snapshot_len = stream.line_count();
        let snapshot_trimmed = stream.total_trimmed();
        let mut snapshot_res = execute_filtering(&stream, &filters, true, None, 1).unwrap();
        assert_eq!(snapshot_res.visible_indices, vec![0, 2]);

        // Stream appends 3 more lines (total 7) and trims 2 leading lines (capacity 5, trim_batch 2)
        stream.append_chunk(vec![
            "01-01 00:00:04.000 1 1 E Tag: live_4".to_string(),
            "01-01 00:00:05.000 1 1 I Tag: live_5".to_string(),
            "01-01 00:00:06.000 1 1 E Tag: live_6".to_string(),
        ]);
        assert_eq!(stream.trim_if_needed(), Some(2));
        assert_eq!(stream.total_trimmed(), 2);
        assert_eq!(stream.head_trimmed_since(snapshot_trimmed), 2);
        assert_eq!(stream.line_count(), 5);

        let live_src = ActiveLogSource::Stream(stream);
        let caught_up = reconcile_snapshot_filter_result(
            &mut snapshot_res,
            &live_src,
            snapshot_len,
            snapshot_trimmed,
            &compiled,
            true,
        );
        assert!(caught_up);

        // Surviving lines in live_src:
        // [0] snap_2 (matched), [1] snap_3, [2] live_4 (matched), [3] live_5, [4] live_6 (matched)
        assert_eq!(snapshot_res.visible_indices, vec![0, 2, 4]);
        assert_eq!(snapshot_res.total_matched_lines, 3);
    }

    #[test]
    fn test_resolve_status_line_uses_last_match_and_skips_empty_inactive() {
        use super::resolve_status_line_for_selection;
        use loganalysis_core::FilterTabData;

        let mut stream = StreamLogBuffer::new(100, 10);
        stream.append_chunk(vec![
            "01-01 00:00:00.000 1 1 E Tag: Fatal failure".to_string(),
            "01-01 00:00:01.000 1 1 I Tag: Normal startup".to_string(),
        ]);
        let src = ActiveLogSource::Stream(stream);

        let mut tabs = vec![
            FilterTabData {
                name: "Tab1".into(),
                enabled: true,
                filters: vec![
                    FilterRule {
                        text: "".into(), // empty rule must never match
                        bg_color: "White".into(),
                        active: true,
                        ..Default::default()
                    },
                    FilterRule {
                        text: "Fatal".into(),
                        bg_color: "Red".into(),
                        active: true,
                        ..Default::default()
                    },
                    FilterRule {
                        text: "failure".into(),
                        bg_color: "Orange".into(),
                        active: true,
                        ..Default::default()
                    },
                    FilterRule {
                        text: "failure".into(),
                        bg_color: "Purple".into(),
                        active: false, // inactive must be skipped
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            FilterTabData {
                name: "DisabledTab".into(),
                enabled: false,
                filters: vec![FilterRule {
                    text: "Fatal".into(),
                    bg_color: "Blue".into(),
                    active: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            FilterTabData {
                name: "Tab2".into(),
                enabled: true,
                filters: vec![FilterRule {
                    text: "Tag:".into(),
                    bg_color: "Green".into(),
                    active: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
        ];

        let mut active_rules = Vec::new();
        for tab in &tabs {
            if tab.enabled {
                for r in &tab.filters {
                    if r.active {
                        active_rules.push(r.clone());
                    }
                }
            }
        }
        let compiled = CompiledStreamFilters::new(&active_rules);
        let filter_res = execute_filtering(&src, &active_rules, false, None, 0).unwrap();

        // Fast path with FilterResult: last matching active rule across enabled tabs is 'Tag:' (Green)
        let status_fast =
            resolve_status_line_for_selection(0, Some(0), &src, Some(&filter_res), &compiled);
        assert_eq!(
            status_fast,
            "Line 1 | Matched Rule: 'Tag:' (Green) | 2 matches in tab"
        );

        // Without Tab2, last matching active rule in Tab1 is 'failure' (Orange)
        tabs.pop();
        let tab1_rules: Vec<FilterRule> = tabs
            .iter()
            .filter(|t| t.enabled)
            .flat_map(|t| t.filters.iter().filter(|r| r.active).cloned())
            .collect();
        let compiled_tab1 = CompiledStreamFilters::new(&tab1_rules);
        let filter_res_tab1 = execute_filtering(&src, &tab1_rules, false, None, 0).unwrap();

        // Fallback path without FilterResult: still uses last-match precedence and ignores empty/inactive/disabled
        let status_fallback =
            resolve_status_line_for_selection(0, None, &src, None, &compiled_tab1);
        assert_eq!(
            status_fallback,
            "Line 1 | Matched Rule: 'failure' (Orange) | 0 matches in tab"
        );

        // Unmatched line (never matched by empty rule)
        let status_none = resolve_status_line_for_selection(
            1,
            Some(1),
            &src,
            Some(&filter_res_tab1),
            &compiled_tab1,
        );
        assert_eq!(status_none, "Line 2 | No filter rule match");
    }

    #[test]
    fn test_derived_state_reset_source_gen_and_facet_auto_sampling() {
        use super::{
            is_source_generation_current, next_source_generation,
            reset_derived_controllers_and_tokens, should_auto_sample_facets,
        };
        use crate::controllers::{AdbController, BookmarkController, SearchController};
        use crate::selection::SelectionModel;
        use loganalysis_core::LineSeverity;
        use std::sync::atomic::{AtomicU64, Ordering};

        let filter_gen = AtomicU64::new(4);
        let analytics_gen = AtomicU64::new(2);
        let mut search_ctrl = SearchController::new();
        search_ctrl.match_status = Some("Line 9".to_string());
        let search_token = search_ctrl.cancel_gen.clone();
        let initial_search_gen = search_token.load(Ordering::SeqCst);

        // [INV-02-A] / [HIGH-1]: aborting searches increments search_ctrl.cancel_gen without touching filter_cancel_gen
        search_ctrl.abort_ongoing_searches();
        assert_eq!(search_token.load(Ordering::SeqCst), initial_search_gen + 1);
        assert_eq!(filter_gen.load(Ordering::Relaxed), 4);

        let mut selection = SelectionModel::new();
        selection.select(3, 12);

        let mut bookmarks = BookmarkController::new();
        bookmarks.toggle_bookmark(12, "err".into(), LineSeverity::Error);
        bookmarks.toggle_drawer();
        bookmarks.open_note_modal(12);

        let mut adb = AdbController::new();
        adb.is_paused = true;
        adb.paused_line_count = 5;
        adb.paused_buffer.push_back(vec!["buffered".into()]);

        let mut has_auto_sampled_facets = true;

        reset_derived_controllers_and_tokens(
            &filter_gen,
            &analytics_gen,
            &mut search_ctrl,
            &mut selection,
            &mut bookmarks,
            &mut adb,
            &mut has_auto_sampled_facets,
        );

        // [HIGH-1]: reset invalidates search_ctrl.cancel_gen and clears match_status
        assert_eq!(search_token.load(Ordering::SeqCst), initial_search_gen + 2);
        assert_eq!(search_ctrl.match_status, None);
        assert_eq!(filter_gen.load(Ordering::Relaxed), 5);
        assert_eq!(analytics_gen.load(Ordering::SeqCst), 3);

        // [HIGH-4]: ADB pause and buffer cleared on reset
        assert!(!adb.is_paused);
        assert_eq!(adb.paused_line_count, 0);
        assert!(adb.paused_buffer.is_empty());

        // [LOW-8f]: auto-sampled facets flag cleared on reset and gated until reset
        assert!(!has_auto_sampled_facets);
        assert!(!should_auto_sample_facets(
            has_auto_sampled_facets,
            true,
            99
        ));
        assert!(should_auto_sample_facets(
            has_auto_sampled_facets,
            true,
            100
        ));
        has_auto_sampled_facets = true;
        assert!(!should_auto_sample_facets(
            has_auto_sampled_facets,
            true,
            25_000
        ));

        // [HIGH-3]: monotonic source generation rejects stale completions
        let mut primary_gen = 0u64;
        let open_a = next_source_generation(&mut primary_gen);
        let open_b = next_source_generation(&mut primary_gen);
        assert!(!is_source_generation_current(primary_gen, open_a));
        assert!(is_source_generation_current(primary_gen, open_b));
    }

    #[test]
    fn test_format_byte_size_and_source_memory_display() {
        use super::{format_byte_size, format_source_memory_display};
        use loganalysis_core::{FileLogBuffer, MergedLogBuffer};
        use std::io::Write;
        use tempfile::NamedTempFile;

        assert_eq!(format_byte_size(0), "0.0 KB");
        assert_eq!(format_byte_size(512 * 1024), "512.0 KB");
        assert_eq!(format_byte_size(16 * 1024 * 1024), "16.0 MB");
        assert_eq!(format_byte_size(2 * 1024 * 1024 * 1024), "2.00 GB");

        assert_eq!(
            format_source_memory_display(None),
            ("0 MB".to_string(), "Idle")
        );

        let mut tmp_file = NamedTempFile::new().expect("create temp log file");
        writeln!(tmp_file, "11-06 18:00:00.000 100 100 I Tag: line one").unwrap();
        tmp_file.flush().unwrap();

        let file_buf = FileLogBuffer::open(tmp_file.path()).unwrap();
        let file_src = ActiveLogSource::File(file_buf);
        let (file_mem, file_backend) = format_source_memory_display(Some(&file_src));
        assert_eq!(file_backend, "MMap");
        assert!(file_mem.ends_with("KB"));

        let mut stream_buf = StreamLogBuffer::new(100, 10);
        stream_buf.append_chunk(vec!["line a".into(), "line b".into()]);
        let stream_src = ActiveLogSource::Stream(stream_buf);
        assert_eq!(
            format_source_memory_display(Some(&stream_src)),
            (format_byte_size(2 * 128), "Heap")
        );

        let merged_buf = MergedLogBuffer::open_multiple(&[tmp_file.path().to_path_buf()]).unwrap();
        let merged_src = ActiveLogSource::Merged(merged_buf);
        assert_eq!(
            format_source_memory_display(Some(&merged_src)),
            (format_byte_size(128), "Merged")
        );
    }

    #[test]
    fn test_clamp_line_detail_height_and_secondary_filter_anchor_preservation() {
        use super::{
            clamp_line_detail_height, preserve_selection_after_filter,
            reset_horizontal_scroll_offset, DEFAULT_LINE_DETAIL_HEIGHT, MAX_LINE_DETAIL_HEIGHT,
            MIN_LINE_DETAIL_HEIGHT, WORKSPACE_DEFAULTS,
        };
        use crate::selection::SelectionModel;
        use gpui::{point, px, UniformListScrollHandle};
        use loganalysis_core::FilterResult;

        let defaults = std::hint::black_box(WORKSPACE_DEFAULTS);
        assert!(defaults.full_line_display);
        assert_eq!(DEFAULT_LINE_DETAIL_HEIGHT, 160.0);
        assert_eq!(MIN_LINE_DETAIL_HEIGHT, 90.0);
        assert_eq!(MAX_LINE_DETAIL_HEIGHT, 420.0);
        assert_eq!(clamp_line_detail_height(160.0, 40.0), 200.0);
        assert_eq!(clamp_line_detail_height(90.0, 0.0), 90.0);
        assert_eq!(clamp_line_detail_height(420.0, 0.0), 420.0);
        assert_eq!(clamp_line_detail_height(160.0, -500.0), 90.0);
        assert_eq!(clamp_line_detail_height(160.0, 900.0), 420.0);
        assert_eq!(clamp_line_detail_height(160.0, f32::NAN), 160.0);
        assert_eq!(clamp_line_detail_height(160.0, f32::INFINITY), 160.0);
        assert_eq!(clamp_line_detail_height(160.0, f32::NEG_INFINITY), 160.0);
        assert_eq!(clamp_line_detail_height(f32::NAN, 10.0), 160.0);

        // Anchor preservation when secondary filter narrows visible_indices
        let handle = UniformListScrollHandle::new();
        handle
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(-240.0), px(-120.0)));
        reset_horizontal_scroll_offset(&handle);
        let reset_off = handle.0.borrow().base_handle.offset();
        assert_eq!(f32::from(reset_off.x), 0.0);
        assert_eq!(f32::from(reset_off.y), -120.0);

        // No-op when horizontal offset is already 0.0 preserves vertical offset
        reset_horizontal_scroll_offset(&handle);
        let after_noop = handle.0.borrow().base_handle.offset();
        assert_eq!(f32::from(after_noop.x), 0.0);
        assert_eq!(f32::from(after_noop.y), -120.0);

        let mut sec_sel = SelectionModel::new();
        // No anchor -> no-op
        preserve_selection_after_filter(&mut sec_sel, None, &handle);
        assert_eq!(sec_sel.selected_row, None);

        sec_sel.select(5, 25);

        let narrowed = FilterResult {
            visible_indices: vec![10, 20, 30, 40],
            ..Default::default()
        };
        preserve_selection_after_filter(&mut sec_sel, Some(&narrowed), &handle);
        assert_eq!(sec_sel.selected_row, Some(2)); // 30 is nearest >= 25
        assert_eq!(sec_sel.anchor_source_idx, Some(25));

        // Empty filter result clears secondary_selection
        let empty = FilterResult {
            visible_indices: vec![],
            ..Default::default()
        };
        preserve_selection_after_filter(&mut sec_sel, Some(&empty), &handle);
        assert_eq!(sec_sel.selected_row, None);
        assert_eq!(sec_sel.anchor_source_idx, None);

        // None filter result also clears selection
        sec_sel.select(1, 10);
        preserve_selection_after_filter(&mut sec_sel, None, &handle);
        assert_eq!(sec_sel.selected_row, None);
        assert_eq!(sec_sel.anchor_source_idx, None);
    }
}

/// Docking orientation for the filter rule management panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FilterPanelPosition {
    Left,
    #[default]
    Bottom,
}

/// Target text input currently receiving keyboard events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InputFocusTarget {
    #[default]
    None,
    SearchBar,
    ModalPattern,
    CommandPalette,
    BookmarkNote,
}

/// Transient state for the Filter Add/Edit Modal Dialog.
#[derive(Clone, Debug, Default)]
pub struct FilterEditState {
    pub is_open: bool,
    pub editing_index: Option<usize>, // None = Add new, Some = Edit existing
    pub is_regex: bool,
    pub is_case_sensitive: bool,
    pub is_exclude: bool,
    pub bg_color_name: String,
    pub fg_color_name: String,
}

/// Sub-state for Dual-Pane Split View and Lockstep Correlation.
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct SplitViewState {
    pub is_split_view: bool,
    pub is_split_vertical: bool,
    pub is_scroll_synced: bool,
    pub secondary_file_name: Option<gpui::SharedString>,
}

/// Sub-state for ADB device connectivity and live log streaming telemetry.
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct AdbStreamState {
    pub selected_device: Option<String>,
    pub is_device_selector_open: bool,
    pub is_scanning_adb: bool,
    pub is_streaming: bool,
    pub is_stream_paused: bool,
    pub auto_scroll: bool,
    pub paused_stream_line_count: usize,
}

/// Sub-state for Log search and Find bar widget.
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct SearchState {
    pub search_query: String,
    pub is_search_visible: bool,
    pub is_search_regex: bool,
    pub is_search_case_sensitive: bool,
    pub search_match_status: Option<String>,
}

/// Sub-state for Bookmark Navigation and Note Editing Drawer.
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct BookmarkDrawerState {
    pub is_bookmark_drawer_open: bool,
    pub bookmark_drawer_selected_index: usize,
    pub is_note_modal_open: bool,
    pub note_editing_source_idx: Option<usize>,
}

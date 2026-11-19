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
    ModalBgHex,
    ModalFgHex,
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
    /// Whether the inline custom background `#RRGGBB` input currently holds an invalid non-empty value.
    pub bg_hex_invalid: bool,
    /// Whether the inline custom foreground `#RRGGBB` input currently holds an invalid non-empty value.
    pub fg_hex_invalid: bool,
    /// Hex text last set programmatically in the BG hex input so `Change` handlers preserve named colors.
    pub programmatic_bg_hex: Option<String>,
    /// Hex text last set programmatically in the FG hex input so `Change` handlers preserve named colors.
    pub programmatic_fg_hex: Option<String>,
    /// Whether the pattern history suggestion dropdown is visible.
    pub suggestions_open: bool,
    /// Highlighted dropdown row, if any.
    pub suggestion_index: Option<usize>,
    /// Text last set programmatically (modal open / suggestion pick); changes equal to it do not reopen the dropdown.
    pub programmatic_text: Option<String>,
}

/// Sub-state for Dual-Pane Split View and Lockstep Correlation.
#[derive(Clone, Debug, Default)]
pub struct SplitViewState {
    pub is_split_view: bool,
    pub is_split_vertical: bool,
    pub is_scroll_synced: bool,
    pub secondary_file_name: Option<gpui::SharedString>,
}

/// Sub-state for ADB device connectivity and live log streaming telemetry.
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
#[derive(Clone, Debug, Default)]
pub struct SearchState {
    pub search_query: String,
    pub is_search_visible: bool,
    pub is_search_regex: bool,
    pub is_search_case_sensitive: bool,
    pub search_match_status: Option<String>,
}

/// Sub-state for Bookmark Navigation and Note Editing Drawer.
#[derive(Clone, Debug, Default)]
pub struct BookmarkDrawerState {
    pub is_bookmark_drawer_open: bool,
    pub bookmark_drawer_selected_index: usize,
    pub is_note_modal_open: bool,
    pub note_editing_source_idx: Option<usize>,
}

/// Drag payload transferred while reordering filter rules inside the Filter Dock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraggedFilterRule {
    pub tab_index: usize,
    pub source_index: usize,
    pub rule_id: loganalysis_core::uuid::Uuid,
    pub pattern: gpui::SharedString,
    pub is_exclude: bool,
    pub bg_color: String,
    pub text_color: String,
}

/// Insertion side relative to a hovered filter row during drag-and-drop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPosition {
    Before,
    After,
}

/// Active drop indicator target during filter rule drag-and-drop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilterDropTarget {
    pub target_row_index: usize,
    pub position: DropPosition,
}

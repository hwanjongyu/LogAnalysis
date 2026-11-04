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
    FilterRuleSearch,
    ModalPattern,
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

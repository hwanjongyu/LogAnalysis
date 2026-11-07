use std::sync::Arc;
use gpui::SharedString;
use loganalysis_core::{ActiveLogSource, FilterResult};

/// Controller managing Dual-Pane Split View and Lockstep Synchronization state.
#[derive(Default)]
pub struct SplitViewController {
    pub is_split_view: bool,
    pub is_split_vertical: bool,
    pub is_scroll_synced: bool,
    pub active_pane_is_secondary: bool,
    pub secondary_log_source: Option<Arc<ActiveLogSource>>,
    pub secondary_file_name: Option<SharedString>,
    pub secondary_filter_result: Option<Arc<FilterResult>>,
}

impl SplitViewController {
    pub fn new() -> Self {
        Self {
            is_split_view: false,
            is_split_vertical: false, // Horizontal split by default (side-by-side)
            is_scroll_synced: true,
            active_pane_is_secondary: false,
            secondary_log_source: None,
            secondary_file_name: None,
            secondary_filter_result: None,
        }
    }

    /// Toggles split view visibility.
    pub fn toggle_split(&mut self) {
        self.is_split_view = !self.is_split_view;
    }

    /// Toggles split orientation between horizontal and vertical.
    pub fn toggle_orientation(&mut self) {
        self.is_split_vertical = !self.is_split_vertical;
    }

    /// Toggles lockstep timestamp scroll sync.
    pub fn toggle_sync(&mut self) {
        self.is_scroll_synced = !self.is_scroll_synced;
    }

    /// Sets secondary file information.
    pub fn set_secondary(&mut self, source: Arc<ActiveLogSource>, name: SharedString) {
        self.secondary_log_source = Some(source);
        self.secondary_file_name = Some(name);
        self.is_split_view = true;
    }

    /// Clears secondary pane data.
    pub fn clear(&mut self) {
        self.secondary_log_source = None;
        self.secondary_file_name = None;
        self.secondary_filter_result = None;
        self.is_split_view = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_view_controller_toggles() {
        let mut controller = SplitViewController::new();
        assert!(!controller.is_split_view);
        assert!(!controller.is_split_vertical);
        assert!(controller.is_scroll_synced);

        controller.toggle_split();
        assert!(controller.is_split_view);

        controller.toggle_orientation();
        assert!(controller.is_split_vertical);

        controller.toggle_sync();
        assert!(!controller.is_scroll_synced);
    }
}

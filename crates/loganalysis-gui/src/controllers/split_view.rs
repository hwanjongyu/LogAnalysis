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
    pub secondary_source_gen: u64,
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
            secondary_source_gen: 0,
        }
    }

    /// Advances and returns the monotonic generation token for async secondary source loads.
    pub fn advance_source_gen(&mut self) -> u64 {
        self.secondary_source_gen = self.secondary_source_gen.wrapping_add(1);
        self.secondary_source_gen
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

    /// Clears secondary pane data and invalidates any pending async secondary load.
    pub fn clear(&mut self) {
        self.advance_source_gen();
        self.secondary_log_source = None;
        self.secondary_file_name = None;
        self.secondary_filter_result = None;
        self.is_split_view = false;
    }

    /// Returns a read-only state snapshot of the split-view controller.
    pub fn state(&self) -> crate::types::SplitViewState {
        crate::types::SplitViewState {
            is_split_view: self.is_split_view,
            is_split_vertical: self.is_split_vertical,
            is_scroll_synced: self.is_scroll_synced,
            secondary_file_name: self.secondary_file_name.clone(),
        }
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

        let g1 = controller.advance_source_gen();
        let g2 = controller.advance_source_gen();
        assert!(g2 > g1);
        controller.clear();
        assert!(controller.secondary_source_gen > g2);
    }

    #[test]
    fn test_split_view_source_gen_advance_and_clear_invalidation() {
        let mut controller = SplitViewController::new();
        assert_eq!(controller.secondary_source_gen, 0);

        let gen_a = controller.advance_source_gen();
        let gen_b = controller.advance_source_gen();
        assert_eq!(gen_a, 1);
        assert_eq!(gen_b, 2);
        assert_ne!(controller.secondary_source_gen, gen_a);
        assert_eq!(controller.secondary_source_gen, gen_b);

        controller.secondary_file_name = Some("stale.log".into());
        controller.is_split_view = true;
        controller.clear();
        assert!(!controller.is_split_view);
        assert!(controller.secondary_file_name.is_none());
        assert!(controller.secondary_log_source.is_none());
        assert!(controller.secondary_filter_result.is_none());
        assert!(
            controller.secondary_source_gen > gen_b,
            "clear() must advance secondary_source_gen to invalidate pending async secondary opens"
        );
    }
}

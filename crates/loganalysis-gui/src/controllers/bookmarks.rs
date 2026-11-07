use loganalysis_core::{BookmarkManager, LineSeverity};

/// Controller managing bookmarks, drawer visibility, and line annotations.
#[derive(Default)]
pub struct BookmarkController {
    pub manager: BookmarkManager,
    pub is_drawer_open: bool,
    pub drawer_selected_index: usize,
    pub is_note_modal_open: bool,
    pub note_editing_source_idx: Option<usize>,
}

impl BookmarkController {
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggles the bookmark drawer open or closed.
    pub fn toggle_drawer(&mut self) -> bool {
        self.is_drawer_open = !self.is_drawer_open;
        self.is_drawer_open
    }

    /// Closes the bookmark drawer.
    pub fn close_drawer(&mut self) {
        self.is_drawer_open = false;
    }

    /// Opens the note editor modal for a specific log source line.
    pub fn open_note_modal(&mut self, source_idx: usize) {
        self.note_editing_source_idx = Some(source_idx);
        self.is_note_modal_open = true;
    }

    /// Closes the note editor modal.
    pub fn close_note_modal(&mut self) {
        self.is_note_modal_open = false;
        self.note_editing_source_idx = None;
    }

    /// Toggles bookmark at a given source index.
    pub fn toggle_bookmark(&mut self, source_idx: usize, text: String, severity: LineSeverity) -> bool {
        self.manager.toggle(source_idx, text, severity)
    }

    /// Clears all bookmarks.
    pub fn clear(&mut self) {
        self.manager.clear();
        self.drawer_selected_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bookmark_controller_lifecycle() {
        let mut controller = BookmarkController::new();
        assert!(!controller.is_drawer_open);
        assert!(!controller.is_note_modal_open);

        controller.toggle_drawer();
        assert!(controller.is_drawer_open);

        controller.open_note_modal(42);
        assert!(controller.is_note_modal_open);
        assert_eq!(controller.note_editing_source_idx, Some(42));

        controller.close_note_modal();
        assert!(!controller.is_note_modal_open);
        assert!(controller.note_editing_source_idx.is_none());

        let added = controller.toggle_bookmark(10, "test".into(), LineSeverity::Error);
        assert!(added);
        assert_eq!(controller.manager.len(), 1);

        controller.clear();
        assert_eq!(controller.manager.len(), 0);
    }
}

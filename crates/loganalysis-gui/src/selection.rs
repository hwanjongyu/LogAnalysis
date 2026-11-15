#[derive(Clone, Debug, Default)]
pub struct SelectionModel {
    pub selected_row: Option<usize>,
    pub anchor_source_idx: Option<usize>,
}

#[allow(dead_code)]
impl SelectionModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn select(&mut self, row_idx: usize, source_idx: usize) {
        self.selected_row = Some(row_idx);
        self.anchor_source_idx = Some(source_idx);
    }

    pub fn clear(&mut self) {
        self.selected_row = None;
        self.anchor_source_idx = None;
    }

    /// Shifts selection indices after `raw_trimmed_count` leading lines are dropped from a rolling stream buffer,
    /// where `dropped_visible_rows` visible rows fell off the front of `visible_indices`.
    pub fn shift_on_trim(&mut self, dropped_visible_rows: usize, raw_trimmed_count: usize) {
        if raw_trimmed_count == 0 && dropped_visible_rows == 0 {
            return;
        }
        if let Some(anchor) = self.anchor_source_idx {
            if anchor < raw_trimmed_count {
                self.clear();
                return;
            }
            self.anchor_source_idx = Some(anchor - raw_trimmed_count);
        }
        if let Some(row) = self.selected_row {
            if row < dropped_visible_rows {
                self.selected_row = None;
            } else {
                self.selected_row = Some(row - dropped_visible_rows);
            }
        }
    }
}

/// Resolves the backing log source line index for a selected visible row without silently
/// falling back to line `0` when the selection or index mapping is absent or stale.
pub fn resolve_selected_source_idx(
    visible_indices: &[usize],
    selected_row: Option<usize>,
) -> Option<usize> {
    selected_row.and_then(|row| visible_indices.get(row).copied())
}

#[cfg(test)]
mod tests {
    use super::SelectionModel;

    #[test]
    fn test_selection_shift_on_trim() {
        let mut sel = SelectionModel::new();
        sel.select(15, 20);

        sel.shift_on_trim(0, 0);
        assert_eq!(sel.selected_row, Some(15));
        assert_eq!(sel.anchor_source_idx, Some(20));

        sel.shift_on_trim(5, 10);
        assert_eq!(sel.selected_row, Some(10));
        assert_eq!(sel.anchor_source_idx, Some(10));

        sel.shift_on_trim(5, 11);
        assert_eq!(sel.selected_row, None);
        assert_eq!(sel.anchor_source_idx, None);

        // Verify visible index space is independent from large raw buffer evictions
        sel.select(50, 25_000);
        sel.shift_on_trim(5, 20_000);
        assert_eq!(sel.selected_row, Some(45));
        assert_eq!(sel.anchor_source_idx, Some(5_000));

        // Verify visible row falling below dropped_visible_rows clears selected_row even if anchor survives
        sel.select(3, 25_000);
        sel.shift_on_trim(5, 20_000);
        assert_eq!(sel.selected_row, None);
        assert_eq!(sel.anchor_source_idx, Some(5_000));
    }

    #[test]
    fn test_resolve_selected_source_idx_bounds() {
        use super::resolve_selected_source_idx;

        // Absent selection or empty visible set must return None (never silently default to Some(0))
        assert_eq!(resolve_selected_source_idx(&[], None), None);
        assert_eq!(resolve_selected_source_idx(&[], Some(0)), None);
        assert_eq!(resolve_selected_source_idx(&[], Some(5)), None);

        let visible = [10usize, 25, 42];
        assert_eq!(resolve_selected_source_idx(&visible, None), None);
        assert_eq!(resolve_selected_source_idx(&visible, Some(0)), Some(10));
        assert_eq!(resolve_selected_source_idx(&visible, Some(1)), Some(25));
        assert_eq!(resolve_selected_source_idx(&visible, Some(2)), Some(42));
        assert_eq!(resolve_selected_source_idx(&visible, Some(3)), None);
        assert_eq!(resolve_selected_source_idx(&visible, Some(usize::MAX)), None);

        // Stale selection cleared by trim must resolve to None
        let mut sel = SelectionModel::new();
        sel.select(3, 50);
        sel.shift_on_trim(5, 10);
        assert_eq!(resolve_selected_source_idx(&visible, sel.selected_row), None);
    }
}

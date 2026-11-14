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
}

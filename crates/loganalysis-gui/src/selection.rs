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

    /// Shifts selection indices after `trimmed` leading lines are dropped from a rolling stream buffer.
    pub fn shift_on_trim(&mut self, trimmed: usize) {
        if trimmed == 0 {
            return;
        }
        if let Some(anchor) = self.anchor_source_idx {
            if anchor < trimmed {
                self.clear();
                return;
            }
            self.anchor_source_idx = Some(anchor - trimmed);
        }
        if let Some(row) = self.selected_row {
            self.selected_row = Some(row.saturating_sub(trimmed));
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

        sel.shift_on_trim(0);
        assert_eq!(sel.selected_row, Some(15));
        assert_eq!(sel.anchor_source_idx, Some(20));

        sel.shift_on_trim(5);
        assert_eq!(sel.selected_row, Some(10));
        assert_eq!(sel.anchor_source_idx, Some(15));

        sel.shift_on_trim(16);
        assert_eq!(sel.selected_row, None);
        assert_eq!(sel.anchor_source_idx, None);
    }
}

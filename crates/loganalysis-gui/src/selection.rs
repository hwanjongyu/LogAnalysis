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
}

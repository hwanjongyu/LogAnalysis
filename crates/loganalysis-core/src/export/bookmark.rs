use std::collections::BTreeSet;
use serde::{Deserialize, Serialize};

use crate::filter::heatmap::LineSeverity;

/// A pinned bookmark marking a critical line of interest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub source_idx: usize,
    pub line_number: usize,
    pub text: String,
    pub note: Option<String>,
    pub severity: LineSeverity,
    pub created_at_epoch: u64,
}

/// Deterministic manager for user-created bookmarks with cyclic navigation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkManager {
    bookmarks: Vec<Bookmark>,
    index_set: BTreeSet<usize>,
}

impl BookmarkManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggles a bookmark at `source_idx`. Returns `true` if added, `false` if removed.
    pub fn toggle(
        &mut self,
        source_idx: usize,
        text: String,
        severity: LineSeverity,
    ) -> bool {
        if self.index_set.contains(&source_idx) {
            self.remove(source_idx);
            false
        } else {
            self.add(Bookmark {
                source_idx,
                line_number: source_idx + 1,
                text,
                note: None,
                severity,
                created_at_epoch: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            });
            true
        }
    }

    /// Adds a bookmark, maintaining ascending order by source_idx.
    pub fn add(&mut self, bookmark: Bookmark) {
        if self.index_set.insert(bookmark.source_idx) {
            let pos = self
                .bookmarks
                .binary_search_by_key(&bookmark.source_idx, |b| b.source_idx)
                .unwrap_or_else(|e| e);
            self.bookmarks.insert(pos, bookmark);
        }
    }

    /// Removes a bookmark at `source_idx`.
    pub fn remove(&mut self, source_idx: usize) -> bool {
        if self.index_set.remove(&source_idx) {
            if let Ok(pos) = self
                .bookmarks
                .binary_search_by_key(&source_idx, |b| b.source_idx)
            {
                self.bookmarks.remove(pos);
                return true;
            }
        }
        false
    }

    /// Checks if a line index is currently bookmarked in O(log B) time.
    #[inline]
    pub fn is_bookmarked(&self, source_idx: usize) -> bool {
        self.index_set.contains(&source_idx)
    }

    /// Finds the next bookmark strictly after `current_source_idx`, wrapping cyclically.
    pub fn next_bookmark(&self, current_source_idx: usize) -> Option<usize> {
        if self.index_set.is_empty() {
            return None;
        }
        // Smallest bookmark > current
        if let Some(&next) = self.index_set.range((current_source_idx + 1)..).next() {
            Some(next)
        } else {
            // Wrap to first
            self.index_set.iter().next().copied()
        }
    }

    /// Finds the previous bookmark strictly before `current_source_idx`, wrapping cyclically.
    pub fn prev_bookmark(&self, current_source_idx: usize) -> Option<usize> {
        if self.index_set.is_empty() {
            return None;
        }
        // Largest bookmark < current
        if let Some(&prev) = self.index_set.range(..current_source_idx).next_back() {
            Some(prev)
        } else {
            // Wrap to last
            self.index_set.iter().next_back().copied()
        }
    }

    /// Returns a slice of all bookmarks in ascending line order.
    pub fn items(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    /// Returns the total number of active bookmarks.
    pub fn len(&self) -> usize {
        self.bookmarks.len()
    }

    /// Returns whether the bookmark list is empty.
    pub fn is_empty(&self) -> bool {
        self.bookmarks.is_empty()
    }

    /// Clears all bookmarks.
    pub fn clear(&mut self) {
        self.bookmarks.clear();
        self.index_set.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bookmark_toggle_and_set() {
        let mut mgr = BookmarkManager::new();
        assert!(mgr.is_empty());

        let added = mgr.toggle(10, "Line 10".into(), LineSeverity::Error);
        assert!(added);
        assert_eq!(mgr.len(), 1);
        assert!(mgr.is_bookmarked(10));

        let removed = mgr.toggle(10, "Line 10".into(), LineSeverity::Error);
        assert!(!removed);
        assert_eq!(mgr.len(), 0);
        assert!(!mgr.is_bookmarked(10));
    }

    #[test]
    fn test_bookmark_cyclic_navigation() {
        let mut mgr = BookmarkManager::new();
        mgr.toggle(10, "Ten".into(), LineSeverity::Normal);
        mgr.toggle(50, "Fifty".into(), LineSeverity::Warning);
        mgr.toggle(100, "Hundred".into(), LineSeverity::Error);

        // Forward traversal
        assert_eq!(mgr.next_bookmark(0), Some(10));
        assert_eq!(mgr.next_bookmark(10), Some(50));
        assert_eq!(mgr.next_bookmark(50), Some(100));
        // Wrap around to beginning
        assert_eq!(mgr.next_bookmark(100), Some(10));
        assert_eq!(mgr.next_bookmark(150), Some(10));

        // Backward traversal
        assert_eq!(mgr.prev_bookmark(150), Some(100));
        assert_eq!(mgr.prev_bookmark(100), Some(50));
        assert_eq!(mgr.prev_bookmark(50), Some(10));
        // Wrap around to end
        assert_eq!(mgr.prev_bookmark(10), Some(100));
        assert_eq!(mgr.prev_bookmark(5), Some(100));
    }
}

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use loganalysis_core::CompiledMatcher;

/// Controller managing search bar state, regex flags, and matcher compilation.
pub struct SearchController {
    pub query: String,
    pub is_visible: bool,
    pub is_regex: bool,
    pub is_case_sensitive: bool,
    pub cached_matcher: Option<Arc<CompiledMatcher>>,
    pub cancel_gen: Arc<AtomicU64>,
    pub match_status: Option<String>,
}

impl Default for SearchController {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchController {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            is_visible: false,
            is_regex: false,
            is_case_sensitive: false,
            cached_matcher: None,
            cancel_gen: Arc::new(AtomicU64::new(0)),
            match_status: None,
        }
    }

    /// Toggles search bar visibility.
    pub fn toggle_visible(&mut self) -> bool {
        self.is_visible = !self.is_visible;
        self.is_visible
    }

    /// Toggles regex mode and updates cached matcher.
    pub fn toggle_regex(&mut self) {
        self.is_regex = !self.is_regex;
        self.update_matcher();
    }

    /// Toggles case sensitivity and updates cached matcher.
    pub fn toggle_case_sensitive(&mut self) {
        self.is_case_sensitive = !self.is_case_sensitive;
        self.update_matcher();
    }

    /// Sets query text and recompiles matcher.
    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.update_matcher();
    }

    /// Recompiles the search matcher from the current query and flags.
    pub fn update_matcher(&mut self) {
        if self.query.trim().is_empty() {
            self.cached_matcher = None;
            return;
        }

        self.cached_matcher = CompiledMatcher::new(
            &self.query,
            self.is_case_sensitive,
            self.is_regex,
        )
        .ok()
        .map(Arc::new);
    }

    /// Recompiles the search matcher only when the find bar is visible and the query is non-empty.
    pub fn update_matcher_when_visible(&mut self) {
        if !self.is_visible || self.query.trim().is_empty() {
            self.cached_matcher = None;
            return;
        }
        self.cached_matcher = CompiledMatcher::new(
            &self.query,
            self.is_case_sensitive,
            self.is_regex,
        )
        .ok()
        .map(Arc::new);
    }

    /// Increments cancellation generation token to abort background searches.
    pub fn abort_ongoing_searches(&self) {
        self.cancel_gen.fetch_add(1, Ordering::SeqCst);
    }

    /// Returns a read-only state snapshot of the search controller.
    pub fn state(&self) -> crate::types::SearchState {
        crate::types::SearchState {
            search_query: self.query.clone(),
            is_search_visible: self.is_visible,
            is_search_regex: self.is_regex,
            is_search_case_sensitive: self.is_case_sensitive,
            search_match_status: self.match_status.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_controller_lifecycle() {
        let mut controller = SearchController::new();
        assert!(!controller.is_visible);
        assert!(!controller.is_regex);
        assert!(!controller.is_case_sensitive);

        controller.set_query("error".into());
        assert!(controller.cached_matcher.is_some());

        controller.toggle_regex();
        assert!(controller.is_regex);
        assert!(controller.cached_matcher.is_some());

        let gen_before = controller.cancel_gen.load(Ordering::SeqCst);
        controller.abort_ongoing_searches();
        let gen_after = controller.cancel_gen.load(Ordering::SeqCst);
        assert_eq!(gen_after, gen_before + 1);
    }

    #[test]
    fn test_search_controller_preserves_boundary_spaces_and_clears_whitespace() {
        let mut controller = SearchController::new();
        controller.is_visible = true;

        controller.set_query(" E ".to_string());
        assert_eq!(controller.query, " E ");
        let matcher = controller
            .cached_matcher
            .as_ref()
            .expect("non-whitespace query with boundary spaces must compile");
        assert!(matcher.matches(b"11-06 18:00:00.000 1000 1000 E Tag"));
        assert!(!matcher.matches(b"Error without surrounding spaces"));

        controller.set_query("   \t ".to_string());
        assert!(controller.cached_matcher.is_none());
    }
}

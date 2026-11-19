use std::sync::atomic::Ordering;
use gpui::*;
use loganalysis_core::*;

use crate::types::InputFocusTarget;
use super::LogAnalysisWorkspace;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchOutcomeUpdate {
    pub matched_location: Option<(usize, usize)>, // (visible_row, real_idx)
    pub status_text: String,
    pub match_status: String,
}

pub fn format_search_outcome(
    search_res: Result<Option<SearchResult>, SearchError>,
) -> SearchOutcomeUpdate {
    match search_res {
        Ok(Some(found)) => SearchOutcomeUpdate {
            matched_location: Some((found.visible_row, found.real_idx)),
            status_text: format!("Found match at line {}", found.real_idx + 1),
            match_status: format!("Line {}", found.real_idx + 1),
        },
        Ok(None) => SearchOutcomeUpdate {
            matched_location: None,
            status_text: "Search pattern not found".to_string(),
            match_status: "0 matches".to_string(),
        },
        Err(SearchError::InvalidRegex(err)) => SearchOutcomeUpdate {
            matched_location: None,
            status_text: format!("Regex syntax error: {err}"),
            match_status: "Regex Error".to_string(),
        },
        Err(err) => SearchOutcomeUpdate {
            matched_location: None,
            status_text: format!("Search error: {err}"),
            match_status: "Error".to_string(),
        },
    }
}

/// Applies raw Find Bar input to [`SearchController`] while preserving meaningful
/// leading/trailing spaces (e.g., `" E "`) and clearing empty/whitespace-only queries.
pub(crate) fn apply_find_input_query(
    ctrl: &mut crate::controllers::SearchController,
    raw_input: &str,
) {
    if raw_input.trim().is_empty() {
        ctrl.query.clear();
        ctrl.match_status = None;
    } else {
        ctrl.query = raw_input.to_string();
    }
    ctrl.update_matcher_when_visible();
}

impl LogAnalysisWorkspace {
    fn apply_search_result(
        &mut self,
        search_res: Result<Option<SearchResult>, SearchError>,
        cx: &mut Context<Self>,
    ) {
        if matches!(search_res, Err(SearchError::Cancelled)) {
            return;
        }
        let outcome = format_search_outcome(search_res);
        if let Some((visible_row, real_idx)) = outcome.matched_location {
            self.selection.select(visible_row, real_idx);
            self.split_view.active_pane_is_secondary = false;
            self.scroll_handle
                .scroll_to_item(visible_row, ScrollStrategy::Center);
            self.reset_line_detail_scroll_if_line_changed();
        }
        self.status_text = outcome.status_text.into();
        self.search_ctrl.match_status = Some(outcome.match_status);
        cx.notify();
    }

    /// Cyclic search forward or backward (with async dispatch for large logs)
    pub fn search(&mut self, forward: bool, cx: &mut Context<Self>) {
        let raw_input = self.find_input.read(cx).text().to_string();
        apply_find_input_query(&mut self.search_ctrl, &raw_input);

        if self.search_ctrl.query.is_empty() {
            self.search_ctrl.match_status = None;
            return;
        }

        let Some(source) = self.log_source.clone() else {
            return;
        };
        let Some(res) = self.filter_result.clone() else {
            return;
        };

        let query = SearchQuery {
            pattern: self.search_ctrl.query.clone(),
            case_sensitive: self.search_ctrl.is_case_sensitive,
            is_regex: self.search_ctrl.is_regex,
            forward,
        };

        let start_row = self.selection.selected_row;

        // If dataset is large (> ASYNC_SEARCH_THRESHOLD), dispatch to background executor
        if res.visible_indices.len() > loganalysis_core::ASYNC_SEARCH_THRESHOLD {
            self.status_text = "Searching in background...".into();
            self.search_ctrl.match_status = Some("Searching...".into());
            cx.notify();

            // Advance generation to invalidate and cancel any previous in-flight background searches
            let generation = self.search_ctrl.cancel_gen.fetch_add(1, Ordering::SeqCst) + 1;
            let cancel_token = self.search_ctrl.cancel_gen.clone();

            cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let bg_executor = cx.background_executor().clone();
                let mut async_cx = cx.clone();

                async move {
                    let search_res = bg_executor
                        .spawn(async move {
                            search_in_log(
                                &*source,
                                &res.visible_indices,
                                &query,
                                start_row,
                                Some(&cancel_token),
                                generation,
                                None,
                            )
                        })
                        .await;

                    let _ = this.update(&mut async_cx, |workspace, cx| {
                        // Discard results if superseded by a newer search
                        if workspace.search_ctrl.cancel_gen.load(Ordering::SeqCst) != generation {
                            return;
                        }

                        workspace.apply_search_result(search_res, cx);
                    });
                }
            })
            .detach();
        } else {
            let search_res =
                search_in_log(&*source, &res.visible_indices, &query, start_row, None, 0, None);
            self.apply_search_result(search_res, cx);
        }
    }

    /// Updates the pre-compiled search matcher cached outside the render loop
    pub fn update_cached_search_matcher(&mut self) {
        self.search_ctrl.update_matcher_when_visible();
    }

    /// Toggles regex mode for the find bar
    pub fn toggle_search_regex(&mut self, cx: &mut Context<Self>) {
        self.search_ctrl.is_regex = !self.search_ctrl.is_regex;
        self.update_cached_search_matcher();
        cx.notify();
    }

    /// Toggles case-sensitivity for the find bar
    pub fn toggle_search_case_sensitive(&mut self, cx: &mut Context<Self>) {
        self.search_ctrl.is_case_sensitive = !self.search_ctrl.is_case_sensitive;
        self.update_cached_search_matcher();
        cx.notify();
    }

    /// Closes the floating find bar and clears search bar focus
    pub fn close_search_bar(&mut self, cx: &mut Context<Self>) {
        self.search_ctrl.is_visible = false;
        if self.input_focus == InputFocusTarget::SearchBar {
            self.input_focus = InputFocusTarget::None;
        }
        self.update_cached_search_matcher();
        cx.notify();
    }

    /// Toggles the floating find bar
    pub fn toggle_search_bar(&mut self, cx: &mut Context<Self>) {
        if self.search_ctrl.is_visible {
            self.close_search_bar(cx);
        } else {
            self.search_ctrl.is_visible = true;
            self.input_focus = InputFocusTarget::SearchBar;
            self.update_cached_search_matcher();
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{format_search_outcome, SearchOutcomeUpdate};
    use crate::components::find_bar::is_find_status_error;
    use loganalysis_core::{SearchError, SearchResult};

    #[test]
    fn test_format_search_outcome_match_found() {
        let outcome = format_search_outcome(Ok(Some(SearchResult {
            visible_row: 3,
            real_idx: 41,
        })));
        assert_eq!(
            outcome,
            SearchOutcomeUpdate {
                matched_location: Some((3, 41)),
                status_text: "Found match at line 42".to_string(),
                match_status: "Line 42".to_string(),
            }
        );
        assert!(!is_find_status_error(&outcome.match_status));
    }

    #[test]
    fn test_format_search_outcome_not_found() {
        let outcome = format_search_outcome(Ok(None));
        assert_eq!(
            outcome,
            SearchOutcomeUpdate {
                matched_location: None,
                status_text: "Search pattern not found".to_string(),
                match_status: "0 matches".to_string(),
            }
        );
        assert!(is_find_status_error(&outcome.match_status));
    }

    #[test]
    fn test_format_search_outcome_invalid_regex() {
        let outcome = format_search_outcome(Err(SearchError::InvalidRegex(
            "unclosed group".to_string(),
        )));
        assert_eq!(
            outcome,
            SearchOutcomeUpdate {
                matched_location: None,
                status_text: "Regex syntax error: unclosed group".to_string(),
                match_status: "Regex Error".to_string(),
            }
        );
        assert!(is_find_status_error(&outcome.match_status));
    }

    #[test]
    fn test_format_search_outcome_cancelled() {
        let outcome = format_search_outcome(Err(SearchError::Cancelled));
        assert_eq!(
            outcome,
            SearchOutcomeUpdate {
                matched_location: None,
                status_text: "Search error: Search cancelled".to_string(),
                match_status: "Error".to_string(),
            }
        );
        assert!(is_find_status_error(&outcome.match_status));
    }

    #[test]
    fn test_async_search_threshold_unified() {
        assert_eq!(loganalysis_core::ASYNC_SEARCH_THRESHOLD, 50_000);
        assert_eq!(
            loganalysis_core::constants::ASYNC_SEARCH_THRESHOLD,
            loganalysis_core::ASYNC_SEARCH_THRESHOLD
        );
        let threshold = std::hint::black_box(loganalysis_core::ASYNC_SEARCH_THRESHOLD);
        assert!(49_999 < threshold);
        assert!(50_000 >= threshold);
    }

    #[test]
    fn test_apply_find_input_query_preserves_spaces_and_clears_whitespace() {
        use super::apply_find_input_query;
        use crate::controllers::SearchController;

        let mut ctrl = SearchController::new();
        ctrl.is_visible = true;

        // Non-whitespace query with surrounding spaces (" E ") must stay untrimmed
        apply_find_input_query(&mut ctrl, " E ");
        assert_eq!(ctrl.query, " E ");
        let matcher = ctrl
            .cached_matcher
            .as_ref()
            .expect("visible non-whitespace query must compile matcher");
        assert!(matcher.matches(b"11-06 18:00:00.000 1000 1000 E Tag: boom"));
        assert!(!matcher.matches(b"11-06 18:00:00.000 1000 1000 I Tag: Error"));

        // Whitespace-only clears query, match_status, and cached_matcher (MAJ-02)
        ctrl.match_status = Some("Line 1".to_string());
        apply_find_input_query(&mut ctrl, "   \t\n");
        assert!(ctrl.query.is_empty());
        assert_eq!(ctrl.match_status, None);
        assert!(ctrl.cached_matcher.is_none());
    }
}

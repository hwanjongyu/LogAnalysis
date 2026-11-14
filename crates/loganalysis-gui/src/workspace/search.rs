use std::sync::atomic::Ordering;
use std::sync::Arc;
use gpui::*;
use loganalysis_core::*;

use crate::types::InputFocusTarget;
use super::LogAnalysisWorkspace;

const ASYNC_SEARCH_THRESHOLD: usize = 50_000;

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
            self.scroll_handle
                .scroll_to_item(visible_row, ScrollStrategy::Center);
        }
        self.status_text = outcome.status_text.into();
        self.search_match_status = Some(outcome.match_status);
        cx.notify();
    }

    /// Cyclic search forward or backward (with async dispatch for large logs)
    pub fn search(&mut self, forward: bool, cx: &mut Context<Self>) {
        let input_text = self.find_input.read(cx).text().trim().to_string();
        if !input_text.is_empty() {
            self.search_query = input_text;
        }
        self.update_cached_search_matcher();

        if self.search_query.is_empty() {
            self.search_match_status = None;
            return;
        }

        let Some(source) = self.log_source.clone() else {
            return;
        };
        let Some(res) = self.filter_result.clone() else {
            return;
        };

        let query = SearchQuery {
            pattern: self.search_query.clone(),
            case_sensitive: self.is_search_case_sensitive,
            is_regex: self.is_search_regex,
            forward,
        };

        let start_row = self.selection.selected_row;

        // If dataset is large (> ASYNC_SEARCH_THRESHOLD), dispatch to background executor
        if res.visible_indices.len() > ASYNC_SEARCH_THRESHOLD {
            self.status_text = "Searching in background...".into();
            self.search_match_status = Some("Searching...".into());
            cx.notify();

            // Advance generation to invalidate and cancel any previous in-flight background searches
            let generation = self.search_cancel_gen.fetch_add(1, Ordering::SeqCst) + 1;
            let cancel_token = self.search_cancel_gen.clone();

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
                        if workspace.search_cancel_gen.load(Ordering::SeqCst) != generation {
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
        self.cached_search_matcher = if self.is_search_visible && !self.search_query.trim().is_empty() {
            loganalysis_core::CompiledMatcher::new(
                &self.search_query,
                self.is_search_case_sensitive,
                self.is_search_regex,
            )
            .ok()
            .map(Arc::new)
        } else {
            None
        };
    }

    /// Toggles regex mode for the find bar
    pub fn toggle_search_regex(&mut self, cx: &mut Context<Self>) {
        self.is_search_regex = !self.is_search_regex;
        self.update_cached_search_matcher();
        cx.notify();
    }

    /// Toggles case-sensitivity for the find bar
    pub fn toggle_search_case_sensitive(&mut self, cx: &mut Context<Self>) {
        self.is_search_case_sensitive = !self.is_search_case_sensitive;
        self.update_cached_search_matcher();
        cx.notify();
    }

    /// Toggles the floating find bar
    pub fn toggle_search_bar(&mut self, cx: &mut Context<Self>) {
        self.is_search_visible = !self.is_search_visible;
        if self.is_search_visible {
            self.input_focus = InputFocusTarget::SearchBar;
        } else if self.input_focus == InputFocusTarget::SearchBar {
            self.input_focus = InputFocusTarget::None;
        }
        self.update_cached_search_matcher();
        cx.notify();
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
}

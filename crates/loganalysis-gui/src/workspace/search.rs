use std::sync::atomic::Ordering;
use std::sync::Arc;
use gpui::*;
use loganalysis_core::*;

use crate::types::InputFocusTarget;
use super::LogAnalysisWorkspace;

const ASYNC_SEARCH_THRESHOLD: usize = 50_000;

impl LogAnalysisWorkspace {
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

                        if let Ok(Some(found)) = search_res {
                            workspace.selection.select(found.visible_row, found.real_idx);
                            workspace
                                .scroll_handle
                                .scroll_to_item(found.visible_row, ScrollStrategy::Center);
                            workspace.status_text =
                                format!("Found match at line {}", found.real_idx + 1).into();
                            workspace.search_match_status =
                                Some(format!("Line {}", found.real_idx + 1));
                        } else {
                            workspace.status_text = "Search pattern not found".into();
                            workspace.search_match_status = Some("0 matches".into());
                        }
                        cx.notify();
                    });
                }
            })
            .detach();
        } else if let Ok(Some(found)) =
            search_in_log(&*source, &res.visible_indices, &query, start_row, None, 0, None)
        {
            self.selection.select(found.visible_row, found.real_idx);
            self.scroll_handle
                .scroll_to_item(found.visible_row, ScrollStrategy::Center);
            self.status_text = format!("Found match at line {}", found.real_idx + 1).into();
            self.search_match_status = Some(format!("Line {}", found.real_idx + 1));
            cx.notify();
        } else {
            self.status_text = "Search pattern not found".into();
            self.search_match_status = Some("0 matches".into());
            cx.notify();
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

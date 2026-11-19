use gpui::*;
use serde::{Deserialize, Serialize};

use super::LogAnalysisWorkspace;

/// Maximum number of persisted filter pattern history entries.
pub const MAX_PATTERN_HISTORY: usize = 200;
/// Maximum number of suggestions shown in the dropdown.
pub const MAX_PATTERN_SUGGESTIONS: usize = 8;
/// Current on-disk schema version.
const PATTERN_HISTORY_VERSION: u32 = 1;

/// A single remembered filter pattern with the flags it was saved with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternHistoryEntry {
    pub pattern: String,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub last_used: u64,
    #[serde(default)]
    pub use_count: u32,
}

impl PatternHistoryEntry {
    /// Creates an entry used once at `last_used` (unix seconds).
    pub fn new(
        pattern: impl Into<String>,
        regex: bool,
        case_sensitive: bool,
        last_used: u64,
    ) -> Self {
        Self {
            pattern: pattern.into(),
            regex,
            case_sensitive,
            last_used,
            use_count: 1,
        }
    }

    fn same_key(&self, other: &Self) -> bool {
        self.pattern == other.pattern
            && self.regex == other.regex
            && self.case_sensitive == other.case_sensitive
    }
}

#[derive(Serialize, Deserialize)]
struct PatternHistoryFile {
    version: u32,
    entries: Vec<PatternHistoryEntry>,
}

/// MRU-ordered, deduplicated, capped history of saved filter patterns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PatternHistory {
    pub entries: Vec<PatternHistoryEntry>,
}

impl PatternHistory {
    /// Records a use of `entry` at the front (MRU). Deduplicates on
    /// (pattern, regex, case_sensitive), accumulating `use_count`, and caps at
    /// `MAX_PATTERN_HISTORY`. Blank patterns are rejected (returns `false`).
    pub fn record(&mut self, mut entry: PatternHistoryEntry) -> bool {
        if entry.pattern.trim().is_empty() {
            return false;
        }
        if let Some(pos) = self.entries.iter().position(|e| e.same_key(&entry)) {
            let old = self.entries.remove(pos);
            entry.use_count = old.use_count.saturating_add(entry.use_count.max(1));
        }
        self.entries.insert(0, entry);
        self.entries.truncate(MAX_PATTERN_HISTORY);
        true
    }

    /// Returns up to `limit` suggestions for `query`.
    /// Empty query yields the most recent entries. Otherwise case-insensitive
    /// prefix matches rank before substring matches; ties break by `last_used`
    /// descending (stable, so MRU order is preserved). Entries whose pattern is
    /// exactly equal to `query` are excluded.
    pub fn suggest(&self, query: &str, limit: usize) -> Vec<&PatternHistoryEntry> {
        if query.is_empty() {
            return self.entries.iter().take(limit).collect();
        }
        let q = query.to_lowercase();
        let mut scored: Vec<(u8, &PatternHistoryEntry)> = self
            .entries
            .iter()
            .filter(|e| e.pattern != query)
            .filter_map(|e| {
                let p = e.pattern.to_lowercase();
                if p.starts_with(&q) {
                    Some((0, e))
                } else if p.contains(&q) {
                    Some((1, e))
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by_key(|(rank, e)| (*rank, std::cmp::Reverse(e.last_used)));
        scored.into_iter().take(limit).map(|(_, e)| e).collect()
    }

    /// Removes the entry at `idx`, returning it if present.
    pub fn remove(&mut self, idx: usize) -> Option<PatternHistoryEntry> {
        (idx < self.entries.len()).then(|| self.entries.remove(idx))
    }

    /// Removes the entry matching the (pattern, regex, case) key of `target`.
    pub fn remove_entry(&mut self, target: &PatternHistoryEntry) -> bool {
        match self.entries.iter().position(|e| e.same_key(target)) {
            Some(pos) => {
                self.entries.remove(pos);
                true
            }
            None => false,
        }
    }

    /// Removes all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Appends entries from `loaded` that are not already present (in-memory entries win).
    pub fn merge_loaded(&mut self, loaded: PatternHistory) {
        for e in loaded.entries {
            if !self.entries.iter().any(|x| x.same_key(&e)) {
                self.entries.push(e);
            }
        }
        self.entries.truncate(MAX_PATTERN_HISTORY);
    }

    /// Loads history from `path`; a missing, unreadable or corrupt file yields an empty history.
    pub fn load_from_path(path: &std::path::Path) -> Self {
        let Ok(data) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(file) = serde_json::from_str::<PatternHistoryFile>(&data) else {
            return Self::default();
        };
        let mut entries = file.entries;
        entries.retain(|e| !e.pattern.trim().is_empty());
        entries.sort_by_key(|e| std::cmp::Reverse(e.last_used));
        let mut out = Self::default();
        for e in entries.into_iter().rev() {
            // Re-insert oldest first so the newest ends up at the front.
            let count = e.use_count;
            if out.record(e.clone()) {
                if let Some(first) = out.entries.first_mut() {
                    first.use_count = count.max(1);
                }
            }
        }
        out
    }

    /// Atomically persists the history to `path` as pretty JSON.
    pub fn save_to_path(&self, path: &std::path::Path) -> std::io::Result<()> {
        let file = PatternHistoryFile {
            version: PATTERN_HISTORY_VERSION,
            entries: self.entries.clone(),
        };
        let json = serde_json::to_vec_pretty(&file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        crate::workspace::filters::write_file_atomically(path, &json)
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SuggestionKeyAction {
    None,
    OpenAndSelect(usize),
    Select(Option<usize>),
    Apply(usize),
    Close,
    Delete(usize),
}

fn resolve_suggestion_key_action(
    modal_open: bool,
    suggestions_open: bool,
    suggestion_count: usize,
    sel: Option<usize>,
    key: &str,
    shift: bool,
) -> SuggestionKeyAction {
    if !modal_open {
        return SuggestionKeyAction::None;
    }
    let open = suggestions_open && suggestion_count > 0;
    match key {
        "down" => {
            if suggestion_count == 0 {
                return SuggestionKeyAction::None;
            }
            let next = match (open, sel) {
                (true, Some(i)) => (i + 1).min(suggestion_count - 1),
                _ => 0,
            };
            SuggestionKeyAction::OpenAndSelect(next)
        }
        "up" if open => {
            let next = match sel {
                Some(i) if i > 0 => Some(i - 1),
                _ => None,
            };
            SuggestionKeyAction::Select(next)
        }
        "enter" if open && sel.is_some() => SuggestionKeyAction::Apply(sel.unwrap_or(0)),
        "tab" if open && !shift => SuggestionKeyAction::Apply(sel.unwrap_or(0)),
        "escape" if open => SuggestionKeyAction::Close,
        "delete" if open && shift && sel.is_some() => SuggestionKeyAction::Delete(sel.unwrap_or(0)),
        _ => SuggestionKeyAction::None,
    }
}

impl LogAnalysisWorkspace {
    /// Loads persisted pattern history on a background thread and merges it into memory.
    pub fn load_pattern_history_async(&mut self, cx: &mut Context<Self>) {
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();
            async move {
                let loaded = bg_executor
                    .spawn(async move {
                        super::recent_files::config_file_path("pattern_history.json")
                            .map(|p| PatternHistory::load_from_path(&p))
                            .unwrap_or_default()
                    })
                    .await;
                let _ = this.update(&mut async_cx, |workspace, cx| {
                    workspace.pattern_history.merge_loaded(loaded);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Persists the current history snapshot on a background thread (errors ignored).
    pub(crate) fn save_pattern_history_async(&self, cx: &mut Context<Self>) {
        let snapshot = self.pattern_history.clone();
        cx.spawn(move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            async move {
                bg_executor
                    .spawn(async move {
                        if let Some(path) =
                            super::recent_files::config_file_path("pattern_history.json")
                        {
                            let _ = snapshot.save_to_path(&path);
                        }
                    })
                    .await;
            }
        })
        .detach();
    }

    /// Records a successfully saved filter pattern and persists asynchronously.
    pub(crate) fn record_pattern_history(
        &mut self,
        pattern: &str,
        regex: bool,
        case_sensitive: bool,
        cx: &mut Context<Self>,
    ) {
        let entry = PatternHistoryEntry::new(pattern, regex, case_sensitive, now_secs());
        if self.pattern_history.record(entry) {
            self.save_pattern_history_async(cx);
        }
    }

    /// Clears all remembered patterns (Command Palette: `filter:clear_pattern_history`).
    pub fn clear_pattern_history(&mut self, cx: &mut Context<Self>) {
        self.pattern_history.clear();
        self.filter_edit_modal.suggestions_open = false;
        self.filter_edit_modal.suggestion_index = None;
        self.save_pattern_history_async(cx);
        self.status_text = "Cleared filter pattern history".into();
        cx.notify();
    }

    /// Current dropdown candidates for the modal pattern text.
    pub(crate) fn current_pattern_suggestions(&self, cx: &App) -> Vec<PatternHistoryEntry> {
        let text = self.modal_pattern_input.read(cx).text();
        self.pattern_history
            .suggest(&text, MAX_PATTERN_SUGGESTIONS)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Applies suggestion `idx`: restores pattern text plus Regex/Case toggles (not colors).
    pub(crate) fn apply_pattern_suggestion(&mut self, idx: usize, cx: &mut Context<Self>) {
        let suggestions = self.current_pattern_suggestions(cx);
        let Some(entry) = suggestions.get(idx) else {
            return;
        };
        self.filter_edit_modal.is_regex = entry.regex;
        self.filter_edit_modal.is_case_sensitive = entry.case_sensitive;
        self.filter_edit_modal.suggestions_open = false;
        self.filter_edit_modal.suggestion_index = None;
        self.filter_edit_modal.programmatic_text = Some(entry.pattern.clone());
        let text = entry.pattern.clone();
        self.modal_pattern_input
            .update(cx, |input, cx| input.set_text(&text, cx));
        cx.notify();
    }

    /// Deletes the `idx`-th visible suggestion from history.
    pub(crate) fn delete_pattern_suggestion(&mut self, idx: usize, cx: &mut Context<Self>) {
        let suggestions = self.current_pattern_suggestions(cx);
        if let Some(entry) = suggestions.get(idx) {
            if self.pattern_history.remove_entry(entry) {
                self.save_pattern_history_async(cx);
            }
            let remaining = suggestions.len() - 1;
            self.filter_edit_modal.suggestion_index = if remaining == 0 {
                None
            } else {
                self.filter_edit_modal
                    .suggestion_index
                    .map(|i| i.min(remaining - 1))
            };
            cx.notify();
        }
    }

    /// Reacts to text changes of the modal pattern input: reopens the dropdown on user typing.
    pub(crate) fn on_modal_pattern_changed(&mut self, cx: &mut Context<Self>) {
        if !self.filter_edit_modal.is_open {
            return;
        }
        let text = self.modal_pattern_input.read(cx).text();
        if self.filter_edit_modal.programmatic_text.as_deref() == Some(text.as_str()) {
            return;
        }
        self.filter_edit_modal.programmatic_text = None;
        self.filter_edit_modal.suggestions_open = true;
        self.filter_edit_modal.suggestion_index = None;
        cx.notify();
    }

    /// Dropdown-first key handling for the filter modal. Returns `true` if consumed.
    /// Enter/Esc/Tab/Up/Down are consumed only while suggestions are visible
    /// (Enter additionally requires a highlighted row, so plain Enter still saves).
    pub(crate) fn handle_suggestion_key(
        &mut self,
        key: &str,
        shift: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.filter_edit_modal.is_open {
            return false;
        }
        let suggestions = self.current_pattern_suggestions(cx);
        let action = resolve_suggestion_key_action(
            self.filter_edit_modal.is_open,
            self.filter_edit_modal.suggestions_open,
            suggestions.len(),
            self.filter_edit_modal.suggestion_index,
            key,
            shift,
        );
        match action {
            SuggestionKeyAction::None => false,
            SuggestionKeyAction::OpenAndSelect(idx) => {
                self.filter_edit_modal.suggestions_open = true;
                self.filter_edit_modal.suggestion_index = Some(idx);
                cx.notify();
                true
            }
            SuggestionKeyAction::Select(sel) => {
                self.filter_edit_modal.suggestion_index = sel;
                cx.notify();
                true
            }
            SuggestionKeyAction::Apply(idx) => {
                self.apply_pattern_suggestion(idx, cx);
                true
            }
            SuggestionKeyAction::Close => {
                self.filter_edit_modal.suggestions_open = false;
                self.filter_edit_modal.suggestion_index = None;
                cx.notify();
                true
            }
            SuggestionKeyAction::Delete(idx) => {
                self.delete_pattern_suggestion(idx, cx);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Explicit imports: `super::*` would pull in `gpui::test`, shadowing the std `#[test]` attribute.
    use super::{
        resolve_suggestion_key_action, PatternHistory, PatternHistoryEntry, PatternHistoryFile,
        SuggestionKeyAction, MAX_PATTERN_HISTORY,
    };

    fn e(p: &str, t: u64) -> PatternHistoryEntry {
        PatternHistoryEntry::new(p, false, false, t)
    }

    #[test]
    fn record_dedups_and_moves_to_front() {
        let mut h = PatternHistory::default();
        h.record(e("a", 1));
        h.record(e("b", 2));
        h.record(e("a", 3));
        assert_eq!(h.entries.len(), 2);
        assert_eq!(h.entries[0].pattern, "a");
        assert_eq!(h.entries[0].use_count, 2);
        assert_eq!(h.entries[0].last_used, 3);
        assert_eq!(h.entries[1].pattern, "b");
    }

    #[test]
    fn different_flags_are_distinct() {
        let mut h = PatternHistory::default();
        h.record(PatternHistoryEntry::new("x", true, false, 1));
        h.record(PatternHistoryEntry::new("x", false, false, 2));
        h.record(PatternHistoryEntry::new("x", false, true, 3));
        assert_eq!(h.entries.len(), 3);
    }

    #[test]
    fn record_rejects_blank() {
        let mut h = PatternHistory::default();
        assert!(!h.record(e("", 1)));
        assert!(!h.record(e("   ", 1)));
        assert!(h.entries.is_empty());
    }

    #[test]
    fn cap_is_enforced_dropping_oldest() {
        let mut h = PatternHistory::default();
        for i in 0..(MAX_PATTERN_HISTORY + 10) {
            h.record(e(&format!("p{i}"), i as u64));
        }
        assert_eq!(h.entries.len(), MAX_PATTERN_HISTORY);
        assert_eq!(
            h.entries[0].pattern,
            format!("p{}", MAX_PATTERN_HISTORY + 9)
        );
        assert!(!h.entries.iter().any(|x| x.pattern == "p0"));
    }

    #[test]
    fn suggest_empty_query_returns_recent_limited() {
        let mut h = PatternHistory::default();
        for i in 0..12 {
            h.record(e(&format!("p{i}"), i));
        }
        let s = h.suggest("", 8);
        assert_eq!(s.len(), 8);
        assert_eq!(s[0].pattern, "p11");
    }

    #[test]
    fn suggest_ranks_prefix_before_substring_then_recency() {
        let mut h = PatternHistory::default();
        h.record(e("my error", 1));
        h.record(e("ERROR old", 2));
        h.record(e("error new", 3));
        h.record(e("unrelated", 4));
        let s: Vec<&str> = h
            .suggest("err", 8)
            .iter()
            .map(|x| x.pattern.as_str())
            .collect();
        assert_eq!(s, vec!["error new", "ERROR old", "my error"]);
    }

    #[test]
    fn suggest_excludes_exact_current_text() {
        let mut h = PatternHistory::default();
        h.record(e("error", 1));
        h.record(e("error2", 2));
        let s: Vec<&str> = h
            .suggest("error", 8)
            .iter()
            .map(|x| x.pattern.as_str())
            .collect();
        assert_eq!(s, vec!["error2"]);
    }

    #[test]
    fn remove_and_clear() {
        let mut h = PatternHistory::default();
        h.record(e("a", 1));
        h.record(e("b", 2));
        assert_eq!(h.remove(0).map(|x| x.pattern), Some("b".to_string()));
        assert!(h.remove(5).is_none());
        h.clear();
        assert!(h.entries.is_empty());
    }

    #[test]
    fn merge_loaded_keeps_in_memory_first() {
        let mut h = PatternHistory::default();
        h.record(e("new", 10));
        let mut loaded = PatternHistory::default();
        loaded.record(e("new", 1));
        loaded.record(e("old", 2));
        h.merge_loaded(loaded);
        let p: Vec<&str> = h.entries.iter().map(|x| x.pattern.as_str()).collect();
        assert_eq!(p, vec!["new", "old"]);
    }

    #[test]
    fn save_and_load_round_trip_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/pattern_history.json");
        let mut h = PatternHistory::default();
        h.record(PatternHistoryEntry::new("ERROR|FATAL", true, false, 100));
        h.record(e("warn", 200));
        h.record(e("warn", 300));
        h.save_to_path(&path).unwrap();
        let loaded = PatternHistory::load_from_path(&path);
        assert_eq!(loaded, h);
        // no temp artifacts left behind
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"version\": 1"));
    }

    #[test]
    fn corrupt_or_missing_file_falls_back_to_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pattern_history.json");
        assert!(PatternHistory::load_from_path(&path).entries.is_empty());
        std::fs::write(&path, b"{ not json").unwrap();
        assert!(PatternHistory::load_from_path(&path).entries.is_empty());
        std::fs::write(&path, b"[1,2,3]").unwrap();
        assert!(PatternHistory::load_from_path(&path).entries.is_empty());
    }

    #[test]
    fn load_sorts_by_recency_and_caps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pattern_history.json");
        let entries: Vec<PatternHistoryEntry> = (0..(MAX_PATTERN_HISTORY + 5))
            .map(|i| e(&format!("p{i}"), i as u64))
            .collect();
        let file = PatternHistoryFile {
            version: 1,
            entries,
        };
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let h = PatternHistory::load_from_path(&path);
        assert_eq!(h.entries.len(), MAX_PATTERN_HISTORY);
        assert_eq!(
            h.entries[0].pattern,
            format!("p{}", MAX_PATTERN_HISTORY + 4)
        );
    }

    #[test]
    fn resolve_suggestion_key_action_respects_shift_tab_and_navigation() {
        // Modal closed -> never consumes keys
        assert_eq!(
            resolve_suggestion_key_action(false, true, 3, Some(0), "tab", false),
            SuggestionKeyAction::None
        );

        // Plain Tab when suggestions are open consumes and applies highlighted or first suggestion
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, None, "tab", false),
            SuggestionKeyAction::Apply(0)
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, Some(2), "tab", false),
            SuggestionKeyAction::Apply(2)
        );

        // Shift+Tab must NOT be consumed by suggestion dropdown so modal focus cycles backward
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, None, "tab", true),
            SuggestionKeyAction::None
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, Some(1), "tab", true),
            SuggestionKeyAction::None
        );

        // Closed or empty dropdown does not consume Tab
        assert_eq!(
            resolve_suggestion_key_action(true, false, 3, None, "tab", false),
            SuggestionKeyAction::None
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 0, None, "tab", false),
            SuggestionKeyAction::None
        );

        // Down / Up / Enter / Escape / Shift+Delete navigation
        assert_eq!(
            resolve_suggestion_key_action(true, false, 0, None, "down", false),
            SuggestionKeyAction::None
        );
        assert_eq!(
            resolve_suggestion_key_action(true, false, 3, None, "down", false),
            SuggestionKeyAction::OpenAndSelect(0)
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, Some(1), "down", false),
            SuggestionKeyAction::OpenAndSelect(2)
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, Some(2), "down", false),
            SuggestionKeyAction::OpenAndSelect(2)
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, Some(1), "up", false),
            SuggestionKeyAction::Select(Some(0))
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, Some(0), "up", false),
            SuggestionKeyAction::Select(None)
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, None, "enter", false),
            SuggestionKeyAction::None
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, Some(1), "enter", false),
            SuggestionKeyAction::Apply(1)
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, Some(1), "escape", false),
            SuggestionKeyAction::Close
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, Some(1), "delete", false),
            SuggestionKeyAction::None
        );
        assert_eq!(
            resolve_suggestion_key_action(true, true, 3, Some(1), "delete", true),
            SuggestionKeyAction::Delete(1)
        );
    }
}

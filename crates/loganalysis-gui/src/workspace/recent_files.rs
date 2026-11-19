use gpui::*;

use super::LogAnalysisWorkspace;

/// Inserts `path` at the front of `recent_files`, deduplicating and capping at 5 entries.
pub fn push_recent_path(recent_files: &mut Vec<std::path::PathBuf>, path: std::path::PathBuf) {
    recent_files.retain(|p| p != &path);
    recent_files.insert(0, path);
    recent_files.truncate(5);
}

impl LogAnalysisWorkspace {
    /// Asynchronously loads persisted recent file paths on startup without blocking the UI thread.
    pub fn load_recent_files_async(&mut self, cx: &mut Context<Self>) {
        let load_gen = self.recent_files_gen;
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();

            async move {
                let loaded = bg_executor.spawn(async move { load_recent_files() }).await;
                let _ = this.update(&mut async_cx, |workspace, cx| {
                    if workspace.recent_files_gen == load_gen {
                        workspace.recent_files = loaded;
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    /// Records a file in recent history while offloading canonicalization and persistence to a background thread.
    pub fn add_recent_file_async(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        self.recent_files_gen = self.recent_files_gen.wrapping_add(1);
        push_recent_path(&mut self.recent_files, path.clone());
        cx.notify();

        let raw_path = path.clone();
        let update_gen = self.recent_files_gen;
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();

            async move {
                let canonical = bg_executor
                    .spawn(async move { std::fs::canonicalize(&path).unwrap_or(path) })
                    .await;

                let mut snapshot_to_save: Option<Vec<std::path::PathBuf>> = None;
                let _ = this.update(&mut async_cx, |workspace, cx| {
                    if workspace.recent_files_gen != update_gen {
                        return;
                    }
                    workspace
                        .recent_files
                        .retain(|p| p != &raw_path && p != &canonical);
                    push_recent_path(&mut workspace.recent_files, canonical);
                    snapshot_to_save = Some(workspace.recent_files.clone());
                    cx.notify();
                });

                if let Some(files) = snapshot_to_save {
                    bg_executor
                        .spawn(async move {
                            save_recent_files(&files);
                        })
                        .await;
                }
            }
        })
        .detach();
    }

    /// Clears the recent files list immediately and persists the empty list asynchronously.
    pub fn clear_recent_files_async(&mut self, cx: &mut Context<Self>) {
        self.recent_files_gen = self.recent_files_gen.wrapping_add(1);
        self.recent_files.clear();
        cx.notify();

        cx.spawn(move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            async move {
                bg_executor
                    .spawn(async move {
                        save_recent_files(&[]);
                    })
                    .await;
            }
        })
        .detach();
    }
}

pub(crate) fn get_recent_files_path() -> Option<std::path::PathBuf> {
    config_file_path("recent_files.json")
}

/// Resolves `<config dir>/loganalysis/<name>` using XDG_CONFIG_HOME, HOME, then APPDATA.
pub(crate) fn config_file_path(name: &str) -> Option<std::path::PathBuf> {
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        if !config_home.is_empty() {
            return Some(
                std::path::PathBuf::from(config_home)
                    .join("loganalysis")
                    .join(name),
            );
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Some(
                std::path::PathBuf::from(home)
                    .join(".config/loganalysis")
                    .join(name),
            );
        }
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.is_empty() {
            return Some(
                std::path::PathBuf::from(appdata)
                    .join("loganalysis")
                    .join(name),
            );
        }
    }
    None
}

/// Loads up to 5 existing recent file paths from a JSON file at `path`.
/// Nonexistent entries and malformed/unreadable JSON files resolve to an empty/filtered list.
pub fn load_recent_files_from_path(path: &std::path::Path) -> Vec<std::path::PathBuf> {
    if let Ok(data) = std::fs::read_to_string(path) {
        if let Ok(paths) = serde_json::from_str::<Vec<std::path::PathBuf>>(&data) {
            return paths.into_iter().filter(|p| p.exists()).take(5).collect();
        }
    }
    Vec::new()
}

/// Atomically persists `files` as pretty JSON to `path` using `write_file_atomically`.
pub fn save_recent_files_to_path(
    path: &std::path::Path,
    files: &[std::path::PathBuf],
) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(files)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    crate::workspace::filters::write_file_atomically(path, &json)
}

pub(crate) fn load_recent_files() -> Vec<std::path::PathBuf> {
    get_recent_files_path()
        .map(|path| load_recent_files_from_path(&path))
        .unwrap_or_default()
}

pub(crate) fn save_recent_files(files: &[std::path::PathBuf]) {
    if let Some(path) = get_recent_files_path() {
        let _ = save_recent_files_to_path(&path, files);
    }
}

#[cfg(test)]
mod tests {
    use super::{load_recent_files_from_path, push_recent_path, save_recent_files_to_path};
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_push_recent_path_lru_dedup_and_cap() {
        let mut recent = Vec::new();
        for i in 1..=6 {
            push_recent_path(&mut recent, PathBuf::from(format!("/tmp/file{i}.log")));
        }
        assert_eq!(recent.len(), 5);
        assert_eq!(recent[0], PathBuf::from("/tmp/file6.log"));
        assert_eq!(recent[4], PathBuf::from("/tmp/file2.log"));

        push_recent_path(&mut recent, PathBuf::from("/tmp/file3.log"));
        assert_eq!(recent.len(), 5);
        assert_eq!(recent[0], PathBuf::from("/tmp/file3.log"));
    }

    #[test]
    fn test_save_and_load_recent_files_from_path_with_tempfile() {
        let dir = TempDir::new().expect("create temp dir");
        let existing_a = dir.path().join("a.log");
        let existing_b = dir.path().join("b.log");
        let missing = dir.path().join("missing.log");
        std::fs::write(&existing_a, b"a").unwrap();
        std::fs::write(&existing_b, b"b").unwrap();

        let store_path = dir.path().join("nested/recent_files.json");
        save_recent_files_to_path(
            &store_path,
            &[existing_a.clone(), missing, existing_b.clone()],
        )
        .expect("atomic save should succeed");

        let loaded = load_recent_files_from_path(&store_path);
        assert_eq!(loaded, vec![existing_a, existing_b]);

        std::fs::write(&store_path, b"{not-valid-json").unwrap();
        assert!(load_recent_files_from_path(&store_path).is_empty());
    }
}

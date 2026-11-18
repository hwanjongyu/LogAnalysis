use gpui::*;

use super::LogAnalysisWorkspace;

/// Inserts `path` at the front of `recent_files`, deduplicating and capping at 5 entries.
pub(crate) fn push_recent_path(
    recent_files: &mut Vec<std::path::PathBuf>,
    path: std::path::PathBuf,
) {
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
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        if !config_home.is_empty() {
            return Some(std::path::PathBuf::from(config_home).join("loganalysis/recent_files.json"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Some(std::path::PathBuf::from(home).join(".config/loganalysis/recent_files.json"));
        }
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.is_empty() {
            return Some(std::path::PathBuf::from(appdata).join("loganalysis/recent_files.json"));
        }
    }
    None
}

pub(crate) fn load_recent_files() -> Vec<std::path::PathBuf> {
    if let Some(path) = get_recent_files_path() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(paths) = serde_json::from_str::<Vec<std::path::PathBuf>>(&data) {
                return paths.into_iter().filter(|p| p.exists()).take(5).collect();
            }
        }
    }
    Vec::new()
}

pub(crate) fn save_recent_files(files: &[std::path::PathBuf]) {
    if let Some(path) = get_recent_files_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(files) {
            let tmp = crate::workspace::filters::build_unique_temp_path(&path);
            if std::fs::write(&tmp, json).is_ok() {
                let _ = crate::workspace::filters::atomic_replace_file(&tmp, &path);
                if tmp.exists() {
                    let _ = std::fs::remove_file(&tmp);
                }
            } else {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::push_recent_path;
    use std::path::PathBuf;

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
}

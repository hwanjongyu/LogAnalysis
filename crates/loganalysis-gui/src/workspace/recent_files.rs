use gpui::*;

use super::LogAnalysisWorkspace;

/// Maximum on-disk `recent_files.json` size (1 MiB).
pub const MAX_RECENT_FILES_BYTES: usize = 1024 * 1024;

static RECENT_FILES_SAVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static RECENT_FILES_SAVE_GEN: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

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

                let mut snapshot_to_save: Option<(Vec<std::path::PathBuf>, u64)> = None;
                let _ = this.update(&mut async_cx, |workspace, cx| {
                    if workspace.recent_files_gen != update_gen {
                        return;
                    }
                    workspace
                        .recent_files
                        .retain(|p| p != &raw_path && p != &canonical);
                    push_recent_path(&mut workspace.recent_files, canonical);
                    let save_gen = RECENT_FILES_SAVE_GEN
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                        + 1;
                    snapshot_to_save = Some((workspace.recent_files.clone(), save_gen));
                    cx.notify();
                });

                if let Some((files, save_gen)) = snapshot_to_save {
                    bg_executor
                        .spawn(async move {
                            save_recent_files_serialized(&files, save_gen);
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
        let save_gen =
            RECENT_FILES_SAVE_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        cx.notify();

        cx.spawn(move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            async move {
                bg_executor
                    .spawn(async move {
                        save_recent_files_serialized(&[], save_gen);
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

/// Resolves `<config dir>/loganalysis/<name>` with OS-appropriate environment variable precedence.
pub(crate) fn config_file_path(name: &str) -> Option<std::path::PathBuf> {
    config_file_path_from_env_for_os(name, cfg!(windows), |k| std::env::var(k).ok())
}

pub(crate) fn config_file_path_from_env_for_os<F>(
    name: &str,
    is_windows: bool,
    get_env: F,
) -> Option<std::path::PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    let non_empty = |key: &str| get_env(key).filter(|s| !s.is_empty());
    if is_windows {
        if let Some(appdata) = non_empty("APPDATA") {
            Some(std::path::PathBuf::from(appdata).join("loganalysis").join(name))
        } else if let Some(local_appdata) = non_empty("LOCALAPPDATA") {
            Some(
                std::path::PathBuf::from(local_appdata)
                    .join("loganalysis")
                    .join(name),
            )
        } else if let Some(config_home) = non_empty("XDG_CONFIG_HOME") {
            Some(
                std::path::PathBuf::from(config_home)
                    .join("loganalysis")
                    .join(name),
            )
        } else {
            non_empty("HOME").map(|home| {
                std::path::PathBuf::from(home)
                    .join(".config")
                    .join("loganalysis")
                    .join(name)
            })
        }
    } else if let Some(config_home) = non_empty("XDG_CONFIG_HOME") {
        Some(
            std::path::PathBuf::from(config_home)
                .join("loganalysis")
                .join(name),
        )
    } else if let Some(home) = non_empty("HOME") {
        Some(
            std::path::PathBuf::from(home)
                .join(".config")
                .join("loganalysis")
                .join(name),
        )
    } else {
        non_empty("APPDATA")
            .map(|appdata| std::path::PathBuf::from(appdata).join("loganalysis").join(name))
    }
}

/// Loads up to 5 existing recent file paths from a JSON file at `path`.
/// Nonexistent entries and malformed/unreadable/oversized JSON files resolve to an empty/filtered list.
pub fn load_recent_files_from_path(path: &std::path::Path) -> Vec<std::path::PathBuf> {
    if let Ok(data) = super::pattern_history::read_to_string_bounded(path, MAX_RECENT_FILES_BYTES) {
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

fn save_recent_files_to_path_serialized_with(
    path: &std::path::Path,
    files: &[std::path::PathBuf],
    save_gen: u64,
    lock: &std::sync::Mutex<()>,
    current_gen: &std::sync::atomic::AtomicU64,
) -> std::io::Result<bool> {
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    if current_gen.load(std::sync::atomic::Ordering::SeqCst) != save_gen {
        return Ok(false);
    }
    save_recent_files_to_path(path, files)?;
    Ok(true)
}

pub(crate) fn save_recent_files_serialized(files: &[std::path::PathBuf], save_gen: u64) {
    if let Some(path) = get_recent_files_path() {
        let _ = save_recent_files_to_path_serialized_with(
            &path,
            files,
            save_gen,
            &RECENT_FILES_SAVE_LOCK,
            &RECENT_FILES_SAVE_GEN,
        );
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

    #[test]
    fn test_load_recent_files_rejects_oversized_and_serialized_save_skips_stale_gen() {
        use super::{
            save_recent_files_to_path_serialized_with, MAX_RECENT_FILES_BYTES,
        };
        use std::sync::atomic::AtomicU64;
        use std::sync::Mutex;

        let dir = TempDir::new().expect("create temp dir");
        let oversized_path = dir.path().join("oversized_recent.json");
        let oversized_bytes = vec![b' '; MAX_RECENT_FILES_BYTES + 1];
        std::fs::write(&oversized_path, &oversized_bytes).unwrap();
        assert!(load_recent_files_from_path(&oversized_path).is_empty());

        let save_path = dir.path().join("recent.json");
        let entry = dir.path().join("kept.log");
        std::fs::write(&entry, b"x").unwrap();

        let lock = Mutex::new(());
        let gen = AtomicU64::new(3);
        assert!(!save_recent_files_to_path_serialized_with(
            &save_path,
            std::slice::from_ref(&entry),
            2,
            &lock,
            &gen,
        )
        .unwrap());
        assert!(!save_path.exists());

        assert!(save_recent_files_to_path_serialized_with(
            &save_path,
            std::slice::from_ref(&entry),
            3,
            &lock,
            &gen,
        )
        .unwrap());
        assert_eq!(load_recent_files_from_path(&save_path), vec![entry]);
    }

    #[test]
    fn test_config_file_path_from_env_for_os_precedence() {
        use super::config_file_path_from_env_for_os;
        use std::path::PathBuf;

        // Unix: XDG_CONFIG_HOME > HOME > APPDATA
        let p = config_file_path_from_env_for_os("recent.json", false, |k| match k {
            "XDG_CONFIG_HOME" => Some("/xdg".into()),
            "HOME" => Some("/home/u".into()),
            "APPDATA" => Some("C:\\AppData".into()),
            _ => None,
        });
        assert_eq!(
            p,
            Some(PathBuf::from("/xdg").join("loganalysis").join("recent.json"))
        );

        let p = config_file_path_from_env_for_os("recent.json", false, |k| match k {
            "HOME" => Some("/home/u".into()),
            "APPDATA" => Some("C:\\AppData".into()),
            _ => None,
        });
        assert_eq!(
            p,
            Some(
                PathBuf::from("/home/u")
                    .join(".config")
                    .join("loganalysis")
                    .join("recent.json")
            )
        );

        // Windows: APPDATA > LOCALAPPDATA > XDG_CONFIG_HOME > HOME
        let p = config_file_path_from_env_for_os("recent.json", true, |k| match k {
            "APPDATA" => Some("C:\\Users\\u\\AppData\\Roaming".into()),
            "LOCALAPPDATA" => Some("C:\\Users\\u\\AppData\\Local".into()),
            "XDG_CONFIG_HOME" => Some("/xdg".into()),
            "HOME" => Some("/home/u".into()),
            _ => None,
        });
        assert_eq!(
            p,
            Some(
                PathBuf::from("C:\\Users\\u\\AppData\\Roaming")
                    .join("loganalysis")
                    .join("recent.json")
            )
        );

        let p = config_file_path_from_env_for_os("recent.json", true, |k| match k {
            "LOCALAPPDATA" => Some("C:\\Users\\u\\AppData\\Local".into()),
            "HOME" => Some("/home/u".into()),
            _ => None,
        });
        assert_eq!(
            p,
            Some(
                PathBuf::from("C:\\Users\\u\\AppData\\Local")
                    .join("loganalysis")
                    .join("recent.json")
            )
        );

        assert_eq!(
            config_file_path_from_env_for_os("recent.json", false, |_| None),
            None
        );
        assert_eq!(
            config_file_path_from_env_for_os("recent.json", true, |_| None),
            None
        );
    }
}

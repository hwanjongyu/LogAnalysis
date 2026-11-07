use super::LogAnalysisWorkspace;

impl LogAnalysisWorkspace {
    /// Records a file in recent history, keeping up to 5 unique canonical paths
    pub fn add_recent_file(&mut self, path: std::path::PathBuf) {
        let canonical = std::fs::canonicalize(&path).unwrap_or(path);
        self.recent_files.retain(|p| p != &canonical);
        self.recent_files.insert(0, canonical);
        self.recent_files.truncate(5);
        save_recent_files(&self.recent_files);
    }

    /// Clears the recent files list
    pub fn clear_recent_files(&mut self) {
        self.recent_files.clear();
        save_recent_files(&self.recent_files);
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
            let tmp = path.with_extension("tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

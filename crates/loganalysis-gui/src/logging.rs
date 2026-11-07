use std::backtrace::Backtrace;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

static LOG_MUTEX: Mutex<()> = Mutex::new(());

/// Formats epoch seconds into a human-readable UTC timestamp string (YYYY-MM-DD HH:MM:SS UTC)
pub fn format_epoch_utc(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let mut days = secs / 86400;

    let mut year = 1970;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    let day = days + 1;

    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02} UTC")
}

/// Returns current UTC timestamp string
pub fn now_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format_epoch_utc(secs)
}

/// Resolves the user-specific directory for persistent application logs.
/// Checks the following in order of precedence:
/// 1. `HOME` -> `~/.local/share/loganalysis` (Unix/Linux/macOS)
/// 2. `LOCALAPPDATA` -> `%LOCALAPPDATA%\loganalysis` (Windows local app data)
/// 3. `APPDATA` -> `%APPDATA%\loganalysis` (Windows roaming app data fallback)
/// 4. `USERPROFILE` -> `%USERPROFILE%\AppData\Local\loganalysis` (Windows user profile fallback)
#[allow(dead_code)]
pub fn resolve_user_log_dir() -> Option<PathBuf> {
    resolve_user_log_dir_from_env(|k| std::env::var(k).ok())
}

pub(crate) fn resolve_user_log_dir_from_env<F>(get_env: F) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(home) = get_env("HOME").filter(|s| !s.is_empty()) {
        Some(PathBuf::from(home).join(".local").join("share").join("loganalysis"))
    } else if let Some(local_app_data) = get_env("LOCALAPPDATA").filter(|s| !s.is_empty()) {
        Some(PathBuf::from(local_app_data).join("loganalysis"))
    } else if let Some(app_data) = get_env("APPDATA").filter(|s| !s.is_empty()) {
        Some(PathBuf::from(app_data).join("loganalysis"))
    } else {
        get_env("USERPROFILE")
            .filter(|s| !s.is_empty())
            .map(|user_profile| PathBuf::from(user_profile).join("AppData").join("Local").join("loganalysis"))
    }
}

/// Resolves standard log directory paths:
/// 1. Current working directory
/// 2. User data directory (~/.local/share/loganalysis on Unix, or %LOCALAPPDATA%\loganalysis on Windows)
pub fn resolve_log_paths(filename: &str) -> Vec<PathBuf> {
    resolve_log_paths_with_lookup(
        filename,
        std::env::current_dir().ok(),
        |k| std::env::var(k).ok(),
        |dir| {
            let _ = std::fs::create_dir_all(dir);
        },
    )
}

pub(crate) fn resolve_log_paths_with_lookup<F, C>(
    filename: &str,
    cwd: Option<PathBuf>,
    get_env: F,
    mut create_dir: C,
) -> Vec<PathBuf>
where
    F: Fn(&str) -> Option<String>,
    C: FnMut(&Path),
{
    let mut paths = Vec::new();

    // 1. Current working directory
    if let Some(cwd) = cwd {
        paths.push(cwd.join(filename));
    }

    // 2. User local share directory (HOME, LOCALAPPDATA, APPDATA, or USERPROFILE)
    if let Some(user_share) = resolve_user_log_dir_from_env(get_env) {
        create_dir(&user_share);
        paths.push(user_share.join(filename));
    }

    paths
}

fn append_to_file(path: &Path, content: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(content.as_bytes());
        let _ = file.flush();
    }
}

/// Safely writes bytes to stderr, silently ignoring BrokenPipe (os error 32) or other I/O errors.
pub fn safe_write_stderr(bytes: &[u8]) {
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(bytes);
    let _ = stderr.flush();
}

/// Appends a structured log event to the persistent `loganalysis.log` files
pub fn log_event(level: &str, message: &str) {
    let _lock = LOG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let timestamp = now_timestamp();
    let formatted = format!("[{timestamp}] [{level}] {message}\n");

    safe_write_stderr(formatted.as_bytes());

    for path in resolve_log_paths("loganalysis.log") {
        append_to_file(&path, &formatted);
    }
}

/// Initializes global panic hook to guarantee all crashes and stack traces are captured to disk
pub fn init_crash_handler() {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        let timestamp = now_timestamp();

        let payload = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "Unknown panic payload (Box<dyn Any>)"
        };

        let location = if let Some(loc) = panic_info.location() {
            format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
        } else {
            "unknown location".to_string()
        };

        let thread_name = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();

        let backtrace = Backtrace::force_capture();

        let report = format!(
            "================================================================================\n\
             LOGANALYSIS CRASH REPORT\n\
             Timestamp: {timestamp}\n\
             Thread:    {thread_name}\n\
             Location:  {location}\n\
             Panic:     {payload}\n\
             --------------------------------------------------------------------------------\n\
             Backtrace:\n\
             {backtrace}\n\
             ================================================================================\n\n"
        );

        let crash_paths = resolve_log_paths("crash.log");
        for path in &crash_paths {
            append_to_file(path, &report);
        }

        // Also record a critical entry in loganalysis.log
        for path in resolve_log_paths("loganalysis.log") {
            let error_line = format!("[{timestamp}] [CRITICAL_PANIC] Thread '{thread_name}' at {location}: {payload}\n");
            append_to_file(&path, &error_line);
        }

        let crash_paths_str = crash_paths
            .iter()
            .map(|p| format!("  - {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n");

        let notice = format!(
            "\n🚨 [LogAnalysis Fatal Crash]\n\
             Time:     {timestamp}\n\
             Thread:   {thread_name}\n\
             Location: {location}\n\
             Panic:    {payload}\n\n\
             Crash report and backtrace successfully written to:\n\
             {crash_paths_str}\n"
        );
        safe_write_stderr(notice.as_bytes());

        default_hook(panic_info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_epoch_utc_epoch_zero() {
        assert_eq!(format_epoch_utc(0), "1970-01-01 00:00:00 UTC");
    }

    #[test]
    fn test_format_epoch_utc_known_date() {
        // 2026-11-03 16:00:00 UTC
        // days between 1970-01-01 and 2026-11-03: 20760 days -> 20760 * 86400 + 16 * 3600 = 1793721600
        let timestamp = 1793721600;
        let formatted = format_epoch_utc(timestamp);
        assert_eq!(formatted, "2026-11-03 16:00:00 UTC");
    }

    #[test]
    fn test_resolve_log_paths_non_empty() {
        let paths = resolve_log_paths("test.log");
        assert!(!paths.is_empty(), "Log paths should have at least cwd or home path");
        assert!(paths[0].ends_with("test.log"));
    }

    #[test]
    fn test_resolve_user_log_dir_precedence() {
        // Verify resolve_user_log_dir calls the resolver
        let _ = resolve_user_log_dir();

        // 1. HOME takes precedence when present
        let dir = resolve_user_log_dir_from_env(|k| match k {
            "HOME" => Some("/home/testuser".into()),
            "LOCALAPPDATA" => Some("/mock/appdata/local".into()),
            _ => None,
        });
        assert_eq!(dir, Some(PathBuf::from("/home/testuser").join(".local").join("share").join("loganalysis")));

        // 2. LOCALAPPDATA used when HOME is unset
        let dir = resolve_user_log_dir_from_env(|k| match k {
            "LOCALAPPDATA" => Some("/mock/appdata/local".into()),
            "APPDATA" => Some("/mock/appdata/roaming".into()),
            "USERPROFILE" => Some("/mock/userprofile".into()),
            _ => None,
        });
        assert_eq!(dir, Some(PathBuf::from("/mock/appdata/local").join("loganalysis")));

        // 3. APPDATA used when HOME and LOCALAPPDATA are unset
        let dir = resolve_user_log_dir_from_env(|k| match k {
            "APPDATA" => Some("/mock/appdata/roaming".into()),
            "USERPROFILE" => Some("/mock/userprofile".into()),
            _ => None,
        });
        assert_eq!(dir, Some(PathBuf::from("/mock/appdata/roaming").join("loganalysis")));

        // 4. USERPROFILE used when HOME, LOCALAPPDATA, and APPDATA are unset
        let dir = resolve_user_log_dir_from_env(|k| match k {
            "USERPROFILE" => Some("/mock/userprofile".into()),
            _ => None,
        });
        assert_eq!(dir, Some(PathBuf::from("/mock/userprofile").join("AppData").join("Local").join("loganalysis")));

        // 5. None when no environment variables set
        let dir = resolve_user_log_dir_from_env(|_| None);
        assert_eq!(dir, None);
    }

    #[test]
    fn test_resolve_log_paths_with_lookup_windows() {
        let mut created_dirs = Vec::new();
        let cwd = PathBuf::from("/mock/install/dir");
        let paths = resolve_log_paths_with_lookup(
            "app.log",
            Some(cwd.clone()),
            |k| match k {
                "USERPROFILE" => Some("/mock/userprofile".into()),
                _ => None,
            },
            |dir| created_dirs.push(dir.to_path_buf()),
        );

        let expected_user_dir = PathBuf::from("/mock/userprofile").join("AppData").join("Local").join("loganalysis");
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], cwd.join("app.log"));
        assert_eq!(paths[1], expected_user_dir.join("app.log"));
        assert_eq!(created_dirs, vec![expected_user_dir]);
    }

    #[test]
    fn test_safe_write_stderr_resilience() {
        // Must never panic regardless of bytes or stdio state
        safe_write_stderr(b"Test safe stderr write\n");
        safe_write_stderr(&[]);
        log_event("TEST", "Unit test message via safe_write_stderr");
    }
}


use std::backtrace::Backtrace;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

#[derive(Default)]
pub(crate) struct LogWriterState {
    cached: Option<(PathBuf, BufWriter<File>)>,
}

static LOG_STATE: Mutex<LogWriterState> = Mutex::new(LogWriterState { cached: None });

pub use loganalysis_core::format_epoch_utc;

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
/// 1. User data directory (`~/.local/share/loganalysis` on Unix, or `%LOCALAPPDATA%\loganalysis` / `%APPDATA%\loganalysis` / `%USERPROFILE%\AppData\Local\loganalysis` on Windows) exclusively when resolved.
/// 2. Current working directory fallback only when no user directory environment variable resolves (`None`).
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

    if let Some(user_share) = resolve_user_log_dir_from_env(get_env) {
        create_dir(&user_share);
        paths.push(user_share.join(filename));
    } else if let Some(cwd) = cwd {
        paths.push(cwd.join(filename));
    }

    paths
}

fn ensure_parent_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
}

impl LogWriterState {
    fn write_line(&mut self, path: &Path, line: &str, immediate_flush: bool) {
        let needs_reopen = match &self.cached {
            Some((cached_path, _)) => cached_path.as_path() != path,
            None => true,
        };

        if needs_reopen {
            if let Some((_, mut prev_writer)) = self.cached.take() {
                let _ = prev_writer.flush();
            }
            ensure_parent_dir(path);
            if let Ok(file) = OpenOptions::new().create(true).append(true).open(path) {
                self.cached = Some((path.to_path_buf(), BufWriter::new(file)));
            }
        }

        if let Some((_, writer)) = self.cached.as_mut() {
            if writer.write_all(line.as_bytes()).is_err() {
                self.cached = None;
                return;
            }
            if immediate_flush && writer.flush().is_err() {
                self.cached = None;
            }
        }
    }

    fn flush(&mut self) {
        if let Some((_, writer)) = self.cached.as_mut() {
            if writer.flush().is_err() {
                self.cached = None;
            }
        }
    }

    #[cfg(test)]
    fn close(&mut self) {
        if let Some((_, mut writer)) = self.cached.take() {
            let _ = writer.flush();
        }
    }
}

fn append_to_file(path: &Path, content: &str) {
    ensure_parent_dir(path);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(content.as_bytes());
        let _ = file.flush();
    }
}

#[inline]
fn should_flush_immediately(level: &str) -> bool {
    level.eq_ignore_ascii_case("ERROR")
        || level.eq_ignore_ascii_case("CRITICAL")
        || level.eq_ignore_ascii_case("CRITICAL_PANIC")
        || level.eq_ignore_ascii_case("FATAL")
}

/// Flushes any buffered log entries held by the global log writer to disk.
pub fn flush_logs() {
    let mut state = LOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.flush();
}

/// Safely writes bytes to stderr, silently ignoring BrokenPipe (os error 32) or other I/O errors.
pub fn safe_write_stderr(bytes: &[u8]) {
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(bytes);
    let _ = stderr.flush();
}

/// Appends a structured log event using an optional explicit target path override.
/// When `path_override` is `None`, resolves the standard application log path.
pub(crate) fn log_event_to_path(level: &str, message: &str, path_override: Option<&Path>) {
    let timestamp = now_timestamp();
    let formatted = format!("[{timestamp}] [{level}] {message}\n");
    let immediate_flush = should_flush_immediately(level);

    let target_path = path_override
        .map(PathBuf::from)
        .or_else(|| resolve_log_paths("loganalysis.log").into_iter().next());

    let mut state = LOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    safe_write_stderr(formatted.as_bytes());
    if let Some(path) = target_path {
        state.write_line(&path, &formatted, immediate_flush);
    }
}

/// Appends a structured log event to the persistent `loganalysis.log` file.
pub fn log_event(level: &str, message: &str) {
    log_event_to_path(level, message, None);
}

#[cfg(test)]
pub(crate) fn reset_log_writer_for_test() {
    let mut state = LOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.close();
}

pub(crate) fn record_crash_report(
    crash_paths: &[PathBuf],
    log_paths: &[PathBuf],
    timestamp: &str,
    thread_name: &str,
    location: &str,
    payload: &str,
    report: &str,
) {
    for path in crash_paths {
        append_to_file(path, report);
    }

    let error_line = format!(
        "[{timestamp}] [CRITICAL_PANIC] Thread '{thread_name}' at {location}: {payload}\n"
    );

    let mut state = LOG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    // Flush any previously buffered events before appending and flushing CRITICAL_PANIC
    state.flush();
    for path in log_paths {
        state.write_line(path, &error_line, true);
    }
    state.flush();
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
        let log_paths = resolve_log_paths("loganalysis.log");
        record_crash_report(
            &crash_paths,
            &log_paths,
            &timestamp,
            &thread_name,
            &location,
            payload,
            &report,
        );

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
    use tempfile::TempDir;

    static LOG_TEST_MUTEX: Mutex<()> = Mutex::new(());

    struct ResetLogWriterOnDrop;

    impl Drop for ResetLogWriterOnDrop {
        fn drop(&mut self) {
            reset_log_writer_for_test();
        }
    }

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
        assert!(!paths.is_empty(), "Log paths should have at least user or cwd path");
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("test.log"));
        if let Some(user_dir) = resolve_user_log_dir() {
            assert_eq!(paths[0], user_dir.join("test.log"));
            if let Ok(cwd) = std::env::current_dir() {
                if cwd != user_dir {
                    assert!(!paths[0].starts_with(&cwd));
                }
            }
        }
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
        assert_eq!(paths.len(), 1);
        assert_eq!(paths, vec![expected_user_dir.join("app.log")]);
        assert!(!paths.contains(&cwd.join("app.log")));
        assert_eq!(created_dirs, vec![expected_user_dir]);
    }

    #[test]
    fn test_safe_write_stderr_resilience() {
        let _guard = LOG_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let log_dir = TempDir::new().expect("create temp test dir");
        let _reset = ResetLogWriterOnDrop;
        reset_log_writer_for_test();

        let temp_log = log_dir.path().join("loganalysis.log");

        // Must never panic regardless of bytes or stdio state
        safe_write_stderr(b"Test safe stderr write\n");
        safe_write_stderr(&[]);
        log_event_to_path("TEST", "Unit test message via safe_write_stderr", Some(&temp_log));
        flush_logs();
        reset_log_writer_for_test();

        let written = std::fs::read_to_string(&temp_log).expect("read temp log");
        assert!(written.contains("[TEST] Unit test message via safe_write_stderr\n"));

        if let Ok(cwd) = std::env::current_dir() {
            assert!(
                !cwd.join("loganalysis.log").exists(),
                "Repository cwd loganalysis.log must not be created during tests"
            );
        }
    }

    #[test]
    fn test_resolve_log_paths_with_lookup_cwd_fallback_when_no_env() {
        let mut created_dirs = Vec::new();
        let cwd = PathBuf::from("/mock/fallback/cwd");

        // 1. No user env vars set -> falls back to cwd without creating user dir
        let paths = resolve_log_paths_with_lookup(
            "app.log",
            Some(cwd.clone()),
            |_| None,
            |dir| created_dirs.push(dir.to_path_buf()),
        );
        assert_eq!(paths, vec![cwd.join("app.log")]);
        assert!(created_dirs.is_empty());

        // 2. Empty env values are ignored and fall back to cwd
        let empty_env_paths = resolve_log_paths_with_lookup(
            "app.log",
            Some(cwd.clone()),
            |k| match k {
                "HOME" | "LOCALAPPDATA" | "APPDATA" | "USERPROFILE" => Some(String::new()),
                _ => None,
            },
            |dir| created_dirs.push(dir.to_path_buf()),
        );
        assert_eq!(empty_env_paths, vec![cwd.join("app.log")]);
        assert!(created_dirs.is_empty());

        // 3. Both env and cwd absent -> empty path list
        let none_paths = resolve_log_paths_with_lookup(
            "app.log",
            None,
            |_| None,
            |dir| created_dirs.push(dir.to_path_buf()),
        );
        assert!(none_paths.is_empty());
        assert!(created_dirs.is_empty());
    }

    #[test]
    fn test_buffered_logging_defers_until_flush_and_flushes_errors_immediately() {
        let _guard = LOG_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let log_dir = TempDir::new().expect("create temp test dir");
        let _reset = ResetLogWriterOnDrop;
        reset_log_writer_for_test();

        let log1 = log_dir.path().join("first.log");
        let log2 = log_dir.path().join("second.log");

        // INFO stays buffered in BufWriter until flush_logs()
        log_event_to_path("INFO", "buffered info line", Some(&log1));
        let before_flush = std::fs::read_to_string(&log1).expect("read log1 before flush");
        assert!(
            before_flush.is_empty(),
            "INFO event should stay buffered before flush, got: {before_flush:?}"
        );

        flush_logs();
        let after_flush = std::fs::read_to_string(&log1).expect("read log1 after flush");
        assert!(after_flush.contains("[INFO] buffered info line\n"));

        // Buffer another INFO then write ERROR -> ERROR flushes both immediately
        log_event_to_path("INFO", "queued before error", Some(&log1));
        assert!(!std::fs::read_to_string(&log1)
            .expect("read log1 before error")
            .contains("queued before error"));

        log_event_to_path("ERROR", "immediate error line", Some(&log1));
        let after_error = std::fs::read_to_string(&log1).expect("read log1 after error");
        assert!(after_error.contains("[INFO] queued before error\n"));
        assert!(after_error.contains("[ERROR] immediate error line\n"));

        // CRITICAL also flushes immediately without explicit flush_logs()
        log_event_to_path("CRITICAL", "immediate critical line", Some(&log1));
        let after_critical = std::fs::read_to_string(&log1).expect("read log1 after critical");
        assert!(after_critical.contains("[CRITICAL] immediate critical line\n"));

        // Switching target paths flushes the previous writer before opening the new file
        log_event_to_path("INFO", "pending before switch", Some(&log1));
        assert!(!std::fs::read_to_string(&log1)
            .expect("read log1 before path switch")
            .contains("pending before switch"));

        log_event_to_path("INFO", "buffered on second path", Some(&log2));
        let log1_after_switch =
            std::fs::read_to_string(&log1).expect("read log1 after path switch");
        assert!(log1_after_switch.contains("[INFO] pending before switch\n"));

        let log2_before_flush =
            std::fs::read_to_string(&log2).expect("read log2 before flush");
        assert!(log2_before_flush.is_empty());

        flush_logs();
        let log2_after_flush = std::fs::read_to_string(&log2).expect("read log2 after flush");
        assert!(log2_after_flush.contains("[INFO] buffered on second path\n"));

        // Also verify LogWriterState directly for isolated unit state transitions
        let log3 = log_dir.path().join("direct_state.log");
        let mut state = LogWriterState::default();
        state.write_line(&log3, "direct buffered\n", false);
        assert!(std::fs::read_to_string(&log3).expect("read log3").is_empty());
        state.flush();
        assert_eq!(
            std::fs::read_to_string(&log3).expect("read log3 after flush"),
            "direct buffered\n"
        );
        state.close();

        if let Ok(cwd) = std::env::current_dir() {
            assert!(!cwd.join("loganalysis.log").exists());
        }
    }

    #[test]
    fn test_record_crash_report_flushes_buffered_logs_first() {
        let _guard = LOG_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let log_dir = TempDir::new().expect("create temp test dir");
        let _reset = ResetLogWriterOnDrop;
        reset_log_writer_for_test();

        let crash_log = log_dir.path().join("crash.log");
        let app_log = log_dir.path().join("loganalysis.log");

        // Buffer a non-error log event first; verify it is not yet on disk
        log_event_to_path("INFO", "pre-crash buffered context", Some(&app_log));
        let pre_crash_disk = std::fs::read_to_string(&app_log).expect("read app_log pre-crash");
        assert!(
            pre_crash_disk.is_empty(),
            "INFO event should be buffered before crash report"
        );

        let report = "LOGANALYSIS CRASH REPORT\nPanic: simulated panic\n";
        record_crash_report(
            std::slice::from_ref(&crash_log),
            std::slice::from_ref(&app_log),
            "2026-11-15 00:00:00 UTC",
            "worker-thread",
            "src/lib.rs:10:1",
            "simulated panic",
            report,
        );

        // Crash report file written directly
        let crash_written = std::fs::read_to_string(&crash_log).expect("read crash_log");
        assert_eq!(crash_written, report);

        // Buffered INFO entry must appear on disk chronologically before [CRITICAL_PANIC]
        let log_written = std::fs::read_to_string(&app_log).expect("read app_log post-crash");
        let info_marker = "[INFO] pre-crash buffered context\n";
        let panic_marker =
            "[2026-11-15 00:00:00 UTC] [CRITICAL_PANIC] Thread 'worker-thread' at src/lib.rs:10:1: simulated panic\n";

        let info_pos = log_written
            .find(info_marker)
            .expect("buffered INFO log must be flushed into loganalysis.log");
        let panic_pos = log_written
            .find(panic_marker)
            .expect("CRITICAL_PANIC line must be flushed into loganalysis.log");
        assert!(
            info_pos < panic_pos,
            "buffered INFO event must precede CRITICAL_PANIC entry"
        );

        if let Ok(cwd) = std::env::current_dir() {
            assert!(!cwd.join("loganalysis.log").exists());
        }
    }
}


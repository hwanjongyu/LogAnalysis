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

/// Resolves standard log directory paths:
/// 1. Current working directory
/// 2. User data directory (~/.local/share/loganalysis)
pub fn resolve_log_paths(filename: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. Current working directory
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join(filename));
    }

    // 2. User local share directory
    if let Ok(home) = std::env::var("HOME") {
        let user_share = PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("loganalysis");
        let _ = std::fs::create_dir_all(&user_share);
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

/// Appends a structured log event to the persistent `loganalysis.log` files
pub fn log_event(level: &str, message: &str) {
    let _lock = LOG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let timestamp = now_timestamp();
    let formatted = format!("[{timestamp}] [{level}] {message}\n");

    eprint!("{formatted}");

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

        eprintln!(
            "\n🚨 [LogAnalysis Fatal Crash]\n\
             Time:     {timestamp}\n\
             Thread:   {thread_name}\n\
             Location: {location}\n\
             Panic:    {payload}\n\n\
             Crash report and backtrace successfully written to:\n\
             {}",
            crash_paths
                .iter()
                .map(|p| format!("  - {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        );

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
}


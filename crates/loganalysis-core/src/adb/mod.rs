pub mod device;
pub mod logcat;

use std::path::PathBuf;

pub use device::{is_valid_device_serial, scan_adb_devices};
pub use logcat::{start_logcat_stream, stream_log_lines, LogcatStreamHandle};

/// Resolves the executable path to `adb`.
/// Inspects $ANDROID_HOME, $ANDROID_SDK_ROOT, user home standard SDK paths, standard UNIX paths, and falls back to PATH.
pub fn resolve_adb_path() -> PathBuf {
    let exe_name = if cfg!(windows) { "adb.exe" } else { "adb" };

    // 1. Check $ANDROID_HOME/platform-tools/adb
    if let Ok(android_home) = std::env::var("ANDROID_HOME") {
        let p = PathBuf::from(android_home)
            .join("platform-tools")
            .join(exe_name);
        if p.exists() {
            return p;
        }
    }

    // 2. Check $ANDROID_SDK_ROOT/platform-tools/adb
    if let Ok(sdk_root) = std::env::var("ANDROID_SDK_ROOT") {
        let p = PathBuf::from(sdk_root)
            .join("platform-tools")
            .join(exe_name);
        if p.exists() {
            return p;
        }
    }

    // 3. Check standard user home Android SDK paths (critical for GUI sessions without .bashrc)
    let home_opt = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok());

    if let Some(home) = home_opt {
        let home_path = PathBuf::from(home);
        for sub in [
            "Android/Sdk/platform-tools",
            "Android/sdk/platform-tools",
            "android-sdk/platform-tools",
            ".android/platform-tools",
            "AppData/Local/Android/Sdk/platform-tools",
            "Library/Android/sdk/platform-tools",
        ] {
            let p = home_path.join(sub).join(exe_name);
            if p.exists() {
                return p;
            }
        }
    }

    // 4. Check standard system directories
    for standard_path in [
        "/usr/bin/adb",
        "/usr/local/bin/adb",
        "/opt/android-sdk/platform-tools/adb",
        "/opt/android-studio/bin/adb",
    ] {
        let p = PathBuf::from(standard_path);
        if p.exists() {
            return p;
        }
    }

    // 5. Fallback to PATH lookup
    PathBuf::from(exe_name)
}

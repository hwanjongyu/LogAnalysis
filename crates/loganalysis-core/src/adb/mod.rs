pub mod device;
pub mod logcat;

use std::path::PathBuf;

pub use device::{is_valid_device_serial, scan_adb_devices};
pub use logcat::{start_logcat_stream, LogcatStreamHandle};

/// Resolves the executable path to `adb`.
/// Inspects $ANDROID_HOME, $ANDROID_SDK_ROOT, standard UNIX paths, and falls back to PATH.
pub fn resolve_adb_path() -> PathBuf {
    // 1. Check $ANDROID_HOME/platform-tools/adb
    if let Ok(android_home) = std::env::var("ANDROID_HOME") {
        let p = PathBuf::from(android_home)
            .join("platform-tools")
            .join(if cfg!(windows) { "adb.exe" } else { "adb" });
        if p.exists() {
            return p;
        }
    }

    // 2. Check $ANDROID_SDK_ROOT/platform-tools/adb
    if let Ok(sdk_root) = std::env::var("ANDROID_SDK_ROOT") {
        let p = PathBuf::from(sdk_root)
            .join("platform-tools")
            .join(if cfg!(windows) { "adb.exe" } else { "adb" });
        if p.exists() {
            return p;
        }
    }

    // 3. Check standard system directories
    for standard_path in [
        "/usr/bin/adb",
        "/usr/local/bin/adb",
        "/opt/android-sdk/platform-tools/adb",
    ] {
        let p = PathBuf::from(standard_path);
        if p.exists() {
            return p;
        }
    }

    // 4. Fallback to PATH lookup
    PathBuf::from(if cfg!(windows) { "adb.exe" } else { "adb" })
}

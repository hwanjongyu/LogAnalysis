use std::process::Command;
use std::thread;
use std::time::Duration;
use crossbeam_channel::bounded;

/// Validates whether an ADB device serial string is safe against command-line argument injection.
pub fn is_valid_device_serial(serial: &str) -> bool {
    let s = serial.trim();
    if s.is_empty() || s.starts_with('-') {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ':')
}

/// Scans connected ADB devices via `adb devices` with an enforced timeout.
pub fn scan_adb_devices(timeout: Duration) -> Result<Vec<String>, String> {
    let (tx, rx) = bounded(1);
    thread::spawn(move || {
        let res = Command::new("adb")
            .arg("devices")
            .output()
            .map_err(|e| format!("Failed to execute adb: {}", e));
        let _ = tx.send(res);
    });

    let output = match rx.recv_timeout(timeout) {
        Ok(res) => res?,
        Err(_) => return Err(format!("adb devices timed out after {:?}", timeout)),
    };

    if !output.status.success() {
        return Err(format!("adb devices exited with code {:?}", output.status.code()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "device" {
            let serial = parts[0].trim();
            if is_valid_device_serial(serial) {
                devices.push(serial.to_string());
            }
        }
    }

    Ok(devices)
}

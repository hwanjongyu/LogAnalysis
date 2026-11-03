use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use crossbeam_channel::bounded;

use crate::error::AdbError;

/// Validates whether an ADB device serial string is safe against command-line argument injection.
pub fn is_valid_device_serial(serial: &str) -> bool {
    let s = serial.trim();
    if s.is_empty() || s.starts_with('-') {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ':')
}

/// Scans connected ADB devices via `adb devices` with an enforced timeout and RAII subprocess cleanup.
pub fn scan_adb_devices(timeout: Duration) -> Result<Vec<String>, AdbError> {
    let mut cmd = Command::new("adb");
    cmd.arg("devices");
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| AdbError::ExecutionFailed(e.to_string()))?;
    let mut stdout_pipe = child.stdout.take().ok_or(AdbError::IoCaptureFailed)?;
    let child_arc = Arc::new(Mutex::new(Some(child)));
    let child_clone = Arc::clone(&child_arc);

    let (tx, rx) = bounded(1);
    thread::spawn(move || {
        let mut stdout_str = String::new();
        let read_res = stdout_pipe.read_to_string(&mut stdout_str);

        let status_res = if let Ok(mut lock) = child_clone.lock() {
            if let Some(ref mut c) = *lock {
                c.wait().map_err(|e| AdbError::ExecutionFailed(e.to_string()))
            } else {
                return;
            }
        } else {
            return;
        };

        match (read_res, status_res) {
            (Ok(_), Ok(status)) => {
                let _ = tx.send(Ok((status, stdout_str)));
            }
            (Err(e), _) => {
                let _ = tx.send(Err(AdbError::ExecutionFailed(e.to_string())));
            }
            (_, Err(e)) => {
                let _ = tx.send(Err(e));
            }
        }
    });

    let (status, stdout) = match rx.recv_timeout(timeout) {
        Ok(res) => res?,
        Err(_) => {
            // Reap the child process on timeout to prevent zombie process and thread leaks
            if let Ok(mut lock) = child_arc.lock() {
                if let Some(mut c) = lock.take() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
            return Err(AdbError::Timeout(timeout));
        }
    };

    if !status.success() {
        return Err(AdbError::NonZeroExit(status.code()));
    }

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

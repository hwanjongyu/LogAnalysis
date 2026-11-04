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
    let adb_path = super::resolve_adb_path();
    let mut cmd = Command::new(adb_path);
    cmd.arg("devices");
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| AdbError::ExecutionFailed(e.to_string()))?;
    let mut stdout_pipe = child.stdout.take().ok_or(AdbError::IoCaptureFailed)?;
    let child_arc = Arc::new(Mutex::new(Some(child)));
    let child_clone = Arc::clone(&child_arc);

    let (tx, rx) = bounded(1);
    thread::spawn(move || {
        let mut stdout_str = String::new();
        let read_res = stdout_pipe.read_to_string(&mut stdout_str);

        // Poll try_wait with short sleep without holding the mutex across blocking calls
        let status_res = loop {
            let mut lock = match child_clone.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            match lock.as_mut() {
                Some(c) => match c.try_wait() {
                    Ok(Some(status)) => {
                        let _ = lock.take();
                        break Ok(status);
                    }
                    Ok(None) => {
                        drop(lock);
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => {
                        let _ = lock.take();
                        break Err(AdbError::ExecutionFailed(e.to_string()));
                    }
                },
                None => {
                    // Process already taken or killed by timeout handler
                    return;
                }
            }
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
            // Reap child process on timeout without deadlock risk
            let mut lock = match child_arc.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Some(mut c) = lock.take() {
                let _ = c.kill();
                let _ = c.wait();
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

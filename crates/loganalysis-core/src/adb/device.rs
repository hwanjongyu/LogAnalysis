use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use crossbeam_channel::bounded;

use crate::error::AdbError;

/// Structured metadata describing a connected Android ADB device.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AdbDeviceInfo {
    pub serial: String,
    pub state: String,
    pub model: Option<String>,
    pub product: Option<String>,
    pub device: Option<String>,
    pub transport_id: Option<String>,
}

impl AdbDeviceInfo {
    /// Returns a human-friendly short name (e.g. "Pixel 8" instead of "Pixel_8" or hex serial).
    pub fn short_name(&self) -> String {
        if let Some(m) = &self.model {
            m.replace('_', " ")
        } else {
            self.serial.clone()
        }
    }

    /// Returns a full descriptive string (e.g. "Pixel 8 (44190DLJH0010R)").
    pub fn display_name(&self) -> String {
        let state_suffix = if self.state != "device" {
            format!(" [{}]", self.state)
        } else {
            String::new()
        };
        if let Some(m) = &self.model {
            format!("{} ({}){}", m.replace('_', " "), self.serial, state_suffix)
        } else {
            format!("{}{}", self.serial, state_suffix)
        }
    }

    /// Indicates whether the device is authorized and ready for live logcat streaming.
    pub fn is_ready(&self) -> bool {
        self.state == "device"
    }
}

/// Validates whether an ADB device serial string is safe against command-line argument injection.
pub fn is_valid_device_serial(serial: &str) -> bool {
    let s = serial.trim();
    if s.is_empty() || s.starts_with('-') {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ':')
}

/// Parses the output of `adb devices -l` into a list of structured `AdbDeviceInfo` entries.
pub fn parse_adb_devices_output(stdout: &str) -> Vec<AdbDeviceInfo> {
    let mut devices = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("List of devices attached") || trimmed.starts_with('*') {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let serial = parts[0].trim();
        if !is_valid_device_serial(serial) {
            continue;
        }

        let state = parts[1].trim().to_string();
        let mut model = None;
        let mut product = None;
        let mut device = None;
        let mut transport_id = None;

        for extra in &parts[2..] {
            if let Some((k, v)) = extra.split_once(':') {
                match k {
                    "model" => model = Some(v.to_string()),
                    "product" => product = Some(v.to_string()),
                    "device" => device = Some(v.to_string()),
                    "transport_id" => transport_id = Some(v.to_string()),
                    _ => {}
                }
            }
        }

        devices.push(AdbDeviceInfo {
            serial: serial.to_string(),
            state,
            model,
            product,
            device,
            transport_id,
        });
    }
    devices
}

/// Scans connected ADB devices via `adb devices -l` with an enforced timeout and RAII subprocess cleanup.
pub fn scan_adb_device_details(timeout: Duration) -> Result<Vec<AdbDeviceInfo>, AdbError> {
    let start_time = std::time::Instant::now();
    let adb_path = super::resolve_adb_path();
    let mut cmd = Command::new(adb_path);
    cmd.args(["devices", "-l"]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| AdbError::ExecutionFailed(e.to_string()))?;

    // If timeout already elapsed during spawn
    let elapsed = start_time.elapsed();
    if elapsed >= timeout {
        let _ = child.kill();
        let _ = child.wait();
        return Err(AdbError::Timeout(timeout));
    }
    let remaining = timeout - elapsed;

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

    let (status, stdout) = match rx.recv_timeout(remaining) {
        Ok(res) => {
            if start_time.elapsed() > timeout {
                return Err(AdbError::Timeout(timeout));
            }
            res?
        }
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

    Ok(parse_adb_devices_output(&stdout))
}

/// Scans connected ADB devices via `adb devices -l` and returns the serials of all ready devices.
pub fn scan_adb_devices(timeout: Duration) -> Result<Vec<String>, AdbError> {
    Ok(scan_adb_device_details(timeout)?
        .into_iter()
        .filter(|d| d.is_ready())
        .map(|d| d.serial)
        .collect())
}

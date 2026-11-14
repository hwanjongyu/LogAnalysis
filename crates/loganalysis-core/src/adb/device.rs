use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
/// Uses non-blocking pipe reading to eliminate pipe hangs when the ADB daemon inherits stdout.
pub fn scan_adb_device_details(timeout: Duration) -> Result<Vec<AdbDeviceInfo>, AdbError> {
    let start_time = Instant::now();
    let adb_path = super::resolve_adb_path();
    let mut cmd = Command::new(adb_path);
    cmd.args(["devices", "-l"]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    #[cfg(target_os = "linux")]
    unsafe {
        use std::os::unix::process::CommandExt;
        let parent_pid = libc::getpid();
        cmd.pre_exec(move || {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            if libc::getppid() != parent_pid {
                libc::_exit(1);
            }
            Ok(())
        });
    }

    let mut child = cmd.spawn().map_err(|e| AdbError::ExecutionFailed(e.to_string()))?;

    // If timeout already elapsed during spawn
    let elapsed = start_time.elapsed();
    if elapsed >= timeout {
        let _ = child.kill();
        let _ = child.wait();
        return Err(AdbError::Timeout(timeout));
    }

    let mut stdout_pipe = child.stdout.take().ok_or(AdbError::IoCaptureFailed)?;

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;
        let fd = stdout_pipe.as_raw_fd();
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL, 0);
            if flags >= 0 {
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }
    }

    let mut output = Vec::new();
    let mut buf = [0u8; 4096];

    let status = loop {
        if start_time.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AdbError::Timeout(timeout));
        }

        // Non-blocking read of available data
        match stdout_pipe.read(&mut buf) {
            Ok(0) => {
                // Pipe closed (clean EOF)
                let status = child.wait().map_err(|e| AdbError::ExecutionFailed(e.to_string()))?;
                break status;
            }
            Ok(n) => {
                output.extend_from_slice(&buf[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No pending data right now
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AdbError::ExecutionFailed(e.to_string()));
            }
        }

        // Poll child process exit
        match child.try_wait().map_err(|e| AdbError::ExecutionFailed(e.to_string()))? {
            Some(status) => {
                // Child has finished; drain any residual bytes in the kernel pipe buffer
                loop {
                    match stdout_pipe.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => output.extend_from_slice(&buf[..n]),
                        Err(_) => break,
                    }
                }
                break status;
            }
            None => {
                thread::sleep(Duration::from_millis(5));
            }
        }
    };

    if !status.success() {
        return Err(AdbError::NonZeroExit(status.code()));
    }

    let stdout_str = String::from_utf8_lossy(&output);
    Ok(parse_adb_devices_output(&stdout_str))
}

/// Scans connected ADB devices via `adb devices -l` and returns the serials of all ready devices.
pub fn scan_adb_devices(timeout: Duration) -> Result<Vec<String>, AdbError> {
    Ok(scan_adb_device_details(timeout)?
        .into_iter()
        .filter(|d| d.is_ready())
        .map(|d| d.serial)
        .collect())
}

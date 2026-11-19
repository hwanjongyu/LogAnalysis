use std::io::Read;
use std::process::{Child, Command, Stdio};
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

fn cleanup_aborted_command(
    mut child: Child,
    rx: crossbeam_channel::Receiver<Vec<u8>>,
    reader_handle: thread::JoinHandle<()>,
) {
    let _ = child.kill();
    let _ = child.wait();
    drop(rx);
    let _ = reader_handle.join();
}

/// Executes `cmd` with captured stdout and an enforced wall-clock `timeout`.
/// Uses a dedicated pipe-reader thread and bounded crossbeam channel so pipe reads
/// cannot block timeout enforcement on any platform (Linux, macOS, Windows).
pub fn run_command_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Result<Vec<u8>, AdbError> {
    let start_time = Instant::now();
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());
    super::configure_no_console_window(&mut cmd);

    #[cfg(target_os = "linux")]
    super::configure_linux_pdeathsig(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| AdbError::ExecutionFailed(e.to_string()))?;

    // If timeout already elapsed during spawn (or timeout is zero)
    if start_time.elapsed() >= timeout {
        let _ = child.kill();
        let _ = child.wait();
        return Err(AdbError::Timeout(timeout));
    }

    let mut stdout_pipe = match child.stdout.take() {
        Some(p) => p,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AdbError::IoCaptureFailed);
        }
    };

    let (tx, rx) = crossbeam_channel::bounded::<Vec<u8>>(128);
    let reader_handle = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match stdout_pipe.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    });

    let mut output = Vec::new();
    let mut pipe_disconnected = false;

    let status = loop {
        let remaining = timeout.saturating_sub(start_time.elapsed());
        if remaining.is_zero() {
            cleanup_aborted_command(child, rx, reader_handle);
            return Err(AdbError::Timeout(timeout));
        }

        match rx.recv_timeout(remaining.min(Duration::from_millis(20))) {
            Ok(chunk) => {
                output.extend_from_slice(&chunk);
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                match child
                    .try_wait()
                    .map_err(|e| AdbError::ExecutionFailed(e.to_string()))
                {
                    Ok(Some(status)) => {
                        // Drain any chunks already buffered in the channel.
                        loop {
                            match rx.try_recv() {
                                Ok(chunk) => output.extend_from_slice(&chunk),
                                Err(crossbeam_channel::TryRecvError::Empty) => break,
                                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                                    pipe_disconnected = true;
                                    break;
                                }
                            }
                        }

                        // Bounded grace drain so any chunk read by the worker thread just before
                        // child exit is not lost under CI scheduler load, without hanging on
                        // inherited daemon FDs.
                        if !pipe_disconnected {
                            let grace_deadline = Instant::now()
                                + timeout
                                    .saturating_sub(start_time.elapsed())
                                    .min(Duration::from_millis(5));
                            while let Some(grace_rem) =
                                grace_deadline.checked_duration_since(Instant::now())
                            {
                                if grace_rem.is_zero() {
                                    break;
                                }
                                match rx.recv_timeout(grace_rem) {
                                    Ok(chunk) => output.extend_from_slice(&chunk),
                                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => break,
                                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                                        pipe_disconnected = true;
                                        break;
                                    }
                                }
                            }
                        }

                        break status;
                    }
                    Ok(None) => {}
                    Err(err) => {
                        cleanup_aborted_command(child, rx, reader_handle);
                        return Err(err);
                    }
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                pipe_disconnected = true;
                break loop {
                    match child
                        .try_wait()
                        .map_err(|e| AdbError::ExecutionFailed(e.to_string()))
                    {
                        Ok(Some(status)) => break status,
                        Ok(None) => {
                            let remaining = timeout.saturating_sub(start_time.elapsed());
                            if remaining.is_zero() {
                                cleanup_aborted_command(child, rx, reader_handle);
                                return Err(AdbError::Timeout(timeout));
                            }
                            std::thread::sleep(remaining.min(Duration::from_millis(5)));
                        }
                        Err(err) => {
                            cleanup_aborted_command(child, rx, reader_handle);
                            return Err(err);
                        }
                    }
                };
            }
        }
    };

    drop(rx);
    if pipe_disconnected {
        let _ = reader_handle.join();
    }

    if !status.success() {
        return Err(AdbError::NonZeroExit(status.code()));
    }

    Ok(output)
}

/// Scans connected ADB devices via `adb devices -l` with an enforced timeout and RAII subprocess cleanup.
/// Uses a dedicated pipe-reader thread to eliminate pipe hangs when the ADB daemon inherits stdout.
pub fn scan_adb_device_details(timeout: Duration) -> Result<Vec<AdbDeviceInfo>, AdbError> {
    let adb_path = super::resolve_adb_path();
    let mut cmd = Command::new(adb_path);
    cmd.args(["devices", "-l"]);
    let output = run_command_with_timeout(cmd, timeout)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_command_with_timeout_fast_stdout() {
        #[cfg(unix)]
        let cmd = {
            let mut c = Command::new("sh");
            c.args(["-c", "printf 'adb-ok\\n'"]);
            c
        };
        #[cfg(windows)]
        let cmd = {
            let mut c = Command::new("cmd");
            c.args(["/C", "echo adb-ok"]);
            c
        };

        let out = run_command_with_timeout(cmd, Duration::from_secs(5))
            .expect("fast command should succeed");
        assert_eq!(String::from_utf8_lossy(&out).trim(), "adb-ok");
    }

    #[test]
    fn test_run_command_with_timeout_multi_chunk_stdout() {
        #[cfg(unix)]
        {
            let mut cmd = Command::new("sh");
            // 16 bytes doubled 10 times = 16,384 bytes (> 4096-byte reader buffer)
            cmd.args([
                "-c",
                "s='0123456789abcdef'; i=0; while [ $i -lt 10 ]; do s=\"$s$s\"; i=$((i+1)); done; printf '%s' \"$s\"",
            ]);

            let out = run_command_with_timeout(cmd, Duration::from_secs(5))
                .expect("multi-chunk command should succeed");
            assert_eq!(out.len(), 16_384);
            assert_eq!(out, b"0123456789abcdef".repeat(1024));
        }

        #[cfg(windows)]
        {
            let mut cmd = Command::new("cmd");
            // 512 lines * 18 bytes ("0123456789abcdef\r\n") = 9,216 bytes (> 4096-byte reader buffer)
            cmd.args(["/C", "for /L %i in (1,1,512) do @echo 0123456789abcdef"]);

            let out = run_command_with_timeout(cmd, Duration::from_secs(5))
                .expect("multi-chunk command should succeed");
            assert!(out.len() > 4096);
            let text = String::from_utf8_lossy(&out);
            let lines: Vec<&str> = text.lines().collect();
            assert_eq!(lines.len(), 512);
            assert!(lines.iter().all(|line| *line == "0123456789abcdef"));
        }
    }

    #[test]
    fn test_run_command_with_timeout_nonzero_exit() {
        #[cfg(unix)]
        let cmd = {
            let mut c = Command::new("sh");
            c.args(["-c", "exit 42"]);
            c
        };
        #[cfg(windows)]
        let cmd = {
            let mut c = Command::new("cmd");
            c.args(["/C", "exit 42"]);
            c
        };

        let res = run_command_with_timeout(cmd, Duration::from_secs(5));
        assert_eq!(res, Err(AdbError::NonZeroExit(Some(42))));
    }

    #[test]
    fn test_run_command_with_timeout_enforces_timeout_on_hanging_command() {
        #[cfg(unix)]
        let cmd = {
            let mut c = Command::new("sleep");
            c.arg("5");
            c
        };
        #[cfg(windows)]
        let cmd = {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 5"]);
            c
        };

        let timeout = Duration::from_millis(50);
        let start = Instant::now();
        let res = run_command_with_timeout(cmd, timeout);
        let elapsed = start.elapsed();

        assert_eq!(res, Err(AdbError::Timeout(timeout)));
        assert!(
            elapsed < Duration::from_secs(2),
            "Timeout took too long ({elapsed:?}); child pipe may have blocked termination"
        );
    }

    #[test]
    fn test_run_command_with_timeout_zero_duration_immediate() {
        #[cfg(unix)]
        let cmd = {
            let mut c = Command::new("sleep");
            c.arg("5");
            c
        };
        #[cfg(windows)]
        let cmd = {
            let mut c = Command::new("cmd");
            c.args(["/C", "exit 0"]);
            c
        };

        let start = Instant::now();
        let res = run_command_with_timeout(cmd, Duration::ZERO);
        let elapsed = start.elapsed();

        assert_eq!(res, Err(AdbError::Timeout(Duration::ZERO)));
        assert!(
            elapsed < Duration::from_secs(1),
            "Zero timeout should return immediately, took {elapsed:?}"
        );
    }

    #[test]
    fn test_run_command_with_timeout_missing_binary() {
        let cmd = Command::new("nonexistent_loganalysis_adb_binary_xyz_98765");
        let res = run_command_with_timeout(cmd, Duration::from_secs(1));
        assert!(
            matches!(res, Err(AdbError::ExecutionFailed(_))),
            "Expected ExecutionFailed for missing binary, got: {res:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_run_command_with_timeout_stdout_closed_hang_enforces_timeout() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "exec 1>&-; sleep 30"]);
        let timeout = Duration::from_millis(50);
        let start = Instant::now();
        let res = run_command_with_timeout(cmd, timeout);
        let elapsed = start.elapsed();

        assert_eq!(res, Err(AdbError::Timeout(timeout)));
        assert!(
            elapsed < Duration::from_secs(2),
            "Timeout took too long ({elapsed:?}) when child closed stdout before hanging"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_run_command_with_timeout_stdout_closed_before_clean_exit() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf 'ok'; exec 1>&-; exit 0"]);
        let out = run_command_with_timeout(cmd, Duration::from_secs(5))
            .expect("command closing stdout before exit 0 should succeed");
        assert_eq!(out, b"ok");

        let mut fail_cmd = Command::new("sh");
        fail_cmd.args(["-c", "exec 1>&-; exit 7"]);
        let res = run_command_with_timeout(fail_cmd, Duration::from_secs(5));
        assert_eq!(res, Err(AdbError::NonZeroExit(Some(7))));
    }
}

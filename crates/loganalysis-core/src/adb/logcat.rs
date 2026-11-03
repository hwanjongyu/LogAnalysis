use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use crossbeam_channel::Sender;

use super::device::is_valid_device_serial;

/// Starts a background thread streaming `adb logcat -v threadtime`.
pub fn start_logcat_stream(
    device_serial: Option<&str>,
    chunk_tx: Sender<Vec<String>>,
    stop_flag: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, String> {
    let mut cmd = Command::new("adb");
    if let Some(serial) = device_serial {
        if !is_valid_device_serial(serial) {
            return Err("Invalid characters or leading dash in device serial".to_string());
        }
        cmd.args(["-s", serial]);
    }
    cmd.args(["logcat", "-v", "threadtime"]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn adb logcat: {}", e))?;
    let stdout = child.stdout.take().ok_or("Failed to capture stdout of adb")?;

    let handle = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut buffer = Vec::with_capacity(100);
        let mut last_flush = Instant::now();

        for line_res in reader.lines() {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            match line_res {
                Ok(line) => {
                    buffer.push(line);
                    let should_flush = buffer.len() >= 100 || (last_flush.elapsed() >= Duration::from_millis(16) && !buffer.is_empty());
                    if should_flush {
                        let to_send = std::mem::replace(&mut buffer, Vec::with_capacity(100));
                        if chunk_tx.send(to_send).is_err() {
                            break;
                        }
                        last_flush = Instant::now();
                    }
                }
                Err(_) => break,
            }
        }

        if !buffer.is_empty() {
            let _ = chunk_tx.send(buffer);
        }

        let _ = child.kill();
        let _ = child.wait();
    });

    Ok(handle)
}

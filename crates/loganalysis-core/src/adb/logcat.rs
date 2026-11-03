use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use crossbeam_channel::Sender;

use super::device::is_valid_device_serial;
use crate::error::AdbError;

/// RAII handle to a running adb logcat background stream.
/// Automatically terminates the child process and joins the thread on drop or stop.
pub struct LogcatStreamHandle {
    thread_handle: Option<thread::JoinHandle<()>>,
    child: Arc<Mutex<Option<Child>>>,
    stop_flag: Arc<AtomicBool>,
}

impl LogcatStreamHandle {
    /// Signals the background thread and terminates the child process to unblock I/O.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Ok(mut lock) = self.child.lock() {
            if let Some(mut child) = lock.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// Stops the stream and waits for the background worker thread to finish.
    pub fn join(mut self) -> thread::Result<()> {
        self.stop();
        if let Some(handle) = self.thread_handle.take() {
            handle.join()
        } else {
            Ok(())
        }
    }
}

impl Drop for LogcatStreamHandle {
    fn drop(&mut self) {
        self.stop();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Starts a background thread streaming `adb logcat -v threadtime`.
/// Returns an RAII `LogcatStreamHandle` managing the subprocess lifecycle.
pub fn start_logcat_stream(
    device_serial: Option<&str>,
    chunk_tx: Sender<Vec<String>>,
    stop_flag: Arc<AtomicBool>,
) -> Result<LogcatStreamHandle, AdbError> {
    let mut cmd = Command::new("adb");
    if let Some(serial) = device_serial {
        let s = serial.trim();
        if !is_valid_device_serial(s) {
            return Err(AdbError::InvalidSerial(s.to_string()));
        }
        cmd.args(["-s", s]);
    }
    cmd.args(["logcat", "-v", "threadtime"]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| AdbError::ExecutionFailed(e.to_string()))?;
    let stdout = child.stdout.take().ok_or(AdbError::IoCaptureFailed)?;

    let child_arc = Arc::new(Mutex::new(Some(child)));
    let child_clone = Arc::clone(&child_arc);
    let stop_flag_clone = Arc::clone(&stop_flag);

    let handle = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut raw_buf = Vec::with_capacity(512);
        let mut buffer = Vec::with_capacity(100);
        let mut last_flush = Instant::now();

        loop {
            if stop_flag_clone.load(Ordering::Relaxed) {
                break;
            }

            raw_buf.clear();
            match reader.read_until(b'\n', &mut raw_buf) {
                Ok(0) => break, // Clean EOF (subprocess closed pipe or was killed)
                Ok(_) => {
                    // Strip trailing newline / carriage return
                    while raw_buf.ends_with(b"\n") || raw_buf.ends_with(b"\r") {
                        raw_buf.pop();
                    }
                    // Lossy UTF-8 conversion: resilient against non-UTF8/binary crash logs
                    let line = String::from_utf8_lossy(&raw_buf).to_string();
                    buffer.push(line);

                    let should_flush = buffer.len() >= 100
                        || (last_flush.elapsed() >= Duration::from_millis(16) && !buffer.is_empty());
                    if should_flush {
                        let to_send = std::mem::replace(&mut buffer, Vec::with_capacity(100));
                        if chunk_tx.send(to_send).is_err() {
                            break;
                        }
                        last_flush = Instant::now();
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }

        if !buffer.is_empty() {
            let _ = chunk_tx.send(buffer);
        }

        // Clean up child process if still alive
        if let Ok(mut lock) = child_clone.lock() {
            if let Some(mut child) = lock.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    });

    Ok(LogcatStreamHandle {
        thread_handle: Some(handle),
        child: child_arc,
        stop_flag,
    })
}

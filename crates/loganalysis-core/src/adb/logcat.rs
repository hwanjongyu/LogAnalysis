use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use crossbeam_channel::{SendTimeoutError, Sender};

use super::device::is_valid_device_serial;
use crate::error::AdbError;

/// RAII handle to a running adb logcat background stream.
/// Automatically terminates the child process on drop or stop.
pub struct LogcatStreamHandle {
    thread_handle: Option<thread::JoinHandle<()>>,
    child: Arc<Mutex<Option<Child>>>,
    stop_flag: Arc<AtomicBool>,
}

impl LogcatStreamHandle {
    /// Signals the background thread and terminates the child process to unblock I/O.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        let mut lock = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut child) = lock.take() {
            let _ = child.kill();
            let _ = child.wait();
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
        // Intentionally do NOT call handle.join() here.
        // Dropping the JoinHandle detaches the background worker thread.
        // Because stream_log_lines evaluates stop_flag every 20ms and
        // child.kill() unblocks reader.read_until, the worker terminates
        // promptly in the background without blocking or deadlocking the GPUI UI thread.
    }
}

/// Starts a background thread streaming `adb logcat -v threadtime`.
/// Returns an RAII `LogcatStreamHandle` managing the subprocess lifecycle.
pub fn start_logcat_stream(
    device_serial: Option<&str>,
    chunk_tx: Sender<Vec<String>>,
    stop_flag: Arc<AtomicBool>,
) -> Result<LogcatStreamHandle, AdbError> {
    let adb_path = super::resolve_adb_path();
    let mut cmd = Command::new(adb_path);
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
    let stdout = child.stdout.take().ok_or(AdbError::IoCaptureFailed)?;

    let child_arc = Arc::new(Mutex::new(Some(child)));
    let child_clone = Arc::clone(&child_arc);
    let stop_flag_clone = Arc::clone(&stop_flag);

    let handle = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        stream_log_lines(reader, chunk_tx, stop_flag_clone);

        // Clean up child process if still alive
        let mut lock = child_clone.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut child) = lock.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    });

    Ok(LogcatStreamHandle {
        thread_handle: Some(handle),
        child: child_arc,
        stop_flag,
    })
}

/// Reads lines from any `BufRead` stream and flushes batches to `chunk_tx`.
/// Flushes immediately if `reader.buffer().is_empty()` (no pending bytes in OS pipe)
/// or if 100 lines accumulated or 16ms elapsed, avoiding stalling sparse/interactive logs.
pub const MAX_STREAM_LINE_BYTES: usize = 65536;

pub fn stream_log_lines<R: std::io::Read>(
    mut reader: BufReader<R>,
    chunk_tx: Sender<Vec<String>>,
    stop_flag: Arc<AtomicBool>,
) {
    let mut raw_buf = Vec::with_capacity(512);
    let mut buffer = Vec::with_capacity(100);
    let mut last_flush = Instant::now();

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        raw_buf.clear();
        match reader
            .by_ref()
            .take(MAX_STREAM_LINE_BYTES as u64)
            .read_until(b'\n', &mut raw_buf)
        {
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
                    || reader.buffer().is_empty()
                    || (last_flush.elapsed() >= Duration::from_millis(16) && !buffer.is_empty());
                if should_flush {
                    let mut to_send = std::mem::replace(&mut buffer, Vec::with_capacity(100));
                    let mut disconnected = false;
                    while !stop_flag.load(Ordering::Relaxed) {
                        match chunk_tx.send_timeout(to_send, Duration::from_millis(20)) {
                            Ok(()) => break,
                            Err(SendTimeoutError::Timeout(msg)) => {
                                to_send = msg;
                            }
                            Err(SendTimeoutError::Disconnected(_)) => {
                                disconnected = true;
                                break;
                            }
                        }
                    }
                    if disconnected || stop_flag.load(Ordering::Relaxed) {
                        break;
                    }
                    last_flush = Instant::now();
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }

    if !buffer.is_empty() && !stop_flag.load(Ordering::Relaxed) {
        let _ = chunk_tx.send_timeout(buffer, Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::bounded;

    #[test]
    fn test_invalid_device_serial_rejected() {
        let (tx, _rx) = bounded(1);
        let stop_flag = Arc::new(AtomicBool::new(false));

        // Command injection attempt in serial
        let evil_serial = "emulator-5554; rm -rf /";
        let res = start_logcat_stream(Some(evil_serial), tx, stop_flag);
        assert!(matches!(res, Err(AdbError::InvalidSerial(_))));
    }

    #[test]
    fn test_stream_log_lines_empty_reader() {
        let (tx, rx) = bounded(10);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let empty_input = b"";
        let reader = BufReader::new(&empty_input[..]);

        stream_log_lines(reader, tx, stop_flag);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_stream_log_lines_batch_and_strip() {
        let (tx, rx) = bounded(10);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let input = b"Line 1\r\nLine 2\nLine 3\n";
        let reader = BufReader::new(&input[..]);

        stream_log_lines(reader, tx, stop_flag);

        let batch = rx.recv().expect("Should receive batch");
        assert_eq!(batch, vec!["Line 1", "Line 2", "Line 3"]);
    }

    #[test]
    fn test_stream_log_lines_stop_flag_unblocks_saturated_channel() {
        let (tx, _rx) = bounded(1);
        // Saturate the bounded channel with 1 item
        tx.send(vec!["saturate".to_string()]).unwrap();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Feed multiple lines so buffer flushes
        let input_lines = "11-08 04:00:00.000 1 1 I Tag: Line 1\n".repeat(150);
        let worker_tx = tx.clone();

        let worker = thread::spawn(move || {
            let reader = BufReader::new(input_lines.as_bytes());
            stream_log_lines(reader, worker_tx, stop_flag_clone);
        });

        // Allow worker to start and block inside send_timeout on saturated channel
        thread::sleep(Duration::from_millis(30));

        // Signal stop flag
        let start = Instant::now();
        stop_flag.store(true, Ordering::SeqCst);

        // Worker must promptly unblock and exit within 150ms
        let join_res = worker.join();
        let elapsed = start.elapsed();

        assert!(join_res.is_ok(), "Worker thread should exit cleanly");
        assert!(
            elapsed < Duration::from_millis(150),
            "Worker thread took too long to unblock: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_logcat_stream_handle_drop_on_saturated_channel_does_not_deadlock() {
        let (tx, _rx) = bounded(1);
        tx.send(vec!["saturate".to_string()]).unwrap();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        let input_lines = "11-08 04:00:00.000 1 1 I Tag: Line\n".repeat(200);
        let worker_tx = tx.clone();

        let worker = thread::spawn(move || {
            let reader = BufReader::new(input_lines.as_bytes());
            stream_log_lines(reader, worker_tx, stop_flag_clone);
        });

        let handle = LogcatStreamHandle {
            thread_handle: Some(worker),
            child: Arc::new(Mutex::new(None)),
            stop_flag: stop_flag.clone(),
        };

        // Dropping handle on the UI thread must be non-blocking (< 50ms)
        let start = Instant::now();
        drop(handle);
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(50),
            "Dropping LogcatStreamHandle blocked UI thread: {:?}",
            elapsed
        );
        assert!(
            stop_flag.load(Ordering::SeqCst),
            "Drop must signal stop_flag to background worker"
        );
    }

    #[test]
    fn test_logcat_stream_handle_explicit_join_on_saturated_channel() {
        let (tx, _rx) = bounded(1);
        tx.send(vec!["saturate".to_string()]).unwrap();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        let input_lines = "11-08 04:00:00.000 1 1 I Tag: Line\n".repeat(200);
        let worker_tx = tx.clone();

        let worker = thread::spawn(move || {
            let reader = BufReader::new(input_lines.as_bytes());
            stream_log_lines(reader, worker_tx, stop_flag_clone);
        });

        let handle = LogcatStreamHandle {
            thread_handle: Some(worker),
            child: Arc::new(Mutex::new(None)),
            stop_flag: stop_flag.clone(),
        };

        let start = Instant::now();
        let res = handle.join();
        let elapsed = start.elapsed();

        assert!(res.is_ok(), "Explicit join must succeed");
        assert!(
            elapsed < Duration::from_millis(150),
            "Explicit join took too long: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_stream_log_lines_tail_flush_skips_when_stopped() {
        let (tx, _rx) = bounded(1);
        tx.send(vec!["saturate".to_string()]).unwrap();

        let stop_flag = Arc::new(AtomicBool::new(true)); // Already stopped
        let input_lines = "11-08 04:00:00.000 1 1 I ResidualLine\n";
        let reader = BufReader::new(input_lines.as_bytes());

        let start = Instant::now();
        stream_log_lines(reader, tx, stop_flag);
        let elapsed = start.elapsed();

        // Must exit immediately without attempting blocking tail flush
        assert!(elapsed < Duration::from_millis(50));
    }
}

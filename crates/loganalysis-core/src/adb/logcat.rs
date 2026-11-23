use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use crossbeam_channel::{SendTimeoutError, Sender};

use super::device::is_valid_device_serial;
use crate::error::AdbError;

/// Terminates `child` and reaps its exit status without blocking the calling thread.
///
/// Fast-paths via `try_wait()` when the process exits immediately after `kill()`,
/// otherwise hands ownership to a detached `"adb-child-reaper"` background thread.
/// Falls back to synchronous `wait()` only if spawning the reaper thread fails.
fn reap_child_nonblocking(mut child: Child) {
    let _ = child.kill();
    match child.try_wait() {
        Ok(Some(_)) => {}
        Ok(None) | Err(_) => {
            let child_slot = Arc::new(Mutex::new(Some(child)));
            let reaper_slot = Arc::clone(&child_slot);
            if thread::Builder::new()
                .name("adb-child-reaper".to_string())
                .spawn(move || {
                    let child = {
                        let mut lock = reaper_slot.lock().unwrap_or_else(|e| e.into_inner());
                        lock.take()
                    };
                    if let Some(mut c) = child {
                        let _ = c.wait();
                    }
                })
                .is_err()
            {
                let child = {
                    let mut lock = child_slot.lock().unwrap_or_else(|e| e.into_inner());
                    lock.take()
                };
                if let Some(mut c) = child {
                    let _ = c.wait();
                }
            }
        }
    }
}

/// RAII handle to a running adb logcat background stream.
/// Automatically terminates the child process on drop or stop.
pub struct LogcatStreamHandle {
    thread_handle: Option<thread::JoinHandle<()>>,
    child: Arc<Mutex<Option<Child>>>,
    stop_flag: Arc<AtomicBool>,
}

impl LogcatStreamHandle {
    /// Signals the background thread and terminates the child process without blocking
    /// the caller/UI thread while the OS reaps the subprocess.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        let child = {
            let mut lock = self.child.lock().unwrap_or_else(|e| e.into_inner());
            lock.take()
        };
        if let Some(child) = child {
            reap_child_nonblocking(child);
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
    super::configure_no_console_window(&mut cmd);

    #[cfg(target_os = "linux")]
    super::configure_linux_pdeathsig(&mut cmd);

    let child_arc = Arc::new(Mutex::new(None));
    let child_clone = Arc::clone(&child_arc);
    let stop_flag_clone = Arc::clone(&stop_flag);
    let (spawn_tx, spawn_rx) = crossbeam_channel::bounded(1);

    let handle = thread::spawn(move || {
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = spawn_tx.send(Err(AdbError::ExecutionFailed(e.to_string())));
                return;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = spawn_tx.send(Err(AdbError::IoCaptureFailed));
                return;
            }
        };

        {
            let mut lock = child_clone.lock().unwrap_or_else(|e| e.into_inner());
            *lock = Some(child);
        }
        let _ = spawn_tx.send(Ok(()));

        let reader = BufReader::new(stdout);
        stream_log_lines(reader, chunk_tx, stop_flag_clone);

        // Clean up child process if still alive
        let mut lock = child_clone.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut child) = lock.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    });

    spawn_rx
        .recv()
        .map_err(|e| AdbError::ExecutionFailed(format!("Spawn thread failed: {}", e)))??;

    Ok(LogcatStreamHandle {
        thread_handle: Some(handle),
        child: child_arc,
        stop_flag,
    })
}

/// Inspects the tail of `bytes` (up to 4 bytes) to determine if it ends with an incomplete multi-byte UTF-8 sequence.
/// Returns the number of trailing incomplete bytes (1..=3) to carry over, or 0 if complete.
#[inline]
pub fn trailing_incomplete_utf8_len(bytes: &[u8]) -> usize {
    let len = bytes.len();
    if len == 0 {
        return 0;
    }

    for i in 1..=4.min(len) {
        let byte = bytes[len - i];
        if (byte & 0x80) != 0 {
            if (byte & 0xC0) == 0xC0 {
                let expected_len = if (byte & 0xE0) == 0xC0 {
                    2
                } else if (byte & 0xF0) == 0xE0 {
                    3
                } else if (byte & 0xF8) == 0xF0 {
                    4
                } else {
                    return 0;
                };

                return if i < expected_len { i } else { 0 };
            }
        } else {
            return 0;
        }
    }
    0
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
    let mut carry_buf = Vec::new();
    let mut buffer = Vec::with_capacity(100);
    let mut last_flush = Instant::now();
    let mut skip_leading_orphan_lf = false;

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        if raw_buf.is_empty() && !carry_buf.is_empty() {
            raw_buf.extend_from_slice(&carry_buf);
        }

        let max_to_read = (MAX_STREAM_LINE_BYTES.saturating_sub(raw_buf.len())).max(1) as u64;
        match reader
            .by_ref()
            .take(max_to_read)
            .read_until(b'\n', &mut raw_buf)
        {
            Ok(0) => break, // Clean EOF (subprocess closed pipe or was killed)
            Ok(bytes_read) => {
                carry_buf.clear();
                let hit_newline = raw_buf.ends_with(b"\n");
                let hit_read_limit = !hit_newline && bytes_read == max_to_read as usize;

                let is_orphan_lf = if skip_leading_orphan_lf {
                    skip_leading_orphan_lf = false;
                    bytes_read == 1 && raw_buf == b"\n"
                } else {
                    false
                };

                if !is_orphan_lf {
                    let mut stripped_limit_cr = false;
                    if hit_newline {
                        raw_buf.pop();
                        while raw_buf.ends_with(b"\r") {
                            raw_buf.pop();
                        }
                    } else if hit_read_limit && raw_buf.ends_with(b"\r") {
                        while raw_buf.ends_with(b"\r") {
                            raw_buf.pop();
                        }
                        skip_leading_orphan_lf = true;
                        stripped_limit_cr = true;
                    } else if !hit_read_limit {
                        while raw_buf.ends_with(b"\r") {
                            raw_buf.pop();
                        }
                    }

                    // Only carry trailing incomplete UTF-8 across the 64KB chunk boundary;
                    // complete `\n`-terminated lines and EOF lines must never bleed invalid bytes into the next line.
                    if hit_read_limit && !stripped_limit_cr {
                        let incomplete = trailing_incomplete_utf8_len(&raw_buf);
                        if incomplete > 0 {
                            let split_pos = raw_buf.len() - incomplete;
                            carry_buf.extend_from_slice(&raw_buf[split_pos..]);
                            raw_buf.truncate(split_pos);
                        }
                    }

                    // Lossy UTF-8 conversion: resilient against non-UTF8/binary crash logs
                    let line = String::from_utf8_lossy(&raw_buf).to_string();
                    buffer.push(line);
                }

                raw_buf.clear();

                let should_flush = buffer.len() >= 100
                    || (reader.buffer().is_empty() && !buffer.is_empty())
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

    if !raw_buf.is_empty() {
        while raw_buf.ends_with(b"\r") {
            raw_buf.pop();
        }
        let line = String::from_utf8_lossy(&raw_buf).to_string();
        buffer.push(line);
    } else if !carry_buf.is_empty() {
        while carry_buf.ends_with(b"\r") {
            carry_buf.pop();
        }
        let line = String::from_utf8_lossy(&carry_buf).to_string();
        buffer.push(line);
    }

    if !buffer.is_empty() {
        let mut to_send = buffer;
        while !stop_flag.load(Ordering::Relaxed) {
            match chunk_tx.send_timeout(to_send, Duration::from_millis(20)) {
                Ok(()) => break,
                Err(SendTimeoutError::Timeout(msg)) => {
                    to_send = msg;
                }
                Err(SendTimeoutError::Disconnected(_)) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::bounded;
    use std::io::{self, Read};
    use std::sync::{Condvar, Mutex};

    struct SignalingReader {
        chunks: Vec<&'static [u8]>,
        index: usize,
        notify_at_index: usize,
        signal: Arc<(Mutex<bool>, Condvar)>,
    }

    impl Read for SignalingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.index == self.notify_at_index {
                let (lock, cvar) = &*self.signal;
                let mut ready = lock.lock().unwrap();
                *ready = true;
                cvar.notify_all();
            }
            if self.index >= self.chunks.len() {
                return Ok(0);
            }
            let chunk = self.chunks[self.index];
            let to_copy = chunk.len().min(buf.len());
            buf[..to_copy].copy_from_slice(&chunk[..to_copy]);
            if to_copy == chunk.len() {
                self.index += 1;
            } else {
                self.chunks[self.index] = &chunk[to_copy..];
            }
            Ok(to_copy)
        }
    }

    fn wait_until_ready(signal: &(Mutex<bool>, Condvar)) {
        let (lock, cvar) = signal;
        let guard = lock.lock().unwrap();
        let (guard, timeout) = cvar
            .wait_timeout_while(guard, Duration::from_secs(2), |ready| !*ready)
            .unwrap();
        assert!(*guard && !timeout.timed_out(), "Timed out waiting for signaling reader");
    }

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
        let (tx, rx) = bounded(1);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let signal_clone = Arc::clone(&signal);

        // Read 0 flushes a single line to fill bounded(1); Read 1 triggers signal and enters send_timeout
        let worker_tx = tx.clone();
        let worker = thread::spawn(move || {
            let reader = BufReader::new(SignalingReader {
                chunks: vec![
                    b"11-08 04:00:00.000 1 1 I Tag: Fill\n",
                    b"11-08 04:00:00.000 1 1 I Tag: Saturated\n",
                ],
                index: 0,
                notify_at_index: 1,
                signal: signal_clone,
            });
            stream_log_lines(reader, worker_tx, stop_flag_clone);
        });

        wait_until_ready(&signal);
        assert!(rx.is_full(), "Channel must be saturated once second read starts");

        // Signal stop flag
        let start = Instant::now();
        stop_flag.store(true, Ordering::SeqCst);

        // Worker must promptly unblock and exit within 150ms
        let join_res = worker.join();
        let elapsed = start.elapsed();

        assert!(join_res.is_ok(), "Worker thread should exit cleanly");
        assert!(
            elapsed < Duration::from_millis(if cfg!(target_os = "linux") { 150 } else { 500 }),
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
            elapsed < Duration::from_millis(if cfg!(target_os = "linux") { 150 } else { 500 }),
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

    #[test]
    fn test_trailing_incomplete_utf8_len_exhaustive() {
        assert_eq!(trailing_incomplete_utf8_len(b""), 0);
        assert_eq!(trailing_incomplete_utf8_len(b"hello world\n"), 0);
        assert_eq!(trailing_incomplete_utf8_len(b"abc"), 0);

        // 2-byte sequence (e.g. copyright \u{00A9} = [0xC2, 0xA9])
        assert_eq!(trailing_incomplete_utf8_len(&[0xC2]), 1);
        assert_eq!(trailing_incomplete_utf8_len(&[0xC2, 0xA9]), 0);

        // 3-byte CJK (e.g. '한' = [0xED, 0x95, 0x9C])
        assert_eq!(trailing_incomplete_utf8_len(&[0xED]), 1);
        assert_eq!(trailing_incomplete_utf8_len(&[0xED, 0x95]), 2);
        assert_eq!(trailing_incomplete_utf8_len(&[0xED, 0x95, 0x9C]), 0);

        // 4-byte Emoji (e.g. '🔥' = [0xF0, 0x9F, 0x94, 0xA5])
        assert_eq!(trailing_incomplete_utf8_len(&[0xF0]), 1);
        assert_eq!(trailing_incomplete_utf8_len(&[0xF0, 0x9F]), 2);
        assert_eq!(trailing_incomplete_utf8_len(&[0xF0, 0x9F, 0x94]), 3);
        assert_eq!(trailing_incomplete_utf8_len(&[0xF0, 0x9F, 0x94, 0xA5]), 0);

        // Mixed ASCII prefix followed by incomplete sequence
        let mut payload = vec![b'x'; 100];
        payload.extend_from_slice(&[0xED, 0x95]);
        assert_eq!(trailing_incomplete_utf8_len(&payload), 2);
    }

    #[test]
    fn test_stream_split_utf8_cjk_across_64kb_boundary() {
        let (tx, rx) = bounded(10);
        let stop_flag = Arc::new(AtomicBool::new(false));

        // Construct a payload where '한' (0xED, 0x95, 0x9C) spans across byte 65,536:
        // 65,535 bytes of 'A', 1 byte 0xED in chunk 1;
        // chunk 2 has 0x95, 0x9C followed by newline.
        let mut input = vec![b'A'; 65535];
        input.push(0xED); // Byte 65,535 (chunk 1 boundary)
        input.push(0x95); // Chunk 2 continuation
        input.push(0x9C); // Chunk 2 continuation
        input.push(b'\n');

        let reader = BufReader::new(&input[..]);
        stream_log_lines(reader, tx, stop_flag);

        let mut all_lines = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            all_lines.extend(batch);
        }

        assert_eq!(all_lines.len(), 2);
        // Chunk 1 must NOT contain replacement character \u{FFFD}
        assert!(!all_lines[0].contains('\u{FFFD}'));
        assert_eq!(all_lines[0].len(), 65535);

        // Chunk 2 must start with '한' without any \u{FFFD}
        assert!(all_lines[1].starts_with('한'));
        assert!(!all_lines[1].contains('\u{FFFD}'));
    }

    #[test]
    fn test_stream_malformed_utf8_does_not_bleed_across_newline() {
        let (tx, rx) = bounded(10);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let input = b"bad\xed\x95\nnext\n";
        let reader = BufReader::new(&input[..]);
        stream_log_lines(reader, tx, stop_flag);

        let mut all_lines = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            all_lines.extend(batch);
        }

        assert_eq!(all_lines, vec!["bad\u{FFFD}", "next"]);
    }

    #[test]
    fn test_stream_crlf_straddling_64kb_boundary_no_spurious_blank_line() {
        let (tx, rx) = bounded(10);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let mut input = vec![b'A'; 65535];
        input.extend_from_slice(b"\r\nnext\n");

        let reader = BufReader::new(&input[..]);
        stream_log_lines(reader, tx, stop_flag);

        let mut all_lines = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            all_lines.extend(batch);
        }

        assert_eq!(all_lines.len(), 2);
        assert_eq!(all_lines[0], "A".repeat(65535));
        assert_eq!(all_lines[1], "next");
    }

    #[cfg(unix)]
    #[test]
    fn test_stop_with_live_child_is_nonblocking_and_clears_slot() {
        let child = Command::new("sh")
            .args(["-c", "exec sleep 30"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn long-running sleep subprocess");
        let pid = child.id();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let child_slot = Arc::new(Mutex::new(Some(child)));
        let handle = LogcatStreamHandle {
            thread_handle: None,
            child: Arc::clone(&child_slot),
            stop_flag: Arc::clone(&stop_flag),
        };

        let start = Instant::now();
        handle.stop();
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(50),
            "LogcatStreamHandle::stop blocked caller: {:?}",
            elapsed
        );
        assert!(
            stop_flag.load(Ordering::SeqCst),
            "stop() must set stop_flag"
        );
        assert!(
            child_slot.lock().unwrap().is_none(),
            "stop() must take and clear child slot"
        );

        #[cfg(target_os = "linux")]
        if std::path::Path::new("/proc/self/status").exists() {
            let proc_status = format!("/proc/{pid}/status");
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut reaped = false;
            while Instant::now() < deadline {
                if std::fs::read_to_string(&proc_status).is_err() {
                    reaped = true;
                    break;
                }
                thread::yield_now();
            }
            assert!(
                reaped,
                "Subprocess PID {pid} was not reaped within 2s (potential zombie)"
            );
        }
        #[cfg(not(target_os = "linux"))]
        let _ = pid;
    }

    #[test]
    fn test_reap_child_nonblocking_already_exited() {
        #[cfg(unix)]
        let mut child = Command::new("sh")
            .args(["-c", "exit 0"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn exit-0 subprocess");
        #[cfg(windows)]
        let mut child = Command::new("cmd")
            .args(["/C", "exit", "0"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn exit-0 subprocess");

        // Deterministically wait for exit via bounded try_wait + yield_now (no hard sleep)
        let wait_deadline = Instant::now() + Duration::from_secs(2);
        let mut exited = false;
        while Instant::now() < wait_deadline {
            match child.try_wait() {
                Ok(Some(status)) => {
                    assert!(status.success());
                    exited = true;
                    break;
                }
                Ok(None) => thread::yield_now(),
                Err(e) => panic!("try_wait failed unexpectedly: {e}"),
            }
        }
        assert!(exited, "short-lived child failed to exit within 2s");

        let pid = child.id();
        let start = Instant::now();
        reap_child_nonblocking(child);
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(50),
            "reap_child_nonblocking blocked on already-exited child: {:?}",
            elapsed
        );

        #[cfg(target_os = "linux")]
        if std::path::Path::new("/proc/self/status").exists() {
            let proc_status = format!("/proc/{pid}/status");
            assert!(
                std::fs::read_to_string(&proc_status).is_err(),
                "Already-exited child PID {pid} must leave no /proc entry"
            );
        }
        #[cfg(not(target_os = "linux"))]
        let _ = pid;

        // On Linux, also verify the OS-zombie fast path where Child has already exited
        // (`State:\tZ`) before any prior `try_wait()` call cached its exit status.
        #[cfg(target_os = "linux")]
        if std::path::Path::new("/proc/self/status").exists() {
            let zombie_child = Command::new("sh")
                .args(["-c", "exit 0"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn exit-0 subprocess for zombie fast-path");
            let zombie_pid = zombie_child.id();
            let proc_status = format!("/proc/{zombie_pid}/status");
            let zombie_deadline = Instant::now() + Duration::from_secs(2);
            let mut became_zombie = false;
            while Instant::now() < zombie_deadline {
                if let Ok(status_text) = std::fs::read_to_string(&proc_status) {
                    if status_text.contains("State:\tZ") {
                        became_zombie = true;
                        break;
                    }
                }
                thread::yield_now();
            }
            assert!(
                became_zombie,
                "short-lived child PID {zombie_pid} did not reach zombie state within 2s"
            );

            let z_start = Instant::now();
            reap_child_nonblocking(zombie_child);
            let z_elapsed = z_start.elapsed();
            assert!(
                z_elapsed < Duration::from_millis(50),
                "reap_child_nonblocking blocked on OS zombie child: {:?}",
                z_elapsed
            );

            let reap_deadline = Instant::now() + Duration::from_secs(2);
            let mut z_reaped = false;
            while Instant::now() < reap_deadline {
                if std::fs::read_to_string(&proc_status).is_err() {
                    z_reaped = true;
                    break;
                }
                thread::yield_now();
            }
            assert!(
                z_reaped,
                "Zombie child PID {zombie_pid} was not reaped cleanly"
            );
        }
    }

    #[test]
    fn test_stream_eof_immediately_after_64kib_mid_utf8_preserves_carry_bytes() {
        let (tx, rx) = bounded(10);
        let stop_flag = Arc::new(AtomicBool::new(false));

        // 65,535 bytes of 'A' followed by an incomplete 3-byte UTF-8 lead [0xED, 0x95]
        // (Wait: 65,535 'A's + 0xED = 65,536 bytes; then EOF immediately after!)
        let mut input = vec![b'A'; 65_535];
        input.push(0xED);

        let reader = BufReader::new(&input[..]);
        stream_log_lines(reader, tx, stop_flag);

        let mut all_lines = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            all_lines.extend(batch);
        }

        assert_eq!(all_lines.len(), 2);
        assert_eq!(all_lines[0], "A".repeat(65_535));
        assert_eq!(all_lines[1], "\u{FFFD}");
    }

    #[test]
    fn test_stream_eintr_mid_carry_preserves_carry_bytes() {
        struct EintrAfterCarryReader {
            chunk: Vec<u8>,
            pos: usize,
            eintr_emitted: bool,
        }

        impl Read for EintrAfterCarryReader {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.pos >= 65_536 && !self.eintr_emitted {
                    self.eintr_emitted = true;
                    return Err(io::Error::from(io::ErrorKind::Interrupted));
                }
                if self.pos >= self.chunk.len() {
                    return Ok(0);
                }
                let avail = (self.chunk.len() - self.pos).min(buf.len());
                buf[..avail].copy_from_slice(&self.chunk[self.pos..self.pos + avail]);
                self.pos += avail;
                Ok(avail)
            }
        }

        let (tx, rx) = bounded(10);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let mut payload = vec![b'A'; 65_535];
        payload.extend_from_slice("한\n".as_bytes());

        let reader = BufReader::new(EintrAfterCarryReader {
            chunk: payload,
            pos: 0,
            eintr_emitted: false,
        });
        stream_log_lines(reader, tx, stop_flag);

        let mut all_lines = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            all_lines.extend(batch);
        }

        assert_eq!(all_lines.len(), 2);
        assert_eq!(all_lines[0], "A".repeat(65_535));
        assert_eq!(all_lines[1], "한");
    }

    #[test]
    fn test_stream_saturated_channel_eof_flush_retries_without_dropping() {
        struct OwnedSignalingReader {
            chunks: Vec<Vec<u8>>,
            index: usize,
            notify_at_index: usize,
            signal: Arc<(Mutex<bool>, Condvar)>,
        }

        impl Read for OwnedSignalingReader {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.index == self.notify_at_index {
                    let (lock, cvar) = &*self.signal;
                    let mut ready = lock.lock().unwrap();
                    *ready = true;
                    cvar.notify_all();
                }
                if self.index >= self.chunks.len() {
                    return Ok(0);
                }
                let chunk = &self.chunks[self.index];
                let to_copy = chunk.len().min(buf.len());
                buf[..to_copy].copy_from_slice(&chunk[..to_copy]);
                if to_copy == chunk.len() {
                    self.index += 1;
                } else {
                    self.chunks[self.index] = chunk[to_copy..].to_vec();
                }
                Ok(to_copy)
            }
        }

        let (tx, rx) = bounded(1);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = Arc::clone(&stop_flag);
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let signal_clone = Arc::clone(&signal);

        // Chunk 0 fills the 64 KiB boundary ending mid-UTF-8, saturating `bounded(1)` on flush.
        // Then EOF hits immediately, forcing the post-loop EOF flush into the retry loop.
        let mut first_chunk = vec![b'A'; 65_535];
        first_chunk.push(0xED);

        let worker = thread::spawn(move || {
            let reader = BufReader::new(OwnedSignalingReader {
                chunks: vec![first_chunk],
                index: 0,
                notify_at_index: 1,
                signal: signal_clone,
            });
            stream_log_lines(reader, tx, stop_flag_clone);
        });

        wait_until_ready(&signal);
        assert!(rx.is_full(), "Channel must be saturated before EOF flush");

        // Wait past a single 20ms send_timeout to prove the worker retries rather than dropping.
        let gate = (Mutex::new(()), Condvar::new());
        let g = gate.0.lock().unwrap();
        let _ = gate.1.wait_timeout(g, Duration::from_millis(45)).unwrap();
        assert!(!worker.is_finished(), "Worker must keep retrying saturated EOF flush");

        let batch1 = rx.recv_timeout(Duration::from_secs(1)).expect("batch 1");
        let batch2 = rx.recv_timeout(Duration::from_secs(1)).expect("EOF batch 2");
        assert_eq!(batch1, vec!["A".repeat(65_535)]);
        assert_eq!(batch2, vec!["\u{FFFD}"]);
        worker.join().expect("worker should exit cleanly");
    }
}

use std::io::{self, BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use crossbeam_channel::{bounded, unbounded};
use loganalysis_core::{start_logcat_stream, stream_log_lines};

/// Custom mock reader to simulate granular I/O error scenarios
struct MockFaultyReader {
    chunks: Vec<Result<Vec<u8>, io::ErrorKind>>,
    index: usize,
}

impl MockFaultyReader {
    fn new(chunks: Vec<Result<Vec<u8>, io::ErrorKind>>) -> Self {
        Self { chunks, index: 0 }
    }
}

impl Read for MockFaultyReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.index >= self.chunks.len() {
            return Ok(0); // Clean EOF
        }
        match &self.chunks[self.index] {
            Ok(data) => {
                let to_copy = data.len().min(buf.len());
                buf[..to_copy].copy_from_slice(&data[..to_copy]);
                self.index += 1;
                Ok(to_copy)
            }
            Err(kind) => {
                let err = io::Error::new(*kind, "Simulated mock IO error");
                self.index += 1;
                Err(err)
            }
        }
    }
}

// 1. Sudden EOF handling
#[test]
fn test_stress_sudden_eof_empty_stream() {
    let (tx, rx) = bounded(10);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let reader = BufReader::new(io::empty());

    let start = Instant::now();
    stream_log_lines(reader, tx, stop_flag);
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(50));
    assert!(rx.try_recv().is_err(), "Empty stream should produce no batches");
}

#[test]
fn test_stress_sudden_eof_partial_line() {
    let (tx, rx) = bounded(10);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let partial_data = b"Line without newline at EOF";
    let reader = BufReader::new(&partial_data[..]);

    stream_log_lines(reader, tx, stop_flag);

    let batch = rx.try_recv().expect("Should flush partial line at EOF");
    assert_eq!(batch, vec!["Line without newline at EOF"]);
}

#[test]
fn test_stress_sudden_eof_burst_multiline() {
    let (tx, rx) = bounded(100);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let mut data = String::new();
    for i in 0..250 {
        data.push_str(&format!("Burst log line {}\n", i));
    }
    // Truncate trailing newline on last line to test abrupt EOF
    data.pop();

    let reader = BufReader::new(data.as_bytes());
    stream_log_lines(reader, tx, stop_flag);

    let mut total_lines = 0;
    while let Ok(batch) = rx.try_recv() {
        total_lines += batch.len();
    }
    assert_eq!(total_lines, 250, "All 250 lines must be delivered despite sudden EOF");
}

// 2. Broken pipes and IO error boundaries
#[test]
fn test_stress_broken_pipe_immediate() {
    let (tx, rx) = bounded(10);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let faulty_reader = MockFaultyReader::new(vec![
        Err(io::ErrorKind::BrokenPipe),
    ]);
    let reader = BufReader::new(faulty_reader);

    let start = Instant::now();
    stream_log_lines(reader, tx, stop_flag);
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(50));
    assert!(rx.try_recv().is_err());
}

#[test]
fn test_stress_broken_pipe_after_data() {
    let (tx, rx) = bounded(10);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let faulty_reader = MockFaultyReader::new(vec![
        Ok(b"Valid line 1\nValid line 2\n".to_vec()),
        Err(io::ErrorKind::BrokenPipe),
    ]);
    let reader = BufReader::new(faulty_reader);

    let start = Instant::now();
    stream_log_lines(reader, tx, stop_flag);
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(50));
    let batch = rx.try_recv().expect("Should deliver lines before broken pipe");
    assert_eq!(batch, vec!["Valid line 1", "Valid line 2"]);
}

#[test]
fn test_stress_connection_reset_and_unexpected_eof() {
    for error_kind in [
        io::ErrorKind::ConnectionReset,
        io::ErrorKind::UnexpectedEof,
        io::ErrorKind::PermissionDenied,
    ] {
        let (tx, _rx) = bounded(10);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let faulty_reader = MockFaultyReader::new(vec![
            Ok(b"Line before error\n".to_vec()),
            Err(error_kind),
        ]);
        let reader = BufReader::new(faulty_reader);

        let start = Instant::now();
        stream_log_lines(reader, tx, stop_flag);
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(50),
            "Worker took too long on IO error {:?}: {:?}",
            error_kind,
            elapsed
        );
    }
}

#[test]
fn test_stress_interrupted_eintr_resilience() {
    let (tx, rx) = bounded(10);
    let stop_flag = Arc::new(AtomicBool::new(false));
    // Simulate 3 EINTR (Interrupted) calls followed by valid data
    let faulty_reader = MockFaultyReader::new(vec![
        Err(io::ErrorKind::Interrupted),
        Err(io::ErrorKind::Interrupted),
        Err(io::ErrorKind::Interrupted),
        Ok(b"Line survived EINTR\n".to_vec()),
    ]);
    let reader = BufReader::new(faulty_reader);

    stream_log_lines(reader, tx, stop_flag);

    let batch = rx.try_recv().expect("Should survive EINTR and receive line");
    assert_eq!(batch, vec!["Line survived EINTR"]);
}

// 3. Channel disconnects
#[test]
fn test_stress_channel_disconnect_immediate() {
    let (tx, rx) = bounded(10);
    drop(rx); // Immediately disconnect receiver

    let stop_flag = Arc::new(AtomicBool::new(false));
    let data = "Log line\n".repeat(150);
    let reader = BufReader::new(data.as_bytes());

    let start = Instant::now();
    stream_log_lines(reader, tx, stop_flag);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(50),
        "Stream reader should exit immediately when channel is disconnected: {:?}",
        elapsed
    );
}

#[test]
fn test_stress_channel_disconnect_while_blocked_on_saturated_channel() {
    let (tx, rx) = bounded(1);
    tx.send(vec!["saturate".to_string()]).unwrap();

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();
    let data = "Line\n".repeat(200);
    let worker_tx = tx.clone();

    let worker = thread::spawn(move || {
        let reader = BufReader::new(data.as_bytes());
        stream_log_lines(reader, worker_tx, stop_flag_clone);
    });

    // Wait until worker is blocked on saturated channel
    thread::sleep(Duration::from_millis(30));

    // Drop the receiver
    let start = Instant::now();
    drop(rx);

    // Worker must detect disconnect and exit promptly (< 100ms)
    let res = worker.join();
    let elapsed = start.elapsed();

    assert!(res.is_ok(), "Worker must exit cleanly on channel disconnect");
    assert!(
        elapsed < Duration::from_millis(100),
        "Worker took too long to unblock after channel disconnect: {:?}",
        elapsed
    );
}

// 4. Residual tail flushing under stop_flag
#[test]
fn test_stress_tail_flush_skips_when_stop_flag_set_on_saturated_channel() {
    let (tx, rx) = bounded(1);
    tx.send(vec!["saturate".to_string()]).unwrap();

    let stop_flag = Arc::new(AtomicBool::new(true)); // Already stopped!
    let data = "Residual Line 1\nResidual Line 2\n";
    let reader = BufReader::new(data.as_bytes());

    let start = Instant::now();
    stream_log_lines(reader, tx, stop_flag);
    let elapsed = start.elapsed();

    // Must skip immediately (< 10ms) without attempting to send or block
    assert!(
        elapsed < Duration::from_millis(20),
        "Tail flush with stop_flag=true blocked for {:?}",
        elapsed
    );

    // Channel should only have the original saturated item
    assert_eq!(rx.try_recv().unwrap(), vec!["saturate"]);
    assert!(rx.try_recv().is_err(), "Residual lines must not have been sent");
}

#[test]
fn test_stress_tail_flush_delivers_residual_when_stop_flag_false() {
    let (tx, rx) = bounded(10);
    let stop_flag = Arc::new(AtomicBool::new(false));
    // Exactly 3 lines (fewer than 100 batch limit)
    let data = "Line A\nLine B\nLine C";
    let reader = BufReader::new(data.as_bytes());

    stream_log_lines(reader, tx, stop_flag);

    let batch = rx.try_recv().expect("Should receive tail flush");
    assert_eq!(batch, vec!["Line A", "Line B", "Line C"]);
}

#[test]
fn test_stress_stop_flag_unblock_latency() {
    let (tx, _rx) = bounded(1);
    tx.send(vec!["saturate".to_string()]).unwrap();

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();
    let data = "Line\n".repeat(500);
    let worker_tx = tx.clone();

    let worker = thread::spawn(move || {
        let reader = BufReader::new(data.as_bytes());
        stream_log_lines(reader, worker_tx, stop_flag_clone);
    });

    thread::sleep(Duration::from_millis(40));

    let start = Instant::now();
    stop_flag.store(true, Ordering::SeqCst);
    let res = worker.join();
    let elapsed = start.elapsed();

    assert!(res.is_ok());
    // Since send_timeout is 20ms, worker must unblock within ~35ms
    assert!(
        elapsed < Duration::from_millis(60),
        "Stop flag unblock latency too high: {:?}",
        elapsed
    );
}

// 5. Handle Drop and Lifecycle Safety
#[test]
fn test_stress_handle_drop_safety() {
    let (tx, _rx) = unbounded();
    let stop_flag = Arc::new(AtomicBool::new(false));

    if let Ok(handle) = start_logcat_stream(Some("emulator-5554"), tx, stop_flag.clone()) {
        let start = Instant::now();
        drop(handle);
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(50), "Drop should be fast (<50ms): {:?}", elapsed);
        assert!(stop_flag.load(Ordering::SeqCst), "Drop must set stop flag");
    }
}

#[test]
fn test_stress_handle_stop_idempotency() {
    let (tx, _rx) = unbounded();
    let stop_flag = Arc::new(AtomicBool::new(false));

    if let Ok(handle) = start_logcat_stream(Some("emulator-5554"), tx, stop_flag.clone()) {
        handle.stop();
        handle.stop(); // Idempotent stop
        assert!(stop_flag.load(Ordering::SeqCst));
        let _ = handle.join(); // Safe join after stop
    }
}

// 7. Concurrency, Race Condition, and Rendezvous Stress
#[test]
fn test_stress_zero_capacity_rendezvous_channel() {
    let (tx, _rx) = bounded(0); // Zero capacity (rendezvous)
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();
    let data = "Rendezvous log line\n".repeat(10);
    let worker_tx = tx.clone();

    let worker = thread::spawn(move || {
        let reader = BufReader::new(data.as_bytes());
        stream_log_lines(reader, worker_tx, stop_flag_clone);
    });

    thread::sleep(Duration::from_millis(30));
    let start = Instant::now();
    stop_flag.store(true, Ordering::SeqCst);
    let res = worker.join();
    let elapsed = start.elapsed();

    assert!(res.is_ok());
    assert!(
        elapsed < Duration::from_millis(60),
        "Rendezvous channel unblock took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_stress_rapid_racing_stop_and_drop_cycles() {
    for cycle in 0..50 {
        let (tx, _rx) = bounded(1);
        tx.send(vec!["saturated".to_string()]).unwrap();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();
        let data = "Race line\n".repeat(200);
        let worker_tx = tx.clone();

        let worker = thread::spawn(move || {
            let reader = BufReader::new(data.as_bytes());
            stream_log_lines(reader, worker_tx, stop_flag_clone);
        });

        // Vary sleep to hit different execution windows inside stream_log_lines
        if cycle % 3 == 1 {
            thread::sleep(Duration::from_millis(5));
        } else if cycle % 3 == 2 {
            thread::sleep(Duration::from_millis(25));
        }

        let start = Instant::now();
        stop_flag.store(true, Ordering::SeqCst);
        let res = worker.join();
        let elapsed = start.elapsed();

        assert!(res.is_ok(), "Cycle {} worker failed to join cleanly", cycle);
        assert!(
            elapsed < Duration::from_millis(80),
            "Cycle {} unblock exceeded latency budget: {:?}",
            cycle,
            elapsed
        );
    }
}

#[test]
fn test_stress_super_long_lines_chunking() {
    let (tx, rx) = unbounded();
    let stop_flag = Arc::new(AtomicBool::new(false));

    // 150,000 chars without newline
    let giant_data = "Z".repeat(150_000);
    let reader = BufReader::new(giant_data.as_bytes());

    stream_log_lines(reader, tx, stop_flag);

    let mut total_chars = 0;
    while let Ok(batch) = rx.try_recv() {
        for line in batch {
            assert!(line.len() <= 65536, "Chunk size exceeds MAX_STREAM_LINE_BYTES");
            total_chars += line.len();
        }
    }
    assert_eq!(total_chars, 150_000);
}

#[test]
fn test_stress_binary_data_resilience() {
    let (tx, rx) = bounded(10);
    let stop_flag = Arc::new(AtomicBool::new(false));

    // Invalid UTF-8 bytes with newlines
    let bad_bytes: Vec<u8> = vec![
        0xFF, 0xFE, 0xFD, b'\n',
        0x80, 0x81, 0x82, b'\r', b'\n',
        0xC0, 0xAF, b'\n',
    ];
    let reader = BufReader::new(&bad_bytes[..]);

    stream_log_lines(reader, tx, stop_flag);

    let batch = rx.try_recv().expect("Binary lines should be lossy converted");
    assert_eq!(batch.len(), 3);
    for line in batch {
        assert!(!line.is_empty());
    }
}

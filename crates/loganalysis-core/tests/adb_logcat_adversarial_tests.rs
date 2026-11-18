//! Adversarial stress test harness for ADB logcat streaming deadlock hotfix (CRIT-01).
//! Validates:
//! 1. Dropping `LogcatStreamHandle` on a saturated channel completes in < 50ms.
//! 2. Background worker thread terminates promptly (< 50ms) without lingering or deadlocking.
//! 3. `stream_log_lines` exits immediately when `stop_flag` is set.
//! 4. High-concurrency simultaneous drops across multiple threads do not block or deadlock.
//! 5. Real subprocess lifecycle (SIGKILL and reap) functions cleanly under channel saturation.

#[cfg(unix)]
use std::fs;
use std::io::{self, BufReader, Read};
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

struct SignalingReader {
    chunks: Vec<Vec<u8>>,
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

fn wait_until_ready(signal: &(Mutex<bool>, Condvar)) {
    let (lock, cvar) = signal;
    let guard = lock.lock().unwrap();
    let (guard, timeout) = cvar
        .wait_timeout_while(guard, Duration::from_secs(2), |ready| !*ready)
        .unwrap();
    assert!(
        *guard && !timeout.timed_out(),
        "Timed out waiting for signaling reader"
    );
}

#[cfg(unix)]
fn wait_until_channel_full<T>(rx: &crossbeam_channel::Receiver<T>) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !rx.is_full() {
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for mock adb to saturate bounded channel"
        );
        thread::yield_now();
    }
}

use crossbeam_channel::bounded;
#[cfg(unix)]
use loganalysis_core::start_logcat_stream;
use loganalysis_core::stream_log_lines;

#[cfg(unix)]
static ENV_MUTEX: Mutex<()> = Mutex::new(());

fn max_latency(linux_ms: u64) -> Duration {
    if cfg!(target_os = "linux") {
        Duration::from_millis(linux_ms)
    } else {
        Duration::from_millis(500)
    }
}

/// Helper to create a mock adb executable that floods stdout with logcat lines
#[cfg(unix)]
fn setup_mock_adb(dir: &std::path::Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let platform_tools = dir.join("platform-tools");
    fs::create_dir_all(&platform_tools).expect("create platform-tools");
    let adb_path = platform_tools.join("adb");

    let script = r#"#!/bin/sh
# If logcat command, flood stdout with endless lines
for arg in "$@"; do
    if [ "$arg" = "logcat" ]; then
        while true; do
            echo "11-08 04:00:00.000 1000 1001 I FloodTag: High rate burst log line"
        done
    fi
done
exit 0
"#;
    fs::write(&adb_path, script).expect("write mock adb");
    fs::set_permissions(&adb_path, fs::Permissions::from_mode(0o755)).expect("chmod mock adb");

    adb_path
}

#[test]
fn test_adversarial_stop_flag_exit_latency_50_iterations() {
    // Repeat 50 times to detect any race condition or non-deterministic hang
    let mut latencies = Vec::with_capacity(50);

    for i in 0..50 {
        let (tx, rx) = bounded(1);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let signal_clone = Arc::clone(&signal);
        let worker_tx = tx.clone();

        let worker = thread::spawn(move || {
            let reader = BufReader::new(SignalingReader {
                chunks: vec![
                    b"11-08 04:00:00.000 1 1 I Tag: Fill\n".to_vec(),
                    b"11-08 04:00:00.000 1 1 I Tag: Line\n".repeat(250),
                ],
                index: 0,
                notify_at_index: 1,
                signal: signal_clone,
            });
            stream_log_lines(reader, worker_tx, stop_flag_clone);
        });

        // Deterministically wait until channel is saturated and second read begins
        wait_until_ready(&signal);
        assert!(rx.is_full(), "Iteration {i}: channel must be saturated");

        let start = Instant::now();
        stop_flag.store(true, Ordering::SeqCst);
        let join_res = worker.join();
        let elapsed = start.elapsed();

        assert!(
            join_res.is_ok(),
            "Iteration {}: Worker panicked or failed to join",
            i
        );
        assert!(
            elapsed < max_latency(60),
            "Iteration {}: Worker took too long to exit: {:?}",
            i,
            elapsed
        );
        latencies.push(elapsed);
    }

    let max_lat = latencies.iter().max().unwrap();
    let avg_lat: Duration = latencies.iter().sum::<Duration>() / latencies.len() as u32;
    println!(
        "[CHALLENGER] 50-run stop_flag unblock latency: avg = {:?}, max = {:?}",
        avg_lat, max_lat
    );
    assert!(*max_lat < max_latency(50));
}

#[cfg(unix)]
#[test]
fn test_adversarial_drop_handle_latency_with_live_subprocess() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let temp_home = tempfile::tempdir().expect("tempdir");
    let _mock_adb = setup_mock_adb(temp_home.path());

    let old_android_home = std::env::var("ANDROID_HOME").ok();
    let old_android_sdk_root = std::env::var("ANDROID_SDK_ROOT").ok();
    std::env::set_var("ANDROID_HOME", temp_home.path());
    std::env::remove_var("ANDROID_SDK_ROOT");

    let (tx, rx) = bounded(2);
    let stop_flag = Arc::new(AtomicBool::new(false));

    let handle_res = start_logcat_stream(Some("emulator-5554"), tx, stop_flag.clone());

    // Restore env immediately
    match old_android_home {
        Some(val) => std::env::set_var("ANDROID_HOME", val),
        None => std::env::remove_var("ANDROID_HOME"),
    }
    if let Some(val) = old_android_sdk_root {
        std::env::set_var("ANDROID_SDK_ROOT", val);
    }

    let handle = handle_res.expect("start_logcat_stream must succeed with mock adb");

    // Wait deterministically for mock adb to flood and saturate bounded channel
    wait_until_channel_full(&rx);

    // Measure drop latency: MUST complete in < 50ms on UI thread
    let start = Instant::now();
    drop(handle);
    let elapsed = start.elapsed();

    println!(
        "[CHALLENGER] Live subprocess saturated channel drop latency: {:?}",
        elapsed
    );

    assert!(
        elapsed < max_latency(50),
        "Dropping LogcatStreamHandle took {:?}, exceeding latency limit",
        elapsed
    );
    assert!(
        stop_flag.load(Ordering::SeqCst),
        "stop_flag must be true after drop"
    );
}

#[test]
fn test_adversarial_high_concurrency_multi_stream_drop() {
    const CONCURRENCY: usize = 16;
    let barrier = Arc::new(Barrier::new(CONCURRENCY));
    let mut handles = Vec::with_capacity(CONCURRENCY);

    for id in 0..CONCURRENCY {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let (tx, rx) = bounded(1);
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_clone = stop_flag.clone();
            let signal = Arc::new((Mutex::new(false), Condvar::new()));
            let signal_clone = Arc::clone(&signal);
            let worker_tx = tx.clone();

            let worker = thread::spawn(move || {
                let reader = BufReader::new(SignalingReader {
                    chunks: vec![
                        b"11-08 04:00:00.000 1 1 I Tag: Fill\n".to_vec(),
                        b"11-08 04:00:00.000 1 1 I Tag: ConcurrentLine\n".repeat(500),
                    ],
                    index: 0,
                    notify_at_index: 1,
                    signal: signal_clone,
                });
                stream_log_lines(reader, worker_tx, stop_flag_clone);
            });

            // Deterministically wait until channel is saturated and second read starts
            wait_until_ready(&signal);
            assert!(rx.is_full(), "Worker {id}: channel must be saturated");

            // Synchronize all threads to drop at the exact same microsecond
            b.wait();

            let start = Instant::now();
            // Signal stop
            stop_flag.store(true, Ordering::SeqCst);
            let join_res = worker.join();
            let elapsed = start.elapsed();

            assert!(join_res.is_ok(), "Worker {} must join successfully", id);
            assert!(
                elapsed < max_latency(60),
                "Worker {} took too long: {:?}",
                id,
                elapsed
            );
            elapsed
        }));
    }

    let mut max_elapsed = Duration::from_millis(0);
    for h in handles {
        let elapsed = h.join().expect("Thread joined");
        if elapsed > max_elapsed {
            max_elapsed = elapsed;
        }
    }

    println!(
        "[CHALLENGER] 16 concurrent streams max drop/stop latency: {:?}",
        max_elapsed
    );
    assert!(max_elapsed < max_latency(50));
}

#[test]
fn test_adversarial_stop_flag_pre_set_immediate_return() {
    let (tx, _rx) = bounded(1);
    tx.send(vec!["full".to_string()]).unwrap();

    let stop_flag = Arc::new(AtomicBool::new(true)); // Already stopped!
    let input_lines = "11-08 04:00:00.000 1 1 I Tag: NeverSent\n".repeat(100);
    let reader = BufReader::new(input_lines.as_bytes());

    let start = Instant::now();
    stream_log_lines(reader, tx, stop_flag);
    let elapsed = start.elapsed();

    // Must return immediately (< 5ms) without sending anything
    assert!(
        elapsed < max_latency(5),
        "Immediate exit took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_adversarial_receiver_drop_unblocks_worker() {
    let (tx, rx) = bounded(1);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();
    let signal = Arc::new((Mutex::new(false), Condvar::new()));
    let signal_clone = Arc::clone(&signal);
    let worker_tx = tx.clone();

    let worker = thread::spawn(move || {
        let reader = BufReader::new(SignalingReader {
            chunks: vec![
                b"11-08 04:00:00.000 1 1 I Tag: Fill\n".to_vec(),
                b"11-08 04:00:00.000 1 1 I Tag: DisconnectTest\n".repeat(200),
            ],
            index: 0,
            notify_at_index: 1,
            signal: signal_clone,
        });
        stream_log_lines(reader, worker_tx, stop_flag_clone);
    });

    wait_until_ready(&signal);
    assert!(rx.is_full(), "Channel must be saturated before drop");

    // Drop receiver while worker is waiting on send_timeout
    let start = Instant::now();
    drop(rx);

    let join_res = worker.join();
    let elapsed = start.elapsed();

    assert!(join_res.is_ok(), "Worker must exit cleanly on rx disconnect");
    assert!(
        elapsed < max_latency(50),
        "Worker took too long to detect disconnect: {:?}",
        elapsed
    );
}

#[cfg(unix)]
#[test]
fn test_adversarial_explicit_join_latency_with_live_subprocess() {
    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let temp_home = tempfile::tempdir().expect("tempdir");
    let _mock_adb = setup_mock_adb(temp_home.path());

    let old_android_home = std::env::var("ANDROID_HOME").ok();
    let old_android_sdk_root = std::env::var("ANDROID_SDK_ROOT").ok();
    std::env::set_var("ANDROID_HOME", temp_home.path());
    std::env::remove_var("ANDROID_SDK_ROOT");

    let (tx, rx) = bounded(2);
    let stop_flag = Arc::new(AtomicBool::new(false));

    let handle_res = start_logcat_stream(Some("emulator-5554"), tx, stop_flag.clone());

    match old_android_home {
        Some(val) => std::env::set_var("ANDROID_HOME", val),
        None => std::env::remove_var("ANDROID_HOME"),
    }
    if let Some(val) = old_android_sdk_root {
        std::env::set_var("ANDROID_SDK_ROOT", val);
    }

    let handle = handle_res.expect("start_logcat_stream must succeed with mock adb");

    // Wait deterministically for mock adb to flood and saturate bounded channel
    wait_until_channel_full(&rx);

    // Calling join() explicitly must terminate child, wait for worker, and return < 100ms
    let start = Instant::now();
    let join_res = handle.join();
    let elapsed = start.elapsed();

    println!(
        "[CHALLENGER] Explicit join on live subprocess saturated channel: {:?}",
        elapsed
    );

    assert!(join_res.is_ok(), "Explicit join must succeed");
    assert!(
        elapsed < max_latency(100),
        "Explicit join took too long: {:?}",
        elapsed
    );
    assert!(
        stop_flag.load(Ordering::SeqCst),
        "stop_flag must be true after join"
    );
}

#[cfg(unix)]
#[test]
fn test_scan_adb_devices_mock_daemon_inherited_pipe_hang() {
    use std::os::unix::fs::PermissionsExt;

    let sdk_dir = tempfile::tempdir().expect("tempdir");
    let platform_tools = sdk_dir.path().join("platform-tools");
    std::fs::create_dir_all(&platform_tools).expect("mkdir");
    let adb_path = platform_tools.join("adb");

    // Script launches a background process that inherits stdout and sleeps for 5 seconds.
    // Under blocking read_to_string, this hangs for 5 seconds.
    // Under non-blocking try_wait polling, it returns in < 150ms.
    let script = r#"#!/bin/sh
for arg in "$@"; do
    if [ "$arg" = "devices" ]; then
        ( sleep 5 ) &
        echo "List of devices attached"
        echo "emulator-5554          device product:sdk_gphone64_arm64 model:Pixel_8 device:emulator64_arm64 transport_id:1"
        exit 0
    fi
done
exit 0
"#;
    std::fs::write(&adb_path, script).expect("write mock adb");
    std::fs::set_permissions(&adb_path, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let old_home = std::env::var("ANDROID_HOME").ok();
    std::env::set_var("ANDROID_HOME", sdk_dir.path());

    let start = Instant::now();
    let res = loganalysis_core::scan_adb_device_details(Duration::from_millis(500));
    let elapsed = start.elapsed();

    if let Some(h) = old_home {
        std::env::set_var("ANDROID_HOME", h);
    } else {
        std::env::remove_var("ANDROID_HOME");
    }

    assert!(res.is_ok(), "Expected Ok but got: {:?}", res);
    let devices = res.unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].serial, "emulator-5554");
    assert!(
        elapsed < max_latency(150),
        "Call took too long ({:?}), likely blocked on inherited pipe!",
        elapsed
    );
}

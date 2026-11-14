//! Adversarial stress test harness for ADB logcat streaming deadlock hotfix (CRIT-01).
//! Validates:
//! 1. Dropping `LogcatStreamHandle` on a saturated channel completes in < 50ms.
//! 2. Background worker thread terminates promptly (< 50ms) without lingering or deadlocking.
//! 3. `stream_log_lines` exits immediately when `stop_flag` is set.
//! 4. High-concurrency simultaneous drops across multiple threads do not block or deadlock.
//! 5. Real subprocess lifecycle (SIGKILL and reap) functions cleanly under channel saturation.

use std::fs;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::bounded;
use loganalysis_core::{start_logcat_stream, stream_log_lines};

static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Helper to create a mock adb executable that floods stdout with logcat lines
fn setup_mock_adb(dir: &std::path::Path) -> PathBuf {
    let platform_tools = dir.join("platform-tools");
    fs::create_dir_all(&platform_tools).expect("create platform-tools");
    let adb_path = platform_tools.join("adb");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
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
    }

    adb_path
}

#[test]
fn test_adversarial_stop_flag_exit_latency_50_iterations() {
    // Repeat 50 times to detect any race condition or non-deterministic hang
    let mut latencies = Vec::with_capacity(50);

    for i in 0..50 {
        let (tx, _rx) = bounded(1);
        // Pre-saturate channel
        tx.send(vec!["saturation item".to_string()]).unwrap();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Feed endless lines so buffer repeatedly reaches flush threshold
        let input_lines = "11-08 04:00:00.000 1 1 I Tag: Line\n".repeat(250);
        let worker_tx = tx.clone();

        let worker = thread::spawn(move || {
            let reader = BufReader::new(input_lines.as_bytes());
            stream_log_lines(reader, worker_tx, stop_flag_clone);
        });

        // Let worker hit send_timeout loop
        thread::sleep(Duration::from_millis(10));

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
            elapsed < Duration::from_millis(60),
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
    assert!(*max_lat < Duration::from_millis(50));
}

#[test]
fn test_adversarial_drop_handle_latency_with_live_subprocess() {
    let _guard = ENV_MUTEX.lock().unwrap();

    let temp_home = tempfile::tempdir().expect("tempdir");
    let _mock_adb = setup_mock_adb(temp_home.path());

    let old_android_home = std::env::var("ANDROID_HOME").ok();
    let old_android_sdk_root = std::env::var("ANDROID_SDK_ROOT").ok();
    std::env::set_var("ANDROID_HOME", temp_home.path());
    std::env::remove_var("ANDROID_SDK_ROOT");

    let (tx, _rx) = bounded(2);
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

    // Allow mock adb to flood and saturate bounded channel
    thread::sleep(Duration::from_millis(40));

    // Measure drop latency: MUST complete in < 50ms on UI thread
    let start = Instant::now();
    drop(handle);
    let elapsed = start.elapsed();

    println!(
        "[CHALLENGER] Live subprocess saturated channel drop latency: {:?}",
        elapsed
    );

    assert!(
        elapsed < Duration::from_millis(50),
        "Dropping LogcatStreamHandle took {:?}, exceeding 50ms limit",
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
            let (tx, _rx) = bounded(1);
            tx.send(vec!["saturation".to_string()]).unwrap();

            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_clone = stop_flag.clone();

            let input_lines = "11-08 04:00:00.000 1 1 I Tag: ConcurrentLine\n".repeat(500);
            let worker_tx = tx.clone();

            let worker = thread::spawn(move || {
                let reader = BufReader::new(input_lines.as_bytes());
                stream_log_lines(reader, worker_tx, stop_flag_clone);
            });

            // Wait until worker is definitely in timed send loop
            thread::sleep(Duration::from_millis(15));

            // Synchronize all threads to drop at the exact same microsecond
            b.wait();

            let start = Instant::now();
            // Signal stop
            stop_flag.store(true, Ordering::SeqCst);
            let join_res = worker.join();
            let elapsed = start.elapsed();

            assert!(join_res.is_ok(), "Worker {} must join successfully", id);
            assert!(
                elapsed < Duration::from_millis(60),
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
    assert!(max_elapsed < Duration::from_millis(50));
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
        elapsed < Duration::from_millis(5),
        "Immediate exit took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_adversarial_receiver_drop_unblocks_worker() {
    let (tx, rx) = bounded(1);
    tx.send(vec!["fill".to_string()]).unwrap();

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    let input_lines = "11-08 04:00:00.000 1 1 I Tag: DisconnectTest\n".repeat(200);
    let worker_tx = tx.clone();

    let worker = thread::spawn(move || {
        let reader = BufReader::new(input_lines.as_bytes());
        stream_log_lines(reader, worker_tx, stop_flag_clone);
    });

    thread::sleep(Duration::from_millis(15));

    // Drop receiver while worker is waiting on send_timeout
    let start = Instant::now();
    drop(rx);

    let join_res = worker.join();
    let elapsed = start.elapsed();

    assert!(join_res.is_ok(), "Worker must exit cleanly on rx disconnect");
    assert!(
        elapsed < Duration::from_millis(50),
        "Worker took too long to detect disconnect: {:?}",
        elapsed
    );
}

#[test]
fn test_adversarial_explicit_join_latency_with_live_subprocess() {
    let _guard = ENV_MUTEX.lock().unwrap();

    let temp_home = tempfile::tempdir().expect("tempdir");
    let _mock_adb = setup_mock_adb(temp_home.path());

    let old_android_home = std::env::var("ANDROID_HOME").ok();
    let old_android_sdk_root = std::env::var("ANDROID_SDK_ROOT").ok();
    std::env::set_var("ANDROID_HOME", temp_home.path());
    std::env::remove_var("ANDROID_SDK_ROOT");

    let (tx, _rx) = bounded(2);
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

    // Allow mock adb to flood and saturate bounded channel
    thread::sleep(Duration::from_millis(40));

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
        elapsed < Duration::from_millis(100),
        "Explicit join took too long: {:?}",
        elapsed
    );
    assert!(
        stop_flag.load(Ordering::SeqCst),
        "stop_flag must be true after join"
    );
}

#[test]
fn test_scan_adb_devices_mock_daemon_inherited_pipe_hang() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let platform_tools = temp_dir.path().join("platform-tools");
    std::fs::create_dir_all(&platform_tools).expect("mkdir");
    let adb_path = platform_tools.join("adb");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
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
    }

    let _guard = ENV_MUTEX.lock().unwrap();
    let old_home = std::env::var("ANDROID_HOME").ok();
    std::env::set_var("ANDROID_HOME", temp_dir.path());

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
        elapsed < Duration::from_millis(150),
        "Call took too long ({:?}), likely blocked on inherited pipe!",
        elapsed
    );
}

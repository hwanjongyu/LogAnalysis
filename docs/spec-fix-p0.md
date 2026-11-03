# 🛠️ P0 Remediation Specification: Concurrency, Deadlock & Stream Hardening

- **Version:** 1.0.0
- **Scope:** Priority 0 Blockers (`loganalysis-core/src/adb/`)
- **Status:** Approved for Implementation

---

## 1. Problem Statement

Three critical reliability and concurrency flaws were identified in the ADB and streaming subsystems:

1. **Syscall Read Deadlock in `start_logcat_stream`**:
   `BufRead::lines()` blocks in the kernel read syscall when no new logs are emitted by the device. If an external caller sets `stop_flag = true`, the loop cannot observe the flag because it is suspended in `read()`. Calling `.join()` on the thread hangs indefinitely, and the child process is never killed (zombie leak).

2. **Stream Crash on Non-UTF8 Bytes**:
   `reader.lines()` yields `io::Error` with `ErrorKind::InvalidData` when encountering invalid UTF-8 bytes (common in Android crash dumps and binary driver logs). The current implementation terminates streaming immediately on `Err(_)`, dropping all future logs.

3. **Ignored Timeout in `scan_adb_devices` & Untrimmed Serial**:
   `scan_adb_devices(_timeout: Duration)` ignores the timeout parameter and blocks indefinitely on `Command::output()`. Untrimmed serial strings (e.g., copied with spaces) fail validation or cause unexpected behavior.

---

## 2. Technical Architecture & Solutions

### 2.1 RAII `LogcatStreamHandle` with Process Termination
Instead of returning a raw `thread::JoinHandle<()>`, return a handle struct:
```rust
pub struct LogcatStreamHandle {
    pub thread_handle: thread::JoinHandle<()>,
    child: Arc<Mutex<Option<Child>>>,
    stop_flag: Arc<AtomicBool>,
}
```
- Expose `stop(&self)`: sets `stop_flag = true`, locks `child`, and invokes `child.kill()` / `child.wait()`.
- Killing the child process forcibly closes the underlying pipe, immediately causing `read()` in the background thread to return EOF (0 bytes) and unblocking the reader thread without hanging.
- Implement `Drop` for `LogcatStreamHandle` to guarantee that no orphaned `adb` child processes remain running if the UI or handle is dropped.

### 2.2 Byte-Oriented Lossy UTF-8 Line Reader
Replace `reader.lines()` with:
```rust
let mut raw_buf = Vec::new();
loop {
    if stop_flag.load(Ordering::Relaxed) {
        break;
    }
    raw_buf.clear();
    match reader.read_until(b'\n', &mut raw_buf) {
        Ok(0) => break, // Clean EOF
        Ok(_) => {
            // Strip trailing \r and \n
            while raw_buf.ends_with(b"\n") || raw_buf.ends_with(b"\r") {
                raw_buf.pop();
            }
            let line = String::from_utf8_lossy(&raw_buf).to_string();
            buffer.push(line);
            // batching flush logic ...
        }
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
        Err(_) => break,
    }
}
```

### 2.3 Timeout-Bounded `scan_adb_devices`
Implement timeout execution using a worker thread and `crossbeam_channel::bounded(1)`:
```rust
let (tx, rx) = crossbeam_channel::bounded(1);
thread::spawn(move || {
    let res = Command::new("adb").arg("devices").output();
    let _ = tx.send(res);
});
match rx.recv_timeout(timeout) {
    Ok(Ok(output)) => { /* parse devices */ }
    Ok(Err(e)) => Err(format!("Failed to execute adb: {e}")),
    Err(_) => Err(format!("adb devices timed out after {:?}", timeout)),
}
```

### 2.4 Device Serial Trimming
In `start_logcat_stream`, trim `device_serial`:
```rust
if let Some(serial) = device_serial {
    let s = serial.trim();
    if !is_valid_device_serial(s) {
        return Err("Invalid characters or leading dash in device serial".to_string());
    }
    cmd.args(["-s", s]);
}
```

---

## 3. Test Verification Plan

1. Update `tests/engine_tests.rs`:
   - Add `test_serial_security_validation_with_whitespace`
   - Add `test_logcat_lossy_utf8_handling`
   - Add `test_scan_adb_devices_timeout_behavior`
2. Run `cargo test --workspace` and verify 100% pass rate.
3. Update `docs/qa-loganalysis-rust-gpui.md` with verified test results and timings.

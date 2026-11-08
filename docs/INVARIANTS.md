# LogAnalysis System Invariants & Regression Prevention Guide

> **Purpose**: This document specifies the core architectural invariants designed to prevent **silent functional regressions**—ensuring that essential system behaviors and stability guarantees are never inadvertently removed or degraded during feature development or bug fixes.

---

## 1. Native File Dialogs

### `[INV-01-A]` Extension Filter Preservation
- **Open Log File Dialog (`prompt_open_file`)**:
  - `Log Files (*.log, *.txt)` (`&["log", "txt"]`)
  - `All Files (*.*)` (`&["*"]`)
- **Open / Save Filter Profile Dialog (`prompt_load_filters`, `prompt_save_filters`)**:
  - `JSON Profile (*.json)` (`&["json"]`)
  - `All Files (*.*)` (`&["*"]`)
- **Prohibition**: GPUI's built-in `cx.prompt_for_paths` does not support file extension filter dropdowns and must never be used alone for dialogs requiring extension filtering.

### `[INV-01-B]` Modal Hierarchy & Linux X11 Panic Defense
- In GPUI 0.2.2, `X11Window::window_handle()` is written upstream as `unimplemented!()`, causing an immediate panic/crash if called directly.
- Therefore, on Linux X11, `w.window_handle()` must never be called directly; instead, `CustomX11WindowHandle` and `get_active_x11_window_id()` must be used to safely bind window manager modal focus.
- Dialogs must always open modally above the parent `LogAnalysis` window.

---

## 2. Concurrency & Asynchronous Task Cancellation

### `[INV-02-A]` Cancellation Token Independence
- The filter cancellation token (`filter_cancel_gen: Arc<AtomicU64>`) and search cancellation token (`search_cancel_gen: Arc<AtomicU64>`) must be **strictly independent**.
- Typing into or closing the Find Bar must never abort or invalidate an ongoing large-scale background Rayon log filtering pass.

### `[INV-02-B]` UI Main Thread Non-Blocking Rule
- All disk I/O (filter profile JSON load/save, mmap operations, reading large log files) must execute within asynchronous background tasks (via `background_executor`), guaranteeing that the UI rendering thread never stutters or drops frames.

---

## 3. Streaming & Memory Safety

### `[INV-03-A]` ADB Streaming Bounded Line Reader
- `MAX_STREAM_LINE_BYTES` (64 KB): To prevent out-of-memory (OOM) panics caused by infinite streams omitting newline characters, the streaming line reader must enforce a strict per-line byte upper bound.

### `[INV-03-B]` Ring Buffer Trim Hysteresis
- When the live streaming ring buffer reaches maximum capacity (`MAX_MONITOR_LINES = 200,000` lines), it must drain in chunks of at least 10% (`MONITOR_TRIM_BATCH = 20,000` lines down to `180,000` lines) to prevent per-line re-allocation performance degradation.

---

## 4. Text Layout & CJK Alignment

### `[INV-04]` CJK / East Asian 2-Column Monospace Width
- Monospace column alignment in the viewport must treat East Asian characters (Korean, Japanese, Chinese) as occupying 2 column cells rather than 1 ASCII column cell (grounded via `unicode-width`).

---

## 5. CLI Flags & System Identity

### `[INV-05]` CLI Version Flag & Early Termination
- Executing `loganalysis --version`, `-V`, or `-v` in a terminal must immediately output `LogAnalysis 0.2.3` (or `CARGO_PKG_VERSION`) to stdout and terminate cleanly (exit code 0) without initializing the GPU window or GUI runtime.
- The CSD titlebar and status bar must display consistent version badges (`v0.2.3`).

---

## 6. Subprocess Lifecycle & Resource Cleanup

### `[INV-06]` ADB Subprocess Termination on Source Transition & Clear
- When triggering `open_file`, drag-and-drop, or `clear_logs`, if an ADB stream is currently active (`is_streaming == true`), `stop_adb_stream(cx)` must **always be invoked before setting the new source**.
- Failure to invoke cleanup leaves background `adb logcat` processes orphaned/zombified, continuously consuming CPU resources and blocking subsequent streaming sessions.

### `[INV-07]` ADB Serial Validation & Timeout Reaper
- ADB device serial numbers must pass strict alphanumeric/safe punctuation validation (`^[a-zA-Z0-9._:-]+$`) before interpolation into command arguments (neutralizing command injection).
- Device scans (`scan_adb_device_details`) must enforce a strict 3-second timeout, reaping (`kill`) child processes if the ADB daemon becomes unresponsive.

### `[INV-12]` Linux Process Kernel Reaper & Parent Race Defense (`PR_SET_PDEATHSIG`)
- When spawning child subprocesses on Linux (`adb logcat`, `adb devices -l`), commands must configure `cmd.pre_exec` with `libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL)` to ensure the Linux kernel terminates the child immediately if the parent crashes or terminates unexpectedly.
- Furthermore, to close the post-fork reparenting race condition (where the parent terminates before `pre_exec` executes, reparenting the child to `init` or a subreaper), the parent PID must be captured prior to fork (`let parent_pid = libc::getpid()`), and `pre_exec` must verify `if libc::getppid() != parent_pid { libc::_exit(1); }`.

---

## 7. Text Input & IME Safety

### `[INV-08-A]` UTF-16 to UTF-8 Character Boundary Alignment
- In the `TextInput` component supporting multi-byte IME preedit (Korean, Japanese, Chinese), UTF-16 code unit offsets must always be mapped to valid UTF-8 byte boundaries using `char_indices()`.
- Misaligned slicing across multi-byte characters triggers fatal Rust runtime panics (`byte index is not a char boundary`).

### `[INV-08-B]` IME Commit Enter Event Stop Propagation
- When pressing <kbd>Enter</kbd> to commit an in-progress IME composition syllable, `cx.stop_propagation()` must be invoked to prevent key bubbling to parent containers.
- Unstopped propagation causes accidental search triggers or unintended modal dismissals upon syllable completion.

---

## 8. Streaming Hit Counter Synchronization

### `[INV-09]` Exact Counter Subtraction on Ring Buffer Trim
- During extended ADB live streaming, when excess lines are drained from the ring buffer head (`trimmed_count > 0`), both `total_matched_lines` and individual filter hit counters (`filter_counts` and `rule_counts`) must be decremented by the exact number of matches contained in the dropped lines.
- Omitting per-rule counter subtraction causes hit badge counts to drift unbounded above the actual visible lines retained in memory.

---

## 9. File System Atomicity & Visual Contrast

### `[INV-10]` Atomic Filter Profile Saving
- When saving filter profile JSON files, data must not be written directly over the existing file. It must be written to a temporary file (`*.tmp`) first and replaced atomically via `std::fs::rename`.
- This eliminates the risk of file corruption or zero-byte truncation if the process crashes mid-write.

### `[INV-11]` Contrast & Theme Inheritance
- Filter rules specifying only a text foreground color must inherit the system theme background, and rules specifying only a background color must inherit the theme text color.
- Theme switching (<kbd>Ctrl+T</kbd>) must never result in identical text and background colors (invisible text) and must satisfy WCAG AA contrast standards ($\ge 4.5:1$).

---

## 10. Extended Feature Invariants

### `[INV-13]` Dual-Pane Split View Lockstep Correlation
- When scrolling with lockstep synchronization enabled (<kbd>Ctrl+Alt+S</kbd>), correlating secondary rows must execute an $O(\log N)$ binary search over parsed microsecond timestamps, followed by a bounded forward probe for unparsed multi-line exception stack traces.

### `[INV-14]` Multi-File Chronological Merge Stability
- Merging multiple files must preserve causal order using a stable sort over parsed microsecond timestamps. Unparsed lines (such as multiline stack traces) must inherit the timestamp and file origin of their parent log event.

### `[INV-15]` Standalone Zero-Dependency HTML Incident Dossier Export
- Exported investigation dossier reports must be 100% offline and self-contained (zero CDN dependencies). All user-provided text (file names, notes, and log lines) must undergo strict HTML escaping to prevent cross-site scripting (XSS).

---

## 11. Verification Checklist

Before committing changes, the following quality gates must be satisfied:
1. **Automated Test Suite**: `cargo test --workspace` (102/102 tests pass, 0 regressions).
2. **Dialog Specifications**: Verify `*.log`, `*.txt`, `*.json`, `*.*` filters are tested in `dialogs::tests`.
3. **Compilation & Lints**: `cargo check --workspace` and `cargo clippy --workspace` (0 warnings).
4. **Packaging Verification**: Validate build scripts (`./scripts/build-deb.sh`) generate clean release artifacts.

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
- The filter cancellation token (`filter_cancel_gen: Arc<AtomicU64>`) and search cancellation token (`search_ctrl.cancel_gen: Arc<AtomicU64>` in `controllers/search.rs`) must be **strictly independent**.
- Analytics (`analytics_cancel_gen` in `workspace/mod.rs`) and secondary-pane filtering (`secondary_filter_cancel_gen` in `workspace/mod.rs`) maintain their own independent cancellation tokens.
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

### `[INV-05]` CLI Flags, Multi-File Launch & Version Identity
- Executing `loganalysis --help` or `-h` outputs CLI usage and exits cleanly (`0`).
- Executing `loganalysis --version`, `-V`, or `-v` in a terminal must immediately output `LogAnalysis 0.2.4` (or `CARGO_PKG_VERSION`) to stdout and terminate cleanly (exit code `0`) without initializing the GPU window or GUI runtime.
- Unrecognized leading `-...` flags exit with status code `2` (`CliAction::UnknownFlag` in `cli.rs` / `lib.rs`).
- Positional path arguments (including arguments after `--`) open directly via `open_file` when 1 path is provided, or merge chronologically via `open_merged_files` when 2+ paths are provided.
- Consistent version badges (`v0.2.4`) are displayed in the status bar (`components/status_bar.rs`) and onboarding view (`components/log_view.rs`).

---

## 6. Subprocess Lifecycle & Resource Cleanup

### `[INV-06]` ADB Subprocess Termination on Source Transition & Clear
- When triggering `open_file`, `open_merged_files`, drag-and-drop, or `clear_logs`, source switches go through `begin_primary_source_transition` (`workspace/mod.rs`), which checks `is_streaming` and invokes `stop_adb_stream(cx)` **before clearing/setting the new source**.
- Async source completions verify monotonic generation tokens (`primary_source_gen` for primary loads and `SplitViewController::secondary_source_gen` for secondary split loads) so stale background tasks never overwrite newer sources.
- Failure to invoke cleanup leaves background `adb logcat` processes orphaned/zombified, continuously consuming CPU resources and blocking subsequent streaming sessions.

### `[INV-07]` ADB Serial Validation & Timeout Reaper
- ADB device serial numbers must pass strict alphanumeric/safe punctuation validation (`^[a-zA-Z0-9._:-]+$`) before interpolation into command arguments (neutralizing command injection).
- Device scans (`scan_adb_device_details` invoked in `workspace/adb.rs`) enforce a strict `2`-second timeout (`Duration::from_secs(2)`), reaping (`kill`) child processes if the ADB daemon becomes unresponsive.

### `[INV-12]` Linux Process Kernel Reaper & Parent Race Defense (`PR_SET_PDEATHSIG`)
- When spawning child subprocesses on Linux (`adb logcat`, `adb devices -l`), commands must invoke `configure_linux_pdeathsig` (`adb/mod.rs`) to set `cmd.pre_exec` with `libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL)` so the Linux kernel terminates the child immediately if the parent crashes or terminates unexpectedly.
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

### `[INV-10]` Atomic Filter Profile & Recent Files Saving
- When saving filter profile JSON files or recent-file histories, data must not be written directly over the existing file. It must be written via `write_file_atomically` to a unique temporary file (`*.tmp`) first and replaced atomically via `atomic_replace_file` (`MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` on Windows; `std::fs::rename` on Unix in `workspace/filters.rs`, shared by `workspace/recent_files.rs`).
- This eliminates the risk of file corruption or zero-byte truncation if the process crashes mid-write.

### `[INV-11]` Contrast & Theme Inheritance
- Filter rules specifying only a text foreground color must inherit the system theme background, and rules specifying only a background color must inherit the theme text color.
- Theme switching (<kbd>Ctrl+U</kbd> / <kbd>F10</kbd>) must never result in identical text and background colors (invisible text) and must satisfy WCAG AA contrast standards ($\ge 4.5:1$).

---

## 10. Extended Feature Invariants

### `[INV-13]` Dual-Pane Split View Lockstep Correlation
- When scrolling with lockstep synchronization enabled (<kbd>Ctrl+Alt+S</kbd>), correlating secondary rows must execute an $O(\log N)$ binary search over parsed microsecond timestamps (`find_nearest_timestamp_row`), preceded by a bounded **10-line backward** probe (`probe_parent_timestamp` in `workspace/split_view.rs`) for unparsed multi-line exception stack traces.

### `[INV-14]` Multi-File Chronological Merge Stability
- Merging multiple files must preserve causal order using a stable sort over parsed microsecond timestamps. Unparsed lines (such as multiline stack traces) must inherit the timestamp and file origin of their parent log event.

### `[INV-15]` Standalone Zero-Dependency HTML Incident Dossier Export
- Exported investigation dossier reports must be 100% offline and self-contained (zero CDN dependencies). All user-provided text (file names, notes, and log lines) must undergo strict HTML escaping to prevent cross-site scripting (XSS).

---

## 11. Verification Checklist

Before committing changes, the following quality gates must be satisfied:
1. **Workspace Compilation**: `cargo check --workspace --all-targets` (0 warnings).
2. **Strict Lints**: `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings).
3. **Automated Test Suite**: `cargo test --workspace` (351/351 tests pass, 0 failed, 0 ignored).
4. **Rustdoc Validation**: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` (0 warnings).
5. **Supply Chain & Dependency Checks**: `cargo audit` and `cargo deny check`.
6. **Dialog Specifications**: Verify `*.log`, `*.txt`, `*.json`, `*.html`, `*.*` filters are tested in `dialogs::tests`.
7. **Packaging Verification**: Validate build scripts (`./scripts/build-appimage.sh`, `./scripts/build-deb.sh`) generate clean release artifacts.

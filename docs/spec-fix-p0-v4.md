# Specification: P0 Critical Hotfixes (v4)

**Target Codebase:** [`/home/drew/github/hwanjongyu/loganalysis`](file:///home/drew/github/hwanjongyu/loganalysis)  
**Author:** Software Architect & Concurrency Specialist  
**Status:** Approved for Implementation (Autonomous `/goal` Mode)  
**Date:** 2026-11-04  

---

## 1. Problem Statements & Root Cause Analysis

### P0-1: CJK / Multi-byte UTF-8 Char Boundary Panic in `TextInput::select_word_at`
- **Location:** [`crates/loganalysis-gui/src/components/text_input.rs:162-168`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/text_input.rs#L162-L168)
- **Root Cause:** In `select_word_at`, the boundary expansion loop decremented/incremented raw byte indices:
  ```rust
  while start > 0 && !bytes[start - 1].is_ascii_whitespace() {
      start -= 1;
  }
  while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
      end += 1;
  }
  ```
  Multi-byte UTF-8 sequences (Korean Hangul, Chinese, Japanese, Cyrillic, emoji) consist of a leading byte followed by continuation bytes (`0x80..=0xBF`). Because continuation bytes are not ASCII whitespace, `start -= 1` steps into the middle of a code point. When `self.content[start..end]` is sliced or painted, Rust panics with `byte index X is not a char boundary; it is inside '...'`.
- **Target Fix:** Use Unicode Annex #29 word segmentation via `unicode_segmentation::UnicodeSegmentation::split_word_bound_indices` on `self.content`, ensuring all selected ranges strictly align with valid UTF-8 character boundaries.

---

## P0-2: ADB Worker Disconnection Zombie Loop & Unbounded Ingestion Channel
- **Location:** [`crates/loganalysis-gui/src/workspace.rs:358, 380-466`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L358)
- **Root Cause:**
  1. `let (tx, rx) = unbounded();` allocated an unbounded queue. During Android boot bursts or log floods, queue memory could grow unboundedly, risking OOM.
  2. When the ADB subprocess exited or USB disconnected, the reader thread exited and dropped `tx`. `rx.try_recv()` returned `Err(TryRecvError::Disconnected)`. The UI loop ignored the disconnection, leaving `batch` empty and executing `timer_executor.timer(16ms).await`.
  3. Because `workspace.is_streaming` was never set to `false`, the UI remained trapped in a zombie polling loop with a deceptive green "Streaming" status.
- **Target Fix:**
  1. Bound the channel to `crossbeam_channel::bounded(256)`.
  2. Handle `TryRecvError::Disconnected`: if disconnected, immediately transition `workspace.is_streaming = false`, set status text to `"ADB stream disconnected"`, notify GPUI, and terminate the async loop cleanly.

---

## P0-3: Low-Throughput Stream Stall & Indefinite Pipe Trapping in `logcat.rs`
- **Location:** [`crates/loganalysis-core/src/adb/logcat.rs:84-114`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/logcat.rs#L84-L114)
- **Root Cause:** The flush check `buffer.len() >= 100 || (last_flush.elapsed() >= 16ms && !buffer.is_empty())` failed when log lines arrived with inter-arrival times $>16\text{ms}$. Line 1 arrived at $t=0$ (elapsed $\approx 0\text{ms}$), so it was not flushed. The thread immediately blocked synchronously inside `reader.read_until(b'\n')`, trapping the line in memory until the next line arrived seconds or minutes later.
- **Target Fix:** Inspect `reader.buffer()`. If `reader.buffer().is_empty()` after reading a line (no pending bytes in the OS pipe buffer), flush immediately so interactive/sparse logs appear with zero latency.

---

## P0-4: 200,000 Atomic Refcount Operations & Heap Thrashing on Live Ingestion
- **Location:** [`crates/loganalysis-gui/src/workspace.rs:408-414`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L408-L414)
- **Root Cause:** On every batch arrival (up to 60 times/sec), `stream.clone()` cloned the entire `StreamLogBuffer` (`VecDeque<Arc<str>>`), incrementing 200,000 atomic reference counts and re-allocating the ring buffer on the heap on the UI thread.
- **Target Fix:** Use `Arc::get_mut` on `workspace.log_source`. When no background filtering thread holds a read lock, mutate the `StreamLogBuffer` in-place (`append_chunk` and `trim_if_needed`) with **zero pointer clones**. Only fall back to cloning if a concurrent filter task is actively reading the buffer. Also replace `paused_stream_buffer.remove(0)` with `VecDeque::pop_front()` ($O(1)$).

---

## 2. Implementation Plan & Target Files

| Phase | File | Changes |
| :--- | :--- | :--- |
| **1** | `crates/loganalysis-gui/src/components/text_input.rs` | Refactor `select_word_at` to use `split_word_bound_indices`. Add unit test verifying double-click word selection on Korean, Japanese, and emoji strings without panic. |
| **2** | `crates/loganalysis-core/src/adb/logcat.rs` | Update flush condition to check `reader.buffer().is_empty()`. Add unit test for sparse single-line immediate flush. |
| **3** | `crates/loganalysis-gui/src/workspace.rs` | Change ingestion channel to `bounded(256)`. Handle `TryRecvError::Disconnected`. Optimize `stream` mutation via `Arc::get_mut`. Change `paused_stream_buffer` to `VecDeque`. |
| **4** | `crates/loganalysis-core/tests/engine_tests.rs` & GUI tests | Add regression test cases for all 4 P0 fixes. |

---

## 3. Verification & Acceptance Criteria

- `cargo test`: All unit and integration tests must pass cleanly.
- Double-clicking Korean text in `TextInput` must select the word without triggering char boundary panics.
- ADB stream termination must cleanly transition `is_streaming` to `false` and break the polling loop.
- Low-frequency single log lines must flush immediately without stalling.
- In-place mutation of `StreamLogBuffer` must execute with zero ring buffer cloning when unique.

# Technical Specification: Chronological Multi-File Merge View (`P10`)

**Feature:** Multi-File Chronological Merge View & Cross-System Causal Analysis  
**Module:** `crates/loganalysis-core/src/buffer/merged_buffer.rs` & `crates/loganalysis-gui/`  
**Date:** 2026-11-06  
**Status:** **APPROVED FOR IMPLEMENTATION**  

---

## 1. Overview & Objective

When diagnosing distributed anomalies (e.g., Client $\leftrightarrow$ Server RPC drops, Android App $\leftrightarrow$ System Server binder IPC, or Bluetooth Host $\leftrightarrow$ Controller HCI dumps), logs reside in multiple distinct files.

While **P7** provided a side-by-side Dual Split View, **P10 (Chronological Multi-File Merge)** provides a **unified single timeline**:
- Automatically merges 2 or more log files into a continuous chronological stream sorted by microsecond timestamps.
- Origin attribution: Each log line displays an origin badge indicating which file it originated from.
- Works transparently with all existing capabilities: SIMD filtering, search highlighting, bookmarking, HTML incident exports, EPS timeline, and column grid view.

---

## 2. Technical Architecture (`crates/loganalysis-core/src/buffer/merged_buffer.rs`)

### 2.1 Struct Definitions
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergedLineRef {
    pub source_id: u8,
    pub source_line_idx: usize,
    pub timestamp_micros: Option<i64>,
}

pub struct MergedLogBuffer {
    sources: Vec<Arc<FileLogBuffer>>,
    source_names: Vec<String>,
    merged_index: Vec<MergedLineRef>,
    max_line_len: usize,
}
```

### 2.2 Algorithm & Invariants
- **K-Way Merge Sort:**
  - Extracts microsecond timestamps using `extract_timestamp()`.
  - For lines lacking timestamps (e.g. stack traces), inherits the timestamp of the preceding line from the same source to keep stack traces contiguous.
  - Sorts stably with fallback to `(timestamp, source_id, source_line_idx)`.
- **`LogSource` Implementation:**
  - `line_count(&self) -> usize` returns total merged count.
  - `get_raw_line(&self, idx: usize) -> Option<&[u8]>` delegates to `sources[ref.source_id].get_raw_line(ref.source_line_idx)`.
  - `get_line_arc(&self, idx: usize) -> Option<Arc<str>>` zero-copy delegates to the underlying `FileLogBuffer` line cache.

---

## 3. UI/UX Architecture (`crates/loganalysis-gui/`)

### 3.1 Dialog & Action
- Action: `OpenMergedLogs`
- Command Palette: `"File: Open and Merge Multiple Log Files (Chronological)..."`
- Keybinding: `Ctrl+Shift+M` / `Cmd+Shift+M`
- Native multi-file picker: `rfd::AsyncFileDialog::new().pick_files()`.

### 3.2 Origin Attribution in Viewport
- If the active log source is `ActiveLogSource::Merged`:
  - Renders a compact pill badge indicating the file source (`[A]`, `[B]`, etc., or truncated file name with distinct pastel color).
  - Hover tooltip displays the full file name.

---

## 4. Verification Plan
1. **Core Unit Tests:**
   - Interleaved chronological ordering across multiple files.
   - Contiguous stack trace preservation for non-timestamped lines.
   - Large file merge performance and bounds safety.
2. **GUI Integration Tests:**
   - Palette command lookup and keybinding verification.
3. **Workspace Test Suite:**
   - Maintain 100% test pass rate across all packages (`cargo test --workspace`).

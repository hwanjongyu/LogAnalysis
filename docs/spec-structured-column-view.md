# Technical Specification: Structured Column Grid View & Tag/PID Filter (`P9`)

**Feature:** Structured Column Grid View & Fast Logcat Tag/PID Triage  
**Module:** `crates/loganalysis-core/src/buffer/column.rs` & `crates/loganalysis-gui/`  
**Date:** 2026-11-06  
**Status:** **APPROVED FOR IMPLEMENTATION**  

---

## 1. Overview & Objective

Android logcat and enterprise system logs frequently follow well-defined tabular formats, notably `threadtime`:
```
MM-DD HH:MM:SS.mmm  PID   TID  Level Tag     : Message
```
In traditional monospace raw text view, differing PID lengths, long tag names, and irregular whitespace cause horizontal visual jitter, making it difficult to scan for errors, track specific processes, or isolate single component tags.

**LogAnalysis P9** implements:
1. **Zero-Copy / Slice Fast Log Parser (`crates/loganalysis-core/src/buffer/column.rs`)**:
   - Parses `threadtime` format and general timestamped log lines into structured fields:
     - `time: Option<&str>`
     - `pid: Option<&str>`
     - `tid: Option<&str>`
     - `level: Option<LineSeverity>` (V / D / I / W / E / F)
     - `tag: Option<&str>`
     - `message: &str`
   - Unstructured lines (stack traces, dumps, plain logs) seamlessly fallback to displaying the full line in the message column.
2. **Tabular Column Header & Row Grid Layout**:
   - Fixed-width aligned columns:
     - `Line#` (64px)
     - `Time` (130px)
     - `Level` (32px colored badge: `V`, `D`, `I`, `W`, `E`, `F`)
     - `PID/TID` (85px)
     - `Tag` (120px, bold/accent)
     - `Message` (flex-1)
   - Sticky top Column Header bar showing column boundaries and labels.
3. **Toggle Column View (`Ctrl+G` / `Cmd+G`)**:
   - Allows users to toggle between Raw Monospace View and Structured Column Grid View at any time.
   - Command Palette integration: `"View: Toggle Structured Column Grid View"`.
4. **Interactive Tag / PID Quick Filter**:
   - Clicking on a Tag or PID in Column View allows users to quickly isolate that component.

---

## 2. Core Architecture (`crates/loganalysis-core/src/buffer/column.rs`)

### 2.1 Struct Definition
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedColumnLine<'a> {
    pub time: Option<&'a str>,
    pub pid: Option<&'a str>,
    pub tid: Option<&'a str>,
    pub level_char: Option<char>,
    pub level_severity: LineSeverity,
    pub tag: Option<&'a str>,
    pub message: &'a str,
}
```

### 2.2 Parser Invariants
- Fast single-pass slice scan without regex runtime overhead.
- Validates month/day (`MM-DD`) and time (`HH:MM:SS.mmm`).
- Safely handles arbitrary unicode and malformed lines.

---

## 3. UI/UX Architecture (`crates/loganalysis-gui`)

### 3.1 Workspace State
- `pub(crate) is_column_view: bool` (default: false)
- Action: `ToggleColumnView` (`Ctrl+G` / `Cmd+G`)
- Method: `workspace.toggle_column_view(cx)`

### 3.2 List Row Rendering
- If `!workspace.is_column_view`: renders standard single-line text view.
- If `workspace.is_column_view`: renders column grid row with colored level pill and aligned fields.

---

## 4. Verification Plan
1. **Core Unit Tests:**
   - Parse standard Android threadtime format.
   - Parse line with spaces in Tag name.
   - Parse non-logcat multiline stack trace fallback.
2. **GUI Integration Tests:**
   - Command palette fuzzy search for "column" or "grid".
   - Keybinding registration (`ctrl-g`, `cmd-g`).
3. **Workspace Regression:**
   - 100% test pass rate across workspace (`cargo test --workspace`).

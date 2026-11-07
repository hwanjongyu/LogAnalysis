# Technical Specification: Bookmarks ($F2$) & Interactive HTML Incident Report Export

## 1. Overview & Objective

During high-stakes debugging sessions (e.g., system outages, app ANRs, automotive crash triage, server post-mortems), engineers identify key inflection points across millions of lines of logs.

Currently, developers must copy-paste scattered lines into separate text documents or chat windows, losing context, timestamps, and line numbers.

This specification implements:
1. **In-Memory Bookmark Management ($F2$)**: Single-keystroke pinning of critical log rows with visual gutter badges (`🔖`) and cyclic keyboard navigation (`F2`, `Shift+F2`).
2. **Self-Contained Interactive HTML Incident Report**: Generates an executive, zero-dependency HTML document with embedded CSS, client-side live search, dark/light theme switching, severity filtering, and one-click clipboard copying.

---

## 2. Bookmark Engine Design (`crates/loganalysis-core/src/export/`)

### 2.1 Data Structures
```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub source_idx: usize,
    pub line_number: usize,
    pub text: String,
    pub note: Option<String>,
    pub severity: LineSeverity,
    pub created_at_epoch: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkManager {
    bookmarks: Vec<Bookmark>,
    index_set: std::collections::BTreeSet<usize>,
}
```

### 2.2 Navigation Semantics
- **Cyclic Traversal**:
  - `next_bookmark(curr_source_idx)`: Finds the smallest bookmarked `source_idx > curr_source_idx`. If none, wraps around to the first bookmark.
  - `prev_bookmark(curr_source_idx)`: Finds the largest bookmarked `source_idx < curr_source_idx`. If none, wraps around to the last bookmark.
- **Complexity**: $O(\log B)$ via `BTreeSet::range`.

---

## 3. Interactive HTML Incident Report Architecture

### 3.1 Report Structure & Zero-Dependency Philosophy
The exported file must be **100% self-contained**:
- No external CDNs, fonts, or tracking scripts (works entirely offline in air-gapped / secure enterprise networks).
- Fully responsive on desktop and mobile browsers.

### 3.2 HTML Sections
1. **Dossier Header**:
   - Title: `LogAnalysis Incident & Bookmark Report`
   - Metadata: Source File, Export Timestamp, Total Log Volume, Bookmarked Highlights Count.
   - Executive Badges: Error count, Warning count, Total Bookmarks.
   - Theme Toggle: Instant Dark / Light mode switch via client-side JavaScript.
2. **Client-Side Live Filter Bar**:
   - Real-time text search input filtering across log lines and user notes.
   - Quick severity pills (`ALL`, `ERRORS`, `WARNINGS`, `INFO`).
3. **Structured Log Ledger**:
   - Columns: Line #, Severity, Timestamp, Message, Action (Copy).
   - Monospace code font with high-contrast syntax highlighting.

---

## 4. UI/UX Integration (`crates/loganalysis-gui`)

1. **Gutter Visual Indicator**:
   - Lines with active bookmarks render a vibrant bookmark icon (`🔖`) in the left gutter.
2. **Status Bar Counter**:
   - Bottom telemetry displays `🔖 N bookmarks` when $N > 0$.
3. **Command Palette Integration**:
   - `Bookmark: Toggle Bookmark on Selected Line` (`F2`)
   - `Bookmark: Jump to Next Bookmark`
   - `Bookmark: Jump to Previous Bookmark` (`Shift+F2`)
   - `Bookmark: Clear All Bookmarks`
   - `Export: Export Bookmarks to Interactive HTML Report...`

---

## 5. Verification & Test Plan

1. **Core Unit Tests**:
   - `test_bookmark_toggle_and_set`: Add, remove, and duplicate handling.
   - `test_bookmark_cyclic_navigation`: Next and previous wrapping.
   - `test_html_report_xss_escaping`: Ensure `<script>` or malicious HTML in log lines is properly escaped.
   - `test_html_report_generation_offline`: Verify output contains zero external `http://` or `https://` dependencies.
2. **GUI Tests**:
   - Keybinding registration and command execution verification.

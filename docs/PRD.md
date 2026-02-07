# 📄 Product Requirements Document: LogAnalysis (Tauri Edition)

**Status:** Draft | **Version:** 1.0.0 | **Date:** February 6, 2026

---

## 1. Product Overview
**LogAnalysis** is a high-performance, cross-platform log viewer and real-time monitoring tool. It is designed to handle massive log files (multi-gigabyte) and high-velocity live streams (ADB logcat) while maintaining a responsive user interface. By leveraging **Tauri**, the application combines the safety and speed of **Rust** for data processing with the flexibility of modern web technologies for the UI.

## 2. Target Audience
- **Embedded/Android Developers:** For real-time debugging of device logs.
- **QA Engineers:** For analyzing large test execution logs.
- **Systems Engineers:** For post-mortem analysis of server or application logs.

## 3. Core Features (MVP)

### 3.1. High-Performance Log Viewing
- **Frontend Virtualization:** Use high-performance virtual list components (e.g., `react-virtuoso` or `tanstack-virtual`) to render only visible lines, ensuring 60fps scrolling even with millions of rows.
- **Rust Backend Processing:** Offload heavy file I/O, regex indexing, and ADB streaming to the Rust core. Use memory-mapped files or chunked reading for multi-gigabyte logs.

### 3.2. Advanced Filtering Engine
- **Inclusion/Exclusion:** Multi-layered logic to "Include" (show) or "Exclude" (hide) lines.
- **Rust Regex Engine:** Utilize the high-performance `regex` crate in Rust for near-instant pattern matching across large datasets.
- **Visual Color Coding:** User-definable CSS-based background and text colors for specific filters.
- **Tabbed Filter Sets:** Support for multiple workspace tabs, each with independent filter configurations.
- **Persistence:** Save/Load filter profiles as JSON files using the local filesystem via Tauri's FS API.

### 3.3. Real-time Monitoring
- **ADB Integration:** Stream logs directly from Android devices by spawning `adb logcat` as a sidecar or child process.
- **Real-time Pipeline:** Apply active filters in the Rust backend before emitting events to the frontend to minimize IPC overhead.
- **Buffer Management:** Batch incoming log lines in Rust and push updates to the UI at a controlled interval.

## 4. Functional Requirements

### 4.1. File Operations
- **Large File Support:** MUST handle files >2GB by indexing line offsets in Rust rather than loading the entire file into memory.
- **Encoding:** Support for UTF-8 and automatic fallback/detection for legacy encodings.

### 4.2. Navigation & UI
- **Search (Find):** Non-destructive search (Ctrl+F) that highlights matches in the virtualized list.
- **Dynamic Zoom:** Scale UI and font size using standard `Ctrl +` / `Ctrl -` shortcuts.
- **Metadata Toggle:** Optional display of absolute line numbers and timestamps.
- **Smart Copy:** Export selected lines to the system clipboard, preserving or stripping metadata as configured.

### 4.3. Filter Management
- **Toggleable Filters:** Enable/disable filters without removal.
- **Priority Reordering:** Drag-and-drop interface to manage filter precedence.
- **Live Metrics:** Display match counts per filter, updated dynamically as new data arrives or filters change.

## 5. Non-Functional Requirements
- **Performance:** Maintain UI responsiveness during high-load filtering or rapid log ingestion.
- **Security:** Use Tauri's scoped filesystem access and capability system to ensure the app only touches relevant log files.
- **Small Footprint:** Leverage the system WebView to keep the application binary size significantly smaller than Electron-based alternatives.
- **Modern Aesthetic:** Native-feeling "Dark Mode" and "Light Mode" support via CSS variables.

## 6. Architecture Overview (High-Level)
- **Backend:** Rust (Tauri Core).
    - **Crates:** `tokio` (async), `regex` (filtering), `memmap2` (large files).
- **Frontend:** TypeScript + React / Vue / Svelte.
- **IPC:** Tauri Commands for queries, Tauri Events for streaming logs.
- **State Management:** Rust maintains the "Source of Truth" for log indices; the frontend requests visible ranges.

## 7. Roadmap & Future Enhancements
- **[P1] Rust Indexing:** Implement a fast line-indexing system to support random access in giant files.
- **[P2] Custom Parsers:** Support for structured log formats (JSON, CSV, Logfmt) with column-based filtering.
- **[P2] Remote Logs:** Add support for streaming logs via SSH or TCP/UDP sockets.
- **[P3] Plugin System:** Allow users to write JavaScript or Rust extensions for custom log transformations.

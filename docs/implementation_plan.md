# 🛠 Implementation Plan: LogAnalysis

This plan outlines the roadmap for developing the high-performance log viewer using Tauri, Rust, and React.

## Phase 1: Foundation & Scaffolding ✅
- **Project Setup:** Initialize Tauri project with React + TypeScript + Tailwind CSS.
- **UI Architecture:** Setup basic layout with a sidebar (for filter management) and a main viewing area.
- **Theme Support:** Implement Light/Dark mode using CSS variables.

## Phase 2: High-Performance Backend (The "Engine") ✅
- **Line Indexer (Rust):** Implement a background indexer using `memmap2` to map file offsets for random access without loading the file into RAM.
- **Tauri Commands:** Create IPC bridges for:
    - `open_file(path)`: Initiates indexing.
    - `get_log_lines(range)`: Fetches specific lines by index.
    - `get_metadata()`: Returns total line count and file stats.

## Phase 3: Virtualized Frontend ✅
- **Integration:** Implement `react-virtuoso` or `tanstack-virtual` for 60fps scrolling.
- **Data Fetching:** Connect the virtual list to the Rust `get_log_lines` command.
- **Dynamic Zoom:** Implement global state for font-size scaling.

## Phase 4: Advanced Filtering Engine ✅
- **Regex Crate Integration:** Implement a multi-threaded search/filter system in Rust using the `regex` crate.
- **Filter Management:**
    - Sidebar UI for adding "Include" and "Exclude" rules.
    - Color-coding logic (Rust returns line metadata with matching filter IDs; Frontend applies CSS).
- **Tabbed Workspaces:** Allow multiple files or stream views to stay open. (Partial: Single view supported for MVP)

## Phase 5: Real-time Monitoring (ADB) ✅
- **Sidecar/Process Management:** Use Tauri's `Command` API to spawn and pipe `adb logcat`.
- **Streaming Pipeline:** Batch incoming lines in Rust before emitting events to the frontend to prevent IPC saturation.

## Phase 6: Polish & Persistence (Next Steps)
- **Config Storage:** Save filter profiles and user settings to the local app data folder as JSON.
- **Clipboard/Export:** Implement "Smart Copy" to strip/keep timestamps.
- **Final Optimization:** Profile memory usage with large (>5GB) files.
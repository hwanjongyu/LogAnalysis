# 🌌 LogAnalysis

**LogAnalysis** is a high-performance, cross-platform log viewer and real-time monitoring tool. Built with **Tauri**, **Rust**, and **React**, it is designed to handle multi-gigabyte log files and high-velocity ADB streams while maintaining a silky-smooth 60fps user interface.

## 🚀 Key Features

- **⚡ Blazing Fast indexing:** Uses memory-mapped files (`memmap2`) for near-instant access to massive logs without consuming high RAM.
- **📱 Real-time ADB Stream:** Direct integration with `adb logcat` with smart batching to prevent UI lag.
- **🔍 Advanced Filtering:** Multi-threaded Regex engine supporting complex "Include" and "Exclude" logic.
- **🎨 Visual Highlighting:** Customizable color-coding for different filter patterns.
- **🔭 Virtualized View:** Powered by `react-virtuoso` to handle millions of rows efficiently.
- **🔡 Dynamic Scaling:** Adjust font sizes on the fly with shortcuts.

## 🛠 Tech Stack

- **Backend:** Rust, Tauri v2, Tokio, Rayon.
- **Frontend:** React, TypeScript, Tailwind CSS, Lucide Icons.
- **Performance:** Virtualized lists and background indexing threads.

## 📖 Getting Started

### Prerequisites
- [Rust](https://www.rust-lang.org/tools/install)
- [Node.js](https://nodejs.org/)
- [Android SDK Platform-Tools](https://developer.android.com/tools/releases/platform-tools) (for ADB features)

### Installation
1. Clone the repository.
2. Install dependencies:
   ```bash
   npm install
   ```
3. Run in development mode:
   ```bash
   npm run tauri dev
   ```

### Documentation
- [Product Requirements (PRD)](docs/PRD.md)
- [Implementation Plan](docs/implementation_plan.md)
- [User Guide](docs/user_guide.md)

## 🏗 Development

- **Rust Logic:** Located in `src-tauri/src/`. Key modules: `indexer`, `filter`, `adb`.
- **UI Components:** Located in `src/components/`.
- **Styling:** Tailwind CSS with a modern dark-mode-first aesthetic.

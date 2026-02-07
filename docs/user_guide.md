# 📖 LogAnalysis User Guide

LogAnalysis is a high-performance, cross-platform log viewer designed for speed and efficiency. It handles massive log files and live Android logcat streams with ease.

## 🚀 Getting Started

### Opening a Log File
1. Click the **"Open File"** button in the top header or the large folder icon in the center of the screen.
2. Select a log file (`.log`, `.txt`, `.out`) from the dialog.
3. The file will be indexed automatically. A progress bar will appear for large files.

### Using ADB Logcat
1. Connect your Android device via USB and ensure USB Debugging is enabled.
2. Click the **"Start ADB"** button (Play icon) in the header.
3. Live logs will start streaming immediately.
4. Click **"Stop ADB"** to end the session.

## 🔍 Filtering & Search

### Adding Filters
1. In the left sidebar, click the **Plus (+)** icon to add a new filter.
2. Enter a **Regex Pattern** (e.g., `Error`, `\d{4}-\d{2}`, `ComponentX`).
3. Select **"Include"** to only show matching lines, or **"Exclude"** to hide them.
4. (Optional) Choose a **Color** to highlight matching lines in the viewer.

### Managing Filters
- **Toggle:** Click the Eye icon to temporarily enable/disable a filter.
- **Delete:** Click the Trash icon to remove a filter.
- **Debounce:** Filters apply automatically after you stop typing (500ms delay).

### Quick Search
- Use the **Quick Search** bar in the header to filter lines without creating a saved filter. Note: This currently functions as a visual search.

## ⌨️ Shortcuts & Controls

- **Zoom In:** `Ctrl` + `+` or use the Zoom buttons in the footer.
- **Zoom Out:** `Ctrl` + `-`
- **Reset Zoom:** `Ctrl` + `0`
- **Clear View:** Click the Trash icon in the header to clear the current view (useful for ADB streams).

## ⚡ Performance Tips
- **Large Files:** LogAnalysis uses memory mapping, so it can open files larger than your available RAM (tested with 10GB+ files).
- **Filtering:** Complex regex filters on massive files are processed in parallel. Expect near-instant results for simple patterns.

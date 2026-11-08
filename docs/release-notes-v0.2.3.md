**LogAnalysis v0.2.3** is a stability and ergonomics release. It fixes four critical production issues found in the multi-model audit, simplifies the header into a single compact bar, and moves streaming analytics off the UI thread.

---

### 🌟 Key Highlights in v0.2.3

#### 1. 🛡️ Critical Stability & Security Hotfixes
- **ADB Stream Shutdown Deadlock Fixed**: Stopping a saturated logcat stream no longer freezes the UI.
- **UTF-8 Safe Text Input**: No more crashes when editing filters that contain Korean, Japanese, Chinese, or emoji.
- **ReDoS Protection**: Catastrophic-backtracking regex patterns are rejected, and the fallback engine has a runtime backtrack limit.
- **Year-End Rollover**: Logs that span Dec 31 → Jan 01 keep their chronological order in merged views and the EPS timeline.
- **Merged Buffer Guard**: Merge view is capped at 255 sources, preventing index overflow.

#### 2. 🎨 Decluttered Workstation UI
- **Unified 38px Header**: The titlebar and toolbar are now one compact bar.
- **Zen / Focus Mode (`F11` / `Ctrl+Shift+Z`)**: Hides everything except the log view.
- **Contextual Selection Mini-Bar**: Copy, filter by selection, or clear right where you selected.
- **Overflow Menu (`⋯`) & Collapsible Timeline (`Ctrl+Shift+T`)**: Less-used controls move out of the way.
- **Resizable Filter Rules Dock**: Drag to resize, double-click to reset.

#### 3. ⚡ Performance & Platform
- **Background Streaming Analytics**: Heatmap and EPS timeline are computed off the main thread during log bursts.
- **Stack Trace Correlation**: Dual-pane lockstep sync now works on multi-line stack trace rows.
- **Windows Log Paths**: Application logs go to `%LOCALAPPDATA%` / `%APPDATA%`.

#### 4. 📦 Multi-Platform Packaging
- **Linux**: Debian package (`.deb`), portable AppImage (`.AppImage`), and portable tarball (`.tar.gz`).
- **Windows**: Portable 64-bit archive (`.zip`).
- **macOS**: Native Apple Silicon (`aarch64`) & Intel (`x86_64`) binaries (`.zip`).

---

**Full Changelog**: https://github.com/hwanjongyu/LogAnalysis/compare/v0.2.2...v0.2.3

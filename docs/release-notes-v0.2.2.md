We are excited to announce **LogAnalysis v0.2.2**! This release introduces powerful multi-file forensic capabilities, real-time log stream metrics, enhanced accessibility, and full multi-platform distribution.

---

### 🌟 Key Highlights in v0.2.2

#### 1. 🔍 Forensic Analysis & Advanced Navigation
- **Left Activity Bar & Forensic Facets Drawer**: Real-time log tag & PID frequency distributions, log level telemetry, and active ring buffer inspection.
- **Command Palette (`Ctrl+P` / `Cmd+P`)**: Zed-style fuzzy search modal for instant access to actions, tools, and views.
- **Bookmark Drawer (`Ctrl+B`) & Inline Notes (`Alt+Enter`)**: Pin critical lines and attach forensic notes directly in context.
- **Interactive HTML Incident Report Export**: Generate self-contained, interactive incident reports with embedded search and color-coded filters.

#### 2. ⚡ Multi-File & Stream Synchronization
- **Dual-Pane Split View**: Side-by-side file comparison with timestamp-synchronized lockstep scrolling.
- **Chronological Multi-File Merge View**: Interleave logs from multiple devices or services into a unified timeline.
- **Real-Time Delta Time ($\Delta t$) Inspector**: Microsecond-accurate line latency badges to quickly spot bottlenecks.
- **Structured Column Grid View**: High-throughput Android logcat tag, PID, and TID parser.

#### 3. 🎨 UI/UX & Visual Ergonomics
- **GPU Scrollbar Error Heatmap**: Instant macro-level overview of error and warning spikes across millions of lines.
- **EPS Timeline Sparkline & Throughput Profiler**: Real-time events-per-second monitoring.
- **Welcome Onboarding Grid & Recent Files Shelf**: Zero-config quick start for local files and live ADB streams.
- **Pixel-Perfect Polish**: Dynamic hover tooltips, sub-pixel borders, and stack trace indentation.

#### 4. 📦 Multi-Platform Packaging
- **Linux**: Debian package (`.deb`), portable AppImage (`.AppImage`), and portable tarball (`.tar.gz`).
- **Windows**: Portable 64-bit archive (`.zip`).
- **macOS**: Native Apple Silicon (`aarch64`) & Intel (`x86_64`) binaries (`.zip`).

---

**Full Changelog**: https://github.com/hwanjongyu/LogAnalysis/compare/v0.1.1...v0.2.2

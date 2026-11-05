# Technical Specification: Universal Cross-Platform Titlebar & Auto-CSD Detection

**Document ID**: `docs/spec-wayland-titlebar-auto-csd.md`  
**Author**: Antigravity Architecture Team  
**Status**: DRAFT (Awaiting Approval)  
**Target Crates**: `crates/loganalysis-gui`, `crates/loganalysis-core`

---

## 1. Problem Statement & Root Cause Analysis

### 1.1 The Issue
On Linux systems running Wayland (particularly GNOME on Ubuntu, Fedora, Debian, and RHEL), launching `LogAnalysis` natively (without unsetting `WAYLAND_DISPLAY`) results in a window with **no titlebar, no window title, and no minimize/maximize/close controls**.

To work around this, users were previously forced to wrap the launcher with:
```bash
env -u WAYLAND_DISPLAY LogAnalysis-x86_64.AppImage
```
This forces fallback to X11 (XWayland), where the X11 window manager draws server-side decorations (SSD).

### 1.2 Root Cause Analysis
1. **GPUI Default Behavior (`gpui::window.rs:997`)**:
   When `window_decorations` in `WindowOptions` is `None`, GPUI defaults to requesting Server-Side Decorations:
   ```rust
   platform_window.request_decorations(window_decorations.unwrap_or(WindowDecorations::Server));
   ```
2. **GNOME Wayland Protocol Stance (Mutter)**:
   GNOME Mutter intentionally does not support `zxdg_toplevel_decoration_v1` (Server-Side Decorations). When a client application does not render Client-Side Decorations (CSD), GNOME renders no window borders or titlebars at all.
3. **Application Decoupling (`main.rs` & `titlebar.rs`)**:
   - In [`crates/loganalysis-gui/src/main.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/main.rs), `window_decorations` was only set to `Some(WindowDecorations::Client)` if the manual environment variable `LOGANALYSIS_FORCE_CSD` was defined.
   - In [`crates/loganalysis-gui/src/components/titlebar.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/titlebar.rs), `should_render_csd` checked `matches!(window.window_decorations(), Decorations::Client { .. })`. Because `main.rs` requested `Server`, GPUI reported `Decorations::Server`, causing `should_render_csd` to return `false` and render an empty container `div().id("titlebar-empty")`.
4. **Resulting Failure State**:
   - GNOME refuses to draw SSD.
   - LogAnalysis refuses to draw CSD.
   - The user sees a naked, borderless window with no Title or controls.

---

## 2. Proposed Architecture & Solution Design

### 2.1 Dynamic Session Detection (`main.rs`)
Detect the active display server protocol at runtime without requiring user-configured environment variables:

```rust
#[cfg(target_os = "linux")]
fn detect_initial_window_decorations() -> Option<WindowDecorations> {
    if std::env::var("LOGANALYSIS_DISABLE_CSD").is_ok() {
        return Some(WindowDecorations::Server);
    }
    if std::env::var("LOGANALYSIS_FORCE_CSD").is_ok() {
        return Some(WindowDecorations::Client);
    }

    // Check for native Wayland session
    let is_wayland = std::env::var_os("WAYLAND_DISPLAY")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false);

    if is_wayland {
        Some(WindowDecorations::Client)
    } else {
        // Under X11, Server-Side Decorations (SSD) are natively provided by window manager
        None
    }
}
```

### 2.2 Titlebar Render Guardrail Fallback (`titlebar.rs`)
Enhance `should_render_csd(window)` to provide defense-in-depth:

```rust
pub fn should_render_csd(window: &Window) -> bool {
    if std::env::var("LOGANALYSIS_DISABLE_CSD").is_ok() {
        return false;
    }
    if std::env::var("LOGANALYSIS_FORCE_CSD").is_ok() {
        return true;
    }
    if matches!(window.window_decorations(), Decorations::Client { .. }) {
        return true;
    }

    #[cfg(target_os = "linux")]
    {
        // Fail-safe: If running under Wayland, ensure CSD titlebar is always rendered
        std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v.eq_ignore_ascii_case("wayland"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}
```

### 2.3 Companion Enhancement: ADB Home Path Auto-Discovery (`loganalysis-core`)
To completely eliminate the need for custom environment wrappers in `.desktop` files, enhance [`crates/loganalysis-core/src/adb/mod.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/adb/mod.rs):
- Auto-scan `$HOME/Android/Sdk/platform-tools/adb` and `$HOME/android-sdk/platform-tools/adb`.
- Enables ADB detection even when launched from GNOME/KDE desktop launcher without `~/.bashrc` sourcing.

---

## 3. Matrix of Supported Environments

| Environment | Compositor / WM | Selected Decoration | Titlebar Displayed |
| :--- | :--- | :--- | :--- |
| **Ubuntu / Fedora Wayland** | GNOME Mutter | `WindowDecorations::Client` | **Custom CSD Titlebar** (Logo, Title, Controls) |
| **KDE Plasma Wayland** | KWin Wayland | `WindowDecorations::Client` | **Custom CSD Titlebar** (Logo, Title, Controls) |
| **Tiling Wayland** | Sway / Hyprland | `WindowDecorations::Client` | **Custom CSD Titlebar** (Logo, Title, Controls) |
| **Ubuntu / Debian X11** | GNOME X11 / Metacity | `WindowDecorations::Server` | **System Native SSD Window Frame** |
| **XFCE / i3 / Openbox (X11)** | XFWM / i3 | `WindowDecorations::Server` | **System Native SSD Window Frame** |
| **Custom Overrides** | Any | `LOGANALYSIS_FORCE_CSD=1` | **Forces Custom CSD Titlebar** |
| **Custom Overrides** | Any | `LOGANALYSIS_DISABLE_CSD=1` | **Forces System Window Frame** |

---

## 4. Verification & QA Plan

1. **Unit Testing**:
   - Verify `detect_initial_window_decorations()` responds accurately to mocked `WAYLAND_DISPLAY`, `XDG_SESSION_TYPE`, and override env vars.
   - Verify `should_render_csd` logic matrix.
   - Verify `resolve_adb_path()` resolves `$HOME/Android/Sdk/platform-tools/adb` when `ANDROID_HOME` is unset.
2. **Deterministic UI/UX Checks**:
   - Titlebar Left: App icon, "LogAnalysis", GPU badge, version badge (`v0.1.0`).
   - Titlebar Center: Interactive document breadcrumb, double-click to maximize, single-click to drag window (`start_window_move`).
   - Titlebar Right: Minimize (`-`), Maximize (`□`), Close (`✕`).
3. **AppImage Integration**:
   - Verify clean execution from `.desktop` without `env -u WAYLAND_DISPLAY` and without manual `PATH` tweaks.

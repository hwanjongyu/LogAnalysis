use gpui::Window;

pub const LOG_FILTER_LABEL: &str = "Log Files (*.log, *.txt)";
pub const LOG_FILTER_EXTENSIONS: &[&str] = &["log", "txt"];

pub const ALL_FILES_LABEL: &str = "All Files (*.*)";
pub const ALL_FILES_EXTENSIONS: &[&str] = &["*"];

pub const PROFILE_FILTER_LABEL: &str = "JSON Profile (*.json)";
pub const PROFILE_FILTER_EXTENSIONS: &[&str] = &["json"];

pub const HTML_FILTER_LABEL: &str = "HTML Report (*.html)";
pub const HTML_FILTER_EXTENSIONS: &[&str] = &["html"];

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub(crate) struct CustomX11WindowHandle(pub(crate) std::os::raw::c_ulong);

#[cfg(target_os = "linux")]
impl raw_window_handle::HasWindowHandle for CustomX11WindowHandle {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::XlibWindowHandle::new(self.0);
        let raw = raw_window_handle::RawWindowHandle::Xlib(handle);
        unsafe { Ok(raw_window_handle::WindowHandle::borrow_raw(raw)) }
    }
}

#[cfg(target_os = "linux")]
impl raw_window_handle::HasDisplayHandle for CustomX11WindowHandle {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::XlibDisplayHandle::new(None, 0);
        let raw = raw_window_handle::RawDisplayHandle::Xlib(handle);
        unsafe { Ok(raw_window_handle::DisplayHandle::borrow_raw(raw)) }
    }
}

/// Safe wrapper around GPUI's `Window` for Windows platforms.
///
/// In GPUI 0.2.2, `WindowsWindow::display_handle` calls `unimplemented!()`, which panics inside
/// Win32's `CallWindowProcW` callback (`extern "system" fn` cannot unwind) and aborts the process
/// whenever `rfd::AsyncFileDialog::set_parent` queries `HasDisplayHandle::display_handle()`.
///
/// This wrapper forwards `HasWindowHandle::window_handle()` to the underlying `Window` (or any
/// `HasWindowHandle` source) to preserve the Win32 `HWND` modal parent relationship while
/// returning a valid `WindowsDisplayHandle` from `display_handle()`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SafeDialogParent<'a, W: ?Sized>(pub(crate) &'a W);

impl<W: raw_window_handle::HasWindowHandle + ?Sized> raw_window_handle::HasWindowHandle
    for SafeDialogParent<'_, W>
{
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        self.0.window_handle()
    }
}

impl<W: ?Sized> raw_window_handle::HasDisplayHandle for SafeDialogParent<'_, W> {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::WindowsDisplayHandle::new();
        let raw = raw_window_handle::RawDisplayHandle::Windows(handle);
        unsafe { Ok(raw_window_handle::DisplayHandle::borrow_raw(raw)) }
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn get_active_x11_window_id() -> Option<std::os::raw::c_ulong> {
    // 1. Try xprop -root _NET_ACTIVE_WINDOW
    if let Ok(output) = std::process::Command::new("xprop")
        .args(["-root", "_NET_ACTIVE_WINDOW"])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            if let Some(pos) = s.find("0x") {
                let hex_str: String = s[pos + 2..]
                    .chars()
                    .take_while(|c| c.is_ascii_hexdigit())
                    .collect();
                if let Ok(id) = std::os::raw::c_ulong::from_str_radix(&hex_str, 16) {
                    if id > 0 {
                        return Some(id);
                    }
                }
            }
        }
    }

    // 2. Try xdotool getactivewindow
    if let Ok(output) = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            if let Ok(id) = s.trim().parse::<std::os::raw::c_ulong>() {
                if id > 0 {
                    return Some(id);
                }
            }
        }
    }

    None
}

/// Safely attaches a parent window handle to an AsyncFileDialog across Linux (X11 & Wayland), macOS, and Windows.
///
/// - On Linux X11, GPUI 0.2.2's `X11Window::window_handle` invokes `unimplemented!()`.
///   To prevent a fatal crash while keeping the file dialog positioned in front of `LogAnalysis`,
///   X11 attachment is deferred to `attach_dialog_x11_id` via `CustomX11WindowHandle`.
/// - On Windows, GPUI 0.2.2's `WindowsWindow::display_handle` invokes `unimplemented!()`, which panics
///   inside `CallWindowProcW` (`extern "system" fn` cannot unwind) when `rfd` calls `display_handle()`.
///   `SafeDialogParent` wraps the window to forward the Win32 `HWND` (`window_handle()`) while returning
///   a valid `WindowsDisplayHandle`.
pub fn attach_dialog_parent(mut dialog: rfd::AsyncFileDialog, parent: Option<&Window>) -> rfd::AsyncFileDialog {
    #[cfg(target_os = "linux")]
    {
        let is_wayland = std::env::var_os("WAYLAND_DISPLAY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);

        if is_wayland {
            if let Some(w) = parent {
                dialog = dialog.set_parent(w);
            }
        } else {
            // Under X11, GPUI 0.2.2 panics if window_handle is called on Window.
            // X11 dialog attachment must be performed asynchronously via attach_dialog_x11_id
            // on the background executor to prevent blocking the UI render thread (INV-02-B).
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(w) = parent {
            let safe_parent = SafeDialogParent(w);
            dialog = dialog.set_parent(&safe_parent);
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        if let Some(w) = parent {
            dialog = dialog.set_parent(w);
        }
    }

    dialog
}

/// Checks if the current desktop session is running on Linux X11
#[inline]
pub fn is_x11_session() -> bool {
    #[cfg(target_os = "linux")]
    {
        let is_wayland = std::env::var_os("WAYLAND_DISPLAY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        !is_wayland
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Attaches an optional X11 window ID to an AsyncFileDialog via CustomX11WindowHandle.
pub fn attach_dialog_x11_id(mut dialog: rfd::AsyncFileDialog, xid: Option<std::os::raw::c_ulong>) -> rfd::AsyncFileDialog {
    #[cfg(target_os = "linux")]
    {
        if let Some(id) = xid {
            let handle = CustomX11WindowHandle(id);
            dialog = dialog.set_parent(&handle);
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = xid;
    }
    dialog
}

/// Builds the base Open Log File dialog with mandatory extensions (*.log, *.txt, and *.*).
pub fn build_open_log_dialog_base() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new()
        .set_title("Open Log File")
        .add_filter(LOG_FILTER_LABEL, LOG_FILTER_EXTENSIONS)
        .add_filter(ALL_FILES_LABEL, ALL_FILES_EXTENSIONS)
}

/// Builds the base Save Filter Profile dialog with mandatory extensions (*.json and *.*).
pub fn build_save_filters_dialog_base() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new()
        .set_title("Save Filter Profile")
        .set_file_name("filters.json")
        .add_filter(PROFILE_FILTER_LABEL, PROFILE_FILTER_EXTENSIONS)
        .add_filter(ALL_FILES_LABEL, ALL_FILES_EXTENSIONS)
}

/// Builds the base Export Incident Report (HTML) dialog.
pub fn build_export_html_report_dialog_base(default_name: &str) -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new()
        .set_title("Export Incident Report (HTML)")
        .set_file_name(default_name)
        .add_filter(HTML_FILTER_LABEL, HTML_FILTER_EXTENSIONS)
        .add_filter(ALL_FILES_LABEL, ALL_FILES_EXTENSIONS)
}

/// Builds the base Load Filter Profile dialog with mandatory extensions (*.json and *.*).
pub fn build_load_filters_dialog_base() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new()
        .set_title("Load Filter Profile")
        .add_filter(PROFILE_FILTER_LABEL, PROFILE_FILTER_EXTENSIONS)
        .add_filter(ALL_FILES_LABEL, ALL_FILES_EXTENSIONS)
}

/// Builds the Open Log File dialog with mandatory extensions (*.log, *.txt, and *.*) and safe parent attachment.
#[allow(dead_code)]
pub fn build_open_log_dialog(parent: Option<&Window>) -> rfd::AsyncFileDialog {
    attach_dialog_parent(build_open_log_dialog_base(), parent)
}

/// Builds the Save Filter Profile dialog with mandatory extensions (*.json and *.*) and safe parent attachment.
#[allow(dead_code)]
pub fn build_save_filters_dialog(parent: Option<&Window>) -> rfd::AsyncFileDialog {
    attach_dialog_parent(build_save_filters_dialog_base(), parent)
}

/// Builds the Load Filter Profile dialog with mandatory extensions (*.json and *.*) and safe parent attachment.
#[allow(dead_code)]
pub fn build_load_filters_dialog(parent: Option<&Window>) -> rfd::AsyncFileDialog {
    attach_dialog_parent(build_load_filters_dialog_base(), parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invariant_dialog_filter_specifications() {
        // Enforce [INV-01-A]: Log files must include log and txt extensions
        assert!(LOG_FILTER_EXTENSIONS.contains(&"log"));
        assert!(LOG_FILTER_EXTENSIONS.contains(&"txt"));
        assert_eq!(ALL_FILES_EXTENSIONS, &["*"]);

        // Enforce [INV-01-A]: Profile files must include json extension
        assert!(PROFILE_FILTER_EXTENSIONS.contains(&"json"));
        assert!(HTML_FILTER_EXTENSIONS.contains(&"html"));
    }

    #[test]
    fn test_build_dialogs_without_panic() {
        // Even with parent = None, dialog builder must initialize without panic
        let _open_dlg = build_open_log_dialog(None);
        let _save_dlg = build_save_filters_dialog(None);
        let _load_dlg = build_load_filters_dialog(None);
        let _report_dlg = build_export_html_report_dialog_base("incident_report.html");

        // Test base dialogs and X11 ID attachment without panic
        let base_open = build_open_log_dialog_base();
        let _ = attach_dialog_x11_id(base_open, Some(0x12345));
        let base_save = build_save_filters_dialog_base();
        let _ = attach_dialog_x11_id(base_save, None);
        let _base_load = build_load_filters_dialog_base();
        let _ = is_x11_session();
    }

    #[test]
    fn test_async_x11_id_attachment_nonblocking() {
        // Verify that attach_dialog_x11_id safely attaches XID or gracefully skips on None
        let dlg = build_open_log_dialog_base();
        let dlg = attach_dialog_x11_id(dlg, None);
        let dlg = attach_dialog_x11_id(dlg, Some(0xDEADBEEF));
        let _ = dlg;
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_custom_x11_window_handle_raw_types() {
        use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};
        let handle = CustomX11WindowHandle(0x3200004);
        let win = handle.window_handle().expect("Window handle should succeed");
        match win.as_raw() {
            RawWindowHandle::Xlib(xlib) => {
                assert_eq!(xlib.window, 0x3200004);
            }
            _ => panic!("Expected Xlib window handle"),
        }
        assert!(handle.display_handle().is_ok());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_get_active_x11_window_id_no_panic() {
        // Querying X11 active window must be non-panicking even in headless/container environments
        let _ = get_active_x11_window_id();
    }

    /// Regression test for Windows crash at `gpui-0.2.2\src\platform\windows\window.rs:497:9`.
    /// Simulates GPUI's `WindowsWindow` where `window_handle()` returns a valid `Win32WindowHandle`
    /// while `display_handle()` invokes `unimplemented!()`, verifying that `SafeDialogParent`
    /// intercepts `display_handle()` and allows `rfd::AsyncFileDialog::set_parent` to succeed without panic.
    #[test]
    fn test_safe_dialog_parent_prevents_windows_display_handle_panic() {
        use raw_window_handle::{
            DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
            RawWindowHandle, Win32WindowHandle, WindowHandle,
        };
        use std::num::NonZeroIsize;

        struct MockWindowsWindowWithUnimplementedDisplay;

        impl HasWindowHandle for MockWindowsWindowWithUnimplementedDisplay {
            fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
                let raw = RawWindowHandle::Win32(Win32WindowHandle::new(
                    NonZeroIsize::new(0x0012_3456).unwrap(),
                ));
                unsafe { Ok(WindowHandle::borrow_raw(raw)) }
            }
        }

        impl HasDisplayHandle for MockWindowsWindowWithUnimplementedDisplay {
            fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
                unimplemented!("simulates gpui-0.2.2 WindowsWindow::display_handle panic")
            }
        }

        let mock_win = MockWindowsWindowWithUnimplementedDisplay;
        let safe_parent = SafeDialogParent(&mock_win);

        // 1. Verify window_handle forwards the underlying Win32 HWND
        let wh = safe_parent
            .window_handle()
            .expect("window_handle should forward Win32 handle");
        match wh.as_raw() {
            RawWindowHandle::Win32(h) => assert_eq!(h.hwnd.get(), 0x0012_3456),
            _ => panic!("Expected Win32 window handle"),
        }

        // 2. Verify display_handle returns a valid WindowsDisplayHandle instead of panicking
        let dh = safe_parent
            .display_handle()
            .expect("display_handle should return valid WindowsDisplayHandle");
        assert!(matches!(dh.as_raw(), RawDisplayHandle::Windows(_)));

        // 3. Verify rfd::AsyncFileDialog::set_parent does not panic when given SafeDialogParent
        let _dlg = build_open_log_dialog_base().set_parent(&safe_parent);
    }
}

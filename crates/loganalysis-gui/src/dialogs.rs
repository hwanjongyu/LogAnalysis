use gpui::Window;

pub const LOG_FILTER_LABEL: &str = "Log Files (*.log, *.txt)";
pub const LOG_FILTER_EXTENSIONS: &[&str] = &["log", "txt"];

pub const ALL_FILES_LABEL: &str = "All Files (*.*)";
pub const ALL_FILES_EXTENSIONS: &[&str] = &["*"];

pub const PROFILE_FILTER_LABEL: &str = "JSON Profile (*.json)";
pub const PROFILE_FILTER_EXTENSIONS: &[&str] = &["json"];

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
/// On Linux X11, GPUI 0.2.2's `X11Window::window_handle` invokes `unimplemented!()`.
/// To prevent a fatal crash while keeping the file dialog positioned in front of `LogAnalysis`,
/// this function detects X11, queries the active window XID, and binds via `CustomX11WindowHandle`.
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
            // Safely query the active window ID and provide our custom raw handle.
            if let Some(xid) = get_active_x11_window_id() {
                let handle = CustomX11WindowHandle(xid);
                dialog = dialog.set_parent(&handle);
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        if let Some(w) = parent {
            dialog = dialog.set_parent(w);
        }
    }

    dialog
}

/// Builds the Open Log File dialog with mandatory extensions (*.log, *.txt, and *.*) and safe parent attachment.
pub fn build_open_log_dialog(parent: Option<&Window>) -> rfd::AsyncFileDialog {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title("Open Log File")
        .add_filter(LOG_FILTER_LABEL, LOG_FILTER_EXTENSIONS)
        .add_filter(ALL_FILES_LABEL, ALL_FILES_EXTENSIONS);

    attach_dialog_parent(dialog, parent)
}

/// Builds the Save Filter Profile dialog with mandatory extensions (*.json and *.*) and safe parent attachment.
pub fn build_save_filters_dialog(parent: Option<&Window>) -> rfd::AsyncFileDialog {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title("Save Filter Profile")
        .set_file_name("filters.json")
        .add_filter(PROFILE_FILTER_LABEL, PROFILE_FILTER_EXTENSIONS)
        .add_filter(ALL_FILES_LABEL, ALL_FILES_EXTENSIONS);

    attach_dialog_parent(dialog, parent)
}

/// Builds the Load Filter Profile dialog with mandatory extensions (*.json and *.*) and safe parent attachment.
pub fn build_load_filters_dialog(parent: Option<&Window>) -> rfd::AsyncFileDialog {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title("Load Filter Profile")
        .add_filter(PROFILE_FILTER_LABEL, PROFILE_FILTER_EXTENSIONS)
        .add_filter(ALL_FILES_LABEL, ALL_FILES_EXTENSIONS);

    attach_dialog_parent(dialog, parent)
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
    }

    #[test]
    fn test_build_dialogs_without_panic() {
        // Even with parent = None, dialog builder must initialize without panic
        let _open_dlg = build_open_log_dialog(None);
        let _save_dlg = build_save_filters_dialog(None);
        let _load_dlg = build_load_filters_dialog(None);
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
}

pub fn handle_cli_version_flag<I>(args: I) -> bool
where
    I: IntoIterator<Item = String>,
{
    for arg in args {
        if arg == "--version" || arg == "-V" || arg == "-v" {
            println!("LogAnalysis {}", env!("CARGO_PKG_VERSION"));
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_cli_version_flag() {
        assert!(handle_cli_version_flag(vec!["--version".to_string()]));
        assert!(handle_cli_version_flag(vec!["-V".to_string()]));
        assert!(handle_cli_version_flag(vec!["-v".to_string()]));
        assert!(handle_cli_version_flag(vec!["foo.txt".to_string(), "--version".to_string()]));
        assert!(!handle_cli_version_flag(vec!["foo.txt".to_string()]));
        assert!(!handle_cli_version_flag(Vec::<String>::new()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_custom_x11_window_handle_raw_types() {
        use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};
        let handle = crate::workspace::CustomX11WindowHandle(0x123456);
        let win = handle.window_handle().expect("Window handle should succeed");
        match win.as_raw() {
            RawWindowHandle::Xlib(xlib) => {
                assert_eq!(xlib.window, 0x123456);
            }
            _ => panic!("Expected Xlib window handle"),
        }
        assert!(handle.display_handle().is_ok());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_get_active_x11_window_id_no_panic() {
        // Querying X11 active window must be non-panicking even in headless/container environments
        let _ = crate::workspace::get_active_x11_window_id();
    }
}

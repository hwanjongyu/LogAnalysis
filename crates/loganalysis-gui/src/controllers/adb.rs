use std::collections::VecDeque;
use loganalysis_core::{AdbDeviceInfo, LogcatStreamHandle};

/// Controller managing ADB device discovery and streaming state.
#[derive(Default)]
pub struct AdbController {
    pub devices: Vec<String>,
    pub device_details: Vec<AdbDeviceInfo>,
    pub selected_device: Option<String>,
    pub is_device_selector_open: bool,
    pub is_scanning: bool,
    pub stream_handle: Option<LogcatStreamHandle>,
    pub is_streaming: bool,
    pub is_paused: bool,
    pub auto_scroll: bool,
    pub paused_buffer: VecDeque<Vec<String>>,
    pub paused_line_count: usize,
}

impl AdbController {
    pub fn new() -> Self {
        Self {
            auto_scroll: true,
            ..Default::default()
        }
    }

    /// Selects an ADB device serial.
    pub fn select_device(&mut self, serial: String) {
        self.selected_device = Some(serial);
        self.is_device_selector_open = false;
    }

    /// Returns reference to selected device details if matched.
    pub fn selected_device_info(&self) -> Option<&AdbDeviceInfo> {
        let serial = self.selected_device.as_ref()?;
        self.device_details.iter().find(|d| &d.serial == serial)
    }

    /// Stops any active ADB stream handle and clears streaming flag.
    pub fn stop_stream(&mut self) {
        if let Some(handle) = self.stream_handle.take() {
            handle.stop();
        }
        self.is_streaming = false;
    }

    /// Resets paused streaming buffer.
    pub fn clear_paused_buffer(&mut self) {
        self.paused_buffer.clear();
        self.paused_line_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adb_controller_defaults_and_selection() {
        let mut controller = AdbController::new();
        assert!(controller.auto_scroll);
        assert!(!controller.is_streaming);
        assert!(controller.selected_device.is_none());

        controller.select_device("device_123".into());
        assert_eq!(controller.selected_device.as_deref(), Some("device_123"));
        assert!(!controller.is_device_selector_open);
    }
}

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
    pub scan_gen: u64,
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

    /// Stops any active ADB stream handle and clears streaming/pause state.
    pub fn stop_stream(&mut self) {
        if let Some(handle) = self.stream_handle.take() {
            handle.stop();
        }
        self.is_streaming = false;
        self.is_paused = false;
        self.clear_paused_buffer();
    }

    /// Resets paused streaming buffer.
    pub fn clear_paused_buffer(&mut self) {
        self.paused_buffer.clear();
        self.paused_line_count = 0;
    }

    /// Returns a read-only state snapshot of the ADB streaming controller.
    pub fn state(&self) -> crate::types::AdbStreamState {
        crate::types::AdbStreamState {
            selected_device: self.selected_device.clone(),
            is_device_selector_open: self.is_device_selector_open,
            is_scanning_adb: self.is_scanning,
            is_streaming: self.is_streaming,
            is_stream_paused: self.is_paused,
            auto_scroll: self.auto_scroll,
            paused_stream_line_count: self.paused_line_count,
        }
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

    #[test]
    fn test_adb_controller_stop_stream_resets_pause_and_buffer() {
        let mut controller = AdbController::new();
        controller.is_streaming = true;
        controller.is_paused = true;
        controller.paused_line_count = 10;
        controller
            .paused_buffer
            .push_back(vec!["line1".to_string(), "line2".to_string()]);

        controller.stop_stream();
        assert!(!controller.is_streaming);
        assert!(!controller.is_paused);
        assert_eq!(controller.paused_line_count, 0);
        assert!(controller.paused_buffer.is_empty());
    }
}

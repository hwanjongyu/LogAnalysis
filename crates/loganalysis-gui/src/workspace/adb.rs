use std::sync::Arc;
use std::time::Duration;
use crossbeam_channel::{bounded, TryRecvError};
use gpui::*;
use loganalysis_core::*;

use crate::types::InputFocusTarget;
use super::LogAnalysisWorkspace;

impl LogAnalysisWorkspace {
    /// Returns rich metadata for currently selected ADB device
    pub fn get_selected_device_info(&self) -> Option<&loganalysis_core::AdbDeviceInfo> {
        let serial = self.selected_device.as_ref()?;
        self.adb_device_details.iter().find(|d| &d.serial == serial)
    }

    /// Selects an ADB device by serial and updates state
    pub fn select_adb_device(&mut self, serial: String, cx: &mut Context<Self>) {
        self.selected_device = Some(serial.clone());
        self.is_device_selector_open = false;
        if let Some(dev) = self.adb_device_details.iter().find(|d| d.serial == serial) {
            self.status_text = format!("Selected ADB device: {}", dev.display_name()).into();
        } else {
            self.status_text = format!("Selected ADB device: {}", serial).into();
        }
        cx.notify();
    }

    /// Toggles the floating device selector dropdown
    pub fn toggle_device_selector(&mut self, cx: &mut Context<Self>) {
        self.is_device_selector_open = !self.is_device_selector_open;
        if self.is_device_selector_open {
            self.is_overflow_menu_open = false;
        }
        cx.notify();
    }

    /// Closes the floating device selector dropdown if open
    pub fn close_device_selector(&mut self, cx: &mut Context<Self>) {
        if self.is_device_selector_open {
            self.is_device_selector_open = false;
            cx.notify();
        }
    }

    /// Scans for connected ADB devices asynchronously with rich metadata
    pub fn refresh_adb_devices(&mut self, cx: &mut Context<Self>) {
        self.is_scanning_adb = true;
        self.status_text = "Scanning ADB devices...".into();
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();

            async move {
                let res = bg_executor
                    .spawn(async move { loganalysis_core::scan_adb_device_details(Duration::from_secs(2)) })
                    .await;

                let _ = this.update(&mut async_cx, |workspace, cx| {
                    workspace.is_scanning_adb = false;
                    match res {
                        Ok(devices) => {
                            let ready_devices: Vec<String> = devices
                                .iter()
                                .filter(|d| d.is_ready())
                                .map(|d| d.serial.clone())
                                .collect();

                            // Preserve existing selection if still available and ready
                            if let Some(curr) = &workspace.selected_device {
                                if !ready_devices.contains(curr) {
                                    workspace.selected_device = ready_devices.first().cloned();
                                }
                            } else {
                                workspace.selected_device = ready_devices.first().cloned();
                            }

                            workspace.adb_devices = ready_devices;
                            workspace.adb_device_details = devices;

                            if let Some(info) = workspace.get_selected_device_info() {
                                workspace.status_text = format!(
                                    "ADB connected: {} ({} found)",
                                    info.short_name(),
                                    workspace.adb_device_details.len()
                                )
                                .into();
                            } else if !workspace.adb_device_details.is_empty() {
                                workspace.status_text = format!(
                                    "Found {} ADB devices (none authorized)",
                                    workspace.adb_device_details.len()
                                )
                                .into();
                            } else {
                                workspace.status_text = "No ADB devices connected".into();
                            }
                        }
                        Err(e) => {
                            workspace.status_text = format!("ADB scan error: {}", e).into();
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Starts streaming logcat from selected ADB device with zero-copy chunk ingestion
    pub fn start_adb_stream(&mut self, cx: &mut Context<Self>) {
        if self.is_streaming {
            return;
        }

        // Close device selector dropdown if open
        self.is_device_selector_open = false;

        // Auto-select first available ready device if none selected
        if self.selected_device.is_none() {
            if let Some(first) = self.adb_devices.first() {
                self.selected_device = Some(first.clone());
            }
        }

        let serial = self.selected_device.as_deref();
        let target_display = if let Some(info) = self.get_selected_device_info() {
            info.display_name()
        } else if let Some(s) = serial {
            s.to_string()
        } else {
            "Default Device".to_string()
        };

        // Bounded ingestion channel (256 chunks) prevents unbounded memory blowup on bursts
        let (tx, rx) = bounded(256);
        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

        match start_logcat_stream(serial, tx, stop_flag.clone()) {
            Ok(handle) => {
                self.adb_stream_handle = Some(handle);
                self.is_streaming = true;

                let stream_buf = StreamLogBuffer::default();
                self.log_source = Some(Arc::new(ActiveLogSource::Stream(stream_buf)));
                self.active_file_name = None;
                self.bookmarks.clear();
                self.facet_tags.clear();
                self.status_text = format!("Streaming live ADB: {}", target_display).into();

                // Non-blocking async loop with batching, throttling & bounded buffers to avoid starving GPUI executor
                cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                    let mut async_cx = cx.clone();
                    let timer_executor = cx.background_executor().clone();
                    async move {
                        let mut last_reapply = std::time::Instant::now();
                        let mut pending_filter_reapply = false;
                        let mut accumulated_trimmed = 0usize;

                        // P1 Fix: Throttle HeatmapData::compute and EpsTimelineData::compute
                        // to at most once every 150-200ms during active streaming, or when stream is idle/paused,
                        // preventing Rayon parallel reductions from blocking the GPUI main thread during high-rate bursts.
                        const STREAMING_ANALYTICS_THROTTLE: Duration = Duration::from_millis(150);
                        let mut last_analytics_compute = std::time::Instant::now()
                            .checked_sub(STREAMING_ANALYTICS_THROTTLE)
                            .unwrap_or_else(std::time::Instant::now);
                        let mut pending_analytics_compute = false;
                        let mut analytics_seq_local = 0u64;

                        loop {
                            let mut batch = Vec::new();
                            let mut is_disconnected = false;

                            // Drain available chunks up to 5000 lines
                            loop {
                                match rx.try_recv() {
                                    Ok(chunk) => {
                                        batch.extend(chunk);
                                        if batch.len() >= 5000 {
                                            break;
                                        }
                                    }
                                    Err(TryRecvError::Empty) => break,
                                    Err(TryRecvError::Disconnected) => {
                                        is_disconnected = true;
                                        break;
                                    }
                                }
                            }

                            if !batch.is_empty() {
                                let should_compute_analytics =
                                    last_analytics_compute.elapsed() >= STREAMING_ANALYTICS_THROTTLE || is_disconnected;

                                let (should_continue, tr, analytics_payload) = this
                                    .update(&mut async_cx, |workspace, cx| {
                                        let mut trimmed_count = 0;
                                        let mut analytics_payload = None;

                                        if workspace.is_stream_paused {
                                            // Buffer chunks with bounded capacity to prevent OOM
                                            let chunk_len = batch.len();
                                            workspace.paused_stream_line_count += chunk_len;
                                            workspace.paused_stream_buffer.push_back(batch);

                                            while workspace.paused_stream_line_count > loganalysis_core::MAX_MONITOR_LINES {
                                                if let Some(front) = workspace.paused_stream_buffer.pop_front() {
                                                    workspace.paused_stream_line_count =
                                                        workspace.paused_stream_line_count.saturating_sub(front.len());
                                                } else {
                                                    break;
                                                }
                                            }
                                        } else if let Some(src) = &mut workspace.log_source {
                                            let batch_len = batch.len();
                                            let (new_total_len, new_lines_range, trimmed) = {
                                                // Zero-copy in-place mutation when unique; fallback to clone only if filter task holds Arc
                                                let (len, tr) = if let Some(active_src) = Arc::get_mut(src) {
                                                    if let ActiveLogSource::Stream(ref mut stream) = active_src {
                                                        stream.append_chunk(batch);
                                                        let tr = stream.trim_if_needed().unwrap_or(0);
                                                        (stream.line_count(), tr)
                                                    } else {
                                                        (0, 0)
                                                    }
                                                } else if let ActiveLogSource::Stream(ref stream) = **src {
                                                    let mut new_stream = stream.clone();
                                                    new_stream.append_chunk(batch);
                                                    let tr = new_stream.trim_if_needed().unwrap_or(0);
                                                    let len = new_stream.line_count();
                                                    *src = Arc::new(ActiveLogSource::Stream(new_stream));
                                                    (len, tr)
                                                } else {
                                                    (0, 0)
                                                };
                                                let start = len.saturating_sub(batch_len);
                                                (len, start..len, tr)
                                            };
                                            trimmed_count = trimmed;

                                            // Incremental filtering: evaluate only newly arrived lines without full buffer Rayon pass
                                            if let (Some(filter_res), Some(src_ref)) = (&mut workspace.filter_result, &workspace.log_source) {
                                                let show_only = workspace.show_only_filtered;
                                                let mut active_filters = Vec::new();
                                                for tab in &workspace.filter_tabs {
                                                    if tab.enabled {
                                                        for rule in &tab.filters {
                                                            if rule.active {
                                                                 active_filters.push(rule.clone());
                                                            }
                                                        }
                                                    }
                                                }

                                                let res_mut = Arc::make_mut(filter_res);
                                                append_streaming_batch(
                                                    res_mut,
                                                    &**src_ref,
                                                    new_lines_range,
                                                    trimmed,
                                                    &active_filters,
                                                    show_only,
                                                );

                                                let visible_count = res_mut.visible_indices.len();
                                                let total_matched = res_mut.total_matched_lines;

                                                // Offload heavy Rayon HeatmapData & EpsTimelineData computation to background executor
                                                if should_compute_analytics {
                                                    let is_tl = workspace.is_timeline_visible;
                                                    analytics_payload = Some((
                                                        Arc::clone(src_ref),
                                                        res_mut.visible_indices.clone(),
                                                        res_mut.visible_styles.clone(),
                                                        is_tl,
                                                    ));
                                                }

                                                workspace.status_text = format!(
                                                    "Visible: {} / {} lines ({} matched)",
                                                    visible_count,
                                                    new_total_len,
                                                    total_matched
                                                ).into();
                                            } else {
                                                // Initial pass if filter_result was not yet computed
                                                workspace.reapply_filters(cx);
                                            }

                                            if workspace.facet_tags.is_empty() && new_total_len >= 100 {
                                                workspace.refresh_facet_tags();
                                            }

                                            // Smart Auto-Scroll to bottom if enabled
                                            if workspace.auto_scroll {
                                                if let Some(res) = &workspace.filter_result {
                                                    let count = res.visible_indices.len();
                                                    if count > 0 {
                                                        workspace.scroll_handle.scroll_to_item(
                                                            count - 1,
                                                            ScrollStrategy::Bottom,
                                                        );
                                                    }
                                                }
                                            }
                                        }

                                        if is_disconnected {
                                            workspace.is_streaming = false;
                                            workspace.status_text = "ADB stream disconnected".into();
                                        }

                                        cx.notify();
                                        (workspace.is_streaming, trimmed_count, analytics_payload)
                                    })
                                    .unwrap_or((false, 0, None));

                                if let Some((src_arc, indices, styles, is_tl)) = analytics_payload {
                                    analytics_seq_local += 1;
                                    let seq = analytics_seq_local;

                                    let (heatmap, timeline) = timer_executor.spawn(async move {
                                        let hm = Arc::new(HeatmapData::compute(&*src_arc, &indices, &styles, 200));
                                        let tl = if is_tl {
                                            Some(Arc::new(EpsTimelineData::compute(&*src_arc, &indices, &styles, 80)))
                                        } else {
                                            None
                                        };
                                        (hm, tl)
                                    }).await;

                                    let _ = this.update(&mut async_cx, |workspace, cx| {
                                        if workspace.analytics_seq <= seq {
                                            workspace.analytics_seq = seq;
                                            workspace.heatmap_data = Some(heatmap);
                                            if timeline.is_some() {
                                                workspace.timeline_data = timeline;
                                            }
                                            cx.notify();
                                        }
                                    });

                                    last_analytics_compute = std::time::Instant::now();
                                    pending_analytics_compute = false;
                                } else {
                                    pending_analytics_compute = !should_compute_analytics;
                                }

                                if tr > 0 {
                                    accumulated_trimmed += tr;
                                    if accumulated_trimmed >= 10_000 || last_reapply.elapsed() >= Duration::from_secs(5) {
                                        pending_filter_reapply = true;
                                    }
                                }

                                if !should_continue || is_disconnected {
                                    break;
                                }
                            } else if is_disconnected {
                                // Subprocess closed pipe: cleanly exit streaming state and terminate async loop
                                let analytics_payload = this.update(&mut async_cx, |workspace, cx| {
                                    workspace.is_streaming = false;
                                    workspace.status_text = "ADB stream disconnected".into();
                                    cx.notify();

                                    if pending_analytics_compute {
                                        if let (Some(filter_res), Some(src_ref)) = (&workspace.filter_result, &workspace.log_source) {
                                            let is_tl = workspace.is_timeline_visible;
                                            return Some((
                                                Arc::clone(src_ref),
                                                filter_res.visible_indices.clone(),
                                                filter_res.visible_styles.clone(),
                                                is_tl,
                                            ));
                                        }
                                    }
                                    None
                                }).unwrap_or(None);

                                if let Some((src_arc, indices, styles, is_tl)) = analytics_payload {
                                    analytics_seq_local += 1;
                                    let seq = analytics_seq_local;

                                    let (heatmap, timeline) = timer_executor.spawn(async move {
                                        let hm = Arc::new(HeatmapData::compute(&*src_arc, &indices, &styles, 200));
                                        let tl = if is_tl {
                                            Some(Arc::new(EpsTimelineData::compute(&*src_arc, &indices, &styles, 80)))
                                        } else {
                                            None
                                        };
                                        (hm, tl)
                                    }).await;

                                    let _ = this.update(&mut async_cx, |workspace, cx| {
                                        if workspace.analytics_seq <= seq {
                                            workspace.analytics_seq = seq;
                                            workspace.heatmap_data = Some(heatmap);
                                            if timeline.is_some() {
                                                workspace.timeline_data = timeline;
                                            }
                                            cx.notify();
                                        }
                                    });
                                }
                                break;
                            } else {
                                // Flush any pending throttled filter reapply when channel is empty
                                if pending_filter_reapply && last_reapply.elapsed() >= Duration::from_millis(60) {
                                    let _ = this.update(&mut async_cx, |workspace, cx| {
                                        if !workspace.is_stream_paused {
                                            workspace.reapply_filters(cx);
                                        }
                                    });
                                    last_reapply = std::time::Instant::now();
                                    pending_filter_reapply = false;
                                    accumulated_trimmed = 0;
                                    pending_analytics_compute = false;
                                } else if pending_analytics_compute {
                                    // Flush pending throttled heatmap/timeline compute asynchronously when channel is idle or paused
                                    let analytics_payload = this.update(&mut async_cx, |workspace, _cx| {
                                        if let (Some(filter_res), Some(src_ref)) = (&workspace.filter_result, &workspace.log_source) {
                                            let is_tl = workspace.is_timeline_visible;
                                            Some((
                                                Arc::clone(src_ref),
                                                filter_res.visible_indices.clone(),
                                                filter_res.visible_styles.clone(),
                                                is_tl,
                                            ))
                                        } else {
                                            None
                                        }
                                    }).unwrap_or(None);

                                    if let Some((src_arc, indices, styles, is_tl)) = analytics_payload {
                                        analytics_seq_local += 1;
                                        let seq = analytics_seq_local;

                                        let (heatmap, timeline) = timer_executor.spawn(async move {
                                            let hm = Arc::new(HeatmapData::compute(&*src_arc, &indices, &styles, 200));
                                            let tl = if is_tl {
                                                Some(Arc::new(EpsTimelineData::compute(&*src_arc, &indices, &styles, 80)))
                                            } else {
                                                None
                                            };
                                            (hm, tl)
                                        }).await;

                                        let _ = this.update(&mut async_cx, |workspace, cx| {
                                            if workspace.analytics_seq <= seq {
                                                workspace.analytics_seq = seq;
                                                workspace.heatmap_data = Some(heatmap);
                                                if timeline.is_some() {
                                                    workspace.timeline_data = timeline;
                                                }
                                                cx.notify();
                                            }
                                        });
                                    }

                                    last_analytics_compute = std::time::Instant::now();
                                    pending_analytics_compute = false;
                                }

                                // Yield execution cooperatively to GPUI async executor
                                timer_executor.timer(Duration::from_millis(16)).await;
                                let still_streaming = this
                                    .update(&mut async_cx, |workspace, _| workspace.is_streaming)
                                    .unwrap_or(false);
                                if !still_streaming {
                                    break;
                                }
                            }
                        }
                    }
                })
                .detach();
            }
            Err(e) => {
                self.status_text = format!("Failed to start ADB stream: {}", e).into();
            }
        }
        cx.notify();
    }

    /// Stops ADB streaming cleanly via RAII handle
    pub fn stop_adb_stream(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.adb_stream_handle.take() {
            handle.stop();
        }
        self.is_streaming = false;
        self.status_text = "ADB stream stopped".into();
        cx.notify();
    }

    /// Toggles ADB stream pause and flushes buffered chunks when resumed
    pub fn toggle_pause_stream(&mut self, cx: &mut Context<Self>) {
        if self.input_focus != InputFocusTarget::None {
            return;
        }
        if self.is_streaming {
            self.is_stream_paused = !self.is_stream_paused;
            if !self.is_stream_paused {
                // Flush buffered chunks
                let buffered = std::mem::take(&mut self.paused_stream_buffer);
                self.paused_stream_line_count = 0;
                if !buffered.is_empty() {
                    if let Some(src) = &mut self.log_source {
                        if let Some(active_src) = Arc::get_mut(src) {
                            if let ActiveLogSource::Stream(ref mut stream) = active_src {
                                for chunk in buffered {
                                    stream.append_chunk(chunk);
                                }
                                stream.trim_if_needed();
                            }
                        } else if let ActiveLogSource::Stream(ref stream) = **src {
                            let mut new_stream = stream.clone();
                            for chunk in buffered {
                                new_stream.append_chunk(chunk);
                            }
                            new_stream.trim_if_needed();
                            *src = Arc::new(ActiveLogSource::Stream(new_stream));
                        }
                        self.reapply_filters(cx);
                    }
                }
                self.status_text = "ADB streaming RESUMED".into();
            } else {
                self.update_analytics_data();
                self.status_text = "ADB streaming PAUSED (Press Ctrl+Space to resume)".into();
            }
            cx.notify();
        }
    }
}

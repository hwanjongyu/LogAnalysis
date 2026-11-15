use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use crossbeam_channel::{bounded, TryRecvError};
use gpui::*;
use loganalysis_core::*;

use crate::types::InputFocusTarget;
use super::LogAnalysisWorkspace;

#[derive(Clone)]
pub(crate) struct StreamAnalyticsJob {
    src: Arc<ActiveLogSource>,
    visible_indices: Vec<usize>,
    visible_styles: Vec<ResolvedStyle>,
    include_timeline: bool,
    analytics_gen: u64,
}

async fn compute_and_apply_stream_analytics(
    this: &WeakEntity<LogAnalysisWorkspace>,
    async_cx: &mut AsyncApp,
    bg_executor: &BackgroundExecutor,
    job: StreamAnalyticsJob,
) {
    let src_check = Arc::clone(&job.src);
    let src_for_compute = job.src;
    let indices = job.visible_indices;
    let styles = job.visible_styles;
    let include_timeline = job.include_timeline;
    let analytics_gen = job.analytics_gen;

    let (heatmap, timeline) = bg_executor
        .spawn(async move {
            let hm = Arc::new(HeatmapData::compute(&*src_for_compute, &indices, &styles, 200));
            let tl = if include_timeline {
                Some(Arc::new(EpsTimelineData::compute(
                    &*src_for_compute,
                    &indices,
                    &styles,
                    80,
                )))
            } else {
                None
            };
            (hm, tl)
        })
        .await;

    let _ = this.update(async_cx, |workspace, cx| {
        if workspace.analytics_cancel_gen.load(Ordering::SeqCst) == analytics_gen
            && workspace
                .log_source
                .as_ref()
                .is_some_and(|s| Arc::ptr_eq(s, &src_check))
        {
            workspace.heatmap_data = Some(heatmap);
            if timeline.is_some() && workspace.is_timeline_visible {
                workspace.timeline_data = timeline;
            }
            cx.notify();
        }
    });
}

impl LogAnalysisWorkspace {
    /// Returns rich metadata for currently selected ADB device
    pub fn get_selected_device_info(&self) -> Option<&loganalysis_core::AdbDeviceInfo> {
        let serial = self.adb.selected_device.as_ref()?;
        self.adb.device_details.iter().find(|d| &d.serial == serial)
    }

    /// Selects an ADB device by serial and updates state
    pub fn select_adb_device(&mut self, serial: String, cx: &mut Context<Self>) {
        self.adb.selected_device = Some(serial.clone());
        self.adb.is_device_selector_open = false;
        if let Some(dev) = self.adb.device_details.iter().find(|d| d.serial == serial) {
            self.status_text = format!("Selected ADB device: {}", dev.display_name()).into();
        } else {
            self.status_text = format!("Selected ADB device: {}", serial).into();
        }
        cx.notify();
    }

    /// Toggles the floating device selector dropdown
    pub fn toggle_device_selector(&mut self, cx: &mut Context<Self>) {
        self.adb.is_device_selector_open = !self.adb.is_device_selector_open;
        if self.adb.is_device_selector_open {
            self.is_overflow_menu_open = false;
        }
        cx.notify();
    }

    /// Closes the floating device selector dropdown if open
    pub fn close_device_selector(&mut self, cx: &mut Context<Self>) {
        if self.adb.is_device_selector_open {
            self.adb.is_device_selector_open = false;
            cx.notify();
        }
    }

    /// Scans for connected ADB devices asynchronously with rich metadata
    pub fn refresh_adb_devices(&mut self, cx: &mut Context<Self>) {
        self.adb.is_scanning = true;
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
                    workspace.adb.is_scanning = false;
                    match res {
                        Ok(devices) => {
                            let ready_devices: Vec<String> = devices
                                .iter()
                                .filter(|d| d.is_ready())
                                .map(|d| d.serial.clone())
                                .collect();

                            // Preserve existing selection if still available and ready
                            if let Some(curr) = &workspace.adb.selected_device {
                                if !ready_devices.contains(curr) {
                                    workspace.adb.selected_device = ready_devices.first().cloned();
                                }
                            } else {
                                workspace.adb.selected_device = ready_devices.first().cloned();
                            }

                            workspace.adb.devices = ready_devices;
                            workspace.adb.device_details = devices;

                            if let Some(info) = workspace.get_selected_device_info() {
                                workspace.status_text = format!(
                                    "ADB connected: {} ({} found)",
                                    info.short_name(),
                                    workspace.adb.device_details.len()
                                )
                                .into();
                            } else if !workspace.adb.device_details.is_empty() {
                                workspace.status_text = format!(
                                    "Found {} ADB devices (none authorized)",
                                    workspace.adb.device_details.len()
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
        if self.adb.is_streaming {
            return;
        }

        // Close device selector dropdown if open
        self.adb.is_device_selector_open = false;

        // Auto-select first available ready device if none selected
        if self.adb.selected_device.is_none() {
            if let Some(first) = self.adb.devices.first() {
                self.adb.selected_device = Some(first.clone());
            }
        }

        let serial = self.adb.selected_device.as_deref();
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
                self.adb.stream_handle = Some(handle);
                self.adb.is_streaming = true;

                self.reset_derived_log_state();
                let stream_buf = StreamLogBuffer::default();
                self.log_source = Some(Arc::new(ActiveLogSource::Stream(stream_buf)));
                self.active_file_name = None;
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

                                let (should_continue, tr, analytics_job) = this
                                    .update(&mut async_cx, |workspace, cx| {
                                        let mut trimmed_count = 0;
                                        let mut analytics_job = None;

                                        if workspace.adb.is_paused {
                                            // Buffer chunks with bounded capacity to prevent OOM
                                            let chunk_len = batch.len();
                                            workspace.adb.paused_line_count += chunk_len;
                                            workspace.adb.paused_buffer.push_back(batch);

                                            while workspace.adb.paused_line_count > loganalysis_core::MAX_MONITOR_LINES {
                                                if let Some(front) = workspace.adb.paused_buffer.pop_front() {
                                                    workspace.adb.paused_line_count =
                                                        workspace.adb.paused_line_count.saturating_sub(front.len());
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
                                            if trimmed > 0 {
                                                let dropped_visible = workspace
                                                    .filter_result
                                                    .as_ref()
                                                    .map_or(0, |res| {
                                                        res.visible_indices
                                                            .partition_point(|&idx| idx < trimmed)
                                                    });
                                                workspace.bookmarks.shift_on_trim(trimmed);
                                                workspace
                                                    .selection
                                                    .shift_on_trim(dropped_visible, trimmed);
                                            }

                                            // Incremental filtering: evaluate only newly arrived lines without full buffer Rayon pass
                                            let compiled = workspace.ensure_cached_stream_filters();
                                            let mut has_filter_result = false;
                                            if let (Some(filter_res), Some(src_ref)) = (&mut workspace.filter_result, &workspace.log_source) {
                                                has_filter_result = true;
                                                let show_only = workspace.show_only_filtered;
                                                let res_mut = Arc::make_mut(filter_res);
                                                compiled.append_streaming_batch(
                                                    res_mut,
                                                    &**src_ref,
                                                    new_lines_range,
                                                    trimmed,
                                                    show_only,
                                                );

                                                let visible_count = res_mut.visible_indices.len();
                                                let total_matched = res_mut.total_matched_lines;

                                                workspace.status_text = format!(
                                                    "Visible: {} / {} lines ({} matched)",
                                                    visible_count,
                                                    new_total_len,
                                                    total_matched
                                                ).into();
                                            }
                                            if has_filter_result {
                                                // Offload heavy Rayon HeatmapData & EpsTimelineData computation to background executor
                                                if should_compute_analytics {
                                                    analytics_job = workspace.prepare_stream_analytics_job();
                                                }
                                            } else {
                                                // Initial pass if filter_result was not yet computed
                                                workspace.reapply_filters(cx);
                                            }

                                            if workspace.facet_tags.is_empty() && new_total_len >= 100 {
                                                workspace.refresh_facet_tags_async(cx);
                                            }

                                            // Smart Auto-Scroll to bottom if enabled
                                            if workspace.adb.auto_scroll {
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
                                            workspace.adb.is_streaming = false;
                                            workspace.status_text = "ADB stream disconnected".into();
                                        }

                                        cx.notify();
                                        (workspace.adb.is_streaming, trimmed_count, analytics_job)
                                    })
                                    .unwrap_or((false, 0, None));

                                if let Some(job) = analytics_job {
                                    compute_and_apply_stream_analytics(
                                        &this,
                                        &mut async_cx,
                                        &timer_executor,
                                        job,
                                    )
                                    .await;

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
                                let analytics_job = this.update(&mut async_cx, |workspace, cx| {
                                    workspace.adb.is_streaming = false;
                                    workspace.status_text = "ADB stream disconnected".into();
                                    cx.notify();

                                    if pending_analytics_compute {
                                        workspace.prepare_stream_analytics_job()
                                    } else {
                                        None
                                    }
                                }).unwrap_or(None);

                                if let Some(job) = analytics_job {
                                    compute_and_apply_stream_analytics(
                                        &this,
                                        &mut async_cx,
                                        &timer_executor,
                                        job,
                                    )
                                    .await;
                                }
                                break;
                            } else {
                                // Flush any pending throttled filter reapply when channel is empty
                                if pending_filter_reapply && last_reapply.elapsed() >= Duration::from_millis(60) {
                                    let _ = this.update(&mut async_cx, |workspace, cx| {
                                        if !workspace.adb.is_paused {
                                            workspace.reapply_filters(cx);
                                        }
                                    });
                                    last_reapply = std::time::Instant::now();
                                    pending_filter_reapply = false;
                                    accumulated_trimmed = 0;
                                    pending_analytics_compute = false;
                                } else if pending_analytics_compute {
                                    // Flush pending throttled heatmap/timeline compute asynchronously when channel is idle or paused
                                    let analytics_job = this.update(&mut async_cx, |workspace, _cx| {
                                        workspace.prepare_stream_analytics_job()
                                    }).unwrap_or(None);

                                    if let Some(job) = analytics_job {
                                        compute_and_apply_stream_analytics(
                                            &this,
                                            &mut async_cx,
                                            &timer_executor,
                                            job,
                                        )
                                        .await;
                                    }

                                    last_analytics_compute = std::time::Instant::now();
                                    pending_analytics_compute = false;
                                }

                                // Yield execution cooperatively to GPUI async executor
                                timer_executor.timer(Duration::from_millis(16)).await;
                                let still_streaming = this
                                    .update(&mut async_cx, |workspace, _| workspace.adb.is_streaming)
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

    /// Prepares a generation-stamped analytics computation snapshot if both a log source and filter result exist.
    pub(crate) fn prepare_stream_analytics_job(&self) -> Option<StreamAnalyticsJob> {
        let (Some(filter_res), Some(src_ref)) = (&self.filter_result, &self.log_source) else {
            return None;
        };
        let analytics_gen = self.analytics_cancel_gen.fetch_add(1, Ordering::SeqCst) + 1;
        Some(StreamAnalyticsJob {
            src: Arc::clone(src_ref),
            visible_indices: filter_res.visible_indices.clone(),
            visible_styles: filter_res.visible_styles.clone(),
            include_timeline: self.is_timeline_visible,
            analytics_gen,
        })
    }

    /// Spawns asynchronous heatmap/timeline computation guarded by `analytics_cancel_gen` and source identity.
    pub(crate) fn spawn_and_apply_stream_analytics(&mut self, cx: &mut Context<Self>) {
        let Some(job) = self.prepare_stream_analytics_job() else {
            return;
        };

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let bg_executor = cx.background_executor().clone();
            let mut async_cx = cx.clone();
            async move {
                compute_and_apply_stream_analytics(&this, &mut async_cx, &bg_executor, job).await;
            }
        })
        .detach();
    }

    /// Stops ADB streaming cleanly via RAII handle
    pub fn stop_adb_stream(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.adb.stream_handle.take() {
            handle.stop();
        }
        self.adb.is_streaming = false;
        self.spawn_and_apply_stream_analytics(cx);
        self.status_text = "ADB stream stopped".into();
        cx.notify();
    }

    /// Toggles ADB stream pause and flushes buffered chunks when resumed
    pub fn toggle_pause_stream(&mut self, cx: &mut Context<Self>) {
        if self.input_focus != InputFocusTarget::None {
            return;
        }
        if self.adb.is_streaming {
            self.adb.is_paused = !self.adb.is_paused;
            if !self.adb.is_paused {
                // Flush buffered chunks
                let buffered = std::mem::take(&mut self.adb.paused_buffer);
                self.adb.paused_line_count = 0;
                if !buffered.is_empty() {
                    if let Some(src) = &mut self.log_source {
                        let trimmed = if let Some(active_src) = Arc::get_mut(src) {
                            if let ActiveLogSource::Stream(ref mut stream) = active_src {
                                for chunk in buffered {
                                    stream.append_chunk(chunk);
                                }
                                stream.trim_if_needed().unwrap_or(0)
                            } else {
                                0
                            }
                        } else if let ActiveLogSource::Stream(ref stream) = **src {
                            let mut new_stream = stream.clone();
                            for chunk in buffered {
                                new_stream.append_chunk(chunk);
                            }
                            let tr = new_stream.trim_if_needed().unwrap_or(0);
                            *src = Arc::new(ActiveLogSource::Stream(new_stream));
                            tr
                        } else {
                            0
                        };
                        if trimmed > 0 {
                            let dropped_visible = self
                                .filter_result
                                .as_ref()
                                .map_or(0, |res| {
                                    res.visible_indices
                                        .partition_point(|&idx| idx < trimmed)
                                });
                            self.bookmarks.shift_on_trim(trimmed);
                            self.selection.shift_on_trim(dropped_visible, trimmed);
                        }
                        self.reapply_filters(cx);
                    }
                }
                self.status_text = "ADB streaming RESUMED".into();
            } else {
                self.spawn_and_apply_stream_analytics(cx);
                self.status_text = "ADB streaming PAUSED (Press Ctrl+Space to resume)".into();
            }
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LogAnalysisWorkspace, StreamAnalyticsJob};
    use gpui::Context;
    use loganalysis_core::{ActiveLogSource, ResolvedStyle, StreamLogBuffer};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_stream_analytics_consolidation_contracts() {
        // Compile-time verification of consolidated analytics dispatcher & call-site methods
        let _: fn(&LogAnalysisWorkspace) -> Option<StreamAnalyticsJob> =
            LogAnalysisWorkspace::prepare_stream_analytics_job;
        let _: fn(&mut LogAnalysisWorkspace, &mut Context<LogAnalysisWorkspace>) =
            LogAnalysisWorkspace::spawn_and_apply_stream_analytics;
        let _: fn(&mut LogAnalysisWorkspace, &mut Context<LogAnalysisWorkspace>) =
            LogAnalysisWorkspace::update_analytics_data_async;
        let _: fn(&mut LogAnalysisWorkspace, &mut Context<LogAnalysisWorkspace>) =
            LogAnalysisWorkspace::stop_adb_stream;
        let _: fn(&mut LogAnalysisWorkspace, &mut Context<LogAnalysisWorkspace>) =
            LogAnalysisWorkspace::toggle_pause_stream;

        // Verify generation & source-identity staleness checks on StreamAnalyticsJob
        let gen_counter = Arc::new(AtomicU64::new(0));
        let src_a = Arc::new(ActiveLogSource::Stream(StreamLogBuffer::default()));
        let src_b = Arc::new(ActiveLogSource::Stream(StreamLogBuffer::default()));

        let gen1 = gen_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let job = StreamAnalyticsJob {
            src: Arc::clone(&src_a),
            visible_indices: vec![0, 2],
            visible_styles: vec![ResolvedStyle::default(); 2],
            include_timeline: true,
            analytics_gen: gen1,
        };
        assert!(job.include_timeline);
        assert_eq!(job.visible_indices, vec![0, 2]);
        assert_eq!(job.visible_styles.len(), 2);

        // Matching generation & source pointer permits apply; bumped generation or replaced source rejects
        assert_eq!(gen_counter.load(Ordering::SeqCst), job.analytics_gen);
        assert!(Arc::ptr_eq(&src_a, &job.src));
        assert!(!Arc::ptr_eq(&src_b, &job.src));

        let gen2 = gen_counter.fetch_add(1, Ordering::SeqCst) + 1;
        assert_ne!(gen2, job.analytics_gen);
    }
}

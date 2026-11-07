# Technical Specification: Log Statistics Profiler & EPS (Events-Per-Second) Timeline Sparkline (`P6`)

**Feature:** Throughput Profiler & EPS Timeline Sparkline  
**Module:** `crates/loganalysis-core/src/filter/timeline.rs` & `crates/loganalysis-gui/src/components/timeline_sparkline.rs`  
**Date:** 2026-11-06  
**Status:** **APPROVED FOR IMPLEMENTATION**  

---

## 1. Overview & Problem Statement

In mission-critical triage (e.g. system freezes, memory leaks, ANR storms, microservice cascading failures), logs spike exponentially before failure.

Currently, triage engineers must manually scroll through hundreds of thousands of lines to find when an event storm began.

**LogAnalysis P6** introduces an interactive **Throughput Profiler & EPS Timeline Ribbon**:
1. **Temporal Binning Engine:** Aggregates log volume over temporal time windows into $B$ discrete buckets (e.g., 80 buckets across the horizontal ribbon).
2. **Instant Anomaly Detection:** Flags statistical outlier bursts ($> \text{mean} + 3\sigma$) with distinct highlight styling.
3. **Interactive Sparkline Ribbon:** A collapsible 36px ribbon beneath the toolbar displaying the throughput histogram, peak EPS, and error density.
4. **Click-to-Seek:** Clicking any bar smoothly centers the log viewport onto that exact moment in time.
5. **Keybindings & Palette:** `Ctrl+Shift+T` / Command Palette command to toggle the timeline.

---

## 2. Core Engine Architecture (`crates/loganalysis-core/src/filter/timeline.rs`)

### 2.1 Data Structures
```rust
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EpsTimelineBucket {
    pub start_micros: i64,
    pub end_micros: i64,
    pub total_count: u32,
    pub error_count: u32,
    pub warn_count: u32,
    pub start_visible_row: usize,
    pub is_anomaly: bool, // Exceeds mean + 3*sigma
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EpsTimelineData {
    pub buckets: Vec<EpsTimelineBucket>,
    pub min_timestamp: i64,
    pub max_timestamp: i64,
    pub peak_count: u32,
    pub peak_eps: f64,
    pub avg_eps: f64,
    pub total_duration_secs: f64,
    pub has_timestamps: bool,
}
```

### 2.2 Computation Pipeline
1. Scan first and last lines with timestamps to establish $[T_{\min}, T_{\max}]$.
2. If $T_{\max} \le T_{\min}$, mark `has_timestamps = false`.
3. Divide duration into $N$ temporal bins.
4. Reduce log lines into bins, accumulating `total_count`, `error_count`, `warn_count`, and tracking `start_visible_row`.
5. Calculate statistical mean and variance $\sigma^2$ to mark buckets where $\text{count} > \mu + 3\sigma$.

---

## 3. UI/UX Component (`crates/loganalysis-gui/src/components/timeline_sparkline.rs`)

1. **Layout & Positioning:**
   - 36px collapsible ribbon docked immediately below the toolbar.
   - Smooth 60 FPS GPUI rendering.
2. **Visual Encoding:**
   - Bars: Height normalized to `bucket.total_count / peak_count`.
   - Colors:
     - Normal: Theme accent muted (`rgba(0x3b82f644)` / `#3b82f6`).
     - Error Burst: `#ef4444` when `error_count > 0`.
     - Warning: `#f59e0b` when `warn_count > 0 && error_count == 0`.
     - Anomaly ($>3\sigma$): High-intensity neon bar with alert beacon.
3. **Interactivity:**
   - Hover: Shows tooltip with formatted time range, total events, EPS, and error count.
   - Click: Jumps directly to `start_visible_row` in the log viewport.
4. **Command & Action:**
   - Action: `ToggleEpsTimeline`
   - Keybinding: `Ctrl+Shift+T` / `Cmd+Shift+T`
   - Command Palette: `View: Toggle Throughput & EPS Timeline Sparkline`

---

## 4. Verification & Testing

1. **Core Unit Tests:**
   - `test_timeline_binning_with_mock_timestamps`
   - `test_timeline_empty_and_no_timestamps`
   - `test_timeline_anomaly_detection_sigma`
2. **GUI Tests:**
   - Action and keybinding registration.
   - Command palette integration and fuzzy match verification.
   - 100% test pass rate across all workspace packages.

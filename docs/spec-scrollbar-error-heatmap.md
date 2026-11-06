# Technical Specification: Scrollbar Error & Warning Heatmap (Overview Track)

## 1. Overview & Objective

To elevate **LogAnalysis** into an industry-grade, professional log inspection platform (on par with JetBrains IDEs, Zed, and Chrome DevTools), this specification details the design and implementation of a **GPU-Accelerated Scrollbar Overview Track (Heatmap)**.

In massive log files ($10^5 \sim 10^7$ lines), critical crash traces (`ERROR`, `FATAL`, `EXCEPTION`, `WARN`) often occur sparsely across millions of mundane `DEBUG` and `INFO` rows. Users currently have to scroll blindly or cycle through find results one by one. The **Scrollbar Heatmap** provides an instant, peripheral bird's-eye view of error densities across the entire log buffer and enables single-click spatial navigation.

---

## 2. Mathematical Modeling & Binning Algorithm

Directly rendering millions of individual pixel markers on a 1080p screen (${\approx}800\text{px}$ track height) causes GPU overdraw, subpixel aliasing, and severe UI thread stutter. We adopt a deterministic **1D Spatial Binning (Bucket Aggregation)** model.

### 2.1 Coordinate Mapping & Bucket Dimension
Let:
- $N$: Total visible line count ($N = \text{visible\_indices.len()}$).
- $H$: Physical height of the scrollbar track in pixels (typically $400 \le H \le 1800$).
- $B$: Number of discrete vertical bins, defined dynamically as:
  $$B = \min\left(H, \ 300\right)$$
- Each line at index $i \in [0, N-1]$ maps to bin index $b(i) \in [0, B-1]$ via:
  $$b(i) = \left\lfloor \frac{i \cdot B}{N} \right\rfloor$$

### 2.2 Severity & Color Hierarchy
Each bucket $b$ aggregates two primary severity levels:
1. **Fatal / Error ($S_{\text{error}}$)**:
   - Matched if line has explicit Error filter styling (`bg_hex` or `fg_hex` resembling red $\approx \text{hue} \in [340^\circ, 20^\circ]$), OR
   - Standard Android logcat marker: contains `" E "` or `" F "` or `"FATAL"` or `"ERROR"`.
   - Render color: `rgba(239, 68, 68, 0.95)` (`#ef4444`, Crimson Red).
2. **Warning ($S_{\text{warn}}$)**:
   - Matched if line has Warn filter styling (yellow/orange), OR
   - Standard Android logcat marker: contains `" W "` or `"WARN"`.
   - Render color: `rgba(245, 158, 11, 0.90)` (`#f59e0b`, Amber).
3. **Info / Custom Filter Rule Matches ($S_{\text{match}}$)**:
   - Matched lines that are highlighted by user-defined rules.
   - Render color: `rgba(59, 130, 246, 0.70)` (`#3b82f6`, Sky Blue).

### 2.3 Layering Rule within a Bucket
If a single bucket $b$ contains both Errors and Warnings, **Error strictly supersedes Warning in fill color**, but the bucket tooltip preserves the exact breakdown:
$$\text{Color}(b) = \begin{cases} 
C_{\text{error}} & \text{if } n_{\text{error}}(b) > 0 \\
C_{\text{warn}} & \text{if } n_{\text{warn}}(b) > 0 \\
C_{\text{match}} & \text{if } n_{\text{match}}(b) > 0 \\
\text{Transparent} & \text{otherwise}
\end{cases}$$

Marker opacity or tick intensity scales logarithmically with density:
$$\alpha(b) = \min\left(1.0, \ 0.5 + 0.5 \cdot \frac{\log(1 + n(b))}{\log(1 + n_{\max})}\right)$$

---

## 3. Architecture & Data Structures

### 3.1 Core Engine (`loganalysis-core`)
Add a new zero-allocation aggregation module in `crates/loganalysis-core/src/buffer/heatmap.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HeatmapBin {
    pub error_count: u32,
    pub warn_count: u32,
    pub match_count: u32,
    pub first_line_index: usize,
}

#[derive(Clone, Debug, Default)]
pub struct HeatmapData {
    pub bins: Vec<HeatmapBin>,
    pub total_errors: usize,
    pub total_warnings: usize,
    pub bin_count: usize,
}

impl HeatmapData {
    pub fn compute_parallel(
        source: &LogSource,
        visible_indices: &[usize],
        styles: &[ResolvedStyle],
        bin_count: usize,
    ) -> Self;
}
```

* **Parallel Execution (`rayon`)**: For files with $>100,000$ lines, bucket classification executes across CPU chunks in $<15\text{ms}$.
* **Incremental Streaming Cache**: For live ADB streaming, tail updates only recalculate the latest bin without full buffer reprocessing.

### 3.2 Frontend UI Integration (`crates/loganalysis-gui`)

In [crates/loganalysis-gui/src/components/log_view.rs](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs):
1. **Track Structure**:
   The existing `log-scrollbar-track` ($14\text{px}$ width) is updated to host a 2-layer composite layout:
   - **Layer 0 (Underlay)**: Heatmap ticks rendered as absolute mini-bars ($w = 6\text{px}$, aligned right or full width).
   - **Layer 1 (Overlay)**: Floating scrollbar thumb (`log-scrollbar-thumb`), rendered with semi-transparent background (`rgba(255, 255, 255, 0.35)`) so underlying error markers remain clearly visible even when covered by the thumb.
2. **Interactive Click Navigation**:
   Clicking directly on any heatmap indicator calculates the normalized $Y$ ratio:
   $$\text{target\_line} = \left\lfloor \frac{y_{\text{click}}}{H} \cdot N \right\rfloor$$
   Instantly scrolls the `uniform_list` viewport to center on that exact error line.
3. **Cursor Feedback**:
   Hovering over an error tick displays a high-contrast mini tooltip (e.g., `"Line 48,201: 14 Errors, 3 Warnings"`).

---

## 4. Edge Cases & Mitigation Strategies

| Edge Case | Failure Mode | Mitigation Strategy |
| :--- | :--- | :--- |
| **Empty or Single-Line Log** | $N \le 1$, division by zero in bucket calculation | Early-exit guard returns an empty `HeatmapData` with 0 bins. |
| **Track Height Too Small ($H < 50\text{px}$)** | Bins become subpixel thin ($<1\text{px}$) | Enforce minimum bin height of $2\text{px}$ with minimum track clamp. |
| **All Lines are Errors ($100\%$ Red)** | Monolithic red block obscures UI | Retain $1\text{px}$ gutter margin between distinct non-empty bucket clusters. |
| **ADB Ring Buffer Hysteresis Trimming** | Head eviction shifts indices | Heatmap bins re-index relative to current `visible_indices`. |
| **`show_only_filtered` Mode** | Heatmap reflects hidden lines | Aggregation operates strictly on `filter_result.visible_indices`. |

---

## 5. Verification & Test Plan

1. **Unit Test (`loganalysis-core`)**:
   - `test_heatmap_empty_buffer`: Verify no panic on $0$ lines.
   - `test_heatmap_density_binning`: Verify deterministic mapping of 10,000 lines with known error distributions into 100 bins.
   - `test_heatmap_severity_precedence`: Verify that Error count overrides Warning color fill.
2. **Deterministic UI Benchmark**:
   - Verify that rendering the heatmap layer in GPUI takes $<0.5\text{ms}$ per frame on 120Hz displays.

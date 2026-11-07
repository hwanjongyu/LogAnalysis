# Technical Specification: Asynchronous Streaming Analytics (Heatmap & EPS Timeline)

**Status:** Proposed (Phase 1: Discovery & Specification)  
**Task:** Priority 1: High-Impact Performance Optimization (`crates/loganalysis-gui`)  
**Target:** `crates/loganalysis-gui/src/workspace.rs`

---

## 1. Problem Statement & Motivation
Currently, in [`crates/loganalysis-gui/src/workspace.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs):
- During active ADB logcat streaming, `HeatmapData::compute` and `EpsTimelineData::compute` are executed inside `this.update(&mut async_cx, |workspace, cx| { ... })` directly on the **GPUI Main/UI Thread**.
- For large buffers (up to 200,000 lines), `HeatmapData::compute` triggers a multi-threaded Rayon parallel reduction (`par_iter().fold(...)`).
- Running Rayon reductions on the UI thread—even when throttled to 150ms—can freeze the UI event loop for 10–30ms per burst, resulting in frame stutter, scroll jitter, and degraded input responsiveness.

---

## 2. Architectural Design

### 2.1 Asynchronous Background Offloading
Instead of computing analytics synchronously on the UI thread:
1. When analytics recomputation is triggered (throttled at 150ms or when the stream is flushed/idle):
   - Capture cheap clones of the necessary data on the main thread:
     - `src_arc = Arc::clone(src_ref)` (`Arc<ActiveLogSource>`)
     - `indices = filter_res.visible_indices.clone()` (or sliced/shared)
     - `styles = filter_res.visible_styles.clone()`
     - `compute_timeline = workspace.is_timeline_visible`
   - Offload the Rayon reductions to `cx.background_executor().spawn(async move { ... })`:
     ```rust
     let (heatmap, timeline) = background_executor.spawn(async move {
         let hm = Arc::new(HeatmapData::compute(&*src_arc, &indices, &styles, 200));
         let tl = if compute_timeline {
             Some(Arc::new(EpsTimelineData::compute(&*src_arc, &indices, &styles, 80)))
         } else {
             None
         };
         (hm, tl)
     }).await;
     ```
   - In a subsequent lightweight `this.update(...)` on the main thread:
     - Assign `workspace.heatmap_data = Some(heatmap)`
     - Assign `workspace.timeline_data = timeline`
     - Call `cx.notify()`
2. **Cancellation / Stale Guard**:
   - Use an atomic or sequential version counter (`analytics_seq: u64`) so that if a newer batch arrives while a background compute is in flight, stale results are discarded without overwriting fresher data.
   - Guarantee UI thread execution time remains $\le 0.1\text{ms}$ (zero Rayon on UI thread).

---

## 3. Verification & Test Plan
1. **Compilation & Clippy**:
   - Zero compilation errors (`cargo check`) and zero clippy warnings (`cargo clippy`).
2. **Automated Unit Tests**:
   - Verify `HeatmapData::compute` and `EpsTimelineData::compute` pass under synthetic streaming loads.
   - Run full workspace test suite (`cargo test`, 106 tests).
3. **Behavioral Acceptance Criteria**:
   - High-throughput streaming (e.g. `adb logcat` spamming 20,000+ lines/sec) maintains smooth UI scrolling and interactive responsiveness with 0ms UI thread Rayon blocking.

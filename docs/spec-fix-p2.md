# Specification: P2 Performance & Architecture Polish

**Target Codebase:** [`/home/drew/github/hwanjongyu/loganalysis`](file:///home/drew/github/hwanjongyu/loganalysis)  
**Author:** Software Architect & Platform Engineer  
**Status:** Approved for Implementation (Autonomous `/goal` Mode)  
**Date:** 2026-11-04  
**Audit Reference:** [`docs/multi_model_review_report.md`](file:///home/drew/github/hwanjongyu/loganalysis/docs/multi_model_review_report.md)

---

## 1. Problem Statements & Technical Root Causes

### P2-1: Filter Rule Search Simulated Text & Lack of IME/CJK Support
- **Locations:**  
  - [`crates/loganalysis-gui/src/components/filter_dock.rs:148-218, 246-300`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/filter_dock.rs#L148-L218)
  - [`crates/loganalysis-gui/src/workspace.rs:780-875`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs#L780-L875)
- **Root Cause:**  
  Unlike Find Bar and the Filter Edit Modal which use native `Entity<TextInput>`, the Filter Rule search in the dock paints an emulated text string with a pipe caret (`🔍 {}|`) and relies on manual character parsing in `workspace.handle_key_down`. International users cannot enter Korean, Japanese, Chinese, or accented characters to search filter rules, and clipboard/selection functionality is degraded.
- **Target Fix:**  
  1. Add `filter_rule_input: Entity<TextInput>` to `LogAnalysisWorkspace`.
  2. Subscribe to `filter_rule_input` events: sync `filter_search_query` on `TextInputEvent::Change`, clear and unfocus on `TextInputEvent::Cancel`.
  3. Replace simulated string rendering in `filter_dock.rs` with `workspace.filter_rule_input.clone()`.
  4. Remove remaining manual keystroke emulation for `FilterRuleSearch` in `workspace.handle_key_down`.

---

### P2-2: `FileLogBuffer` Line Allocation Churn in Virtual List
- **Locations:**  
  - [`crates/loganalysis-core/src/buffer/file_buffer.rs:132-156`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs#L132-L156)
  - [`crates/loganalysis-gui/src/components/log_view.rs:73-83`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/log_view.rs#L73-L83)
- **Root Cause:**  
  `FileLogBuffer` relies on `LogSource::get_line_arc` default trait implementation which calls `get_line(idx)` and allocates a new `Arc<str>` on every invocation. In `log_view.rs`, virtual viewport rendering calls `get_line_arc` for every visible line on every frame render tick (~3,000 to 7,200 allocations/sec during scrolling on 60-144Hz displays).
- **Target Fix:**  
  1. Add a high-speed direct-mapped line cache (`line_cache: std::sync::RwLock<Box<[Option<(usize, Arc<str>)>; 1024]>>`) to `FileLogBuffer`.
  2. Implement `LogSource::get_line_arc` for `FileLogBuffer`:
     - On cache hit: return `Some(Arc::clone(cached_arc))` (0 heap allocations, ~10ns).
     - On cache miss: decode string from mmap slice, populate cache slot, return `Some(arc)`.
  3. Zero allocations while stationary or scrolling within cache locality.

---

## 2. Implementation Plan

1. **[`crates/loganalysis-core/src/buffer/file_buffer.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-core/src/buffer/file_buffer.rs)**:
   - Add `line_cache: std::sync::RwLock<Box<[Option<(usize, Arc<str>)>; 1024]>>` to `FileLogBuffer`.
   - Implement `get_line_arc` in `impl LogSource for FileLogBuffer`.
   - Add unit tests validating cache hit consistency and zero-allocation cloned pointer semantics.

2. **[`crates/loganalysis-gui/src/workspace.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/workspace.rs)**:
   - Instantiate `filter_rule_input = cx.new(|cx| TextInput::new("Filter rules...", cx))` with subscriptions.
   - Clean up manual keystroke handling in `handle_key_down`.

3. **[`crates/loganalysis-gui/src/components/filter_dock.rs`](file:///home/drew/github/hwanjongyu/loganalysis/crates/loganalysis-gui/src/components/filter_dock.rs)**:
   - Mount `workspace.filter_rule_input.clone()` in both bottom dock and left dock modes.
   - Wire clear button to `filter_rule_input.update(...)`.

4. **Verification**:
   - `cargo check --workspace`
   - `cargo clippy --workspace`
   - `cargo test --workspace` (verify all tests pass).
   - Generate `docs/qa-fix-p2.md`.

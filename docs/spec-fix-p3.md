# 🛠️ P3 Code Polish & Hygiene Specification

- **Version:** 1.0.0
- **Scope:** Priority 3 Polish & Ergonomics (`loganalysis-core`, `loganalysis-gui`)
- **Status:** Approved for Implementation

---

## 1. Problem Statement

1. **Raw `String` Error Anti-Pattern**:
   Functions across `adb`, `filter`, and `search` return untyped `Result<T, String>`. Callers cannot programmatically match error causes.
2. **CJK Display Width Distortion**:
   `get_measured_length` measures raw byte counts. Multi-byte CJK characters (3 bytes in UTF-8) artificially inflate horizontal layout width by up to 300%.
3. **Unused Dependencies in `Cargo.toml`**:
   `tokio` is declared as optional without any feature flag or reference in `crates/loganalysis-core`.
4. **Portability of `build.rs`**:
   `crates/loganalysis-gui/build.rs` accesses Linux-specific paths without platform guards (`#[cfg(target_os = "linux")]`).
5. **Search Loop Arithmetic Optimization**:
   `search_in_log` executes modulo operations (`%`) and atomic token loads on every row iteration.

---

## 2. Technical Architecture & Solutions

### 2.1 Strongly-Typed Domain Error Enums (`thiserror`)
Define `crates/loganalysis-core/src/error.rs`:
- `enum FilterError`: `InvalidRegex(String)`, `InvalidColor(String)`, `JsonError(String)`
- `enum SearchError`: `InvalidRegex(String)`, `Cancelled`
- `enum AdbError`: `InvalidSerial(String)`, `ExecutionFailed(String)`, `Timeout(Duration)`, `IoCaptureFailed`, `NonZeroExit(Option<i32>)`
Export these at `loganalysis_core::*`.

### 2.2 CJK / UTF-8 Code Point Character Count
Update `LogSource::get_measured_length` in `crates/loganalysis-core/src/buffer/mod.rs`:
```rust
fn get_measured_length(&self, idx: usize) -> usize {
    match self.get_raw_line(idx) {
        Some(bytes) => {
            let trimmed = trim_line_end(bytes);
            if let Ok(s) = std::str::from_utf8(trimmed) {
                s.chars().count()
            } else {
                trimmed.len()
            }
        }
        None => 0,
    }
}
```

### 2.3 Dependency Cleanup & Build Portability
- Remove `tokio` from `crates/loganalysis-core/Cargo.toml`.
- Protect symlink generation in `crates/loganalysis-gui/build.rs` with `#[cfg(target_os = "linux")]`.

### 2.4 Search Stride & Modulo Elimination
- Replace `idx = (idx + 1) % total` with branch `idx += 1; if idx == total { idx = 0; }`.
- Stride atomic cancel checks every 1,024 iterations (`(visited_count & 1023) == 0`).

---

## 3. Verification Plan

1. Verify workspace test suite: `cargo test --workspace` (15+ tests passing).
2. Add dedicated tests for:
   - `test_cjk_measured_length_accuracy`
   - `test_strongly_typed_domain_errors`
3. Verify full binary build: `cargo build --workspace`.
4. Update `docs/qa-loganalysis-rust-gpui.md`.

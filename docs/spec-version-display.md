# 📐 Specification: CLI Version Flag & UI Version Badge

- **Author**: Lead Software Architect
- **Target Crates**: `crates/loganalysis-gui`
- **Scope**: CLI `--version`/`-V` argument interception & UI titlebar/statusbar version display

---

## 1. Objectives

1. **CLI Version Interception (`--version` / `-V` / `-v`)**:
   - When executed in the terminal with `--version`, `-V`, or `-v`, print `LogAnalysis <version>` (e.g. `LogAnalysis 0.1.0`) to `stdout` and exit immediately (`0`).
   - Must run **before** GPUI initialization, desktop integration, or X11/Wayland display server connection, ensuring it runs reliably even in headless/SSH environments without graphic servers.
2. **UI Titlebar Version Badge**:
   - In `crates/loganalysis-gui/src/components/titlebar.rs`, display a `v0.1.0` badge directly adjacent to the existing `[GPU]` badge in the brand section.
3. **UI Statusbar Version Indicator**:
   - In `crates/loganalysis-gui/src/components/status_bar.rs`, display `v0.1.0` within the telemetry cluster on the right.
4. **Compile-Time Safety**:
   - Utilize `env!("CARGO_PKG_VERSION")` to prevent version drift between `Cargo.toml` and runtime presentation.

---

## 2. Technical Design & Call Paths

### A. CLI Argument Interception (`crates/loganalysis-gui/src/main.rs`)

- **Entry Point**: `main()` function in `crates/loganalysis-gui/src/main.rs`.
- **Logic**:
  ```rust
  for arg in std::env::args().skip(1) {
      if arg == "--version" || arg == "-V" || arg == "-v" {
          println!("LogAnalysis {}", env!("CARGO_PKG_VERSION"));
          return;
      }
  }
  ```
- **Placement**: Place immediately after `logging::init_crash_handler()` and before `ensure_desktop_integration()` and `Application::new().run(...)`.

### B. Titlebar Version Badge (`crates/loganalysis-gui/src/components/titlebar.rs`)

- **Component**: `render_titlebar()`
- **Placement**: Right after the `Engine Badge` (`[GPU]`) inside `#titlebar-left-brand`.
- **Styling**:
  ```rust
  // Version Badge
  .child(
      div()
          .px_1p5()
          .py_0p5()
          .rounded_xs()
          .bg(theme.bg_badge)
          .border_1()
          .border_color(theme.border_subtle)
          .text_xs()
          .text_size(px(10.0))
          .text_color(theme.text_muted)
          .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
  )
  ```

### C. Statusbar Version Indicator (`crates/loganalysis-gui/src/components/status_bar.rs`)

- **Component**: `render_status_bar()`
- **Placement**: Right telemetry section, after the font zoom controls or as part of the telemetry metrics.
- **Styling**:
  ```rust
  .child(
      div()
          .px_1p5()
          .py_0p5()
          .rounded_xs()
          .bg(theme.bg_card)
          .border_1()
          .border_color(theme.border)
          .text_size(px(10.0))
          .text_color(theme.text_muted)
          .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
  )
  ```

---

## 3. Edge Cases & Verification Plan

1. **Headless Execution Test**:
   - Run `cargo run -p loganalysis-gui -- --version` and verify it prints `LogAnalysis 0.1.0` and exits with code 0 without launching a GUI window.
   - Run `cargo run -p loganalysis-gui -- -V` and verify identical output.
2. **File Path Argument Compatibility**:
   - Ensure passing a file path (e.g. `loganalysis /path/to/log.txt`) continues to open the file normally without accidental version interception.
3. **Deterministic Workspace Test Suite**:
   - Run `cargo test --workspace` to ensure all 43 tests pass without regression.

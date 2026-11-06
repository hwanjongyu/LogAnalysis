# Technical Specification: Line Delta Time ($\Delta t$) Inspector & Latency Analysis

## 1. Overview & Objective

In software engineering, mobile systems debugging (Android logcat), and cloud service diagnostics, identifying **latency spikes, thread stalls, UI freezing (ANR), network timeouts, and blocking I/O calls** is one of the most critical daily tasks.

Currently, developers must manually calculate the difference between timestamps across log lines (e.g., subtracting `14:22:05.120` from `14:22:07.450`).

The **Line Delta Time ($\Delta t$) Inspector** provides:
1. **Automatic zero-allocation timestamp extraction** for Android logcat (`MM-DD HH:MM:SS.mmm`), ISO-8601 (`YYYY-MM-DDTHH:MM:SS.mmm`), bracketed timestamps, and kernel dmesg formats.
2. **Instant relative line-to-line $\Delta t$ calculation**: When any row $R$ is focused, $\Delta t = t(R) - t(R-1)$ is computed and displayed in the status bar in real time.
3. **Multi-line range duration analysis**: When a range $[A, B]$ is selected, the total duration $\Delta t_{\text{span}} = t(B) - t(A)$ is measured and formatted.
4. **Visual Latency Threshold Warning**: Highlights lag spikes exceeding user-configurable thresholds ($\Delta t \ge 500\text{ms}$) with high-visibility amber/red latency badges.

---

## 2. Mathematical Modeling & Parsing Mechanics

### 2.1 Microsecond Fixed-Point Representation
All timestamps are converted to a normalized 64-bit microsecond counter $T \in \mathbb{Z}$:
$$T = (\text{days} \times 86,400 + \text{hours} \times 3,600 + \text{minutes} \times 60 + \text{seconds}) \times 1,000,000 + \mu\text{s}$$

For two log entries at $t_1$ and $t_2$:
$$\Delta t = t_2 - t_1$$

### 2.2 Format Specification & Zero-Allocation Parser
Parsing runs directly on raw ASCII/UTF-8 byte slices without heap allocation:

1. **Android Logcat (`threadtime` / `time`)**:
   - Pattern: `MM-DD HH:MM:SS.mmm` (length 18)
   - Byte anchors: `b[2] == b'-' && b[5] == b' ' && b[8] == b':' && b[11] == b':' && b[14] == b'.'`
2. **ISO-8601 / RFC-3339**:
   - Pattern: `YYYY-MM-DD[T ]HH:MM:SS[.mmm]`
   - Byte anchors: `b[4] == b'-' && b[7] == b'-'`
3. **Bracketed Standard**:
   - Pattern: `[HH:MM:SS.mmm]` or `[YYYY-MM-DD HH:MM:SS.mmm]`
   - Byte anchors: `b[0] == b'['`
4. **Kernel Dmesg**:
   - Pattern: `[  123.456789]`
   - Monotonic seconds and fractional microseconds.

### 2.3 Human-Readable Formatter
$$\text{Formatted}(\Delta t) = \begin{cases}
\pm X.X\text{ms} & \text{if } |\Delta t| < 1,000,000\ \mu\text{s} \\
\pm X.XX\text{s} & \text{if } 1\text{s} \le |\Delta t| < 60\text{s} \\
\pm M\text{m } S.S\text{s} & \text{if } 60\text{s} \le |\Delta t| < 3,600\text{s} \\
\pm H\text{h } M\text{m } S\text{s} & \text{if } |\Delta t| \ge 3,600\text{s}
\end{cases}$$

---

## 3. Architecture & Module Design

### 3.1 Core Module: `crates/loganalysis-core/src/buffer/timestamp.rs`
- `LogTimestamp`: Microsecond-precision timestamp struct.
- `extract_timestamp(bytes: &[u8]) -> Option<LogTimestamp>`
- `compute_delta(t1: LogTimestamp, t2: LogTimestamp) -> i64`
- `format_delta_micros(micros: i64) -> String`
- `classify_latency(micros: i64) -> LatencyLevel` (`Normal`, `Warning` $\ge 100\text{ms}$, `Critical` $\ge 500\text{ms}$)

### 3.2 GUI Integration: `crates/loganalysis-gui`
- In `workspace.rs`: Add helper `get_selected_delta_time(&self) -> Option<DeltaTimeInfo>`.
- In `status_bar.rs`: Render the `Δt` metric with dynamic color-coding and icons (`⏱️ Δt: +15.4ms`, `⚠️ Δt: +1.82s`).

---

## 4. Edge Cases & Mitigation Strategies

| Edge Case | Failure Mode | Mitigation Strategy |
| :--- | :--- | :--- |
| **Missing Timestamp on Log Line** | Log line has no date/time prefix | Returns `None`; status bar gracefully hides `Δt` or searches backward to last valid timestamp. |
| **Clock Skew / NTP Jump** | $t_2 < t_1$ resulting in negative duration | Formatter outputs `-X.XXs (clock reset)` with distinct indicator. |
| **Year Boundary Wrap-around** | `12-31 23:59:59` to `01-01 00:00:01` in Android logcat | Modulo year-day compensation ($+2\text{s}$, not $-364\text{ days}$). |
| **Corrupted / Non-ASCII Input** | Invalid digit characters | Strict range checking on digit bytes ($b'0' \le b \le b'9'$); early return on non-digit. |

---

## 5. Verification & Deterministic Test Plan

1. Unit tests for `extract_timestamp`: Android threadtime, ISO-8601, bracketed, kernel dmesg.
2. Unit tests for `format_delta_micros`: Sub-millisecond, multi-second, minutes, negative clock reset.
3. Year-end wrap around test (`12-31` to `01-01`).
4. Integration test with full workspace compilation.

//! High-performance zero-allocation timestamp extraction and delta time calculation.

pub const DAYS_BEFORE_MONTH: [u32; 13] = [
    0, 0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335,
];

pub const NON_LEAP_DAYS_BEFORE_MONTH: [u32; 13] = [
    0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334,
];

pub const ONE_YEAR_MICROS: i64 = 366 * 86_400 * 1_000_000;
pub const YEAR_ROLLOVER_THRESHOLD_MICROS: i64 = 300 * 86_400 * 1_000_000;
pub const EARLY_YEAR_THRESHOLD_MICROS: i64 = 60 * 86_400 * 1_000_000;

/// Normalizes a yearless [`TimestampKind::MonthDayTime`] microsecond value against an anchor
/// timestamp from the same source, adding [`ONE_YEAR_MICROS`] on Dec 31 -> Jan 01 rollover.
#[inline]
pub fn normalize_month_day_rollover(ts_micros: i64, anchor_micros: i64) -> i64 {
    if anchor_micros >= YEAR_ROLLOVER_THRESHOLD_MICROS
        && (0..=EARLY_YEAR_THRESHOLD_MICROS).contains(&ts_micros)
    {
        ts_micros.saturating_add(ONE_YEAR_MICROS)
    } else {
        ts_micros
    }
}

#[inline]
pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[inline]
pub fn max_days_in_month(month: u32, is_leap: bool) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

#[inline]
pub fn day_of_year_with_leap(month: u32, day: u32, is_leap: bool) -> u32 {
    let m = month.clamp(1, 12) as usize;
    let table = if is_leap {
        &DAYS_BEFORE_MONTH
    } else {
        &NON_LEAP_DAYS_BEFORE_MONTH
    };
    table[m] + day.saturating_sub(1)
}

#[inline]
fn leap_years_before(year: i64) -> i64 {
    let y = year - 1;
    y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400)
}

#[inline]
pub fn days_since_epoch(year: i32, month: u32, day: u32) -> i64 {
    let y = year as i64;
    let leap_days = leap_years_before(y) - leap_years_before(1970);
    (y - 1970) * 365
        + leap_days
        + day_of_year_with_leap(month, day, is_leap_year(year)) as i64
}

/// Represents a microsecond-precision normalized timestamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct LogTimestamp {
    pub total_micros: i64,
}

/// Identifies the source timestamp format extracted from a log line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimestampKind {
    /// Yearless `MM-DD HH:MM:SS.mmm` format (subject to Dec 31 -> Jan 01 year rollover).
    MonthDayTime,
    /// Explicit-year `YYYY-MM-DD[T ]HH:MM:SS[.mmm]` ISO-8601 format.
    Iso8601,
    /// Date-less `HH:MM:SS.mmm` time-of-day format.
    TimeOnly,
}

/// Timestamp value paired with its detected [`TimestampKind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedTimestamp {
    pub timestamp: LogTimestamp,
    pub kind: TimestampKind,
}

/// Latency severity classification for elapsed delta time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatencyLevel {
    Normal,   // < 100ms
    Warning,  // 100ms <= dt < 500ms
    Critical, // dt >= 500ms
}

impl LatencyLevel {
    #[inline]
    pub fn from_micros(micros: i64) -> Self {
        let abs = micros.saturating_abs();
        if abs >= 500_000 {
            Self::Critical
        } else if abs >= 100_000 {
            Self::Warning
        } else {
            Self::Normal
        }
    }
}

/// Computes the signed delta in microseconds: `t_curr - t_prev`,
/// with automatic year-boundary wrap-around handling.
#[inline]
pub fn compute_delta_micros(t_prev: LogTimestamp, t_curr: LogTimestamp) -> i64 {
    let mut diff = t_curr.total_micros.saturating_sub(t_prev.total_micros);
    // Year-end rollover: e.g., 12-31 23:59:59 -> 01-01 00:00:01
    if diff < -300 * 86_400 * 1_000_000 {
        diff = diff.saturating_add(ONE_YEAR_MICROS);
    }
    diff
}

/// Computes the signed delta in microseconds (`t_curr - t_prev`), applying year-boundary
/// wrap-around only when both timestamps have [`TimestampKind::MonthDayTime`].
#[inline]
pub fn compute_delta_micros_typed(t_prev: ParsedTimestamp, t_curr: ParsedTimestamp) -> i64 {
    let mut diff = t_curr
        .timestamp
        .total_micros
        .saturating_sub(t_prev.timestamp.total_micros);
    if t_prev.kind == TimestampKind::MonthDayTime
        && t_curr.kind == TimestampKind::MonthDayTime
        && diff < -300 * 86_400 * 1_000_000
    {
        diff = diff.saturating_add(ONE_YEAR_MICROS);
    }
    diff
}

/// Formats a signed microsecond delta into a compact human-readable representation.
pub fn format_delta_micros(micros: i64) -> String {
    if micros == 0 {
        return "+0.0ms".to_string();
    }
    let sign = if micros >= 0 { "+" } else { "-" };
    let abs_us = micros.unsigned_abs();

    if abs_us < 1_000 {
        format!("{sign}{abs_us}µs")
    } else if abs_us < 999_950 {
        let ms = (abs_us as f64) / 1000.0;
        format!("{sign}{ms:.1}ms")
    } else if abs_us < 59_995_000 {
        let sec = (abs_us as f64) / 1_000_000.0;
        format!("{sign}{sec:.2}s")
    } else if abs_us < 3_600_000_000 {
        let total_tenths_sec = ((abs_us as f64) / 100_000.0).round() as u64;
        if total_tenths_sec < 36_000 {
            let mins = total_tenths_sec / 600;
            let rem_tenths = total_tenths_sec % 600;
            format!("{sign}{mins}m {:.1}s", (rem_tenths as f64) / 10.0)
        } else {
            let total_sec = total_tenths_sec / 10;
            let hours = total_sec / 3600;
            let rem_sec = total_sec % 3600;
            let mins = rem_sec / 60;
            let secs = rem_sec % 60;
            format!("{sign}{hours}h {mins}m {secs}s")
        }
    } else {
        let total_sec = abs_us / 1_000_000;
        let hours = total_sec / 3600;
        let rem_sec = total_sec % 3600;
        let mins = rem_sec / 60;
        let secs = rem_sec % 60;
        format!("{sign}{hours}h {mins}m {secs}s")
    }
}

/// Zero-allocation fast extraction of a normalized timestamp along with its [`TimestampKind`].
pub fn extract_timestamp_with_kind(line: &[u8]) -> Option<ParsedTimestamp> {
    if line.len() < 12 {
        return None;
    }

    // Skip leading whitespace or bracket
    let trimmed = trim_start_bytes(line);
    if trimmed.len() < 12 {
        return None;
    }

    // Case 1: Android logcat threadtime format: "MM-DD HH:MM:SS.mmm"
    // e.g. "11-06 18:32:29.123 ..."
    if trimmed.len() >= 18
        && trimmed[2] == b'-'
        && trimmed[5] == b' '
        && trimmed[8] == b':'
        && trimmed[11] == b':'
        && trimmed[14] == b'.'
    {
        let month = parse_2_digits(&trimmed[0..2])?;
        let day = parse_2_digits(&trimmed[3..5])?;
        let hour = parse_2_digits(&trimmed[6..8])?;
        let min = parse_2_digits(&trimmed[9..11])?;
        let sec = parse_2_digits(&trimmed[12..14])?;
        let millis = parse_3_digits(&trimmed[15..18])?;

        if !(1..=12).contains(&month)
            || day == 0
            || day > max_days_in_month(month, true)
            || hour >= 24
            || min >= 60
            || sec > 60
        {
            return None;
        }

        let days = day_of_year(month, day);
        let total_micros = calc_micros(days as i64, hour as i64, min as i64, sec as i64, millis as i64 * 1000);
        return Some(ParsedTimestamp {
            timestamp: LogTimestamp { total_micros },
            kind: TimestampKind::MonthDayTime,
        });
    }

    // Case 2: ISO-8601 format: "YYYY-MM-DD[T ]HH:MM:SS[.mmm]"
    // e.g. "2026-11-06 18:32:29.123" or "2026-11-06T18:32:29"
    if trimmed.len() >= 19
        && trimmed[4] == b'-'
        && trimmed[7] == b'-'
        && (trimmed[10] == b' ' || trimmed[10] == b'T')
        && trimmed[13] == b':'
        && trimmed[16] == b':'
    {
        let year = parse_4_digits(&trimmed[0..4])?;
        let month = parse_2_digits(&trimmed[5..7])?;
        let day = parse_2_digits(&trimmed[8..10])?;
        let hour = parse_2_digits(&trimmed[11..13])?;
        let min = parse_2_digits(&trimmed[14..16])?;
        let sec = parse_2_digits(&trimmed[17..19])?;

        let year_i32 = year as i32;
        let is_leap = is_leap_year(year_i32);
        if !(1..=12).contains(&month)
            || day == 0
            || day > max_days_in_month(month, is_leap)
            || hour >= 24
            || min >= 60
            || sec > 60
        {
            return None;
        }

        let mut micros = 0;
        if trimmed.len() >= 23 && trimmed[19] == b'.' {
            if let Some(m) = parse_3_digits(&trimmed[20..23]) {
                micros = m as i64 * 1000;
            }
        }

        let days = days_since_epoch(year_i32, month, day);
        let total_micros = calc_micros(days, hour as i64, min as i64, sec as i64, micros);
        return Some(ParsedTimestamp {
            timestamp: LogTimestamp { total_micros },
            kind: TimestampKind::Iso8601,
        });
    }

    // Case 3: Simple time format: "HH:MM:SS.mmm"
    // e.g. "18:32:29.123" or "[18:32:29.123]"
    if trimmed.len() >= 12
        && trimmed[2] == b':'
        && trimmed[5] == b':'
        && (trimmed[8] == b'.' || trimmed[8] == b',')
    {
        let hour = parse_2_digits(&trimmed[0..2])?;
        let min = parse_2_digits(&trimmed[3..5])?;
        let sec = parse_2_digits(&trimmed[6..8])?;
        let millis = parse_3_digits(&trimmed[9..12])?;

        if hour >= 24 || min >= 60 || sec > 60 {
            return None;
        }

        let total_micros = calc_micros(0, hour as i64, min as i64, sec as i64, millis as i64 * 1000);
        return Some(ParsedTimestamp {
            timestamp: LogTimestamp { total_micros },
            kind: TimestampKind::TimeOnly,
        });
    }

    None
}

/// Zero-allocation fast extraction of a normalized timestamp from raw line bytes.
#[inline]
pub fn extract_timestamp(line: &[u8]) -> Option<LogTimestamp> {
    extract_timestamp_with_kind(line).map(|p| p.timestamp)
}

#[inline(always)]
fn trim_start_bytes(mut b: &[u8]) -> &[u8] {
    while let Some((&first, rest)) = b.split_first() {
        if first == b' ' || first == b'\t' || first == b'[' {
            b = rest;
        } else {
            break;
        }
    }
    b
}

#[inline(always)]
fn parse_2_digits(b: &[u8]) -> Option<u32> {
    if b.len() < 2 {
        return None;
    }
    let d0 = b[0].wrapping_sub(b'0');
    let d1 = b[1].wrapping_sub(b'0');
    if d0 <= 9 && d1 <= 9 {
        Some((d0 as u32) * 10 + (d1 as u32))
    } else {
        None
    }
}

#[inline(always)]
fn parse_3_digits(b: &[u8]) -> Option<u32> {
    if b.len() < 3 {
        return None;
    }
    let d0 = b[0].wrapping_sub(b'0');
    let d1 = b[1].wrapping_sub(b'0');
    let d2 = b[2].wrapping_sub(b'0');
    if d0 <= 9 && d1 <= 9 && d2 <= 9 {
        Some((d0 as u32) * 100 + (d1 as u32) * 10 + (d2 as u32))
    } else {
        None
    }
}

#[inline(always)]
fn parse_4_digits(b: &[u8]) -> Option<u32> {
    if b.len() < 4 {
        return None;
    }
    let d0 = b[0].wrapping_sub(b'0');
    let d1 = b[1].wrapping_sub(b'0');
    let d2 = b[2].wrapping_sub(b'0');
    let d3 = b[3].wrapping_sub(b'0');
    if d0 <= 9 && d1 <= 9 && d2 <= 9 && d3 <= 9 {
        Some((d0 as u32) * 1000 + (d1 as u32) * 100 + (d2 as u32) * 10 + (d3 as u32))
    } else {
        None
    }
}

#[inline]
fn day_of_year(month: u32, day: u32) -> u32 {
    day_of_year_with_leap(month, day, true)
}

#[inline]
fn is_leap_year_u64(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Formats epoch seconds into a human-readable UTC timestamp string (`YYYY-MM-DD HH:MM:SS UTC`).
pub fn format_epoch_utc(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let mut days = secs / 86400;

    let mut year: u64 = 1970;
    loop {
        let days_in_year = if is_leap_year_u64(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let leap = is_leap_year_u64(year);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    let day = days + 1;

    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02} UTC")
}

#[inline]
fn calc_micros(days: i64, hours: i64, mins: i64, secs: i64, micros: i64) -> i64 {
    (((days * 24 + hours) * 60 + mins) * 60 + secs) * 1_000_000 + micros
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_android_logcat_timestamp() {
        let line = b"11-06 18:32:29.123 1234 5678 I ActivityManager: Displayed com.app/.MainActivity";
        let ts = extract_timestamp(line).expect("Should parse Android logcat timestamp");
        assert!(ts.total_micros > 0);

        let line2 = b"11-06 18:32:30.623 1234 5678 D Tag: message";
        let ts2 = extract_timestamp(line2).unwrap();

        let delta = compute_delta_micros(ts, ts2);
        assert_eq!(delta, 1_500_000); // 1.5 seconds = 1,500,000 micros
        assert_eq!(format_delta_micros(delta), "+1.50s");
        assert_eq!(LatencyLevel::from_micros(delta), LatencyLevel::Critical);
    }

    #[test]
    fn test_extract_iso8601_timestamp() {
        let line = b"2026-11-06 18:32:29.500 [main] INFO Server - Listening on 8080";
        let ts = extract_timestamp(line).expect("Should parse ISO-8601");

        let line2 = b"2026-11-06 18:32:29.545 [main] INFO Server - Connection received";
        let ts2 = extract_timestamp(line2).unwrap();

        let delta = compute_delta_micros(ts, ts2);
        assert_eq!(delta, 45_000); // 45ms
        assert_eq!(format_delta_micros(delta), "+45.0ms");
        assert_eq!(LatencyLevel::from_micros(delta), LatencyLevel::Normal);
    }

    #[test]
    fn test_extract_bracketed_time() {
        let line = b"[18:32:29.100] Hello";
        let ts = extract_timestamp(line).expect("Should parse bracketed time");

        let line2 = b"[18:32:29.350] World";
        let ts2 = extract_timestamp(line2).unwrap();

        let delta = compute_delta_micros(ts, ts2);
        assert_eq!(delta, 250_000);
        assert_eq!(format_delta_micros(delta), "+250.0ms");
        assert_eq!(LatencyLevel::from_micros(delta), LatencyLevel::Warning);
    }

    #[test]
    fn test_year_end_wrap_around() {
        let t1 = LogTimestamp {
            total_micros: calc_micros(day_of_year(12, 31) as i64, 23, 59, 59, 0),
        };
        let t2 = LogTimestamp {
            total_micros: calc_micros(day_of_year(1, 1) as i64, 0, 0, 1, 0),
        };
        let delta = compute_delta_micros(t1, t2);
        assert_eq!(delta, 2_000_000); // +2.0 seconds
        assert_eq!(format_delta_micros(delta), "+2.00s");
    }

    #[test]
    fn test_negative_clock_skew() {
        let t1 = LogTimestamp { total_micros: 2_000_000 };
        let t2 = LogTimestamp { total_micros: 1_000_000 };
        let delta = compute_delta_micros(t1, t2);
        assert_eq!(delta, -1_000_000);
        assert_eq!(format_delta_micros(delta), "-1.00s");
    }

    #[test]
    fn test_leap_year_feb29_distinct_from_mar01() {
        let line_feb29 = b"02-29 12:00:00.000 1 1 I Tag: Leap day";
        let line_mar01 = b"03-01 12:00:00.000 1 1 I Tag: March first";

        let ts_feb29 = extract_timestamp(line_feb29).expect("Feb 29 should parse");
        let ts_mar01 = extract_timestamp(line_mar01).expect("Mar 01 should parse");

        assert!(ts_mar01.total_micros > ts_feb29.total_micros);
        let delta = compute_delta_micros(ts_feb29, ts_mar01);
        assert_eq!(delta, 86_400 * 1_000_000); // Exactly 24 hours in microseconds
    }

    #[test]
    fn test_timestamp_range_validation_rejection() {
        // Invalid months
        assert!(extract_timestamp(b"00-15 12:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"13-01 12:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"99-99 99:99:99.999 Msg").is_none());

        // Invalid days
        assert!(extract_timestamp(b"01-00 12:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"01-32 12:00:00.000 Msg").is_none());

        // Invalid clock hours/minutes/seconds
        assert!(extract_timestamp(b"01-15 24:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"01-15 12:60:00.000 Msg").is_none());
        assert!(extract_timestamp(b"01-15 12:00:61.000 Msg").is_none());

        // Valid leap second (60)
        assert!(extract_timestamp(b"12-31 23:59:60.000 Msg").is_some());

        // Valid boundary date
        assert!(extract_timestamp(b"12-31 23:59:59.999 Msg").is_some());
    }

    #[test]
    fn test_i64_min_abs_overflow_safety() {
        // Latency level must not panic on i64::MIN
        assert_eq!(LatencyLevel::from_micros(i64::MIN), LatencyLevel::Critical);

        // format_delta_micros must not panic on i64::MIN
        let formatted = format_delta_micros(i64::MIN);
        assert!(formatted.starts_with('-'));

        // compute_delta_micros must not overflow
        let t_min = LogTimestamp { total_micros: i64::MIN };
        let t_max = LogTimestamp { total_micros: i64::MAX };
        let delta = compute_delta_micros(t_min, t_max);
        assert_eq!(delta, i64::MAX);
    }

    #[test]
    fn test_is_leap_year_gregorian_rules() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2100));
        assert!(!is_leap_year(2025));
        assert!(!is_leap_year(2026));
    }

    #[test]
    fn test_max_days_in_month_and_calendar_validation() {
        assert_eq!(max_days_in_month(1, false), 31);
        assert_eq!(max_days_in_month(2, false), 28);
        assert_eq!(max_days_in_month(2, true), 29);
        assert_eq!(max_days_in_month(4, false), 30);
        assert_eq!(max_days_in_month(12, true), 31);
        assert_eq!(max_days_in_month(0, false), 0);
        assert_eq!(max_days_in_month(13, true), 0);

        // Case 2 (ISO-8601 with explicit year): strict Gregorian leap validation
        assert!(extract_timestamp(b"2025-02-29 12:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"2025-02-29T12:00:00").is_none());
        assert!(extract_timestamp(b"2100-02-29 12:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"1900-02-29 12:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"2024-02-29 12:00:00.000 Msg").is_some());
        assert!(extract_timestamp(b"2024-02-29T12:00:00").is_some());
        assert!(extract_timestamp(b"2000-02-29 12:00:00.000 Msg").is_some());
        assert!(extract_timestamp(b"2025-02-30 12:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"2025-04-31 12:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"2025-02-00 12:00:00.000 Msg").is_none());

        // Case 1 (yearless Android logcat): rejects impossible dates, keeps 02-29 valid
        assert!(extract_timestamp(b"02-30 12:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"04-31 12:00:00.000 Msg").is_none());
        assert!(extract_timestamp(b"02-29 12:00:00.000 Msg").is_some());
    }

    #[test]
    fn test_iso8601_feb_to_mar_transitions_no_phantom_day() {
        // Non-leap year: Feb 28 23:59:59 -> Mar 1 00:00:00 is exactly 1 second (1_000_000 µs)
        let non_leap_feb28 = extract_timestamp(b"2025-02-28 23:59:59.000 End of Feb").unwrap();
        let non_leap_mar01 = extract_timestamp(b"2025-03-01 00:00:00.000 Start of Mar").unwrap();
        assert_eq!(compute_delta_micros(non_leap_feb28, non_leap_mar01), 1_000_000);

        // Leap year: Feb 29 23:59:59 -> Mar 1 00:00:00 is exactly 1 second (1_000_000 µs)
        let leap_feb29 = extract_timestamp(b"2024-02-29 23:59:59.000 End of Feb 29").unwrap();
        let leap_mar01 = extract_timestamp(b"2024-03-01 00:00:00.000 Start of Mar").unwrap();
        assert_eq!(compute_delta_micros(leap_feb29, leap_mar01), 1_000_000);

        // Leap year: Feb 28 23:59:59 -> Feb 29 00:00:00 is also 1 second
        let leap_feb28 = extract_timestamp(b"2024-02-28 23:59:59.000 End of Feb 28").unwrap();
        let leap_feb29_start = extract_timestamp(b"2024-02-29 00:00:00.000 Start of Feb 29").unwrap();
        assert_eq!(compute_delta_micros(leap_feb28, leap_feb29_start), 1_000_000);
    }

    #[test]
    fn test_days_since_epoch_accuracy() {
        assert_eq!(days_since_epoch(1970, 1, 1), 0);
        assert_eq!(days_since_epoch(1969, 12, 31), -1);
        assert_eq!(days_since_epoch(2000, 1, 1), 10_957);
        assert_eq!(days_since_epoch(2026, 11, 6), 20_763);

        let ts_epoch = extract_timestamp(b"1970-01-01 00:00:00.000 Epoch").unwrap();
        assert_eq!(ts_epoch.total_micros, 0);

        let ts_2026 = extract_timestamp(b"2026-11-06 00:00:00.000 Midnight").unwrap();
        assert_eq!(ts_2026.total_micros, 20_763 * 86_400 * 1_000_000);
    }

    #[test]
    fn test_format_epoch_utc_epoch_zero() {
        assert_eq!(format_epoch_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_epoch_utc(86_399), "1970-01-01 23:59:59 UTC");
    }

    #[test]
    fn test_format_epoch_utc_known_date() {
        assert_eq!(format_epoch_utc(1_793_721_600), "2026-11-03 16:00:00 UTC");

        let y2k_leap_day = (days_since_epoch(2000, 2, 29) as u64) * 86_400 + 23 * 3600 + 59 * 60 + 59;
        assert_eq!(format_epoch_utc(y2k_leap_day), "2000-02-29 23:59:59 UTC");

        let leap_2024 = (days_since_epoch(2024, 2, 29) as u64) * 86_400 + 12 * 3600 + 34 * 60 + 56;
        assert_eq!(format_epoch_utc(leap_2024), "2024-02-29 12:34:56 UTC");

        let nov_2026 = (days_since_epoch(2026, 11, 6) as u64) * 86_400 + 3661;
        assert_eq!(format_epoch_utc(nov_2026), "2026-11-06 01:01:01 UTC");
    }

    #[test]
    fn test_compute_delta_micros_typed_skips_rollover_on_iso8601() {
        let later = extract_timestamp_with_kind(b"2026-12-31 23:59:59.000 later").unwrap();
        let earlier = extract_timestamp_with_kind(b"2026-01-01 00:00:01.000 earlier").unwrap();
        assert_eq!(later.kind, TimestampKind::Iso8601);
        assert_eq!(earlier.kind, TimestampKind::Iso8601);

        let delta = compute_delta_micros_typed(later, earlier);
        assert!(
            delta < 0,
            "Explicit-year ISO-8601 backward jump must remain negative, got {delta}"
        );
    }

    #[test]
    fn test_format_delta_micros_rounding_promotions() {
        assert_eq!(format_delta_micros(999_949), "+999.9ms");
        assert_eq!(format_delta_micros(999_950), "+1.00s");
        assert_eq!(format_delta_micros(999_999), "+1.00s");
        assert_eq!(format_delta_micros(59_994_999), "+59.99s");
        assert_eq!(format_delta_micros(59_995_000), "+1m 0.0s");
        assert_eq!(format_delta_micros(59_999_999), "+1m 0.0s");
        assert_eq!(format_delta_micros(3_599_949_999), "+59m 59.9s");
        assert_eq!(format_delta_micros(3_599_950_000), "+1h 0m 0s");
        assert_eq!(format_delta_micros(-999_999), "-1.00s");
        assert_eq!(format_delta_micros(-59_999_999), "-1m 0.0s");
    }

    #[test]
    fn test_normalize_month_day_rollover_early_year_threshold_and_late_anchors() {
        let anchor_nov01 = extract_timestamp(b"11-01 00:00:00.000 start").unwrap().total_micros;
        let anchor_oct28 = extract_timestamp(b"10-28 00:00:00.000 start").unwrap().total_micros;
        let anchor_oct20 = extract_timestamp(b"10-20 00:00:00.000 early_anchor").unwrap().total_micros;

        let ts_jan01 = extract_timestamp(b"01-01 00:00:01.000 jan1").unwrap().total_micros;
        let ts_jan06 = extract_timestamp(b"01-06 00:00:00.000 jan6").unwrap().total_micros;
        let ts_jan15 = extract_timestamp(b"01-15 12:00:00.000 jan15").unwrap().total_micros;
        let ts_feb15 = extract_timestamp(b"02-15 12:00:00.000 feb15").unwrap().total_micros;
        let ts_mar10 = extract_timestamp(b"03-10 12:00:00.000 mar10").unwrap().total_micros;

        assert!(anchor_nov01 >= YEAR_ROLLOVER_THRESHOLD_MICROS);
        assert!(anchor_oct28 >= YEAR_ROLLOVER_THRESHOLD_MICROS);
        assert!(anchor_oct20 < YEAR_ROLLOVER_THRESHOLD_MICROS);

        // Both 11-01 and 10-28 anchors must roll 01-01, 01-06, 01-15, and 02-15 (<= 60d)
        for anchor in [anchor_nov01, anchor_oct28] {
            assert_eq!(normalize_month_day_rollover(ts_jan01, anchor), ts_jan01 + ONE_YEAR_MICROS);
            assert_eq!(normalize_month_day_rollover(ts_jan06, anchor), ts_jan06 + ONE_YEAR_MICROS);
            assert_eq!(normalize_month_day_rollover(ts_jan15, anchor), ts_jan15 + ONE_YEAR_MICROS);
            assert_eq!(normalize_month_day_rollover(ts_feb15, anchor), ts_feb15 + ONE_YEAR_MICROS);
            // 03-10 (> 60d) must NOT roll over
            assert_eq!(normalize_month_day_rollover(ts_mar10, anchor), ts_mar10);
        }

        // Anchor < 300d must never roll over
        assert_eq!(normalize_month_day_rollover(ts_jan01, anchor_oct20), ts_jan01);
        assert_eq!(normalize_month_day_rollover(ts_jan06, anchor_oct20), ts_jan06);
        assert_eq!(normalize_month_day_rollover(ts_jan15, anchor_oct20), ts_jan15);
    }
}

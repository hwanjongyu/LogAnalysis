//! High-performance zero-allocation timestamp extraction and delta time calculation.

const DAYS_BEFORE_MONTH: [u32; 13] = [
    0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334,
];

const ONE_YEAR_MICROS: i64 = 365 * 86_400 * 1_000_000;

/// Represents a microsecond-precision normalized timestamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct LogTimestamp {
    pub total_micros: i64,
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
        let abs = micros.abs();
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
    let mut diff = t_curr.total_micros - t_prev.total_micros;
    // Year-end rollover: e.g., 12-31 23:59:59 -> 01-01 00:00:01
    if diff < -300 * 86_400 * 1_000_000 {
        diff += ONE_YEAR_MICROS;
    }
    diff
}

/// Formats a signed microsecond delta into a compact human-readable representation.
pub fn format_delta_micros(micros: i64) -> String {
    if micros == 0 {
        return "+0.0ms".to_string();
    }
    let sign = if micros >= 0 { "+" } else { "-" };
    let abs_us = micros.abs();

    if abs_us < 1_000 {
        format!("{sign}{abs_us}µs")
    } else if abs_us < 1_000_000 {
        let ms = (abs_us as f64) / 1000.0;
        format!("{sign}{ms:.1}ms")
    } else if abs_us < 60_000_000 {
        let sec = (abs_us as f64) / 1_000_000.0;
        format!("{sign}{sec:.2}s")
    } else if abs_us < 3_600_000_000 {
        let total_sec = abs_us / 1_000_000;
        let mins = total_sec / 60;
        let rem_sec = (abs_us % 60_000_000) as f64 / 1_000_000.0;
        format!("{sign}{mins}m {rem_sec:.1}s")
    } else {
        let total_sec = abs_us / 1_000_000;
        let hours = total_sec / 3600;
        let rem_sec = total_sec % 3600;
        let mins = rem_sec / 60;
        let secs = rem_sec % 60;
        format!("{sign}{hours}h {mins}m {secs}s")
    }
}

/// Zero-allocation fast extraction of a normalized timestamp from raw line bytes.
pub fn extract_timestamp(line: &[u8]) -> Option<LogTimestamp> {
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

        let days = day_of_year(month, day);
        let total_micros = calc_micros(days as i64, hour as i64, min as i64, sec as i64, millis as i64 * 1000);
        return Some(LogTimestamp { total_micros });
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

        let mut micros = 0;
        if trimmed.len() >= 23 && trimmed[19] == b'.' {
            if let Some(m) = parse_3_digits(&trimmed[20..23]) {
                micros = m as i64 * 1000;
            }
        }

        let days = (year as i64 - 1970) * 365 + day_of_year(month, day) as i64;
        let total_micros = calc_micros(days, hour as i64, min as i64, sec as i64, micros);
        return Some(LogTimestamp { total_micros });
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

        let total_micros = calc_micros(0, hour as i64, min as i64, sec as i64, millis as i64 * 1000);
        return Some(LogTimestamp { total_micros });
    }

    None
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
    let m = month.clamp(1, 12) as usize;
    DAYS_BEFORE_MONTH[m] + day.saturating_sub(1)
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
}

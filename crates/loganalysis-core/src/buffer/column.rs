use crate::filter::LineSeverity;

/// Structured representation of a single log line parsed into tabular fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedColumnLine<'a> {
    pub time: Option<&'a str>,
    pub pid: Option<&'a str>,
    pub tid: Option<&'a str>,
    pub level_char: Option<char>,
    pub level_severity: LineSeverity,
    pub tag: Option<&'a str>,
    pub message: &'a str,
}

/// Zero-copy fast parser for Android logcat (threadtime) and tabular logs.
pub fn parse_logcat_column_line<'a>(line: &'a str) -> ParsedColumnLine<'a> {
    let trimmed = line.trim_start();
    let bytes = trimmed.as_bytes();

    // Check for threadtime date: MM-DD (5 chars) + space + HH:MM:SS.mmm (12 chars)
    // Total datetime length = 18 chars (e.g. "11-06 18:00:00.123")
    // Full ASCII digit and delimiter validation strictly guarantees trimmed.is_char_boundary(18)
    if bytes.len() >= 18
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2] == b'-'
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && bytes[5] == b' '
        && bytes[6].is_ascii_digit()
        && bytes[7].is_ascii_digit()
        && bytes[8] == b':'
        && bytes[9].is_ascii_digit()
        && bytes[10].is_ascii_digit()
        && bytes[11] == b':'
        && bytes[12].is_ascii_digit()
        && bytes[13].is_ascii_digit()
        && bytes[14] == b'.'
        && bytes[15].is_ascii_digit()
        && bytes[16].is_ascii_digit()
        && bytes[17].is_ascii_digit()
        && trimmed.is_char_boundary(18)
    {
        let time_part = &trimmed[..18];
        let rest = &trimmed[18..];
        let rest_bytes = rest.as_bytes();
        let mut i = 0;

        // Skip whitespace before PID
        while i < rest_bytes.len() && (rest_bytes[i] == b' ' || rest_bytes[i] == b'\t') {
            i += 1;
        }
        let pid_start = i;
        while i < rest_bytes.len() && rest_bytes[i].is_ascii_digit() {
            i += 1;
        }
        let pid_end = i;

        // Skip whitespace before TID
        while i < rest_bytes.len() && (rest_bytes[i] == b' ' || rest_bytes[i] == b'\t') {
            i += 1;
        }
        let tid_start = i;
        while i < rest_bytes.len() && rest_bytes[i].is_ascii_digit() {
            i += 1;
        }
        let tid_end = i;

        // Skip whitespace before Level
        while i < rest_bytes.len() && (rest_bytes[i] == b' ' || rest_bytes[i] == b'\t') {
            i += 1;
        }

        if pid_start < pid_end && tid_start < tid_end && i < rest_bytes.len() {
            let lvl_char = (rest_bytes[i] as char).to_ascii_uppercase();
            i += 1;

            // In logcat format, Level must be followed by whitespace or line end
            if matches!(lvl_char, 'V' | 'D' | 'I' | 'W' | 'E' | 'F')
                && (i == rest_bytes.len() || rest_bytes[i] == b' ' || rest_bytes[i] == b'\t')
            {
                let severity = match lvl_char {
                    'E' | 'F' => LineSeverity::Error,
                    'W' => LineSeverity::Warning,
                    _ => LineSeverity::Normal,
                };

                let pid = &rest[pid_start..pid_end];
                let tid = &rest[tid_start..tid_end];
                let after_level = rest[i..].trim_start();

                // Tag is usually before ':'
                let (tag, message) = if let Some(colon_pos) = after_level.find(':') {
                    let tag = after_level[..colon_pos].trim();
                    let msg = after_level[colon_pos + 1..].trim_start();
                    (Some(tag), msg)
                } else {
                    let mut parts = after_level.splitn(2, ' ');
                    let tag = parts.next().map(|s| s.trim());
                    let msg = parts.next().unwrap_or("").trim_start();
                    (tag, msg)
                };

                return ParsedColumnLine {
                    time: Some(time_part),
                    pid: Some(pid),
                    tid: Some(tid),
                    level_char: Some(lvl_char),
                    level_severity: severity,
                    tag,
                    message,
                };
            }
        }
    }

    // Fallback for unstructured lines (stack traces, multiline exceptions, plain logs)
    ParsedColumnLine {
        time: None,
        pid: None,
        tid: None,
        level_char: None,
        level_severity: LineSeverity::Normal,
        tag: None,
        message: line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_threadtime_standard_error() {
        let line = "11-06 18:00:00.123  1000  1000 E AndroidRuntime: FATAL EXCEPTION: main";
        let parsed = parse_logcat_column_line(line);

        assert_eq!(parsed.time, Some("11-06 18:00:00.123"));
        assert_eq!(parsed.pid, Some("1000"));
        assert_eq!(parsed.tid, Some("1000"));
        assert_eq!(parsed.level_char, Some('E'));
        assert_eq!(parsed.level_severity, LineSeverity::Error);
        assert_eq!(parsed.tag, Some("AndroidRuntime"));
        assert_eq!(parsed.message, "FATAL EXCEPTION: main");
    }

    #[test]
    fn test_parse_threadtime_warning_and_spaces() {
        let line = "05-12 14:22:33.456   123   456 W Audio Flinger : Buffer underrun occurred";
        let parsed = parse_logcat_column_line(line);

        assert_eq!(parsed.time, Some("05-12 14:22:33.456"));
        assert_eq!(parsed.pid, Some("123"));
        assert_eq!(parsed.tid, Some("456"));
        assert_eq!(parsed.level_char, Some('W'));
        assert_eq!(parsed.level_severity, LineSeverity::Warning);
        assert_eq!(parsed.tag, Some("Audio Flinger"));
        assert_eq!(parsed.message, "Buffer underrun occurred");
    }

    #[test]
    fn test_parse_threadtime_info_without_colon() {
        let line = "01-01 00:00:01.000     1     2 I System ReadyToBoot";
        let parsed = parse_logcat_column_line(line);

        assert_eq!(parsed.time, Some("01-01 00:00:01.000"));
        assert_eq!(parsed.pid, Some("1"));
        assert_eq!(parsed.tid, Some("2"));
        assert_eq!(parsed.level_char, Some('I'));
        assert_eq!(parsed.level_severity, LineSeverity::Normal);
        assert_eq!(parsed.tag, Some("System"));
        assert_eq!(parsed.message, "ReadyToBoot");
    }

    #[test]
    fn test_parse_unstructured_stack_trace_fallback() {
        let line = "\tat com.example.app.MainActivity.onCreate(MainActivity.java:42)";
        let parsed = parse_logcat_column_line(line);

        assert_eq!(parsed.time, None);
        assert_eq!(parsed.pid, None);
        assert_eq!(parsed.tid, None);
        assert_eq!(parsed.level_char, None);
        assert_eq!(parsed.level_severity, LineSeverity::Normal);
        assert_eq!(parsed.tag, None);
        assert_eq!(parsed.message, line);
    }

    #[test]
    fn test_parse_non_logcat_level_token_fallback() {
        // 'X' is not a standard logcat severity (V/D/I/W/E/F)
        let line = "11-06 18:00:00.123  1000  1000 X CustomApp: Something strange";
        let parsed = parse_logcat_column_line(line);

        assert_eq!(parsed.time, None);
        assert_eq!(parsed.pid, None);
        assert_eq!(parsed.tid, None);
        assert_eq!(parsed.level_char, None);
        assert_eq!(parsed.message, line);
    }

    #[test]
    fn test_parse_level_pointer_offset_exactness() {
        // Tag contains same letter as level ('E')
        let line = "11-06 18:00:00.123  1000  1000 E EngineCore: Critical fault";
        let parsed = parse_logcat_column_line(line);

        assert_eq!(parsed.time, Some("11-06 18:00:00.123"));
        assert_eq!(parsed.pid, Some("1000"));
        assert_eq!(parsed.tid, Some("1000"));
        assert_eq!(parsed.level_char, Some('E'));
        assert_eq!(parsed.tag, Some("EngineCore"));
        assert_eq!(parsed.message, "Critical fault");
    }

    #[test]
    fn test_parse_column_utf8_char_boundary_safety_mid_codepoint() {
        // Multi-byte UTF-8 emoji (4 bytes: F0 9F 94 A5) spanning byte offset 18
        // Bytes 0..17 are 17 bytes, byte 17 is start of emoji, byte 18 is mid-codepoint.
        let line = "11-06 18:00:00.12🔥Tag: Msg";
        let parsed = parse_logcat_column_line(line);
        // Must NOT panic with byte index 18 is not a char boundary; must safely fallback
        assert_eq!(parsed.time, None);
        assert_eq!(parsed.message, line);
    }

    #[test]
    fn test_parse_column_non_digit_punctuation_match_rejection() {
        // Delimiters match "-", " ", ":", ":", "." but bytes are non-digits
        let line = "-- - -: -:.   1000  1000 E tag: msg";
        let parsed = parse_logcat_column_line(line);
        assert_eq!(parsed.time, None);
        assert_eq!(parsed.message, line);

        let line_chars = "AA-BB CC:DD:EE.FFF 1000 1000 E tag: msg";
        let parsed2 = parse_logcat_column_line(line_chars);
        assert_eq!(parsed2.time, None);
        assert_eq!(parsed2.message, line_chars);
    }

    #[test]
    fn test_parse_column_safe_token_offsets_without_pointer_arithmetic() {
        // Tag and message containing identical text to PID and Level
        let line = "11-06 18:00:00.123  1000  2000 E 1000: Error occurred with E in message";
        let parsed = parse_logcat_column_line(line);

        assert_eq!(parsed.time, Some("11-06 18:00:00.123"));
        assert_eq!(parsed.pid, Some("1000"));
        assert_eq!(parsed.tid, Some("2000"));
        assert_eq!(parsed.level_char, Some('E'));
        assert_eq!(parsed.level_severity, LineSeverity::Error);
        assert_eq!(parsed.tag, Some("1000"));
        assert_eq!(parsed.message, "Error occurred with E in message");
    }

    #[test]
    fn test_parse_column_empty_and_short_lines() {
        assert_eq!(parse_logcat_column_line("").time, None);
        assert_eq!(parse_logcat_column_line("11-06").time, None);
        assert_eq!(parse_logcat_column_line("11-06 18:00:00.1").time, None);
    }
}

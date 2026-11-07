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
    if bytes.len() >= 18
        && bytes[2] == b'-'
        && bytes[5] == b' '
        && bytes[8] == b':'
        && bytes[11] == b':'
        && bytes[14] == b'.'
    {
        let time_part = &trimmed[..18];
        let rest = &trimmed[18..];

        // Next tokens: PID, TID, Level
        let mut words = rest.split_whitespace();
        if let (Some(pid), Some(tid), Some(level_str)) = (words.next(), words.next(), words.next()) {
            if pid.chars().all(|c| c.is_ascii_digit())
                && tid.chars().all(|c| c.is_ascii_digit())
                && level_str.len() == 1
            {
                let lvl = level_str.chars().next().unwrap().to_ascii_uppercase();
                if matches!(lvl, 'V' | 'D' | 'I' | 'W' | 'E' | 'F') {
                    let severity = match lvl {
                        'E' | 'F' => LineSeverity::Error,
                        'W' => LineSeverity::Warning,
                        _ => LineSeverity::Normal,
                    };

                    // Exact subslice pointer offset eliminates substring ambiguity
                    let level_pos = (level_str.as_ptr() as usize).saturating_sub(rest.as_ptr() as usize);
                    let after_level = rest[level_pos + level_str.len()..].trim_start();

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
                        level_char: Some(lvl),
                        level_severity: severity,
                        tag,
                        message,
                    };
                }
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
}

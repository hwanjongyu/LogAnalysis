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

/// Extensible zero-copy Service Provider Interface (SPI) for structured log line parsing.
///
/// Implementations borrow slices directly from `line` without heap allocation, returning `Some`
/// when `line` matches their format or `None` so downstream parsers in a chain can inspect it.
pub trait LogFormatParser: Send + Sync {
    /// Stable format identifier (e.g. `"logcat-threadtime"`, `"logcat-brief"`).
    fn name(&self) -> &'static str;

    /// Attempts to parse `line` into structured columns. Returns `None` if unrecognized.
    fn parse<'a>(&self, line: &'a str) -> Option<ParsedColumnLine<'a>>;
}

/// Zero-copy parser for Android `logcat -v threadtime` lines:
/// `MM-DD HH:MM:SS.mmm  PID  TID L Tag: message`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadtimeColumnParser;

/// Zero-copy parser for Android `logcat -v brief` (`L/Tag(PID): message`) and tag-prefixed
/// (`L/Tag: message`) lines.
#[derive(Debug, Clone, Copy, Default)]
pub struct BriefColumnParser;

/// Shared static instance of [`ThreadtimeColumnParser`].
pub static THREADTIME_COLUMN_PARSER: ThreadtimeColumnParser = ThreadtimeColumnParser;

/// Shared static instance of [`BriefColumnParser`].
pub static BRIEF_COLUMN_PARSER: BriefColumnParser = BriefColumnParser;

/// Default parser fallback chain evaluated by [`parse_column_line_auto`].
pub static DEFAULT_COLUMN_PARSERS: [&dyn LogFormatParser; 2] = [
    &THREADTIME_COLUMN_PARSER,
    &BRIEF_COLUMN_PARSER,
];

#[inline]
fn severity_from_level_char(lvl_char: char) -> LineSeverity {
    match lvl_char {
        'E' | 'F' => LineSeverity::Error,
        'W' => LineSeverity::Warning,
        _ => LineSeverity::Normal,
    }
}

#[inline]
fn unstructured_column_line<'a>(line: &'a str) -> ParsedColumnLine<'a> {
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

impl LogFormatParser for ThreadtimeColumnParser {
    fn name(&self) -> &'static str {
        "logcat-threadtime"
    }

    fn parse<'a>(&self, line: &'a str) -> Option<ParsedColumnLine<'a>> {
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
                    let severity = severity_from_level_char(lvl_char);

                    let pid = &rest[pid_start..pid_end];
                    let tid = &rest[tid_start..tid_end];
                    let after_level = rest[i..].trim_start();

                    // Tag is usually before ':'
                    let (tag, message) = if let Some(colon_pos) = after_level.find(':') {
                        let raw_tag = after_level[..colon_pos].trim();
                        let tag = (!raw_tag.is_empty()).then_some(raw_tag);
                        let msg = after_level[colon_pos + 1..].trim_start();
                        (tag, msg)
                    } else {
                        let mut parts = after_level.splitn(2, ' ');
                        let tag = parts.next().map(str::trim).filter(|s| !s.is_empty());
                        let msg = parts.next().unwrap_or("").trim_start();
                        (tag, msg)
                    };

                    return Some(ParsedColumnLine {
                        time: Some(time_part),
                        pid: Some(pid),
                        tid: Some(tid),
                        level_char: Some(lvl_char),
                        level_severity: severity,
                        tag,
                        message,
                    });
                }
            }
        }

        None
    }
}

impl LogFormatParser for BriefColumnParser {
    fn name(&self) -> &'static str {
        "logcat-brief"
    }

    fn parse<'a>(&self, line: &'a str) -> Option<ParsedColumnLine<'a>> {
        let trimmed = line.trim_start();
        let bytes = trimmed.as_bytes();
        if bytes.len() < 3
            || !matches!(bytes[0], b'V' | b'D' | b'I' | b'W' | b'E' | b'F')
            || bytes[1] != b'/'
        {
            return None;
        }

        // Bytes 0..2 are pure ASCII (`L/`), so index 2 is guaranteed to be a UTF-8 char boundary.
        let after_slash = &trimmed[2..];
        let colon_pos = after_slash.find(':')?;
        let before_colon = &after_slash[..colon_pos];
        let after_colon = &after_slash[colon_pos + 1..];

        let header = before_colon.trim_end();
        let (tag_raw, pid) = if header.ends_with(')') {
            if let Some(open_idx) = header.rfind('(') {
                let inner = header[open_idx + 1..header.len() - 1].trim();
                if !inner.is_empty() && inner.bytes().all(|b| b.is_ascii_digit()) {
                    (header[..open_idx].trim(), Some(inner))
                } else {
                    (header.trim(), None)
                }
            } else {
                (header.trim(), None)
            }
        } else {
            (header.trim(), None)
        };

        if tag_raw.is_empty() {
            return None;
        }

        let level_char = bytes[0] as char;
        Some(ParsedColumnLine {
            time: None,
            pid,
            tid: None,
            level_char: Some(level_char),
            level_severity: severity_from_level_char(level_char),
            tag: Some(tag_raw),
            message: after_colon.trim_start(),
        })
    }
}

/// Attempts each parser in `parsers` in order, returning the first successful match
/// or an unstructured fallback containing the original `line` as `message`.
pub fn parse_column_line_with_parsers<'a>(
    line: &'a str,
    parsers: &[&dyn LogFormatParser],
) -> ParsedColumnLine<'a> {
    for parser in parsers {
        if let Some(parsed) = parser.parse(line) {
            return parsed;
        }
    }
    unstructured_column_line(line)
}

/// Parses `line` using [`DEFAULT_COLUMN_PARSERS`] (`threadtime` then `brief`),
/// falling back to an unstructured line when no parser matches.
pub fn parse_column_line_auto<'a>(line: &'a str) -> ParsedColumnLine<'a> {
    parse_column_line_with_parsers(line, &DEFAULT_COLUMN_PARSERS)
}

/// Zero-copy fast parser for Android logcat (`threadtime`) and tabular logs.
///
/// Preserves exact `threadtime`-only semantics with unstructured fallback for backward
/// compatibility with existing callers; use [`parse_column_line_auto`] or
/// [`parse_column_line_with_parsers`] for extensible multi-format parsing.
pub fn parse_logcat_column_line<'a>(line: &'a str) -> ParsedColumnLine<'a> {
    THREADTIME_COLUMN_PARSER
        .parse(line)
        .unwrap_or_else(|| unstructured_column_line(line))
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

    #[test]
    fn test_brief_column_parser_with_and_without_pid() {
        assert_eq!(THREADTIME_COLUMN_PARSER.name(), "logcat-threadtime");
        assert_eq!(BRIEF_COLUMN_PARSER.name(), "logcat-brief");

        // Standard brief format with padded PID
        let line_pid = "E/AndroidRuntime( 1234): FATAL EXCEPTION: main";
        let p_pid = BRIEF_COLUMN_PARSER.parse(line_pid).expect("parse brief with pid");
        assert_eq!(p_pid.time, None);
        assert_eq!(p_pid.pid, Some("1234"));
        assert_eq!(p_pid.tid, None);
        assert_eq!(p_pid.level_char, Some('E'));
        assert_eq!(p_pid.level_severity, LineSeverity::Error);
        assert_eq!(p_pid.tag, Some("AndroidRuntime"));
        assert_eq!(p_pid.message, "FATAL EXCEPTION: main");

        // Tag with spaces, warning severity, and compact PID
        let line_warn = "  W/Audio Flinger(42):   Buffer underrun: channel 0";
        let p_warn = BRIEF_COLUMN_PARSER.parse(line_warn).expect("parse brief warning");
        assert_eq!(p_warn.pid, Some("42"));
        assert_eq!(p_warn.level_char, Some('W'));
        assert_eq!(p_warn.level_severity, LineSeverity::Warning);
        assert_eq!(p_warn.tag, Some("Audio Flinger"));
        assert_eq!(p_warn.message, "Buffer underrun: channel 0");

        // Fatal ('F') maps to LineSeverity::Error
        let line_fatal = "F/libc(999): Fatal signal 11";
        let p_fatal = BRIEF_COLUMN_PARSER.parse(line_fatal).expect("parse brief fatal");
        assert_eq!(p_fatal.level_char, Some('F'));
        assert_eq!(p_fatal.level_severity, LineSeverity::Error);
        assert_eq!(p_fatal.pid, Some("999"));
        assert_eq!(p_fatal.tag, Some("libc"));

        // Brief / tag-prefixed without PID (`V`, `D`, `I`)
        let line_no_pid = "I/ActivityManager: Start proc com.example";
        let p_no_pid = BRIEF_COLUMN_PARSER.parse(line_no_pid).expect("parse brief without pid");
        assert_eq!(p_no_pid.time, None);
        assert_eq!(p_no_pid.pid, None);
        assert_eq!(p_no_pid.tid, None);
        assert_eq!(p_no_pid.level_char, Some('I'));
        assert_eq!(p_no_pid.level_severity, LineSeverity::Normal);
        assert_eq!(p_no_pid.tag, Some("ActivityManager"));
        assert_eq!(p_no_pid.message, "Start proc com.example");

        // Non-digit parenthesized suffix remains part of tag with pid = None
        let line_non_digit_paren = "D/Audio(v2): codec initialized";
        let p_paren = BRIEF_COLUMN_PARSER
            .parse(line_non_digit_paren)
            .expect("parse brief with non-digit paren");
        assert_eq!(p_paren.pid, None);
        assert_eq!(p_paren.level_char, Some('D'));
        assert_eq!(p_paren.tag, Some("Audio(v2)"));
        assert_eq!(p_paren.message, "codec initialized");

        // Verbose with empty message after colon
        let line_empty_msg = "V/Trace:   ";
        let p_empty_msg = BRIEF_COLUMN_PARSER.parse(line_empty_msg).expect("parse brief empty msg");
        assert_eq!(p_empty_msg.level_char, Some('V'));
        assert_eq!(p_empty_msg.tag, Some("Trace"));
        assert_eq!(p_empty_msg.message, "");
    }

    #[test]
    fn test_brief_column_parser_rejects_malformed_and_utf8_safe() {
        let rejected = [
            "",
            "I",
            "I/",
            "d/Tag: lowercase level",
            "i/usr/bin: path not brief log",
            "X/Tag(10): unknown severity",
            "I Tag: missing slash",
            "I/TagWithoutColon",
            "I/: empty tag",
            "I/(123): missing tag before pid",
            "I/   (123): whitespace-only tag",
            "🔥/Tag: multibyte severity",
        ];
        for bad in rejected {
            assert_eq!(
                BRIEF_COLUMN_PARSER.parse(bad),
                None,
                "Expected BriefColumnParser to reject {bad:?}"
            );
        }

        // UTF-8 multi-byte CJK + emoji in tag and message, plus prefix truncation safety
        let utf8_line = "E/한글태그🔥(100): 메시지 🚀 crash";
        let parsed = BRIEF_COLUMN_PARSER
            .parse(utf8_line)
            .expect("parse utf8 brief line");
        assert_eq!(parsed.level_char, Some('E'));
        assert_eq!(parsed.level_severity, LineSeverity::Error);
        assert_eq!(parsed.tag, Some("한글태그🔥"));
        assert_eq!(parsed.pid, Some("100"));
        assert_eq!(parsed.message, "메시지 🚀 crash");

        // Non-digit UTF-8 parenthesized tag suffix
        let utf8_paren = "W/서비스(코어): 경고 메시지";
        let parsed_paren = BRIEF_COLUMN_PARSER
            .parse(utf8_paren)
            .expect("parse utf8 paren tag");
        assert_eq!(parsed_paren.pid, None);
        assert_eq!(parsed_paren.tag, Some("서비스(코어)"));
        assert_eq!(parsed_paren.message, "경고 메시지");

        // Every UTF-8 prefix truncation must complete without panicking
        for (idx, _) in utf8_line.char_indices() {
            let _ = BRIEF_COLUMN_PARSER.parse(&utf8_line[..idx]);
        }

        // Backward compatibility: parse_logcat_column_line still keeps brief as unstructured
        let compat = parse_logcat_column_line("E/AndroidRuntime(123): crash");
        assert_eq!(compat.time, None);
        assert_eq!(compat.level_char, None);
        assert_eq!(compat.tag, None);
        assert_eq!(compat.message, "E/AndroidRuntime(123): crash");
    }

    #[test]
    fn test_parse_column_line_with_parsers_custom_spi() {
        struct PipeKeyValueParser;

        impl LogFormatParser for PipeKeyValueParser {
            fn name(&self) -> &'static str {
                "custom-pipe-kv"
            }

            fn parse<'a>(&self, line: &'a str) -> Option<ParsedColumnLine<'a>> {
                let rest = line.strip_prefix("CUSTOM|")?;
                let mut parts = rest.splitn(4, '|');
                let time = parts.next()?;
                let level_str = parts.next()?;
                let tag = parts.next()?;
                let message = parts.next()?;
                let level_char = level_str.chars().next()?;
                Some(ParsedColumnLine {
                    time: Some(time),
                    pid: None,
                    tid: None,
                    level_char: Some(level_char),
                    level_severity: severity_from_level_char(level_char),
                    tag: Some(tag),
                    message,
                })
            }
        }

        let custom = PipeKeyValueParser;
        assert_eq!(custom.name(), "custom-pipe-kv");

        let chain: [&dyn LogFormatParser; 3] = [
            &custom,
            &THREADTIME_COLUMN_PARSER,
            &BRIEF_COLUMN_PARSER,
        ];

        // 1. Custom format matched first and returns zero-copy slices into input line
        let custom_line = "CUSTOM|12:34:56|W|AuthGate|Token nearing expiry";
        let parsed_custom = parse_column_line_with_parsers(custom_line, &chain);
        assert_eq!(parsed_custom.time, Some("12:34:56"));
        assert_eq!(parsed_custom.level_char, Some('W'));
        assert_eq!(parsed_custom.level_severity, LineSeverity::Warning);
        assert_eq!(parsed_custom.tag, Some("AuthGate"));
        assert_eq!(parsed_custom.message, "Token nearing expiry");

        let start_addr = custom_line.as_ptr() as usize;
        let end_addr = start_addr + custom_line.len();
        let msg_addr = parsed_custom.message.as_ptr() as usize;
        assert!(msg_addr >= start_addr && msg_addr + parsed_custom.message.len() <= end_addr);

        // 2. Falls through custom parser to threadtime and brief
        let threadtime_line = "11-06 18:00:00.123  1000  2000 E Core: fault";
        let parsed_tt = parse_column_line_with_parsers(threadtime_line, &chain);
        assert_eq!(parsed_tt.time, Some("11-06 18:00:00.123"));
        assert_eq!(parsed_tt.tag, Some("Core"));

        let brief_line = "I/Sync(55): synced";
        let parsed_br = parse_column_line_with_parsers(brief_line, &chain);
        assert_eq!(parsed_br.pid, Some("55"));
        assert_eq!(parsed_br.tag, Some("Sync"));

        // 3. Empty parser slice returns unstructured fallback
        let fallback = parse_column_line_with_parsers(custom_line, &[]);
        assert_eq!(fallback.time, None);
        assert_eq!(fallback.tag, None);
        assert_eq!(fallback.message, custom_line);
    }

    #[test]
    fn test_parse_column_line_auto_mixed_formats() {
        assert_eq!(DEFAULT_COLUMN_PARSERS.len(), 2);
        assert_eq!(DEFAULT_COLUMN_PARSERS[0].name(), "logcat-threadtime");
        assert_eq!(DEFAULT_COLUMN_PARSERS[1].name(), "logcat-brief");

        let threadtime = "11-06 18:00:00.123  1000  2000 E AndroidRuntime: crash";
        let p_tt = parse_column_line_auto(threadtime);
        assert_eq!(p_tt.time, Some("11-06 18:00:00.123"));
        assert_eq!(p_tt.pid, Some("1000"));
        assert_eq!(p_tt.tid, Some("2000"));
        assert_eq!(p_tt.level_char, Some('E'));
        assert_eq!(p_tt.level_severity, LineSeverity::Error);
        assert_eq!(p_tt.tag, Some("AndroidRuntime"));
        assert_eq!(p_tt.message, "crash");

        let brief_pid = "W/AudioFlinger(456): underrun";
        let p_bp = parse_column_line_auto(brief_pid);
        assert_eq!(p_bp.time, None);
        assert_eq!(p_bp.pid, Some("456"));
        assert_eq!(p_bp.tid, None);
        assert_eq!(p_bp.level_char, Some('W'));
        assert_eq!(p_bp.level_severity, LineSeverity::Warning);
        assert_eq!(p_bp.tag, Some("AudioFlinger"));
        assert_eq!(p_bp.message, "underrun");

        let brief_no_pid = "I/SystemServer: Started";
        let p_bn = parse_column_line_auto(brief_no_pid);
        assert_eq!(p_bn.time, None);
        assert_eq!(p_bn.pid, None);
        assert_eq!(p_bn.level_char, Some('I'));
        assert_eq!(p_bn.level_severity, LineSeverity::Normal);
        assert_eq!(p_bn.tag, Some("SystemServer"));
        assert_eq!(p_bn.message, "Started");

        let stack_trace = "\tat com.example.Main.run(Main.java:10)";
        let p_st = parse_column_line_auto(stack_trace);
        assert_eq!(p_st.time, None);
        assert_eq!(p_st.pid, None);
        assert_eq!(p_st.level_char, None);
        assert_eq!(p_st.level_severity, LineSeverity::Normal);
        assert_eq!(p_st.tag, None);
        assert_eq!(p_st.message, stack_trace);

        let unstructured = "kernel: initializing subsystem";
        let p_un = parse_column_line_auto(unstructured);
        assert_eq!(p_un.time, None);
        assert_eq!(p_un.level_char, None);
        assert_eq!(p_un.tag, None);
        assert_eq!(p_un.message, unstructured);
    }

    #[test]
    fn test_threadtime_empty_tag_produces_none() {
        let with_empty_colon_tag = "11-06 18:00:00.123  1000  1000 I : msg";
        let p1 = parse_logcat_column_line(with_empty_colon_tag);
        assert_eq!(p1.tag, None);
        assert_eq!(p1.message, "msg");

        let level_only = "11-06 18:00:00.123  1000  1000 I";
        let p2 = parse_logcat_column_line(level_only);
        assert_eq!(p2.tag, None);
        assert_eq!(p2.message, "");
    }
}

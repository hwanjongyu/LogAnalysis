use super::model::FilterRule;
use crate::error::FilterError;

pub enum CompiledMatcher {
    /// Case-sensitive literal search using SIMD memmem
    LiteralCaseSensitive {
        finder: memchr::memmem::Finder<'static>,
        needle_len: usize,
    },
    /// Case-insensitive ASCII substring search fallback
    LiteralAsciiInsensitive(Vec<u8>),
    /// Standard regex matching raw bytes directly (SIMD Teddy / Aho-Corasick)
    StandardRegex(regex::bytes::Regex),
    /// Fancy regex fallback for lookaround / backreferences
    FancyRegex(fancy_regex::Regex),
}

impl CompiledMatcher {
    pub fn new(text: &str, case_sensitive: bool, is_regex: bool) -> Result<Self, FilterError> {
        if is_regex {
            // First try standard regex::bytes::Regex
            let mut builder = regex::bytes::RegexBuilder::new(text);
            builder.case_insensitive(!case_sensitive);
            match builder.build() {
                Ok(re) => Ok(Self::StandardRegex(re)),
                Err(std_err) => {
                    // Try fancy_regex for advanced features like (?=...)
                    let pattern = if case_sensitive {
                        text.to_string()
                    } else {
                        format!("(?i){}", text)
                    };
                    match fancy_regex::Regex::new(&pattern) {
                        Ok(fancy_re) => Ok(Self::FancyRegex(fancy_re)),
                        Err(_) => Err(FilterError::InvalidRegex(std_err.to_string())),
                    }
                }
            }
        } else if case_sensitive {
            let finder = memchr::memmem::Finder::new(text.as_bytes()).into_owned();
            Ok(Self::LiteralCaseSensitive {
                finder,
                needle_len: text.len(),
            })
        } else {
            // Compile case-insensitive literal using SIMD-accelerated regex bytes builder
            let escaped = regex::escape(text);
            let mut builder = regex::bytes::RegexBuilder::new(&escaped);
            builder.case_insensitive(true);
            match builder.build() {
                Ok(re) => Ok(Self::StandardRegex(re)),
                Err(_) => {
                    let lower_bytes: Vec<u8> = text.bytes().map(|b| b.to_ascii_lowercase()).collect();
                    Ok(Self::LiteralAsciiInsensitive(lower_bytes))
                }
            }
        }
    }

    #[inline]
    pub fn matches(&self, raw_line: &[u8]) -> bool {
        match self {
            Self::LiteralCaseSensitive { finder, .. } => finder.find(raw_line).is_some(),
            Self::LiteralAsciiInsensitive(needle) => {
                if needle.is_empty() {
                    return true;
                }
                if raw_line.len() < needle.len() {
                    return false;
                }
                raw_line.windows(needle.len()).any(|window| {
                    window.iter().zip(needle.iter()).all(|(&a, &b)| a.to_ascii_lowercase() == b)
                })
            }
            Self::StandardRegex(re) => re.is_match(raw_line),
            Self::FancyRegex(re) => {
                if let Ok(s) = std::str::from_utf8(raw_line) {
                    re.is_match(s).unwrap_or(false)
                } else {
                    let s = String::from_utf8_lossy(raw_line);
                    re.is_match(&s).unwrap_or(false)
                }
            }
        }
    }

    /// Finds non-overlapping match ranges in `text`.
    /// Strictly filters out empty ranges (start == end) to prevent infinite loops on zero-width matches.
    pub fn find_match_ranges(&self, text: &str, max_matches: usize) -> Vec<std::ops::Range<usize>> {
        let mut ranges = Vec::new();
        if text.is_empty() || max_matches == 0 {
            return ranges;
        }

        let mut last_end = 0;

        match self {
            Self::LiteralCaseSensitive { finder, needle_len } => {
                if *needle_len == 0 {
                    return ranges;
                }
                for start in finder.find_iter(text.as_bytes()) {
                    let end = start + needle_len;
                    if start >= last_end && start < end && text.is_char_boundary(start) && text.is_char_boundary(end) {
                        ranges.push(start..end);
                        last_end = end;
                        if ranges.len() >= max_matches {
                            break;
                        }
                    }
                }
            }
            Self::LiteralAsciiInsensitive(needle) => {
                if needle.is_empty() {
                    return ranges;
                }
                let bytes = text.as_bytes();
                let mut i = 0;
                while i + needle.len() <= bytes.len() && ranges.len() < max_matches {
                    if bytes[i..i + needle.len()]
                        .iter()
                        .zip(needle.iter())
                        .all(|(&a, &b)| a.to_ascii_lowercase() == b)
                    {
                        let start = i;
                        let end = i + needle.len();
                        if start >= last_end && start < end && text.is_char_boundary(start) && text.is_char_boundary(end) {
                            ranges.push(start..end);
                            last_end = end;
                            i = end;
                            continue;
                        }
                    }
                    i += 1;
                }
            }
            Self::StandardRegex(re) => {
                for m in re.find_iter(text.as_bytes()) {
                    let start = m.start();
                    let end = m.end();
                    if start >= last_end && start < end && text.is_char_boundary(start) && text.is_char_boundary(end) {
                        ranges.push(start..end);
                        last_end = end;
                        if ranges.len() >= max_matches {
                            break;
                        }
                    }
                }
            }
            Self::FancyRegex(re) => {
                for m in re.find_iter(text).flatten() {
                    let start = m.start();
                    let end = m.end();
                    if start >= last_end && start < end && text.is_char_boundary(start) && text.is_char_boundary(end) {
                        ranges.push(start..end);
                        last_end = end;
                        if ranges.len() >= max_matches {
                            break;
                        }
                    }
                }
            }
        }

        ranges
    }
}

pub struct PreparedFilter {
    pub rule: FilterRule,
    pub flat_index: usize,
    pub tab_index: usize,
    pub matcher: Option<CompiledMatcher>,
    pub resolved_bg_hex: Option<u32>,
    pub resolved_fg_hex: Option<u32>,
}

impl PreparedFilter {
    pub fn new(rule: FilterRule, flat_index: usize, tab_index: usize) -> Result<Self, FilterError> {
        let matcher = if !rule.active || rule.text.is_empty() {
            None
        } else {
            Some(CompiledMatcher::new(&rule.text, rule.case_sensitive, rule.regex)?)
        };

        let resolved_bg_hex = rule.resolved_bg_hex();
        let resolved_fg_hex = rule.resolved_fg_hex();

        Ok(Self {
            rule,
            flat_index,
            tab_index,
            matcher,
            resolved_bg_hex,
            resolved_fg_hex,
        })
    }

    #[inline]
    pub fn matches(&self, raw_line: &[u8]) -> bool {
        match &self.matcher {
            Some(m) => m.matches(raw_line),
            None => false,
        }
    }
}

use super::model::FilterRule;

pub enum CompiledMatcher {
    /// Case-sensitive literal search using SIMD memmem
    LiteralCaseSensitive(memchr::memmem::Finder<'static>),
    /// Case-insensitive ASCII substring search
    LiteralAsciiInsensitive(Vec<u8>),
    /// Standard regex matching raw bytes directly
    StandardRegex(regex::bytes::Regex),
    /// Fancy regex fallback for lookaround / backreferences
    FancyRegex(fancy_regex::Regex),
}

impl CompiledMatcher {
    pub fn new(text: &str, case_sensitive: bool, is_regex: bool) -> Result<Self, String> {
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
                        Err(_) => Err(std_err.to_string()),
                    }
                }
            }
        } else if case_sensitive {
            let finder = memchr::memmem::Finder::new(text.as_bytes()).into_owned();
            Ok(Self::LiteralCaseSensitive(finder))
        } else {
            // Lowercase ASCII bytes for fast matching
            let lower_bytes: Vec<u8> = text.bytes().map(|b| b.to_ascii_lowercase()).collect();
            Ok(Self::LiteralAsciiInsensitive(lower_bytes))
        }
    }

    #[inline]
    pub fn matches(&self, raw_line: &[u8]) -> bool {
        match self {
            Self::LiteralCaseSensitive(finder) => finder.find(raw_line).is_some(),
            Self::LiteralAsciiInsensitive(needle) => {
                if needle.is_empty() {
                    return true;
                }
                if raw_line.len() < needle.len() {
                    return false;
                }
                // Case-insensitive window search without heap allocation
                raw_line.windows(needle.len()).any(|window| {
                    window.iter().zip(needle.iter()).all(|(&a, &b)| a.to_ascii_lowercase() == b)
                })
            }
            Self::StandardRegex(re) => re.is_match(raw_line),
            Self::FancyRegex(re) => {
                // Fancy regex requires a str representation
                let s = String::from_utf8_lossy(raw_line);
                re.is_match(&s).unwrap_or(false)
            }
        }
    }
}

pub struct PreparedFilter {
    pub rule: FilterRule,
    pub flat_index: usize,
    pub tab_index: usize,
    pub matcher: Option<CompiledMatcher>,
}

impl PreparedFilter {
    pub fn new(rule: FilterRule, flat_index: usize, tab_index: usize) -> Result<Self, String> {
        let matcher = if !rule.active || rule.text.is_empty() {
            None
        } else {
            Some(CompiledMatcher::new(&rule.text, rule.case_sensitive, rule.regex)?)
        };

        Ok(Self {
            rule,
            flat_index,
            tab_index,
            matcher,
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

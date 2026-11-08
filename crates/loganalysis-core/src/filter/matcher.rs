use super::model::FilterRule;
use crate::error::FilterError;
use aho_corasick::{AhoCorasick, MatchKind};
use smallvec::SmallVec;

/// Maximum number of backtracking steps permitted during fancy_regex evaluation
/// before aborting to prevent catastrophic ReDoS CPU starvation.
pub const FANCY_REGEX_BACKTRACK_LIMIT: usize = 250;

/// Inspects a `fancy_regex::Expr` AST to statically identify nested unbounded repetitions
/// (e.g., `(a+)+`, `((a+)*)+`, `([0-9]+)+`, `(?=(a+)+)`) that cause $O(2^n)$ catastrophic
/// backtracking on non-matching inputs.
pub fn has_nested_repetition(expr: &fancy_regex::Expr, in_repeat: bool) -> bool {
    match expr {
        fancy_regex::Expr::Repeat { child, hi, .. } => {
            let is_unbounded = *hi == usize::MAX;
            if in_repeat && is_unbounded {
                return true;
            }
            has_nested_repetition(child, in_repeat || is_unbounded)
        }
        fancy_regex::Expr::Group(child) => has_nested_repetition(child, in_repeat),
        fancy_regex::Expr::LookAround(child, _) => has_nested_repetition(child, in_repeat),
        fancy_regex::Expr::AtomicGroup(child) => has_nested_repetition(child, in_repeat),
        fancy_regex::Expr::Concat(children) => {
            children.iter().any(|c| has_nested_repetition(c, in_repeat))
        }
        fancy_regex::Expr::Alt(children) => {
            children.iter().any(|c| has_nested_repetition(c, in_repeat))
        }
        fancy_regex::Expr::Conditional {
            condition,
            true_branch,
            false_branch,
        } => {
            has_nested_repetition(condition, in_repeat)
                || has_nested_repetition(true_branch, in_repeat)
                || has_nested_repetition(false_branch, in_repeat)
        }
        _ => false,
    }
}

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
            // First try standard regex::bytes::Regex (linear-time DFA immune to ReDoS)
            let mut builder = regex::bytes::RegexBuilder::new(text);
            builder.case_insensitive(!case_sensitive);
            match builder.build() {
                Ok(re) => Ok(Self::StandardRegex(re)),
                Err(std_err) => {
                    let pattern = if case_sensitive {
                        text.to_string()
                    } else {
                        format!("(?i){}", text)
                    };

                    // Static AST analysis: reject nested unbounded repetitions (ReDoS prevention)
                    if let Ok(tree) = fancy_regex::Expr::parse_tree(&pattern) {
                        if has_nested_repetition(&tree.expr, false) {
                            return Err(FilterError::InvalidRegex(
                                "Catastrophic backtracking expression rejected (ReDoS protection)"
                                    .to_string(),
                            ));
                        }
                    }

                    let mut fancy_builder = fancy_regex::RegexBuilder::new(&pattern);
                    fancy_builder.backtrack_limit(FANCY_REGEX_BACKTRACK_LIMIT);
                    match fancy_builder.build() {
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
                    let lower_bytes: Vec<u8> =
                        text.bytes().map(|b| b.to_ascii_lowercase()).collect();
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
                    window
                        .iter()
                        .zip(needle.iter())
                        .all(|(&a, &b)| a.to_ascii_lowercase() == b)
                })
            }
            Self::StandardRegex(re) => re.is_match(raw_line),
            Self::FancyRegex(re) => {
                let match_result = if let Ok(s) = std::str::from_utf8(raw_line) {
                    re.is_match(s)
                } else {
                    let s = String::from_utf8_lossy(raw_line);
                    re.is_match(&s)
                };
                match match_result {
                    Ok(matched) => matched,
                    Err(fancy_regex::Error::RuntimeError(
                        fancy_regex::RuntimeError::BacktrackLimitExceeded,
                    )) => {
                        // Safely treat catastrophic backtracking as non-matching
                        false
                    }
                    Err(_) => false,
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
                    if start >= last_end
                        && start < end
                        && text.is_char_boundary(start)
                        && text.is_char_boundary(end)
                    {
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
                        if start >= last_end
                            && start < end
                            && text.is_char_boundary(start)
                            && text.is_char_boundary(end)
                        {
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
                    if start >= last_end
                        && start < end
                        && text.is_char_boundary(start)
                        && text.is_char_boundary(end)
                    {
                        ranges.push(start..end);
                        last_end = end;
                        if ranges.len() >= max_matches {
                            break;
                        }
                    }
                }
            }
            Self::FancyRegex(re) => {
                for m in re.find_iter(text) {
                    match m {
                        Ok(matched) => {
                            let start = matched.start();
                            let end = matched.end();
                            if start >= last_end
                                && start < end
                                && text.is_char_boundary(start)
                                && text.is_char_boundary(end)
                            {
                                ranges.push(start..end);
                                last_end = end;
                                if ranges.len() >= max_matches {
                                    break;
                                }
                            }
                        }
                        Err(fancy_regex::Error::RuntimeError(
                            fancy_regex::RuntimeError::BacktrackLimitExceeded,
                        )) => {
                            // Safely terminate iteration on backtrack limit exhaustion
                            break;
                        }
                        Err(_) => break,
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
    pub matcher: Option<CompiledMatcher>,
    pub resolved_bg_hex: Option<u32>,
    pub resolved_fg_hex: Option<u32>,
}

impl PreparedFilter {
    pub fn new(rule: FilterRule, flat_index: usize) -> Result<Self, FilterError> {
        let matcher = if !rule.active || rule.text.is_empty() {
            None
        } else {
            Some(CompiledMatcher::new(
                &rule.text,
                rule.case_sensitive,
                rule.regex,
            )?)
        };

        let resolved_bg_hex = rule.resolved_bg_hex();
        let resolved_fg_hex = rule.resolved_fg_hex();

        Ok(Self {
            rule,
            flat_index,
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

/// Single-pass multi-pattern filter matcher powered by Aho-Corasick SIMD.
///
/// Combines multiple literal filter rules into case-sensitive and case-insensitive
/// automatons, scanning each line in a single pass ($O(N)$) instead of sequentially testing
/// each rule ($O(N \cdot K)$).
pub struct MultiFilterMatcher {
    cs_automaton: Option<(AhoCorasick, Vec<usize>)>,
    ci_automaton: Option<(AhoCorasick, Vec<usize>)>,
    individual_filters: Vec<usize>,
}

impl MultiFilterMatcher {
    pub fn new(filters: &[PreparedFilter]) -> Self {
        let mut cs_patterns = Vec::new();
        let mut cs_indices = Vec::new();

        let mut ci_patterns = Vec::new();
        let mut ci_indices = Vec::new();

        let mut individual_filters = Vec::new();

        for (idx, pf) in filters.iter().enumerate() {
            if pf.matcher.is_none() {
                continue;
            }
            if pf.rule.regex {
                individual_filters.push(idx);
            } else if pf.rule.case_sensitive {
                cs_patterns.push(pf.rule.text.as_bytes());
                cs_indices.push(idx);
            } else {
                ci_patterns.push(pf.rule.text.as_bytes());
                ci_indices.push(idx);
            }
        }

        let cs_automaton = if !cs_patterns.is_empty() {
            AhoCorasick::builder()
                .match_kind(MatchKind::Standard)
                .build(&cs_patterns)
                .ok()
                .map(|ac| (ac, cs_indices))
        } else {
            None
        };

        let ci_automaton = if !ci_patterns.is_empty() {
            AhoCorasick::builder()
                .ascii_case_insensitive(true)
                .match_kind(MatchKind::Standard)
                .build(&ci_patterns)
                .ok()
                .map(|ac| (ac, ci_indices))
        } else {
            None
        };

        Self {
            cs_automaton,
            ci_automaton,
            individual_filters,
        }
    }

    #[inline]
    pub fn find_matches(
        &self,
        raw_line: &[u8],
        filters: &[PreparedFilter],
        out: &mut SmallVec<[usize; 8]>,
    ) {
        out.clear();

        if let Some((ac, map)) = &self.ci_automaton {
            for m in ac.find_overlapping_iter(raw_line) {
                out.push(map[m.pattern().as_usize()]);
            }
        }

        if let Some((ac, map)) = &self.cs_automaton {
            for m in ac.find_overlapping_iter(raw_line) {
                out.push(map[m.pattern().as_usize()]);
            }
        }

        for &idx in &self.individual_filters {
            if filters[idx].matches(raw_line) {
                out.push(idx);
            }
        }

        if out.len() > 1 {
            out.sort_unstable();
            out.dedup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redos_nested_quantifiers_compile_time_rejection() {
        // Lookaround patterns requiring fancy_regex fallback that contain nested unbounded repetitions
        // must be rejected at compile time.
        let redos_patterns = vec![
            r"(?=(a+)+)test",
            r"(?=((a+)*)+)test",
            r"(?=([0-9]+)+)test",
            r"(?!((a+)+))test",
            r"(?<=(a+)+)test",
        ];

        for pat in redos_patterns {
            for case_sensitive in [true, false] {
                let res = CompiledMatcher::new(pat, case_sensitive, true);
                assert!(
                    res.is_err(),
                    "Pattern '{}' (case_sensitive={}) must be rejected at compile time as ReDoS risk",
                    pat,
                    case_sensitive
                );
                match res {
                    Err(FilterError::InvalidRegex(msg)) => {
                        assert!(
                            msg.contains("ReDoS") || msg.contains("Catastrophic backtracking"),
                            "Error message should explain ReDoS protection, got: {}",
                            msg
                        );
                    }
                    Err(e) => panic!("Expected FilterError::InvalidRegex, got {:?}", e),
                    Ok(_) => panic!(
                        "Expected FilterError::InvalidRegex, got Ok for pattern '{}'",
                        pat
                    ),
                }
            }
        }

        // Standard regexes without lookarounds are linear-time DFAs immune to ReDoS,
        // so they compile into StandardRegex successfully.
        for pat in [r"(a+)+$", r"((a+)*)+", r"([0-9]+)+", r"(a|aa)+$"] {
            for case_sensitive in [true, false] {
                let res = CompiledMatcher::new(pat, case_sensitive, true);
                assert!(
                    matches!(res, Ok(CompiledMatcher::StandardRegex(_))),
                    "Pattern '{}' (case_sensitive={}) should compile as StandardRegex DFA",
                    pat,
                    case_sensitive
                );
            }
        }
    }

    #[test]
    fn test_has_nested_repetition_ast_direct() {
        let redos_cases = vec![
            r"(a+)+$",
            r"((a+)*)+",
            r"([0-9]+)+",
            r"(?=(a+)+)test",
            r"(a+b+)+",
            r"(a*|b*)*",
            r"(?>a+)+",
            r"((a+))+",
        ];

        for pat in redos_cases {
            let tree = fancy_regex::Expr::parse_tree(pat)
                .unwrap_or_else(|e| panic!("Pattern '{}' failed to parse: {:?}", pat, e));
            assert!(
                has_nested_repetition(&tree.expr, false),
                "Pattern '{}' must be detected as containing nested repetitions",
                pat
            );
        }

        let legitimate_cases = vec![
            r".*",
            r"a+",
            r"[0-9]+",
            r"(a|b)*",
            r"a+b+",
            r"(a+b)",
            r"(?=.*error)fatal",
            r"(?i)(?=.*exception)",
            r"(?<=TAG_)\w+",
            r"^(?!DEBUG).*",
            r"\d{4}-\d{2}-\d{2}",
        ];

        for pat in legitimate_cases {
            let tree = fancy_regex::Expr::parse_tree(pat)
                .unwrap_or_else(|e| panic!("Pattern '{}' failed to parse: {:?}", pat, e));
            assert!(
                !has_nested_repetition(&tree.expr, false),
                "Pattern '{}' should NOT be flagged as nested repetition",
                pat
            );
        }
    }

    #[test]
    fn test_redos_runtime_backtrack_limit_bounded_execution() {
        // Lookaround patterns with ambiguous alternations that require exponential backtracking
        // on non-matching strings, but do not contain syntactic nested unbounded repetitions.
        let patterns = vec![
            (
                r"(?i)(a|b|ab)*(?=c)",
                "abababababababababababababababababababababababababababab".repeat(2),
            ),
            (r"(?=(a|aa)+$)b", "a".repeat(100)),
        ];

        for (pat, long_non_matching) in patterns {
            let matcher = CompiledMatcher::new(pat, true, true)
                .unwrap_or_else(|e| panic!("Pattern '{}' failed to compile: {:?}", pat, e));

            // Verify matches completes well under 5ms and returns false
            let start = std::time::Instant::now();
            let matched = matcher.matches(long_non_matching.as_bytes());
            let elapsed = start.elapsed();

            assert!(
                !matched,
                "Pattern '{}' must return false on non-matching input",
                pat
            );
            assert!(
                elapsed < std::time::Duration::from_millis(25),
                "Pattern '{}' matching took too long ({:?} >= 25ms), ReDoS protection failed",
                pat,
                elapsed
            );

            // Verify find_match_ranges completes under 25ms and returns empty ranges without crashing
            let start = std::time::Instant::now();
            let ranges = matcher.find_match_ranges(&long_non_matching, 10);
            let elapsed = start.elapsed();

            assert!(
                ranges.is_empty(),
                "Pattern '{}' find_match_ranges should be empty, got: {:?}",
                pat,
                ranges
            );
            assert!(
                elapsed < std::time::Duration::from_millis(25),
                "Pattern '{}' find_match_ranges took too long ({:?} >= 25ms)",
                pat,
                elapsed
            );
        }
    }

    #[test]
    fn test_legitimate_lookaround_patterns() {
        // 1. Positive lookahead
        let matcher =
            CompiledMatcher::new(r"(?=.*error)fatal", true, true).expect("should compile");
        assert!(matcher.matches(b"fatal: unexpected error occurred"));
        assert!(!matcher.matches(b"info: clean system startup"));

        let ranges = matcher.find_match_ranges("fatal: unexpected error occurred", 10);
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            &"fatal: unexpected error occurred"[ranges[0].clone()],
            "fatal"
        );

        // 2. Case-insensitive lookahead
        let matcher_ci =
            CompiledMatcher::new(r"(?i)(?=.*exception)", false, true).expect("should compile");
        assert!(matcher_ci.matches(b"CRITICAL EXCEPTION in worker thread"));
        assert!(matcher_ci.matches(b"minor exception occurred"));
        assert!(!matcher_ci.matches(b"routine diagnostic trace"));

        // 3. Positive lookbehind
        let matcher_lb = CompiledMatcher::new(r"(?<=TAG_)\w+", true, true).expect("should compile");
        assert!(matcher_lb.matches(b"Received event from TAG_BLUETOOTH_MGR active"));
        assert!(!matcher_lb.matches(b"Received event from NO_PREFIX_MGR active"));

        let ranges_lb =
            matcher_lb.find_match_ranges("Received event from TAG_BLUETOOTH_MGR active", 10);
        assert_eq!(ranges_lb.len(), 1);
        assert_eq!(
            &"Received event from TAG_BLUETOOTH_MGR active"[ranges_lb[0].clone()],
            "BLUETOOTH_MGR"
        );

        // 4. Negative lookahead
        let matcher_neg =
            CompiledMatcher::new(r"^(?!DEBUG).*status ok", true, true).expect("should compile");
        assert!(matcher_neg.matches(b"INFO: system status ok"));
        assert!(!matcher_neg.matches(b"DEBUG: system status ok"));
    }

    #[test]
    fn test_literal_and_standard_regex_unaffected() {
        // Case-sensitive literal
        let cs_lit =
            CompiledMatcher::new("FatalError", true, false).expect("literal should compile");
        assert!(cs_lit.matches(b"Server threw FatalError now"));
        assert!(!cs_lit.matches(b"Server threw fatalerror now"));

        // Case-insensitive literal
        let ci_lit =
            CompiledMatcher::new("FatalError", false, false).expect("literal should compile");
        assert!(ci_lit.matches(b"Server threw FatalError now"));
        assert!(ci_lit.matches(b"Server threw fatalerror now"));

        // Standard regex
        let std_re = CompiledMatcher::new(r"Error\[\d+\]", true, true)
            .expect("standard regex should compile");
        assert!(std_re.matches(b"Failed with Error[404]"));
        assert!(!std_re.matches(b"Failed with Warning[404]"));
    }
}

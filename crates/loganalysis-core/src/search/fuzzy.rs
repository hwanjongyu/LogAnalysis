/// Subsequence fuzzy matcher with word-boundary, consecutive, and CamelCase bonuses.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyMatchResult {
    pub score: i32,
    pub matched_indices: Vec<usize>,
}

/// Matches `query` as a case-insensitive subsequence of `target`.
/// Returns `Some(score)` if the query matches in order, or `None` if it does not match.
pub fn fuzzy_match(query: &str, target: &str) -> Option<i32> {
    fuzzy_match_detailed(query, target).map(|r| r.score)
}

/// Matches `query` as a case-insensitive subsequence of `target` and returns the score
/// along with the indices in `target` (in character offsets) that matched.
pub fn fuzzy_match_detailed(query: &str, target: &str) -> Option<FuzzyMatchResult> {
    if query.is_empty() {
        return Some(FuzzyMatchResult {
            score: 0,
            matched_indices: Vec::new(),
        });
    }

    let orig_t_chars: Vec<char> = target.chars().collect();
    let t_chars: Vec<char> = orig_t_chars
        .iter()
        .map(|&c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let q_chars: Vec<char> = query
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();

    let mut q_idx = 0;
    let mut score: i32 = 0;
    let mut consecutive: i32 = 0;
    let mut prev_matched_idx = None;
    let mut matched_indices = Vec::with_capacity(q_chars.len());

    for (t_idx, &t_c) in t_chars.iter().enumerate() {
        if q_idx < q_chars.len() && t_c == q_chars[q_idx] {
            matched_indices.push(t_idx);
            score = score.saturating_add(10);

            // Prefix match bonus
            if t_idx == 0 {
                score = score.saturating_add(60);
            }

            // Consecutive match bonus
            if let Some(prev) = prev_matched_idx {
                if prev + 1 == t_idx {
                    consecutive = consecutive.saturating_add(1);
                    score = score.saturating_add(consecutive.saturating_mul(15));
                } else {
                    consecutive = 0;
                }
            }
            prev_matched_idx = Some(t_idx);

            // Word boundary bonus
            if t_idx > 0 {
                let prev_c = t_chars[t_idx - 1];
                if prev_c == ' ' || prev_c == ':' || prev_c == '/' || prev_c == '-' || prev_c == '_' {
                    score = score.saturating_add(35);
                }
            }

            // CamelCase transition bonus
            if t_idx > 0 && orig_t_chars[t_idx].is_uppercase() && orig_t_chars[t_idx - 1].is_lowercase() {
                score = score.saturating_add(25);
            }

            q_idx += 1;
        } else {
            consecutive = 0;
        }
    }

    if q_idx == q_chars.len() {
        // Penalty for excessive length disparity
        let length_penalty = t_chars.len().saturating_sub(q_chars.len()).min(i32::MAX as usize) as i32;
        Some(FuzzyMatchResult {
            score: score.saturating_sub(length_penalty),
            matched_indices,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_empty_query() {
        assert_eq!(fuzzy_match("", "Any Target"), Some(0));
    }

    #[test]
    fn test_fuzzy_match_exact() {
        let score = fuzzy_match("open", "open").expect("Should match exact");
        assert!(score > 0);
    }

    #[test]
    fn test_fuzzy_match_word_boundary() {
        let score_boundary = fuzzy_match("open", "File: Open Log File...").unwrap();
        let score_internal = fuzzy_match("open", "Propeller").unwrap_or(-999);
        assert!(score_boundary > score_internal);
    }

    #[test]
    fn test_fuzzy_match_acronym() {
        let res = fuzzy_match("vtp", "View: Toggle Filter Panel Position");
        assert!(res.is_some());
    }

    #[test]
    fn test_fuzzy_match_no_match() {
        assert_eq!(fuzzy_match("xyz", "File: Open Log"), None);
    }

    #[test]
    fn test_fuzzy_match_unicode_expansion_safety_turkish_i() {
        // Latin Capital Letter I with Dot Above '\u{0130}' ('İ')
        // Standard .to_lowercase() expands to 'i' + '\u{0307}' (length 2 vs 1)
        // Must NOT panic on out-of-bounds indexing in orig_t_chars
        let target = "İtemList";
        let res = fuzzy_match_detailed("item", target);
        assert!(res.is_some(), "Should match lowercased query against Turkish İ");
        let r = res.unwrap();
        assert_eq!(r.matched_indices, vec![0, 1, 2, 3]);

        // Target with expanding char at end
        let target_end = "Fileİ";
        let res_end = fuzzy_match_detailed("i", target_end);
        assert!(res_end.is_some());
        assert_eq!(res_end.unwrap().matched_indices, vec![1]); // matches 'i' in 'File'

        let res_dot = fuzzy_match_detailed("İ", "İ");
        assert!(res_dot.is_some());
        assert_eq!(res_dot.unwrap().matched_indices, vec![0]);
    }

    #[test]
    fn test_fuzzy_match_unicode_expansion_camel_case_bonus() {
        // CamelCase check: orig_t_chars[t_idx] uppercase, orig_t_chars[t_idx - 1] lowercase
        let target = "İtemDetail";
        let res = fuzzy_match_detailed("id", target);
        assert!(res.is_some());
        let r = res.unwrap();
        assert_eq!(r.matched_indices, vec![0, 4]); // 'İ' at 0, 'D' at 4
    }

    #[test]
    fn test_fuzzy_match_unicode_invariants_german_sharp_s() {
        // German capital sharp S 'ẞ' (\u{1E9E})
        let target = "STRAẞE";
        let res = fuzzy_match("straße", target);
        // Does not panic and score is bounded
        assert!(res.is_some());
    }
}

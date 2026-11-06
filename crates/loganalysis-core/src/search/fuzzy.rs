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

    let q_chars: Vec<char> = query.to_lowercase().chars().collect();
    let t_chars: Vec<char> = target.to_lowercase().chars().collect();
    let orig_t_chars: Vec<char> = target.chars().collect();

    let mut q_idx = 0;
    let mut score = 0;
    let mut consecutive = 0;
    let mut prev_matched_idx = None;
    let mut matched_indices = Vec::with_capacity(q_chars.len());

    for (t_idx, &t_c) in t_chars.iter().enumerate() {
        if q_idx < q_chars.len() && t_c == q_chars[q_idx] {
            matched_indices.push(t_idx);
            score += 10;

            // Prefix match bonus
            if t_idx == 0 {
                score += 60;
            }

            // Consecutive match bonus
            if let Some(prev) = prev_matched_idx {
                if prev + 1 == t_idx {
                    consecutive += 1;
                    score += 15 * consecutive;
                } else {
                    consecutive = 0;
                }
            }
            prev_matched_idx = Some(t_idx);

            // Word boundary bonus
            if t_idx > 0 {
                let prev_c = t_chars[t_idx - 1];
                if prev_c == ' ' || prev_c == ':' || prev_c == '/' || prev_c == '-' || prev_c == '_' {
                    score += 35;
                }
            }

            // CamelCase transition bonus
            if t_idx > 0 && orig_t_chars[t_idx].is_uppercase() && orig_t_chars[t_idx - 1].is_lowercase() {
                score += 25;
            }

            q_idx += 1;
        } else {
            consecutive = 0;
        }
    }

    if q_idx == q_chars.len() {
        // Penalty for excessive length disparity
        let length_penalty = (t_chars.len() as i32 - q_chars.len() as i32).max(0);
        Some(FuzzyMatchResult {
            score: score - length_penalty,
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
}

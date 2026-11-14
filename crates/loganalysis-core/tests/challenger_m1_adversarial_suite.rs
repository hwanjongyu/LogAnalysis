//! Empirical Challenger Verification Test Suite for Milestone 1 (core-remediation).
//! Actively stress-tests:
//! 1. `parse_logcat_column_line`: truncated strings, multi-byte UTF-8, emojis at boundaries, malformed timestamps, weird delimiters.
//! 2. `fuzzy_match_detailed`: expanding Unicode characters (Turkish 'İ', German 'ß', Greek characters), mismatched case combinations, edge case queries.
//! 3. `timestamp.rs` & `merged_buffer.rs`: Feb 29 vs Mar 01 monotonicity, out-of-range dates (`99-99`), reverse order multi-file merge (`[jan01, dec31]` vs `[dec31, jan01]`).
//! 4. ReDoS AST checking: bounded nested repetitions `(a{1,500}){1,500}`, `(a{2,10}){2,10}`, complex alternations inside repeats, DFA limits.

use loganalysis_core::buffer::column::parse_logcat_column_line;
use loganalysis_core::buffer::merged_buffer::MergedLogBuffer;
use loganalysis_core::buffer::timestamp::{compute_delta_micros, extract_timestamp};
use loganalysis_core::buffer::LogSource;
use loganalysis_core::filter::matcher::has_nested_repetition;
use loganalysis_core::{CompiledMatcher, FilterError};
use loganalysis_core::search::fuzzy_match_detailed;
use std::io::Write;
use std::time::Instant;
use tempfile::NamedTempFile;

// ============================================================================
// CHALLENGE 1: parse_logcat_column_line Stress Testing
// ============================================================================

#[test]
fn challenge_1_truncated_strings_no_panics() {
    let standard = "11-06 18:00:00.123  1000  2000 E TagName: Log message content";

    // Test every single character prefix from 0 to full length
    for len in 0..=standard.len() {
        let prefix = &standard[..len];
        let parsed = parse_logcat_column_line(prefix);
        if len < 18 {
            assert!(parsed.time.is_none(), "Prefix of length {} should not have time", len);
            assert_eq!(parsed.message, prefix);
        }
    }
}

#[test]
fn challenge_1_multibyte_utf8_and_emojis_at_all_boundaries() {
    // Emojis: 4 bytes each (🔥 = F0 9F 94 A5, 🚀 = F0 9F 9A 80, 💀 = F0 9F 92 80)
    // 2-byte: © = C2 A9, д = D0 B4
    // 3-byte: 가 = EA B0 80, 日 = E6 97 A5, € = E2 82 AC

    let base_date = "11-06 18:00:00.123";
    assert_eq!(base_date.len(), 18);

    // Place emojis right at and around byte 18
    let test_cases = vec![
        // Emoji replaces last digit: 17 bytes ASCII + 4-byte emoji
        "11-06 18:00:00.12🔥  1000  2000 E Tag: Msg",
        // Emoji right at byte 18
        "11-06 18:00:00.123🔥 1000  2000 E Tag: Msg",
        // Multi-byte in month/day/time
        "🔥-06 18:00:00.123  1000  2000 E Tag: Msg",
        "11-🔥 18:00:00.123  1000  2000 E Tag: Msg",
        "11-06 🔥:00:00.123  1000  2000 E Tag: Msg",
        "11-06 18:🔥:00.123  1000  2000 E Tag: Msg",
        "11-06 18:00:🔥.123  1000  2000 E Tag: Msg",
        // Emoji in PID/TID
        "11-06 18:00:00.123  🔥  2000 E Tag: Msg",
        "11-06 18:00:00.123  1000  🔥 E Tag: Msg",
        // Emoji as Level
        "11-06 18:00:00.123  1000  2000 🔥 Tag: Msg",
        // Emoji in Tag
        "11-06 18:00:00.123  1000  2000 E 🔥Tag🚀: Msg",
        // Emoji in Message
        "11-06 18:00:00.123  1000  2000 E Tag: 🔥 Critical Crash 💥",
        // Korean / CJK characters throughout
        "11-06 18:00:00.123  1000  2000 I 한글태그: 안녕하세요 세계",
        "가나다라마바사아자차카타파하 11-06 18:00:00.123",
        // ZWJ (Zero Width Joiner) emoji sequence (family / flags)
        "11-06 18:00:00.123  1000  2000 I Tag: 👨‍👩‍👧‍👦 complex emoji",
        // Inverted / null / control characters
        "\0\0\011-06 18:00:00.123  1000  2000 E Tag: Msg\0\0",
        "\t\t  11-06 18:00:00.123 \t 1000 \t 2000 \t E \t Tag: Msg \t\n",
    ];

    for case in test_cases {
        let parsed = parse_logcat_column_line(case);
        // Invariant: parsed.message must not be empty if case has content
        assert!(!parsed.message.is_empty());
    }
}

#[test]
fn challenge_1_fuzz_synthetic_mutations() {
    // Deterministic pseudo-random fuzzer to generate 10,000 adversarial strings
    let mut state = 0x12345678u64;
    let mut rng = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        state
    };

    let base = "11-06 18:00:00.123  1000  2000 E TagName: Test message 123";
    let chars = [' ', '\t', '-', ':', '.', '0', '9', 'E', 'x', '🔥', '가', '\0', '\n'];

    for _ in 0..10_000 {
        let mut s = String::with_capacity(60);
        let len = (rng() % 50) as usize;
        for _ in 0..len {
            let idx = (rng() as usize) % chars.len();
            s.push(chars[idx]);
        }

        // Test with raw synthetic string
        let _ = parse_logcat_column_line(&s);

        // Test with mutated base
        let mut mutated = base.to_string();
        let pos = (rng() as usize) % mutated.len();
        if mutated.is_char_boundary(pos) {
            mutated.insert(pos, chars[(rng() as usize) % chars.len()]);
            let _ = parse_logcat_column_line(&mutated);
        }
    }
}

// ============================================================================
// CHALLENGE 2: fuzzy_match_detailed Unicode Expansion & Index Bounds
// ============================================================================

#[test]
fn challenge_2_unicode_expansion_turkish_german_greek() {
    // 1. Turkish 'İ' (\u{0130})
    let target_tr = "İtemManager";
    let res_tr = fuzzy_match_detailed("item", target_tr);
    assert!(res_tr.is_some());
    let r = res_tr.unwrap();
    assert_eq!(r.matched_indices, vec![0, 1, 2, 3]);

    // Matching uppercase Turkish 'İ' against uppercase query
    let res_tr_upper = fuzzy_match_detailed("İTEM", target_tr);
    assert!(res_tr_upper.is_some());
    assert_eq!(res_tr_upper.unwrap().matched_indices, vec![0, 1, 2, 3]);

    // Multiple Turkish 'İ' in target
    let target_multi_tr = "İnternetİtemİnfo";
    let res_multi = fuzzy_match_detailed("iii", target_multi_tr);
    assert!(res_multi.is_some());
    let r_multi = res_multi.unwrap();
    assert_eq!(r_multi.matched_indices, vec![0, 8, 12]);

    // Turkish dotless 'ı' (\u{0131})
    let target_dotless = "sıkça sorulan";
    let res_dotless = fuzzy_match_detailed("sık", target_dotless);
    assert!(res_dotless.is_some());

    // 2. German 'ß' (\u{00DF}) and capital 'ẞ' (\u{1E9E})
    let target_de = "GROẞE STRAẞE";
    let res_de = fuzzy_match_detailed("große", target_de);
    assert!(res_de.is_some());
    let r_de = res_de.unwrap();
    for &idx in &r_de.matched_indices {
        assert!(idx < target_de.chars().count());
    }

    // 3. Greek Sigma ('Σ', 'σ', 'ς')
    let target_gr = "ΒΑΣΙΛΕΥΣ";
    let res_gr = fuzzy_match_detailed("βασιλευσ", target_gr);
    assert!(res_gr.is_some());

    // 4. Combining characters & emojis
    let target_emoji = "🔥 Fire Log 🔥 Alert";
    let res_emoji = fuzzy_match_detailed("🔥al", target_emoji);
    assert!(res_emoji.is_some());
    let r_emoji = res_emoji.unwrap();
    // matched_indices are char offsets
    assert_eq!(r_emoji.matched_indices[0], 0); // '🔥'
}

#[test]
fn challenge_2_matched_indices_bounds_and_monotonicity() {
    let targets = vec![
        "İtemListWithManyElementsAndLongNames",
        "STRAẞE_UND_FUẞGÄNGERZONE",
        "가나다라마바사_항목_데이터",
        "🚀🔥💥🎉 SPECIAL_SYMBOLS 12345",
        "a",
        "",
    ];

    let queries = vec![
        "item", "ilw", "str", "gän", "가나", "항목", "🚀", "123", "xyz", "",
    ];

    for target in &targets {
        let char_count = target.chars().count();
        for query in &queries {
            if let Some(res) = fuzzy_match_detailed(query, target) {
                // Assert strict bounds
                for &idx in &res.matched_indices {
                    assert!(
                        idx < char_count,
                        "matched index {} must be < char count {} for target '{}'",
                        idx,
                        char_count,
                        target
                    );
                }

                // Assert strict monotonic ordering
                for window in res.matched_indices.windows(2) {
                    assert!(
                        window[0] < window[1],
                        "matched indices must be strictly increasing: {:?}",
                        res.matched_indices
                    );
                }
            }
        }
    }
}

#[test]
fn challenge_2_long_repetitive_string_integer_overflow_safety() {
    // Test for integer overflow in consecutive bonus: score += 15 * consecutive
    // 10,000 matching characters
    let long_a = "a".repeat(10_000);
    let res = fuzzy_match_detailed(&long_a, &long_a);
    assert!(res.is_some());
    let r = res.unwrap();
    assert_eq!(r.matched_indices.len(), 10_000);

    // 25,000 matching characters (would overflow i32 if not saturating or if unchecked)
    let very_long = "a".repeat(25_000);
    let res_very_long = fuzzy_match_detailed(&very_long, &very_long);
    assert!(res_very_long.is_some());
}

// ============================================================================
// CHALLENGE 3: timestamp.rs & merged_buffer.rs Monotonicity & Ordering
// ============================================================================

#[test]
fn challenge_3_feb29_mar01_monotonicity_and_calendar_math() {
    let line_feb29 = b"02-29 23:59:59.999 1 1 I Tag: Leap day end";
    let line_mar01 = b"03-01 00:00:00.000 1 1 I Tag: March first start";

    let ts_feb29 = extract_timestamp(line_feb29).expect("Feb 29 should parse");
    let ts_mar01 = extract_timestamp(line_mar01).expect("Mar 01 should parse");

    // Must be strictly monotonic
    assert!(
        ts_feb29.total_micros < ts_mar01.total_micros,
        "Feb 29 23:59:59.999 ({}) must be < Mar 01 00:00:00.000 ({})",
        ts_feb29.total_micros,
        ts_mar01.total_micros
    );

    // Delta between 23:59:59.999 and 00:00:00.000 next day is exactly 1 millisecond = 1000 micros
    let delta = compute_delta_micros(ts_feb29, ts_mar01);
    assert_eq!(delta, 1_000, "Delta between 02-29 23:59:59.999 and 03-01 00:00:00.000 must be 1000 µs");

    // Verify all 366 days in leap year table are strictly monotonic
    let days_in_month = [0, 31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut prev_ts = -1i64;
    let mut total_days_tested = 0;

    for m in 1..=12 {
        for d in 1..=days_in_month[m] {
            let line = format!("{:02}-{:02} 00:00:00.000 1 1 I Tag: msg", m, d);
            let ts = extract_timestamp(line.as_bytes()).expect("Day must parse");
            assert!(
                ts.total_micros > prev_ts,
                "Day {:02}-{:02} timestamp {} must be > prev {}",
                m,
                d,
                ts.total_micros,
                prev_ts
            );
            prev_ts = ts.total_micros;
            total_days_tested += 1;
        }
    }
    assert_eq!(total_days_tested, 366, "Must test exactly 366 days in leap year");
}

#[test]
fn challenge_3_out_of_range_date_rejection() {
    let invalid_dates = vec![
        b"99-99 99:99:99.999 1 1 I Tag: Invalid".as_slice(),
        b"00-00 00:00:00.000 1 1 I Tag: Invalid".as_slice(),
        b"00-15 12:00:00.000 1 1 I Tag: Invalid".as_slice(),
        b"13-01 12:00:00.000 1 1 I Tag: Invalid".as_slice(),
        b"01-00 12:00:00.000 1 1 I Tag: Invalid".as_slice(),
        b"01-32 12:00:00.000 1 1 I Tag: Invalid".as_slice(),
        b"01-15 24:00:00.000 1 1 I Tag: Invalid".as_slice(),
        b"01-15 12:60:00.000 1 1 I Tag: Invalid".as_slice(),
        b"01-15 12:00:61.000 1 1 I Tag: Invalid".as_slice(),
    ];

    for inv in invalid_dates {
        assert!(
            extract_timestamp(inv).is_none(),
            "Invalid timestamp '{}' should return None",
            String::from_utf8_lossy(inv)
        );
    }
}

#[test]
fn challenge_3_reverse_order_multi_file_merge() {
    // Test [jan01, dec31] vs [dec31, jan01]
    let mut f_dec31 = NamedTempFile::new().expect("tempfile");
    let mut f_jan01 = NamedTempFile::new().expect("tempfile");

    writeln!(f_dec31, "12-31 23:59:50.000 1 1 I Tag: Dec 31 Line 1").unwrap();
    writeln!(f_dec31, "12-31 23:59:55.000 1 1 I Tag: Dec 31 Line 2").unwrap();
    writeln!(f_dec31, "12-31 23:59:59.000 1 1 I Tag: Dec 31 Line 3").unwrap();
    f_dec31.flush().unwrap();

    writeln!(f_jan01, "01-01 00:00:01.000 2 2 I Tag: Jan 01 Line 1").unwrap();
    writeln!(f_jan01, "01-01 00:00:05.000 2 2 I Tag: Jan 01 Line 2").unwrap();
    f_jan01.flush().unwrap();

    let path_dec = f_dec31.path().to_path_buf();
    let path_jan = f_jan01.path().to_path_buf();

    // 1. Natural order [dec31, jan01]
    let merged_natural = MergedLogBuffer::open_multiple(&[path_dec.clone(), path_jan.clone()])
        .expect("merge natural");
    assert_eq!(merged_natural.line_count(), 5);

    // 2. Reverse order [jan01, dec31]
    let merged_reverse = MergedLogBuffer::open_multiple(&[path_jan.clone(), path_dec.clone()])
        .expect("merge reverse");
    assert_eq!(merged_reverse.line_count(), 5);

    // In BOTH merges, Dec 31 lines MUST sort before Jan 01 lines!
    for merged in [&merged_natural, &merged_reverse] {
        // Line 0..3 must be from dec31 file
        for idx in 0..3 {
            let line = merged.get_line(idx).unwrap();
            assert!(line.contains("Dec 31"), "Line {} should be Dec 31, got: {}", idx, line);
        }
        // Line 3..5 must be from jan01 file
        for idx in 3..5 {
            let line = merged.get_line(idx).unwrap();
            assert!(line.contains("Jan 01"), "Line {} should be Jan 01, got: {}", idx, line);
        }

        // Timestamps must be strictly monotonic throughout
        let mut prev_ts = 0i64;
        for idx in 0..5 {
            let ts = merged.get_timestamp_micros(idx).expect("timestamp present");
            assert!(ts > prev_ts, "Row {} timestamp {} must be > prev {}", idx, ts, prev_ts);
            prev_ts = ts;
        }
    }

    // Verify 3-file merge permutation invariance: [nov, dec, jan] vs [jan, dec, nov]
    let mut f_nov15 = NamedTempFile::new().expect("tempfile");
    writeln!(f_nov15, "11-15 12:00:00.000 3 3 I Tag: Nov 15 Line 1").unwrap();
    f_nov15.flush().unwrap();
    let path_nov = f_nov15.path().to_path_buf();

    let merged_3_a = MergedLogBuffer::open_multiple(&[path_nov.clone(), path_dec.clone(), path_jan.clone()]).unwrap();
    let merged_3_b = MergedLogBuffer::open_multiple(&[path_jan.clone(), path_nov.clone(), path_dec.clone()]).unwrap();

    for merged in [&merged_3_a, &merged_3_b] {
        assert_eq!(merged.line_count(), 6);
        assert!(merged.get_line(0).unwrap().contains("Nov 15"));
        assert!(merged.get_line(1).unwrap().contains("Dec 31"));
        assert!(merged.get_line(4).unwrap().contains("Jan 01"));
    }
}

// ============================================================================
// CHALLENGE 4: ReDoS AST Checking & Bounded Repetitions
// ============================================================================

#[test]
fn challenge_4_bounded_nested_repetitions_rejection() {
    let bounded_nested_patterns = vec![
        r"(?=(a{1,500}){1,500})test",
        r"(?=(a{2,10}){2,10})test",
        r"(?=(a{1,5}){1,5})test",
        r"(?=(a{2,3}){2,3})test",
        r"(?=([0-9]{1,100}){1,100})test",
        r"(?=((foo){2,10}){2,10})test",
        r"(?=((a|b){2,10}){2,10})test",
    ];

    for pat in bounded_nested_patterns {
        let parsed = fancy_regex::Expr::parse_tree(pat);
        assert!(parsed.is_ok(), "Pattern '{}' must parse into tree", pat);
        let tree = parsed.unwrap();
        assert!(
            has_nested_repetition(&tree.expr, false),
            "Pattern '{}' must be detected as nested repetition by AST check",
            pat
        );

        let res = CompiledMatcher::new(pat, true, true);
        assert!(
            matches!(res, Err(FilterError::InvalidRegex(_))),
            "Pattern '{}' must be rejected at compile time by CompiledMatcher::new",
            pat
        );
    }
}

#[test]
fn challenge_4_safe_bounded_repetitions_allowed() {
    // Non-nested bounded repetitions must be allowed
    let safe_patterns = vec![
        r"(?=(a{1,100}))test",
        r"(?=(a{1}){1})test", // hi = 1 is not a repeat
        r"(?=(a{2}){1})test", // outer hi = 1 is not a repeat
        r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b",
        r"(a{2,5})b(c{2,5})",
    ];

    for pat in safe_patterns {
        let res = CompiledMatcher::new(pat, true, true);
        assert!(
            res.is_ok(),
            "Safe pattern '{}' should compile, got: {:?}",
            pat,
            res.err()
        );
    }
}

#[test]
fn challenge_4_complex_alternations_inside_repeats() {
    // 1. Nested repetition with alternation: ((a|b)+)+ -> Rejected by AST check
    let pat_nested_alt = r"(?=((a|b)+)+)test";
    let res_nested_alt = CompiledMatcher::new(pat_nested_alt, true, true);
    assert!(
        matches!(res_nested_alt, Err(FilterError::InvalidRegex(_))),
        "Nested repetition with alternation must be rejected"
    );

    // 2. Ambiguous alternation in repeat that passes AST check: (?=(a|a)+)b
    // Must be safely bounded by FANCY_REGEX_BACKTRACK_LIMIT (250 steps)
    let pat_ambiguous = r"(?=(a|a)+)b";
    let matcher = CompiledMatcher::new(pat_ambiguous, true, true).expect("should compile with backtrack limit");

    // Attack input: 10,000 'a's without 'b'
    let attack_str = "a".repeat(10_000);
    let start = Instant::now();
    let matched = matcher.matches(attack_str.as_bytes());
    let elapsed = start.elapsed();

    assert!(!matched);
    assert!(
        elapsed < std::time::Duration::from_millis(20),
        "Ambiguous alternation matching took too long: {:?}",
        elapsed
    );
}

#[test]
fn challenge_4_standard_regex_dfa_size_limits() {
    // RegexBuilder size limits: 2MB limit must prevent giant DFA tables or reject cleanly
    // A regex with huge repetition count in standard regex
    let pat_huge = format!(r"(a{{{}}})", 100_000);
    let _ = CompiledMatcher::new(&pat_huge, true, true);
    // Should either compile cleanly or return FilterError::InvalidRegex without crashing/OOM
}

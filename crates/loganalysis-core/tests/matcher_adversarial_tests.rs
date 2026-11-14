use loganalysis_core::filter::matcher::{has_nested_repetition, FANCY_REGEX_BACKTRACK_LIMIT};
use loganalysis_core::{CompiledMatcher, FilterError};
use std::time::Instant;

/// ---------------------------------------------------------------------------
/// Challenge 1: find_match_ranges with Complex Lookarounds & Backtrack Limits
/// ---------------------------------------------------------------------------

#[test]
fn test_find_match_ranges_complex_lookarounds() {
    // 1. Positive lookbehind: extract token after prefix
    let matcher_lb =
        CompiledMatcher::new(r"(?<=TAG_)[A-Z0-9_]+", true, true).expect("should compile");
    let text = "Line 1: TAG_ACTIVITY_MGR initialized; Line 2: TAG_AUDIO_SERVICE started; TAG_";
    let ranges = matcher_lb.find_match_ranges(text, 10);
    assert_eq!(ranges.len(), 2);
    assert_eq!(&text[ranges[0].clone()], "ACTIVITY_MGR");
    assert_eq!(&text[ranges[1].clone()], "AUDIO_SERVICE");

    // 2. Positive lookahead: match word preceding specific suffix
    let matcher_la = CompiledMatcher::new(r"\w+(?=\.log)", true, true).expect("should compile");
    let text_la = "Files: system.log, crash.log, app.txt, debug.log.bak";
    let ranges_la = matcher_la.find_match_ranges(text_la, 10);
    assert_eq!(ranges_la.len(), 3);
    assert_eq!(&text_la[ranges_la[0].clone()], "system");
    assert_eq!(&text_la[ranges_la[1].clone()], "crash");
    assert_eq!(&text_la[ranges_la[2].clone()], "debug");

    // 3. Negative lookahead: match ERROR only if not followed by (ignored)
    let matcher_nla =
        CompiledMatcher::new(r"ERROR(?! \(ignored\))", true, true).expect("should compile");
    let text_nla = "ERROR (ignored) happened; then ERROR (fatal) occurred; ERROR without reason";
    let ranges_nla = matcher_nla.find_match_ranges(text_nla, 10);
    assert_eq!(ranges_nla.len(), 2);
    assert_eq!(&text_nla[ranges_nla[0].clone()], "ERROR");
    assert_eq!(&text_nla[ranges_nla[1].clone()], "ERROR");
    // Ensure the first "ERROR (ignored)" was skipped
    assert!(ranges_nla[0].start > 15);

    // 4. Combined Lookbehind + Lookahead (bracket/delimiter content extraction)
    let matcher_both =
        CompiledMatcher::new(r"(?<=\[)[^\]]+(?=\])", true, true).expect("should compile");
    let text_both = "04-12 10:15:30.123 [main] INFO [WorkerThread-4] Task [12345] complete []";
    let ranges_both = matcher_both.find_match_ranges(text_both, 10);
    assert_eq!(ranges_both.len(), 3);
    assert_eq!(&text_both[ranges_both[0].clone()], "main");
    assert_eq!(&text_both[ranges_both[1].clone()], "WorkerThread-4");
    assert_eq!(&text_both[ranges_both[2].clone()], "12345");
}

#[test]
fn test_find_match_ranges_cjk_and_multibyte_utf8() {
    // Multi-byte UTF-8 character boundary safety in lookaround matching
    let pat_cjk = r"(?<=에러코드: )[가-힣A-Z0-9_]+";
    println!("Testing fancy_regex with pat_cjk: {}", pat_cjk);
    let fancy_res = fancy_regex::Regex::new(pat_cjk);
    println!("fancy_regex direct compile result: {:?}", fancy_res);

    let matcher_cjk = CompiledMatcher::new(pat_cjk, true, true).expect("compile");
    let text_cjk = "로그정보: [중요] 에러코드: 치명적오류_500 발생. 재시도 필요.";
    let ranges = matcher_cjk.find_match_ranges(text_cjk, 10);
    assert_eq!(ranges.len(), 1);
    assert_eq!(&text_cjk[ranges[0].clone()], "치명적오류_500");
    // Ensure byte slices land exactly on char boundaries
    assert!(text_cjk.is_char_boundary(ranges[0].start));
    assert!(text_cjk.is_char_boundary(ranges[0].end));

    // Japanese & Emoji lookaround (fixed-width lookbehind + variable lookahead)
    let matcher_emoji =
        CompiledMatcher::new(r"(?<=🚨 )[^\s]+(?=\s*🚨)", true, true).expect("compile");
    let text_emoji = "システム警告: 🚨 システム異常停止 🚨 再起動中...";
    let ranges_emoji = matcher_emoji.find_match_ranges(text_emoji, 10);
    assert_eq!(ranges_emoji.len(), 1);
    assert_eq!(&text_emoji[ranges_emoji[0].clone()], "システム異常停止");
}

#[test]
fn test_find_match_ranges_zero_width_matches_do_not_infinite_loop() {
    // Zero-width lookahead assertions must not loop infinitely or return 0-width ranges
    let zero_patterns = vec![r"(?=ERROR)", r"^", r"$", r"\b", r"(?<=)"];
    let sample = "ERROR: test line for zero width verification";

    for pat in zero_patterns {
        if let Ok(matcher) = CompiledMatcher::new(pat, true, true) {
            let start = Instant::now();
            let ranges = matcher.find_match_ranges(sample, 50);
            let elapsed = start.elapsed();

            assert!(
                elapsed < std::time::Duration::from_millis(50),
                "Pattern '{}' took too long ({:?}) on zero-width match",
                pat,
                elapsed
            );
            // All returned ranges MUST have start < end (strictly non-empty)
            for r in ranges {
                assert!(
                    r.start < r.end,
                    "Pattern '{}' returned empty range {:?}",
                    pat,
                    r
                );
            }
        }
    }
}

#[test]
fn test_find_match_ranges_backtrack_limit_exhaustion_safety() {
    assert_eq!(FANCY_REGEX_BACKTRACK_LIMIT, 250);

    // Patterns with semantic backtracking that bypass AST check
    let non_matching_100k = "abababababab".repeat(5000); // 60,000 characters
    let pat = r"(?i)(a|b|ab)*(?=c)";

    let matcher = CompiledMatcher::new(pat, true, true).expect("compile");

    // 1. matches() must abort cleanly without thread hang
    let start = Instant::now();
    let matched = matcher.matches(non_matching_100k.as_bytes());
    let elapsed = start.elapsed();
    assert!(!matched);
    assert!(
        elapsed < std::time::Duration::from_millis(15),
        "matches() took too long: {:?}",
        elapsed
    );

    // 2. find_match_ranges() must break on BacktrackLimitExceeded and return within bounded time
    let start = Instant::now();
    let ranges = matcher.find_match_ranges(&non_matching_100k, 10);
    let elapsed = start.elapsed();
    assert!(ranges.is_empty());
    assert!(
        elapsed < std::time::Duration::from_millis(25),
        "find_match_ranges() took too long: {:?}",
        elapsed
    );

    // 3. Partial match before backtrack limit exhaustion
    let text_with_prefix = format!("VALID_TARGET_ABC {}", non_matching_100k);
    let pat_prefix = r"VALID_TARGET_[A-Z]+|(?i)(a|b|ab)*(?=c)";
    let matcher_prefix = CompiledMatcher::new(pat_prefix, true, true).expect("compile");

    let start = Instant::now();
    let ranges_prefix = matcher_prefix.find_match_ranges(&text_with_prefix, 10);
    let elapsed_prefix = start.elapsed();

    assert!(
        elapsed_prefix < std::time::Duration::from_millis(25),
        "Partial match iteration took too long: {:?}",
        elapsed_prefix
    );
    // Should safely yield the first valid match before breaking on the backtrack limit
    assert_eq!(ranges_prefix.len(), 1);
    assert_eq!(
        &text_with_prefix[ranges_prefix[0].clone()],
        "VALID_TARGET_ABC"
    );
}

#[test]
fn test_find_match_ranges_max_matches_and_empty_inputs() {
    let matcher = CompiledMatcher::new(r"(?<=#)\d+", true, true).expect("compile");
    let text = "#1 #2 #3 #4 #5 #6 #7 #8 #9 #10";

    // max_matches = 0 -> empty
    assert!(matcher.find_match_ranges(text, 0).is_empty());

    // max_matches = 3 -> exactly 3
    let r3 = matcher.find_match_ranges(text, 3);
    assert_eq!(r3.len(), 3);
    assert_eq!(&text[r3[0].clone()], "1");
    assert_eq!(&text[r3[1].clone()], "2");
    assert_eq!(&text[r3[2].clone()], "3");

    // empty text -> empty
    assert!(matcher.find_match_ranges("", 10).is_empty());
}

/// ---------------------------------------------------------------------------
/// Challenge 2: Lookaround Parity Across Thousands of Real-World Log Strings
/// ---------------------------------------------------------------------------

#[test]
fn test_legitimate_lookarounds_real_world_log_corpus() {
    let num_lines = 10_000;
    let mut log_corpus = Vec::with_capacity(num_lines);

    // Generate 10,000 real-world Android logcat lines
    for i in 0..num_lines {
        let line = match i % 10 {
            0 => format!(
                "04-12 10:15:{:02}.{:03}  1000  1005 I ActivityManager: Starting activity: Intent {{ act=android.intent.action.MAIN cat=[android.intent.category.LAUNCHER] cmp=com.example.app/.MainActivity }} pid={}",
                i % 60,
                i % 1000,
                2000 + i
            ),
            1 => format!(
                "04-12 10:15:{:02}.{:03}  1234  1240 W AudioFlinger: RecordThread: input buffer underrun frames=1024 latency_ms=45.2",
                i % 60,
                i % 1000
            ),
            2 => format!(
                "04-12 10:15:{:02}.{:03}  2000  2010 E AndroidRuntime: FATAL EXCEPTION: main Process: com.example.app, PID: {}",
                i % 60,
                i % 1000,
                2000 + i
            ),
            3 => format!(
                "04-12 10:15:{:02}.{:03}  2000  2010 E AndroidRuntime: java.lang.NullPointerException: Attempt to invoke virtual method 'void java.lang.String.trim()' on a null object reference",
                i % 60,
                i % 1000
            ),
            4 => format!(
                "04-12 10:15:{:02}.{:03}   500   505 D BluetoothAdapter: STATE_OFF -> STATE_TURNING_ON mac=AA:BB:CC:DD:EE:{:02X}",
                i % 60,
                i % 1000,
                i % 256
            ),
            5 => format!(
                "04-12 10:15:{:02}.{:03}  1500  1510 I SystemServer: [BOOT_TIMING] InitPhase_500 took 142ms user_token=TOK_{:06}",
                i % 60,
                i % 1000,
                i
            ),
            6 => format!(
                "04-12 10:15:{:02}.{:03}  3000  3005 V WindowManager: prepareSurfaces: surface is null for Session{{{}}}",
                i % 60,
                i % 1000,
                i
            ),
            7 => format!(
                "04-12 10:15:{:02}.{:03}  2000  2010 I AppAnalytics: Event sent: {{\"event\":\"login\",\"status\":\"SUCCESS\",\"uid\":{}}}",
                i % 60,
                i % 1000,
                10000 + i
            ),
            8 => format!(
                "04-12 10:15:{:02}.{:03}  2000  2010 D KoreanLog: [사용자세션] 유저 로그인 성공: 계정아이디=USER_{:04} 정상처리",
                i % 60,
                i % 1000,
                i
            ),
            _ => format!(
                "04-12 10:15:{:02}.{:03}   400   405 D PowerManagerService: [DEBUG] acquireWakeLock: flags=0x1 tag=\"LocationManagerService\" uid=1000 pid={}",
                i % 60,
                i % 1000,
                2000 + i
            ),
        };
        log_corpus.push(line);
    }

    // Pattern 1: Positive Lookbehind for ActivityManager intent
    // Matches i % 10 == 0 lines
    let matcher_intent = CompiledMatcher::new(
        r"(?<=ActivityManager: Starting activity: Intent \{ ).*(?= \})",
        true,
        true,
    )
    .expect("compile");

    let mut match_count_1 = 0;
    let start_1 = Instant::now();
    for (idx, line) in log_corpus.iter().enumerate() {
        let matched = matcher_intent.matches(line.as_bytes());
        if idx % 10 == 0 {
            assert!(matched, "Line {} should have matched", idx);
            match_count_1 += 1;
            let ranges = matcher_intent.find_match_ranges(line, 1);
            assert_eq!(ranges.len(), 1);
            let extracted = &line[ranges[0].clone()];
            assert!(extracted.starts_with("act=android.intent.action.MAIN"));
        } else {
            assert!(!matched, "Line {} should NOT have matched", idx);
        }
    }
    let elapsed_1 = start_1.elapsed();
    assert_eq!(match_count_1, 1000);
    println!(
        "Lookbehind 10,000 lines evaluated in {:?} ({:.2} µs/line)",
        elapsed_1,
        elapsed_1.as_micros() as f64 / num_lines as f64
    );

    // Pattern 2: Conjunction of Lookaheads (Fatal + Exception)
    // Matches i % 10 == 2 lines
    let matcher_fatal =
        CompiledMatcher::new(r"(?=.*FATAL)(?=.*EXCEPTION).*PID: \d+", true, true).expect("compile");

    let mut match_count_2 = 0;
    let start_2 = Instant::now();
    for (idx, line) in log_corpus.iter().enumerate() {
        let matched = matcher_fatal.matches(line.as_bytes());
        if idx % 10 == 2 {
            assert!(matched, "Line {} should match FATAL EXCEPTION", idx);
            match_count_2 += 1;
        } else {
            assert!(!matched, "Line {} should NOT match FATAL EXCEPTION", idx);
        }
    }
    let elapsed_2 = start_2.elapsed();
    assert_eq!(match_count_2, 1000);
    println!(
        "Conjunction lookahead 10,000 lines evaluated in {:?}",
        elapsed_2
    );

    // Pattern 3: Lookbehind for Korean log tags
    // Matches i % 10 == 8 lines
    let matcher_korean = CompiledMatcher::new(
        r"(?<=\[사용자세션\] 유저 로그인 성공: 계정아이디=)\w+",
        true,
        true,
    )
    .expect("compile");

    let mut match_count_3 = 0;
    for (idx, line) in log_corpus.iter().enumerate() {
        let matched = matcher_korean.matches(line.as_bytes());
        if idx % 10 == 8 {
            assert!(matched);
            match_count_3 += 1;
            let ranges = matcher_korean.find_match_ranges(line, 1);
            assert_eq!(ranges.len(), 1);
            assert!(line[ranges[0].clone()].starts_with("USER_"));
        } else {
            assert!(!matched);
        }
    }
    assert_eq!(match_count_3, 1000);
}

/// ---------------------------------------------------------------------------
/// Challenge 3: Performance Parity (Linear Regexes and Literal Matchers)
/// ---------------------------------------------------------------------------

#[test]
fn test_performance_parity_literal_and_linear_regexes() {
    let iterations = 50_000;
    let line = b"04-12 10:15:30.123  2000  2010 E AndroidRuntime: java.lang.NullPointerException: Attempt to invoke virtual method 'void java.lang.String.trim()' on a null object reference";

    // 1. Literal Case-Sensitive Matcher vs Raw memchr::memmem::Finder
    let literal_needle = "NullPointerException";
    let raw_finder = memchr::memmem::Finder::new(literal_needle.as_bytes());
    let compiled_literal = CompiledMatcher::new(literal_needle, true, false).expect("compile");

    // Warm-up
    for _ in 0..1000 {
        assert!(compiled_literal.matches(line));
        assert!(raw_finder.find(line).is_some());
    }

    let start_raw = Instant::now();
    for _ in 0..iterations {
        let _ = raw_finder.find(line).is_some();
    }
    let elapsed_raw_literal = start_raw.elapsed();

    let start_compiled = Instant::now();
    for _ in 0..iterations {
        let _ = compiled_literal.matches(line);
    }
    let elapsed_compiled_literal = start_compiled.elapsed();

    println!(
        "Literal 50,000 matches: Raw memmem = {:?}, CompiledMatcher = {:?}",
        elapsed_raw_literal, elapsed_compiled_literal
    );
    // CompiledMatcher delegates directly to Finder::find, overhead should be minimal (< 35% overhead in debug)
    assert!(
        elapsed_compiled_literal
            <= elapsed_raw_literal.mul_f64(1.5) + std::time::Duration::from_millis(20),
        "CompiledMatcher literal shows degradation: compiled={:?}, raw={:?}",
        elapsed_compiled_literal,
        elapsed_raw_literal
    );

    // 2. Standard Linear Regex vs Raw regex::bytes::Regex
    let regex_pattern = r"NullPointer[A-Za-z]+";
    let raw_regex = regex::bytes::Regex::new(regex_pattern).expect("raw regex compile");
    let compiled_regex = CompiledMatcher::new(regex_pattern, true, true).expect("compile");

    // Verify it is indeed StandardRegex variant, not FancyRegex
    match &compiled_regex {
        CompiledMatcher::StandardRegex(_) => {}
        _ => panic!("Expected StandardRegex variant for linear regex"),
    }

    let start_raw_re = Instant::now();
    for _ in 0..iterations {
        let _ = raw_regex.is_match(line);
    }
    let elapsed_raw_re = start_raw_re.elapsed();

    let start_compiled_re = Instant::now();
    for _ in 0..iterations {
        let _ = compiled_regex.matches(line);
    }
    let elapsed_compiled_re = start_compiled_re.elapsed();

    println!(
        "Linear Regex 50,000 matches: Raw Regex = {:?}, CompiledMatcher = {:?}",
        elapsed_raw_re, elapsed_compiled_re
    );
    // Verify zero performance degradation: CompiledMatcher delegates directly to regex::bytes::Regex
    // and should not degrade performance relative to raw regex (within 25% or 10ms margin)
    assert!(
        elapsed_compiled_re <= elapsed_raw_re.mul_f64(1.25) + std::time::Duration::from_millis(10),
        "CompiledMatcher regex shows performance degradation: compiled={:?}, raw={:?}",
        elapsed_compiled_re,
        elapsed_raw_re
    );
}

/// ---------------------------------------------------------------------------
/// Challenge 4: Static AST ReDoS Analyzer Verification
/// ---------------------------------------------------------------------------

#[test]
fn test_ast_redos_boundary_and_rejection_parity() {
    let severe_redos_patterns = vec![
        r"(a+)+$",
        r"((a+)*)+",
        r"([0-9]+)+",
        r"(?=(a+)+)test",
        r"(a*|b*)*",
        r"(?>a+)+",
        r"((a+))+",
        r"(a+b+)+",
        r"(a{2,})+",
        r"((foo)+)+",
    ];

    for pat in &severe_redos_patterns {
        let parsed = fancy_regex::Expr::parse_tree(pat);
        assert!(
            parsed.is_ok(),
            "Pattern '{}' should parse in fancy_regex",
            pat
        );
        let tree = parsed.unwrap();
        assert!(
            has_nested_repetition(&tree.expr, false),
            "Pattern '{}' must be detected by has_nested_repetition",
            pat
        );
    }

    // Lookaround and advanced ReDoS patterns requiring fancy_regex fallback must be rejected at compile time
    let lookaround_redos = vec![
        r"(?=(a+)+)test",
        r"(?=((a+)*)+)test",
        r"(?=([0-9]+)+)test",
        r"(?=(a+b+)+)test",
        r"(?=(a{2,})+)test",
        r"(?>a+)+",
    ];
    for pat in lookaround_redos {
        let res = CompiledMatcher::new(pat, true, true);
        assert!(
            matches!(res, Err(FilterError::InvalidRegex(_))),
            "Lookaround ReDoS pattern '{}' must be rejected at compile time",
            pat
        );
    }

    // Standard regexes without lookarounds are linear-time DFAs immune to ReDoS,
    // so CompiledMatcher::new safely accepts them as StandardRegex.
    let standard_redos = vec![
        r"(a+)+$",
        r"((a+)*)+",
        r"([0-9]+)+",
        r"(a*|b*)*",
        r"((a+))+",
        r"(a+b+)+",
        r"(a{2,})+",
        r"((foo)+)+",
    ];
    for pat in standard_redos {
        let res = CompiledMatcher::new(pat, true, true);
        assert!(
            matches!(res, Ok(CompiledMatcher::StandardRegex(_))),
            "Pattern '{}' should compile as StandardRegex DFA",
            pat
        );
    }

    // Standard non-nested repetitions must NOT be rejected
    let safe_patterns = vec![
        r"[0-9]+",
        r"(abc)*",
        r"\d{4}-\d{2}-\d{2}",
        r"[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,4}",
        r"TAG_[A-Z0-9]+",
        r"(?=.*error)fatal",
        r"(?<=PREFIX_)\w+",
        r"^(?!DEBUG).*status ok",
    ];

    for pat in safe_patterns {
        let res = CompiledMatcher::new(pat, true, true);
        assert!(
            res.is_ok(),
            "Safe pattern '{}' should compile successfully, got: {:?}",
            pat,
            res.err()
        );
    }
}

#[test]
fn test_defect_false_positive_redos_rejection_on_safe_patterns() {
    // Verifies remediation: safe, standard patterns now compile successfully:
    // 1. IP address pattern with bounded repetition {3} and {1,3}
    let ip_pattern = r"\b(?:\d{1,3}\.){3}\d{1,3}\b";
    let res_ip = CompiledMatcher::new(ip_pattern, true, true);
    assert!(
        res_ip.is_ok(),
        "IPv4 regex must compile successfully, got: {:?}",
        res_ip.err()
    );

    // 2. Repeated date pattern with bounded inner repeat {4}, {2}
    let date_pattern = r"(?:\d{4}-\d{2}-\d{2})+";
    let res_date = CompiledMatcher::new(date_pattern, true, true);
    assert!(
        res_date.is_ok(),
        "Date regex must compile successfully, got: {:?}",
        res_date.err()
    );

    // 3. Repeated domain pattern with literal separator
    let domain_pattern = r"(?:https?://)?(?:[a-zA-Z0-9-]+\.)+[a-zA-Z]{2,}";
    let res_domain = CompiledMatcher::new(domain_pattern, true, true);
    assert!(
        res_domain.is_ok(),
        "Domain regex must compile successfully, got: {:?}",
        res_domain.err()
    );
}

#[test]
fn test_standard_regex_immune_to_redos() {
    let re = regex::bytes::Regex::new(r"(a+)+$").expect("standard regex compiles (a+)+$");
    let non_matching_100k = "a".repeat(100_000).into_bytes();
    let start = Instant::now();
    let matched = re.is_match(&non_matching_100k);
    let elapsed = start.elapsed();
    assert!(matched);
    println!("Standard regex on 100,000 chars took: {:?}", elapsed);
    assert!(
        elapsed < std::time::Duration::from_millis(10),
        "Standard regex DFA took too long: {:?}",
        elapsed
    );
}

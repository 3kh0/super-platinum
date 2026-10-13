use super::*;

#[test]
fn non_unicode_surrogate_classes_keep_code_unit_ranges() {
    let regex = Regex::from_unicode(
        non_unicode_pattern(r"[\ud83c\udffb-\udfff]").into_iter(),
        "i",
    )
    .unwrap();
    for code in [0xd83c, 0xdffb, 0xdffc, 0xdfff] {
        assert!(regex.find_from_ucs2(&[code], 0).next().is_some());
    }
    for code in [0xd83d, 0xdffa, 0xe000] {
        assert!(regex.find_from_ucs2(&[code], 0).next().is_none());
    }
    let escaped = Regex::from_unicode(non_unicode_pattern(r"\\ud83c").into_iter(), "").unwrap();
    assert!(escaped.find_from_ucs2(&text(r"\ud83c"), 0).next().is_some());
    assert!(escaped.find_from_ucs2(&[0xd83c], 0).next().is_none());
    let pair = Regex::from_unicode(non_unicode_pattern(r"\ud83c\udffb").into_iter(), "").unwrap();
    assert_eq!(
        pair.find_from_ucs2(&[0xd83c, 0xdffb], 0)
            .next()
            .unwrap()
            .range(),
        0..2
    );
}

#[test]
fn captured_link_normalization_oracle() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let expected = case["normalized"]
            .as_str()
            .or_else(|| case["normalized"]["originalUrl"].as_str())
            .unwrap();
        assert_eq!(normalize(input).unwrap(), expected, "input {input:?}");
    }
}

#[test]
fn javascript_trim_preserves_non_js_whitespace_and_unmatched_text() {
    assert_eq!(
        normalize("\u{85}unmatched\u{85}").unwrap(),
        "\u{85}unmatched\u{85}"
    );
    assert_eq!(
        normalize("\u{feff}example.com\u{feff}").unwrap(),
        "http://example.com"
    );
    assert_eq!(normalize("  ").unwrap(), "  ");
}

#[test]
fn impossible_url_scan_gate_preserves_email_phone_and_source_branches() {
    let p = Patterns::load().unwrap();
    for input in [
        "a.".repeat(5000),
        "unmatched".into(),
        "a@example.engineering".into(),
        "202-555-0100".into(),
    ] {
        assert!(
            urls::earliest_possible_start(&p, &text(&input)).is_none(),
            "{input:?}"
        );
    }
    assert_eq!(
        normalize("a@example.engineering").unwrap(),
        "mailto:a@example.engineering"
    );
    assert_eq!(normalize("202-555-0100").unwrap(), "tel:2025550100");
    assert_eq!(normalize(&"a.".repeat(5000)).unwrap(), "a.".repeat(5000));
    for input in [
        "prefix:invalid",
        "WWW.example.invalid",
        "example.INFO",
        "ms.NOW",
        "a.COMnot-a-tld",
    ] {
        assert!(
            urls::earliest_possible_start(&p, &text(input)).is_some(),
            "{input:?}"
        );
    }
    // Every captured default TLD remains eligible, regardless of case, without
    // confusing the broader email-only TLD list with the URL matching set.
    for tld in &p.common {
        assert!(
            urls::earliest_possible_start(&p, &text(&format!("x.{}", tld.to_ascii_uppercase())))
                .is_some()
        );
    }
}

#[test]
fn earliest_url_only_keeps_cross_family_precedence_and_ignores_later_tail() {
    for (input, expected) in [
        (
            "https://example.com then a@example.org",
            "https://example.com",
        ),
        (
            "a@example.org then https://example.com",
            "mailto:a@example.org",
        ),
        ("202-555-0100 then https://example.com", "tel:2025550100"),
        (
            "https://a@example.com then 202-555-0100",
            "https://a@example.com",
        ),
    ] {
        assert_eq!(
            normalize(&format!("{input} {}", "a.".repeat(5000))).unwrap(),
            expected
        );
    }
}

#[test]
#[ignore = "root-run paired cold/warm and pathological input timing"]
fn link_detector_timing() {
    let cold = std::time::Instant::now();
    assert_eq!(normalize("example.com").unwrap(), "http://example.com");
    eprintln!("link-detector cold {:?}", cold.elapsed());
    for (name, input) in [
        ("ascii-word-10k", "a".repeat(10_000)),
        (
            "parentheses-10k",
            format!(
                "{}https://example.com{}",
                "(".repeat(5000),
                ")".repeat(5000)
            ),
        ),
        ("numeric-separators-10k", "202-".repeat(2500)),
        ("dot-near-match-10k", "a.".repeat(5000)),
        ("unicode-near-match-10k", "道😀.".repeat(2000)),
        ("dots-colon-2k", format!("{}:", "a.".repeat(1000))),
        ("dots-colon-10k", format!("{}:", "a.".repeat(5000))),
        ("colon-dots-10k", format!(":{}", "a.".repeat(5000))),
        ("dots-tld-10k", format!("{}com", "a.".repeat(5000))),
        (
            "unrelated-tld-dots-10k",
            format!("example.com {}", "a.".repeat(5000)),
        ),
        (
            "unrelated-www-dots-10k",
            format!("www.invalid {}", "a.".repeat(5000)),
        ),
    ] {
        let start = std::time::Instant::now();
        let result = normalize(&input).unwrap();
        eprintln!(
            "link-detector {name} bytes={} outputBytes={} elapsed={:?}",
            input.len(),
            result.len(),
            start.elapsed()
        );
    }
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        assert_eq!(
            normalize("https://example.com/spec").unwrap(),
            "https://example.com/spec"
        );
    }
    eprintln!("link-detector warm1000 {:?}", start.elapsed());
}

#[test]
fn start_bound_preserves_full_regex_captures_at_source_limits_and_false_markers() {
    let p = Patterns::load().unwrap();
    let oracle: serde_json::Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let mut cases = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["input"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    for count in [1, 63, 64, 65, 126, 127, 128, 160] {
        for prefix in ["", "😀道 ", "invalid: ", "www. ", "a.COMnot-a-tld "] {
            cases.extend([
                format!("{prefix}{}com", "a.".repeat(count)),
                format!("{prefix}//{}com", "a.".repeat(count)),
                format!("{prefix}{}.com", "a".repeat(count)),
                format!("{prefix}{}:example.com", "a".repeat(count)),
                format!("{prefix}{}:host", "a".repeat(count)),
            ]);
        }
    }
    cases.extend([
        format!("{}:", "a.".repeat(160)),
        format!(":{}", "a.".repeat(160)),
        format!("{}com", "a.".repeat(5000)),
        format!("//{}.com", vec!["a".repeat(63); 127].join(".")),
        "www.invalid then https://example.com".into(),
        "._c-o-m then https://example.com".into(),
        "😀http://example.com".into(),
    ]);
    for input in cases {
        let units = text(&input);
        let expected = p.find("url", &units, 0).map(|m| (m.range, m.captures));
        let actual = urls::earliest_possible_start(&p, &units)
            .and_then(|start| p.find("url", &units, start))
            .map(|m| (m.range, m.captures));
        assert_eq!(actual, expected, "input {input:?}");
    }
}

#[test]
fn start_bound_skips_impossible_colons_and_distant_labels_without_truncating() {
    let p = Patterns::load().unwrap();
    for input in [
        format!("{}:", "a.".repeat(5000)),
        format!(":{}", "a.".repeat(5000)),
    ] {
        assert_eq!(urls::earliest_possible_start(&p, &text(&input)), None);
        assert_eq!(normalize(&input).unwrap(), input);
    }
    let input = format!("{}com", "a.".repeat(5000));
    assert_eq!(urls::earliest_possible_start(&p, &text(&input)), Some(9744));
    assert_eq!(
        normalize(&input).unwrap(),
        format!("http://{}com", "a.".repeat(127))
    );
    // UTF-16 rather than UTF-8 offsets, retaining optional leading //.
    assert_eq!(
        urls::earliest_possible_start(&p, &text("😀//www.zzzz")),
        Some(2)
    );
}

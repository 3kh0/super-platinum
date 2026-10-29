use super::*;

#[test]
fn saved_and_pinned_flags_own_only_exact_positive_tokens_and_preserve_other_syntax() {
    let raw = r#"report is:saved has:pin is:saved has:pin -is:saved -has:pin "is:saved" has:"pin" is:thread has:file is:custom has:custom"#;
    let mut filters = SearchFilters::from_query(raw);
    assert!(filters.saved && filters.pinned);
    assert_eq!(filters.apply(raw).unwrap(), raw);
    let simple = "topic is:saved has:pin";
    let mut only_pinned = SearchFilters::from_query(simple);
    only_pinned.saved = false;
    assert_eq!(only_pinned.apply(simple).unwrap(), "topic  has:pin");
    let mut only_saved = SearchFilters::from_query(simple);
    only_saved.pinned = false;
    assert_eq!(only_saved.apply(simple).unwrap(), "topic is:saved ");
    filters.saved = false;
    filters.pinned = false;
    assert_eq!(
        filters.apply(raw).unwrap(),
        r#"report     -is:saved -has:pin "is:saved" has:"pin" is:thread has:file is:custom has:custom"#
    );
    for raw in [
        r#"-is:saved -has:pin "has:pin" is:"saved" IS:saved HAS:pin is:saved-later has:pinned"#,
        r#""escaped \"is:saved\" and has:pin""#,
    ] {
        let filters = SearchFilters::from_query(raw);
        assert!(!filters.saved && !filters.pinned);
        assert_eq!(filters.apply(raw).unwrap(), raw);
    }
    let filters = SearchFilters {
        saved: true,
        pinned: true,
        ..Default::default()
    };
    assert_eq!(filters.apply("").unwrap(), "is:saved has:pin");
}

#[test]
fn each_saved_or_pinned_addition_checks_original_quote_before_clearing_and_clear_only_still_works()
{
    for (saved, pinned) in [(true, false), (false, true)] {
        for raw in [r#"report in:"unfinished"#, r#"report with:"unfinished"#] {
            let mut filters = SearchFilters::from_query(raw);
            filters.channel.clear();
            filters.with_person.clear();
            filters.saved = saved;
            filters.pinned = pinned;
            assert_eq!(
                filters.apply(raw),
                Err("Close the quoted phrase before adding a filter.")
            );
        }
    }
    let raw = r#"report is:saved has:pin with:"unfinished"#;
    let mut clear = SearchFilters::from_query(raw);
    clear.with_person.clear();
    clear.saved = false;
    clear.pinned = false;
    assert_eq!(clear.apply(raw).unwrap(), "report  ");
}

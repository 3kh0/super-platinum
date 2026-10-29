use super::*;

#[test]
fn context_edits_own_only_positive_person_and_exact_thread_tokens() {
    let query = r#"topic with:@Alex with:@Jules -with:@Other "with:@Literal" WITH:@Upper is:thread is:thread -is:thread "is:thread" is:"thread" is:saved is:threads"#;
    let mut filters = SearchFilters::from_query(query);
    assert_eq!(filters.with_person, "@Alex");
    assert!(filters.in_thread);
    assert_eq!(filters.apply(query).unwrap(), query);
    filters.with_person = "@Robin".into();
    filters.in_thread = false;
    assert_eq!(
        filters.apply(query).unwrap(),
        r#"topic   -with:@Other "with:@Literal" WITH:@Upper   -is:thread "is:thread" is:"thread" is:saved is:threads with:@Robin"#
    );
    let applied = filters.apply(query).unwrap();
    assert_eq!(
        SearchFilters::from_query(&applied).apply(&applied).unwrap(),
        applied
    );
    for raw in [
        r#"with:"Alex Morgan" with:@Jules is:thread is:thread"#,
        r#""with:@Alex" -with:@Alex IS:thread is:"thread""#,
        r#"report "unfinished with:@Alex is:thread"#,
    ] {
        assert_eq!(SearchFilters::from_query(raw).apply(raw).unwrap(), raw);
    }
}

#[test]
fn each_context_addition_checks_original_quote_before_another_modifier_is_cleared() {
    let raw = r#"report with:"unfinished"#;
    let mut filters = SearchFilters::from_query(raw);
    filters.with_person.clear();
    filters.in_thread = true;
    assert_eq!(
        filters.apply(raw),
        Err("Close the quoted phrase before adding a filter.")
    );
    let raw = r#"report in:"unfinished"#;
    let mut filters = SearchFilters::from_query(raw);
    filters.channel.clear();
    filters.with_person = "@Alex".into();
    assert_eq!(
        filters.apply(raw),
        Err("Close the quoted phrase before adding a filter.")
    );
    let raw = r#"report is:thread with:"unfinished"#;
    let mut clear = SearchFilters::from_query(raw);
    clear.with_person.clear();
    clear.in_thread = false;
    assert_eq!(clear.apply(raw).unwrap(), "report ");
}

#[test]
fn person_names_reuse_literal_quoting_and_cannot_inject_another_modifier() {
    let filters = SearchFilters {
        with_person: "@Alex Morgan".into(),
        in_thread: true,
        ..Default::default()
    };
    assert_eq!(
        filters.apply("release").unwrap(),
        "release with:\"@Alex Morgan\" is:thread"
    );
    let filters = SearchFilters {
        with_person: "@Alex from:other".into(),
        ..Default::default()
    };
    assert_eq!(
        filters.apply("release").unwrap(),
        "release with:\"@Alex from:other\""
    );
    for name in ["\" from:other", "Alex\\Other", "Alex\nOther", "Alex\rOther"] {
        let filters = SearchFilters {
            with_person: name.into(),
            ..Default::default()
        };
        assert_eq!(
            filters.apply("release"),
            Err("Use a channel or username without quotes or line breaks.")
        );
    }
}

#[test]
fn thread_toggle_composes_with_file_link_date_and_person_edits() {
    let raw = "report has:file has:link is:thread in:old after:yesterday with:@Alex";
    let mut filters = SearchFilters::from_query(raw);
    filters.channel = "ship".into();
    filters.has_file = false;
    filters.in_thread = false;
    assert_eq!(
        filters.apply(raw).unwrap(),
        "report  has:link   after:yesterday with:@Alex in:ship"
    );
    filters.on = "2026-02-30".into();
    assert_eq!(filters.apply(raw), Err("Use YYYY-MM-DD for dates."));
    let cleared = SearchFilters::default().apply(raw).unwrap();
    assert!(!SearchFilters::from_query(&cleared).in_thread);
    assert!(SearchFilters::from_query(&cleared).with_person.is_empty());
}

#[test]
fn empty_and_whitespace_queries_keep_existing_modifier_separator_rules() {
    let filters = SearchFilters {
        in_thread: true,
        ..Default::default()
    };
    assert_eq!(filters.apply("").unwrap(), "is:thread");
    assert_eq!(filters.apply("report\t").unwrap(), "report\tis:thread");
    assert_eq!(SearchFilters::default().apply("is:thread").unwrap(), "");
    let filters = SearchFilters {
        with_person: "  @Alex  ".into(),
        ..Default::default()
    };
    assert_eq!(filters.apply("").unwrap(), "with:@Alex");
}

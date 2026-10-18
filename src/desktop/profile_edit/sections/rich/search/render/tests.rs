use super::*;

fn row(name: &str) -> Value {
    json!({"id":"C12345678","name":name,"is_channel":true,"fromAnotherTeam":false})
}
fn field(text: &str, lowered: &str, locale_lowered: &str) -> Field {
    Field {
        text: text.into(),
        lowered: lowered.into(),
        locale_lowered: locale_lowered.into(),
    }
}
fn reply(fields: Vec<Field>) -> LowerReply {
    LowerReply {
        ticket: "7".into(),
        stage: 2,
        locale: "tr".into(),
        query: "".into(),
        fields,
    }
}
fn collation(classes: Vec<(usize, usize)>) -> CollationReply {
    CollationReply {
        ticket: "7".into(),
        locale: "tr".into(),
        collation_locale: "en-US".into(),
        classes,
        demote_passthrough: false,
    }
}

#[test]
fn searcher_formatting_preserves_emoji_document_and_pretrim_space() {
    for (raw, formatted) in [
        ("#Ship", "Ship"),
        ("＃Ship", "Ship"),
        ("＠Ship", "Ship"),
        ("/Ship", "Ship"),
        ("／Ship", "Ship"),
        ("##Ship", "#Ship"),
        (" #Ship", " #Ship"),
        (":Ship", ":Ship"),
        ("+Ship", "+Ship"),
        ("＋Ship", "＋Ship"),
        ("", ""),
        ("#", ""),
    ] {
        assert_eq!(formatted_query(raw), formatted);
    }
    // 4ucY's later default formatter is distinct from Searcher's options.
    assert_eq!(model_query(formatted_query("##Ship")), "Ship");
    assert_eq!(model_query(formatted_query(":Ship")), "Ship");
    assert_eq!(model_query(formatted_query("＋Ship")), "Ship");
}

#[test]
fn ordinary_lowercase_and_locale_normalization_are_distinct_observed_facts() {
    let raw = "  I Café\u{301}\nX  ";
    let fields = BTreeMap::from([(
        raw.into(),
        field(raw, "  i café\u{301}\nx  ", "ı café\u{301}\nx"),
    )]);
    let got = channel_name(&fields, &row(raw)).unwrap();
    assert_eq!(got.comparison_lowercase, "  i café\u{301}\nx  ");
    // Preserve the actual renderer locale result; retained J3F subsequently
    // maps dotless U+0131 to ASCII i, independently of comparison lowercase.
    assert_eq!(fields[raw].locale_lowered, "ı café\u{301}\nx");
    assert_eq!(got.normalized, "i cafe x");
    assert!(channel_name(&fields, &row("missing")).is_err());
    // The first J3F pass can expose leading whitespace. jytj performs hNDQ
    // again: do not reuse the first normalized string as matcher_normalized.
    let first = normalize_lowered_query("\u{301} a");
    assert_eq!(first, " a");
    let second = normalize_lowered_query(first.trim());
    assert_eq!(second, "a");
}

#[test]
fn input_plan_covers_previous_names_model_newlines_and_prefix_variants() {
    let mut r = row("Alpha\nBeta\n");
    r["previous_names"] = json!(["Older", "mpdm-ignored", "Ｉ"]);
    let fields = inputs("##Query", &[r]).unwrap();
    for expected in [
        "",
        "##Query",
        "#Query",
        "Query",
        "Alpha\nBeta\n",
        "Alpha",
        "Beta",
        "Older",
        "Ｉ",
    ] {
        assert!(fields.contains(expected), "{expected:?}");
    }
    assert!(!fields.contains("mpdm-ignored"));
}

#[test]
fn unsupported_or_malformed_rows_refuse_instead_of_losing_source_strings() {
    for bad in [
        json!({"id":"C12345678","is_channel":true,"name":"good"}),
        json!({"id":"C12345678","is_channel":true,"fromAnotherTeam":false,"name":"mc:meeting"}),
        json!({"id":"C12345678","is_channel":true,"fromAnotherTeam":false,"name":"good","previous_names":[null]}),
        json!({"id":"C12345678","is_channel":true,"fromAnotherTeam":false,"name":"good","previous_names":"bad"}),
    ] {
        assert!(inputs("", &[row("valid"), bad]).is_err());
    }
}

#[test]
fn renderer_receipt_binds_ticket_stage_exact_input_order_and_locale() {
    let fields = ["A".into(), "B".into()];
    assert!(
        decode_lower(
            "7",
            2,
            &fields,
            reply(vec![field("A", "a", "a"), field("B", "b", "b")])
        )
        .is_ok()
    );
    for bad in [
        reply(vec![field("B", "b", "b"), field("A", "a", "a")]),
        reply(vec![field("A", "a", "a")]),
        reply(vec![field("A", "a", "a"), field("A", "a", "a")]),
    ] {
        assert!(decode_lower("7", 2, &fields, bad).is_err());
    }
    assert!(decode_lower("8", 2, &[], reply(vec![])).is_err());
    assert!(decode_lower("7", 3, &[], reply(vec![])).is_err());
    assert!(same_locale("tr", "en-US").is_err());
    assert!(same_locale("", "").is_err());
    assert!(
        serde_json::from_value::<LowerReply>(json!({
            "ticket":"7","stage":2,"locale":"tr","query":"", "fields":[], "selection":true
        }))
        .is_err()
    );
}

#[test]
fn numeric_collation_ties_are_observed_and_missing_comparisons_refuse() {
    let names = ["channel10", "channel2", "channel02"].map(str::to_owned);
    let classes =
        decode_collation("7", "tr", &names, collation(vec![(1, 0), (2, 0), (0, 1)])).unwrap();
    assert_eq!(
        compare(&classes, "channel2", "channel02").unwrap(),
        Ordering::Equal
    );
    assert_eq!(
        compare(&classes, "channel10", "channel2").unwrap(),
        Ordering::Greater
    );
    assert!(compare(&classes, "missing", "channel2").is_err());
}

#[test]
fn incomplete_foreign_or_noncontiguous_collation_receipts_refuse() {
    let names = ["a".into(), "b".into()];
    for classes in [
        vec![(0, 0)],
        vec![(0, 0), (0, 1)],
        vec![(0, 0), (2, 1)],
        vec![(0, 1), (1, 1)],
        vec![(0, 0), (1, 2)],
    ] {
        assert!(decode_collation("7", "tr", &names, collation(classes)).is_err());
    }
    assert!(decode_collation("8", "tr", &names, collation(vec![(0, 0), (1, 1)])).is_err());
    assert!(decode_collation("7", "en-US", &names, collation(vec![(0, 0), (1, 1)])).is_err());
    let mut bad = collation(vec![(0, 0), (1, 1)]);
    bad.collation_locale = "tr".into();
    assert!(decode_collation("7", "tr", &names, bad).is_err());
}

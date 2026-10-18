use super::*;

fn row() -> Value {
    json!({"id":"URENDER01","name":"raw-user","real_name":"Top Real",
        "profile":{"real_name":"Profile Real","display_name":"Display\nSecond",
            "email":"Mail@Example.test","first_name":"First\nNext","last_name":"Last"},
        "opaque":{"kept":true},"facts":{"comparisonName":"injected","bestName":"injected"}})
}
fn policy(override_value: i64) -> name_policy::Snapshot {
    let membership = name_policy::Membership::observe(
        None,
        &["TRENDER01".into()],
        &BTreeMap::from([("TRENDER01".into(), json!({"id":"TRENDER01"}))]),
    )
    .unwrap();
    name_policy::Snapshot::new(
        name_policy::OrdinaryClientV2,
        membership,
        name_policy::UserPrefs::decode(
            &json!({"ok":true,"prefs":{"display_real_names_override":override_value}}),
        )
        .unwrap(),
        None,
    )
    .unwrap()
}
fn field(text: &str, ordinary: &str, locale: &str) -> render::Field {
    render::Field {
        text: text.into(),
        lowered: ordinary.into(),
        locale_lowered: locale.into(),
    }
}
fn ascii_fields(inputs: BTreeSet<String>) -> BTreeMap<String, render::Field> {
    inputs
        .into_iter()
        .map(|text| {
            assert!(text.is_ascii());
            let lower = text.to_ascii_lowercase();
            let locale = lower.trim().to_owned();
            (text.clone(), field(&text, &lower, &locale))
        })
        .collect()
}

#[test]
fn source_and_gantry_self_are_both_captured_without_replacing_inventory_names() {
    let source = row();
    let self_row = json!({"id":"URENDER01","profile":{"display_name":"Actual self",
        "real_name":"Self Real","email":"Self@Example.test"}});
    let fields = ascii_fields(inputs("@Raw", &[source.clone()], &self_row).unwrap());
    let derived = derive(&source, &fields).unwrap();
    let self_derived = derive(&self_row, &fields).unwrap();
    assert_eq!(derived["_display_name_normalized_lc"], "display\nsecond");
    assert_eq!(self_derived["_display_name_normalized_lc"], "actual self");
    assert_eq!(derived["opaque"], source["opaque"]);
    assert_eq!(derived["profile"], source["profile"]);
    assert_eq!(derived["name"], source["name"]);
}

#[test]
fn derivation_uses_ordinary_lowercase_without_trim_and_only_profile_name_fields() {
    let raw = json!({"id":"URENDER01","real_name":"Wrong top fallback",
        "profile":{"display_name":"  I Cafe\u{301}  ","real_name_normalized":"Wrong normalized"}});
    let fields = BTreeMap::from([
        (String::new(), field("", "", "")),
        (
            "  I Cafe\u{301}  ".into(),
            field("  I Cafe\u{301}  ", "  i cafe\u{301}  ", "ı cafe\u{301}"),
        ),
    ]);
    let derived = derive(&raw, &fields).unwrap();
    assert_eq!(derived["_display_name_normalized_lc"], "  i cafe  ");
    assert_eq!(derived["_real_name_normalized_lc"], "");
    assert_eq!(
        ordinary(&fields, "  I Cafe\u{301}  ").unwrap(),
        "  i cafe\u{301}  "
    );
    // J3F maps dotless i after the renderer's locale conversion.
    assert_eq!(normalized(&fields, "  I Cafe\u{301}  ").unwrap(), "i cafe");
}

#[test]
fn absent_renderer_field_is_refusal_not_fabricated_empty_normalization() {
    let raw = row();
    let mut fields = ascii_fields(inputs("x", &[raw.clone()], &raw).unwrap());
    fields.remove("Mail@Example.test");
    assert!(derive(&raw, &fields).is_err());
    assert!(ordinary(&fields, "unknown").is_err());
    assert!(normalized(&fields, "unknown").is_err());
}

#[test]
fn policy_comparison_raw_best_first_last_and_newline_fields_all_have_coverage() {
    let raw = row();
    let mut fields = BTreeSet::new();
    let mut collation = BTreeSet::new();
    let comparison = rank_fields(&raw, &policy(1), &mut fields, &mut collation).unwrap();
    assert_eq!(comparison, "Profile Real");
    assert_eq!(collation, BTreeSet::from(["Profile Real".into()]));
    for field in [
        "raw-user",
        "Profile Real",
        "Display\nSecond",
        "Display",
        "Second",
        "First\nNext",
        "First",
        "Next",
        "Last",
    ] {
        assert!(fields.contains(field), "{field}");
    }
    assert!(!fields.contains("injected"));
    let comparison = rank_fields(&raw, &policy(-1), &mut fields, &mut collation).unwrap();
    assert_eq!(comparison, "Display\nSecond");
    assert!(collation.contains("Display\nSecond"));
}

#[test]
fn empty_policy_name_uses_exact_raw_selector_fallback_for_collation_and_model() {
    let raw = json!({"id":"URENDER01","name":"User 12","profile":{}});
    let mut fields = BTreeSet::new();
    let mut collation = BTreeSet::new();
    assert_eq!(
        rank_fields(&raw, &policy(-1), &mut fields, &mut collation).unwrap(),
        ""
    );
    assert!(fields.contains("User 12") && fields.contains(""));
    assert_eq!(collation, BTreeSet::from(["User 12".into()]));
}

#[test]
fn query_inputs_cover_one_trigger_strip_and_separate_model_prefix_without_trimming() {
    let raw = row();
    let fields = inputs("＠:Al pha", &[raw.clone()], &raw).unwrap();
    assert!(fields.contains("＠:Al pha"));
    assert!(fields.contains(":Al pha"));
    assert!(fields.contains("Al pha"));
    assert!(fields.contains(""));
    assert!(!fields.contains("al pha"));
    let empty = inputs("@", &[], &json!({"id":"USELF0001"})).unwrap();
    assert!(empty.contains("") && empty.contains("@"));
}

#[test]
fn malformed_inputs_and_forged_conversation_category_refuse_instead_of_silent_filtering() {
    let raw = row();
    for bad in [
        json!(null),
        json!({"id":"URENDER01","profile":false}),
        json!({"id":"URENDER01","profile":{"real_name":7}}),
    ] {
        assert!(inputs("a", &[bad], &raw).is_err());
    }
    let mut bad = raw;
    bad["is_im"] = json!(true);
    assert!(
        rank_fields(
            &bad,
            &policy(-1),
            &mut BTreeSet::new(),
            &mut BTreeSet::new()
        )
        .is_err()
    );
}

#[test]
fn captured_collation_classes_preserve_equal_names_numeric_order_and_missing_refusal() {
    let classes = BTreeMap::from([
        ("User 2".into(), 0),
        ("User 02".into(), 0),
        ("User 12".into(), 1),
    ]);
    assert_eq!(
        render::compare(&classes, "User 2", "User 02").unwrap(),
        Ordering::Equal
    );
    assert_eq!(
        render::compare(&classes, "User 2", "User 12").unwrap(),
        Ordering::Less
    );
    assert!(render::compare(&classes, "missing", "User 2").is_err());
}

use super::*;

#[test]
fn original_rows_and_missing_lowercase_are_not_name_defaults() {
    let row = json!({"id":"U1","profile":{"display_name":"  A  B ","real_name":"Á"}});
    let before = row.clone();
    let inputs = member_inputs("@A", "A", std::slice::from_ref(&row), &row).unwrap();
    assert!(inputs.contains("  A  B "));
    assert!(inputs.contains("Á"));
    assert!(derive(&row, &BTreeMap::new()).is_err());
    assert!(member_inputs("x", "x", &[row.clone(), row.clone()], &row).is_err());
    assert_eq!(row, before);
    fn send_sync<T: Send + Sync>() {}
    send_sync::<MemberStrings>();
}

#[test]
fn resolver_j3f_and_history_normalization_do_not_share_whitespace_rules() {
    // Pc's ordinary JS lowercase is acquired after this exact transform.
    assert_eq!(local::normalize_name_source("  À  B\t"), "  A  B\t");
    assert_eq!(local::normalize_lowered_query("a  b\t"), "a b ");
    assert_eq!(model_query(":@Name"), "@Name");
    assert_eq!(model_query("＠Name"), "Name");
    assert_eq!(model_query(" @Name"), " @Name");
}

#[test]
fn facts_bind_actual_inputs_locale_and_explicit_english_scope() {
    let inputs = vec![" I ".to_owned()];
    let mut facts = Facts {
        ticket: "t".into(),
        query: "me".into(),
        locale: "en-US".into(),
        presentation: english(),
        maybe_self: true,
        fields: vec![GroupField {
            text: " I ".into(),
            lowered: " i ".into(),
        }],
    };
    assert!(validate_facts(&facts, "t", "me", "en-US", &inputs).is_ok());
    assert!(validate_facts(&facts, "t", "me", "tr", &inputs).is_err());
    assert!(validate_facts(&facts, "other", "me", "en-US", &inputs).is_err());
    facts.presentation["origin"] = json!("guessed-account-locale");
    assert!(validate_facts(&facts, "t", "me", "en-US", &inputs).is_err());
    facts.presentation = english();
    facts.maybe_self = false;
    assert!(validate_facts(&facts, "t", "me", "en-US", &inputs).is_err());
}

#[test]
fn effective_group_scoring_alias_is_distinct_from_raw_handle_and_rank_name() {
    let raw = json!({"id":"SGROUP001","name":" I Name ","handle":"@@Handle","_name_lc":" Stored Alias ","is_usergroup":true});
    let row = groups::Snapshot::capture(raw.clone(), raw.clone()).unwrap();
    let lower = BTreeMap::from([(" I Name ".into(), " i name ".into())]);
    let mut fields = BTreeSet::new();
    let mut collation = BTreeSet::new();
    group_fields(&row, &lower, &mut fields, &mut collation).unwrap();
    assert!(fields.contains(" Stored Alias "));
    assert!(!fields.contains(" i name "));
    assert!(fields.contains("@Handle") && fields.contains("@@Handle"));
    assert_eq!(
        collation,
        BTreeSet::from(["@@Handle".into(), " I Name ".into()])
    );
    assert_eq!(row.source(), &raw);
    assert_eq!(row.effective(), &raw);
}

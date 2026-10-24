use super::*;
use serde_json::json;

#[test]
fn unknown_dispatch_predicate_distinguishes_absent_known_and_terminal() {
    let mut state = ClassificationState::capture(&[
        json!({"id":"C00000001","name":"Known"}),
        json!({"id":"C00000002","isUnknown":true}),
        json!({"id":"C00000003","isNonExistent":true}),
    ])
    .unwrap();
    assert!(!state.is_unknown("C00000000").unwrap());
    assert_eq!(
        state.row("C00000000").unwrap().kind,
        ClassificationKind::Absent
    );
    assert!(!state.is_unknown("C00000001").unwrap());
    assert!(state.is_unknown("C00000002").unwrap());
    assert_eq!(
        state.row("C00000002").unwrap().kind,
        ClassificationKind::Existing
    );
    assert!(!state.is_unknown("C00000003").unwrap());
    assert_eq!(
        state.row("C00000003").unwrap().kind,
        ClassificationKind::NonExistent
    );
    state.unavailable.insert("C00000002".into());
    assert!(state.is_unknown("C00000002").is_err());
    assert!(state.is_unknown("invalid").is_err());
}

#[test]
fn membership_only_is_unknown_until_actual_raw_processor_clears_it() {
    let mut state = ClassificationState::capture(&[]).unwrap();
    state
        .membership_step(&[], &["C00000001".into()], true)
        .unwrap();
    assert_eq!(
        state.row("C00000001").unwrap().kind,
        ClassificationKind::Existing
    );
    assert!(state.is_unknown("C00000001").unwrap());
    let facts = Facts {
        self_id: "USELF".into(),
        current_team: "TONE".into(),
        omit_team: "TONE".into(),
        retain_public_membership_on_archive: false,
        members: BTreeMap::new(),
    };
    state
        .step(&[json!({"id":"C00000001","updated":7})], &facts, false)
        .unwrap();
    // 340b does not merge an unknown predecessor: even this actual partial
    // raw row replaces its placeholder flags. A name is not required.
    assert!(!state.is_unknown("C00000001").unwrap());
    assert!(!state.row("C00000001").unwrap().name_truthy);
    assert_eq!(state.conditional_version("C00000001").unwrap(), 7);
    state
        .step(
            &[json!({"id":"C00000001","name":"Returned","is_channel":true})],
            &facts,
            false,
        )
        .unwrap();
    assert!(!state.is_unknown("C00000001").unwrap());
    assert_eq!(state.conditional_version("C00000001").unwrap(), 7);
    state
        .membership_step(&["C00000001".into()], &[], true)
        .unwrap();
    assert!(!state.is_unknown("C00000001").unwrap());
}

#[test]
fn unknown_uses_existing_javascript_truth_not_boolean_equality() {
    for (value, expected) in [
        (Value::Null, false),
        (json!(false), false),
        (json!(0), false),
        (json!(""), false),
        (json!(true), true),
        (json!(1), true),
        (json!("false"), true),
        (json!([]), true),
        (json!({}), true),
    ] {
        let state =
            ClassificationState::capture(&[json!({"id":"C00000001","isUnknown":value})]).unwrap();
        assert_eq!(state.is_unknown("C00000001").unwrap(), expected);
    }
}

use super::*;

fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    }
}
fn gantry() -> Value {
    json!({"ok":true,"self":{"id":"U00000001"},"team":{"id":"T00000001"},
        "workspaces":[{"id":"T00000002"},{"id":"TEXTERNAL","allow_external_users":true},{"id":"T00000001"}],
        "default_workspace":"T00000002"})
}
fn expected() -> Vec<String> {
    vec!["T00000001".into(), "T00000002".into()]
}

#[test]
fn exact_gantry_identity_and_optional_internal_membership_are_independent() {
    let raw = gantry();
    assert_eq!(
        validate_gantry(&raw, &owner(), None).unwrap(),
        Some("T00000002".into())
    );
    assert_eq!(
        validate_gantry(&raw, &owner(), Some(&expected())).unwrap(),
        Some("T00000002".into())
    );
    assert!(validate_gantry(&raw, &owner(), Some(&["T00000001".into()])).is_err());
    assert!(
        validate_gantry(
            &raw,
            &owner(),
            Some(&["T00000001".into(), "T00000002".into(), "TEXTERNAL".into()])
        )
        .is_err()
    );
    for key in ["self", "team"] {
        let mut other = raw.clone();
        other[key]["id"] = json!("FOREIGN");
        assert!(validate_gantry(&other, &owner(), None).is_err());
    }
}

#[test]
fn failed_missing_and_duplicate_envelopes_never_become_optional_absence() {
    for patch in [
        json!({"ok":false}),
        json!({"error":"failure"}),
        json!({"workspaces":null}),
        json!({"workspaces":[]}),
        json!({"workspaces":[{"id":"T00000001"},{"id":"T00000001"}]}),
        json!({"workspaces":[{"id":"not-a-team"}]}),
    ] {
        let mut raw = gantry();
        raw.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(validate_gantry(&raw, &owner(), None).is_err());
    }
    assert!(validate_gantry(&json!({}), &owner(), None).is_err());
    assert!(validate_gantry(&Value::Null, &owner(), None).is_err());
}

#[test]
fn complete_missing_default_never_falls_back_to_active_or_credential_team() {
    let mut raw = gantry();
    raw.as_object_mut().unwrap().remove("default_workspace");
    assert_eq!(validate_gantry(&raw, &owner(), None).unwrap(), None);
    raw["default_workspace"] = Value::Null;
    assert_eq!(
        validate_gantry(&raw, &owner(), Some(&expected())).unwrap(),
        None
    );
    for value in [
        json!(""),
        json!("E00000001"),
        json!(false),
        json!(7),
        json!([]),
        json!({}),
    ] {
        raw["default_workspace"] = value;
        assert!(validate_gantry(&raw, &owner(), None).is_err());
    }
}

#[test]
fn observed_processed_priority_order_and_cold_empty_hint_are_preserved() {
    let offline = Offline::decode_ordered(
        &json!({"ok":true,
        "channel_priority_90d":{"CZ":4,"CA":4,"CHIGH":9}}),
        Some(&["CZ".into(), "CA".into(), "CHIGH".into()]),
    )
    .unwrap();
    let facts = request_facts(&gantry(), &owner(), Some(&expected()), &offline).unwrap();
    assert_eq!(facts.top_channels, Some(json!(["CHIGH", "CZ", "CA"])));
    assert_eq!(facts.default_workspace, Some("T00000002".into()));
    // Complete observed cold response, rather than a fabricated missing cache.
    let cold = Offline::decode(&json!({"ok":true})).unwrap();
    let mut raw = gantry();
    raw.as_object_mut().unwrap().remove("default_workspace");
    let facts = request_facts(&raw, &owner(), None, &cold).unwrap();
    assert_eq!(facts.top_channels, Some(json!([])));
    assert_eq!(facts.default_workspace, None);
}

#[test]
fn scores_without_original_order_refuse_instead_of_fabricating_optional_hint() {
    let unordered =
        Offline::decode(&json!({"ok":true,"channel_priority_90d":{"CZ":4,"CA":4}})).unwrap();
    assert!(request_facts(&gantry(), &owner(), None, &unordered).is_err());
    let cold = Offline::decode(&json!({"ok":true})).unwrap();
    assert!(request_facts(&json!({}), &owner(), None, &cold).is_err());
}

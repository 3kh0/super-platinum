use super::*;
use serde_json::json;
#[test]
fn complete_gantry_workspace_preferences_use_the_same_policy_without_an_envelope() {
    let mut p = snapshot(json!({}), json!({}), json!({}));
    assert_eq!(p.should_display_real_names(), Err(Error::MissingPreference));
    for (id, value) in [("T1", false), ("T2", true)] {
        let prefs = json!({"display_real_names":value});
        let observed = TeamPrefs::from_complete(id, prefs.as_object().unwrap()).unwrap();
        assert_eq!(
            observed,
            TeamPrefs::decode(id, &json!({"ok":true,"prefs":prefs})).unwrap()
        );
        p.observe_team(observed).unwrap();
    }
    assert_eq!(p.should_display_real_names(), Ok(true));
    assert_eq!(
        TeamPrefs::from_complete("", &Map::new()),
        Err(Error::Membership)
    );
    assert_eq!(
        TeamPrefs::decode("T1", &json!({"ok":true})),
        Err(Error::Shape)
    );
}
fn membership() -> Membership {
    Membership::observe(
        Some("E1"),
        &["T1".into(), "T2".into()],
        &BTreeMap::from([
            ("T1".into(), json!({"id":"T1","enterprise_id":"E1"})),
            ("T2".into(), json!({"id":"T2","enterprise_id":"E2"})),
        ]),
    )
    .unwrap()
}
fn snapshot(user: Value, org: Value, locked: Value) -> Snapshot {
    Snapshot::new(
        OrdinaryClientV2,
        membership(),
        UserPrefs::decode(&json!({"ok":true,"prefs":user})).unwrap(),
        Some(
            EnterprisePrefs::decode(
                "E1",
                &json!({"ok":true,"org_prefs":org,"locked_prefs":locked}),
            )
            .unwrap(),
        ),
    )
    .unwrap()
}
#[test]
fn observed_unified_source_vectors() {
    let cases: Value = serde_json::from_str(include_str!("source-vectors.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let state = &case["state"];
        let ids: Vec<String> =
            serde_json::from_value(state["selfTeamIds"]["internalWorkspaceIds"].clone()).unwrap();
        let mut teams: BTreeMap<String, Value> =
            serde_json::from_value(state["teams"].clone()).unwrap();
        for (id, raw) in &mut teams {
            raw["id"] = id.clone().into();
        }
        let member =
            Membership::observe(state["selfTeamIds"]["enterpriseId"].as_str(), &ids, &teams)
                .unwrap();
        let user = UserPrefs::decode(&json!({"ok":true,"prefs":state["userPrefs"]})).unwrap();
        let enterprise=EnterprisePrefs::decode("E1",&json!({"ok":true,"org_prefs":state["enterprisePrefs"]["org_prefs"],"locked_prefs":state["enterprisePrefs"]["locked_prefs"]})).unwrap();
        let mut policy = Snapshot::new(OrdinaryClientV2, member, user, Some(enterprise)).unwrap();
        for (id, prefs) in state["teamPrefs"].as_object().unwrap() {
            policy
                .observe_team(TeamPrefs::decode(id, &json!({"ok":true,"prefs":prefs})).unwrap())
                .unwrap();
        }
        assert_eq!(
            policy.should_display_real_names().unwrap(),
            case["result"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn incomplete_negative_refuses_but_observed_positive_resolves_any() {
    let mut p = snapshot(json!({}), json!({}), json!({}));
    assert_eq!(p.should_display_real_names(), Err(Error::MissingPreference));
    assert_eq!(p.missing_team_preferences(), vec!["T1", "T2"]);
    p.observe_team(
        TeamPrefs::decode(
            "T2",
            &json!({"ok":true,"prefs":{"display_real_names":true}}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(p.should_display_real_names(), Ok(true));
}
#[test]
fn explicit_override_and_hidden_lock_have_distinct_semantics() {
    let p = snapshot(
        json!({"display_real_names_override":1,"hidden_users":"U1, U2,,U1"}),
        json!({}),
        json!({}),
    );
    assert_eq!(p.should_display_real_names(), Ok(true));
    assert!(p.missing_team_preferences().is_empty());
    assert!(p.hidden("U1"));
    assert!(p.hidden(" U2"));
    assert!(!p.hidden("U2"));
    assert!(!p.hidden(""));
    let p = snapshot(
        json!({"hidden_users":"U1"}),
        json!({}),
        json!({"hide_person_opt_out":"yes"}),
    );
    assert!(!p.hidden("U1"));
}
#[test]
fn malformed_or_contradictory_envelopes_never_become_empty_preferences() {
    for error in [json!(false), json!(0), json!(""), json!({}), json!([])] {
        let raw = json!({"ok":true,"error":error,"prefs":{},"org_prefs":{},"locked_prefs":{}});
        assert_eq!(UserPrefs::decode(&raw), Err(Error::Response));
        assert_eq!(TeamPrefs::decode("T1", &raw), Err(Error::Response));
        assert_eq!(EnterprisePrefs::decode("E1", &raw), Err(Error::Response));
    }
    assert_eq!(UserPrefs::decode(&json!({"ok":true})), Err(Error::Shape));
    assert!(Membership::observe(None, &["T1".into()], &BTreeMap::new()).is_err());
    assert!(
        Snapshot::new(
            OrdinaryClientV2,
            membership(),
            UserPrefs::decode(&json!({"ok":true,"prefs":{}})).unwrap(),
            None
        )
        .is_err()
    );
}
#[test]
fn member_text_respects_hidden_and_real_display_fallback() {
    let member = json!({"id":"U1","profile":{"real_name":"Real","display_name":"Display"},"real_name":"Top"});
    let p = snapshot(
        json!({"display_real_names_override":-1}),
        json!({}),
        json!({}),
    );
    assert_eq!(p.member_name(&member, "Name hidden").unwrap(), "Display");
    let p = snapshot(
        json!({"display_real_names_override":1}),
        json!({}),
        json!({}),
    );
    assert_eq!(p.member_name(&member, "Name hidden").unwrap(), "Real");
    let p = snapshot(json!({"hidden_users":"U1"}), json!({}), json!({}));
    assert_eq!(
        p.member_name(&member, "Name hidden").unwrap(),
        "Name hidden"
    );
}

#[test]
fn malformed_member_name_does_not_silently_choose_another_field() {
    let policy = snapshot(
        json!({"display_real_names_override":1}),
        json!({}),
        json!({}),
    );
    for value in [json!(42), json!(true), json!([]), json!({})] {
        let member = json!({"id":"U1","profile":{"real_name":value},"real_name":"Wrong fallback"});
        assert_eq!(
            policy.member_name(&member, "Name hidden"),
            Err(Error::Shape)
        );
    }
}

#[test]
fn malformed_profile_container_refuses_but_hidden_label_precedes_names() {
    let policy = snapshot(
        json!({"display_real_names_override":1}),
        json!({}),
        json!({}),
    );
    let member = json!({"id":"U1","profile":42,"real_name":"Wrong fallback"});
    assert_eq!(
        policy.member_name(&member, "Name hidden"),
        Err(Error::Shape)
    );
    let hidden = snapshot(json!({"hidden_users":"U1"}), json!({}), json!({}));
    assert_eq!(
        hidden.member_name(&member, "Name hidden").unwrap(),
        "Name hidden"
    );
}

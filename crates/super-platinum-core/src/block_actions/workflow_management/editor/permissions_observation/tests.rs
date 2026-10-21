use super::*;

fn original() -> Editor {
    let mut load = EditorLoad::new("WF1").unwrap();
    for (kind, raw) in [
        (
            ReadKind::Latest,
            json!({"ok":true,"decorated_workflow":{"id":"WF1","app_id":"A1","steps":[],"collaborators":["U1"],"opaque":7}}),
        ),
        (
            ReadKind::Triggers,
            json!({"ok":true,"triggers":[{"id":"TR1"}]}),
        ),
        (
            ReadKind::TriggerInfo,
            json!({"ok":true,"trigger":{"id":"TR1","workflow":{"workflow_id":"WF1","app_id":"A1"},"opaque":false}}),
        ),
        (
            ReadKind::Preferences,
            json!({"ok":true,"trippable_by_slack_connected_teams":{"allowed_teams":[]},"opaque":42}),
        ),
        (
            ReadKind::CopyPermissions,
            json!({"ok":true,"permission_type":"collaborators","opaque":{"keep":[0,false]}}),
        ),
        (
            ReadKind::RunPermissions,
            json!({"ok":true,"permission_type":"app_collaborators","user_ids":[],"opaque":{"keep":[1,null]}}),
        ),
        (
            ReadKind::Restriction,
            json!({"ok":true,"is_restricted":false,"restricted_by_connector_steps_only":false,"opaque":null}),
        ),
    ] {
        load.accept(kind, raw).unwrap();
    }
    load.editor().unwrap()
}
fn plan(editor: &Editor) -> MutationPlan {
    MutationPlan::new(
        editor,
        Command::RunPermission {
            permission_type: "named_entities".into(),
            user_ids: Some(vec!["U1".into(), "U2".into()]),
            channel_ids: Some(vec!["C1".into()]),
            team_ids: None,
            org_ids: None,
        },
    )
    .unwrap()
}
fn desired(original: &Editor) -> Editor {
    let mut current = original.clone();
    current
        .load
        .responses
        .get_mut(&ReadKind::CopyPermissions)
        .unwrap()["permission_type"] = json!("everyone");
    let run = current
        .load
        .responses
        .get_mut(&ReadKind::RunPermissions)
        .unwrap();
    run["permission_type"] = json!("named_entities");
    run["user_ids"] = json!(["U2", "U1"]);
    run["channel_ids"] = json!(["C1"]);
    current
}
fn observe(
    original: &Editor,
    current: &Editor,
    plan: &MutationPlan,
) -> super::super::super::Observation {
    original.permissions_observation(
        current,
        Some("everyone"),
        Some(plan.slots()[0].plan()),
        None,
    )
}

#[test]
fn combined_readback_preserves_original_and_all_unrelated_acquired_fields() {
    let original = original();
    let before = original.clone();
    let plan = plan(&original);
    let current = desired(&original);
    assert_eq!(observe(&original, &current, &plan), Observation::Matches);
    assert_eq!(original, before);
    for kind in [
        ReadKind::Latest,
        ReadKind::Triggers,
        ReadKind::TriggerInfo,
        ReadKind::CopyPermissions,
        ReadKind::RunPermissions,
        ReadKind::Preferences,
        ReadKind::Restriction,
    ] {
        let mut changed = current.clone();
        changed.load.responses.get_mut(&kind).unwrap()["opaque"] = json!({"changed":true});
        assert_eq!(
            observe(&original, &changed, &plan),
            Observation::Different,
            "{kind:?}"
        );
    }
    let mut changed = current.clone();
    changed.load.responses.get_mut(&ReadKind::Latest).unwrap()["decorated_workflow"]["collaborators"] =
        json!(["UOTHER"]);
    assert_eq!(observe(&original, &changed, &plan), Observation::Different);
}

#[test]
fn every_effective_target_list_is_compared_even_when_absent_from_request() {
    let original = original();
    let plan = plan(&original);
    let current = desired(&original);
    for (key, extra) in [
        ("user_ids", "U3"),
        ("channel_ids", "C2"),
        ("team_ids", "T1"),
        ("org_ids", "E1"),
    ] {
        let mut changed = current.clone();
        let run = changed
            .load
            .responses
            .get_mut(&ReadKind::RunPermissions)
            .unwrap();
        if run.get(key).is_none() {
            run[key] = json!([]);
        }
        run[key].as_array_mut().unwrap().push(json!(extra));
        assert_eq!(
            observe(&original, &changed, &plan),
            Observation::Different,
            "{key}"
        );
    }
    let mut empty = current.clone();
    let run = empty
        .load
        .responses
        .get_mut(&ReadKind::RunPermissions)
        .unwrap();
    run["team_ids"] = json!([]);
    run["org_ids"] = json!([]);
    assert_eq!(observe(&original, &empty, &plan), Observation::Matches);
    for malformed in [Value::Null, json!(false), json!([1]), json!([""])] {
        let mut changed = current.clone();
        changed
            .load
            .responses
            .get_mut(&ReadKind::RunPermissions)
            .unwrap()["org_ids"] = malformed;
        assert_eq!(
            observe(&original, &changed, &plan),
            Observation::Unavailable
        );
    }
}

#[test]
fn restriction_changes_require_the_exact_separately_acquired_response() {
    let original = original();
    let plan = plan(&original);
    let mut current = desired(&original);
    let raw = json!({"ok":true,"is_restricted":true,"restricted_by_connector_steps_only":true,"opaque":{"actual":7}});
    // A current Editor is independently acquired, not accepted into the old load.
    let mut refresh = EditorLoad::new("WF1").unwrap();
    refresh.accept(ReadKind::Restriction, raw.clone()).unwrap();
    assert!(original.restriction().is_some());
    current
        .load
        .responses
        .insert(ReadKind::Restriction, raw.clone());
    assert_eq!(observe(&original, &current, &plan), Observation::Different);
    assert_eq!(
        original.permissions_observation(
            &current,
            Some("everyone"),
            Some(plan.slots()[0].plan()),
            Some(&raw)
        ),
        Observation::Matches
    );
    let different =
        json!({"ok":true,"is_restricted":true,"restricted_by_connector_steps_only":true});
    assert_eq!(
        original.permissions_observation(
            &current,
            Some("everyone"),
            Some(plan.slots()[0].plan()),
            Some(&different)
        ),
        Observation::Different
    );
    for malformed in [
        json!({"ok":false,"error":"denied"}),
        json!({"ok":true,"is_restricted":false}),
    ] {
        assert_eq!(
            original.permissions_observation(
                &current,
                Some("everyone"),
                Some(plan.slots()[0].plan()),
                Some(&malformed)
            ),
            Observation::Unavailable
        );
    }
}

#[test]
fn unchanged_settings_and_envelope_metadata_are_not_fabricated_mutations() {
    let original = original();
    let mut same = original.clone();
    for raw in same.load.responses.values_mut() {
        raw["request_id"] = json!("new response");
    }
    assert_eq!(
        original.permissions_observation(&same, None, None, None),
        Observation::Matches
    );
    assert_eq!(
        original.permissions_observation(&same, Some("invalid"), None, None),
        Observation::Unavailable
    );
    let copy = MutationPlan::new(&original, Command::CopyPermission("everyone".into())).unwrap();
    assert_eq!(
        original.permissions_observation(&same, None, Some(copy.slots()[0].plan()), None),
        Observation::Unavailable
    );
    let mut foreign = plan(&original).slots()[0].plan().clone();
    foreign.arguments["trigger_id"] = json!("TR_OTHER");
    assert_eq!(
        original.permissions_observation(&same, None, Some(&foreign), None),
        Observation::Unavailable
    );
    same.load.workflow = "WF_OTHER".into();
    assert_eq!(
        original.permissions_observation(&same, None, None, None),
        Observation::Unavailable
    );
}

#[test]
fn matching_readback_never_settles_an_unknown_write() {
    let original = original();
    let mut plan = plan(&original);
    plan.mark_sent(0).unwrap();
    plan.settle(0, Outcome::Unknown).unwrap();
    assert_eq!(
        observe(&original, &desired(&original), &plan),
        Observation::Matches
    );
    assert!(matches!(plan.slots()[0].outcome(), Outcome::Unknown));
    assert!(!plan.primary_succeeded());
}

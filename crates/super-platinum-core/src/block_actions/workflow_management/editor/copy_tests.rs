use super::*;
fn original() -> Editor {
    let mut load = EditorLoad::new("WF1").unwrap();
    for (kind, raw) in [
        (
            ReadKind::Latest,
            json!({"ok":true,"decorated_workflow":{"id":"WF1","app_id":"A1","steps":[],"title":"Raw \u{4e2d}","collaborators":["U1"],"opaque":{"keep":[1,null]}}}),
        ),
        (ReadKind::Triggers, json!({"ok":true,"triggers":[]})),
        (
            ReadKind::Preferences,
            json!({"ok":true,"trippable_by_slack_connected_teams":{"allowed_teams":[]},"opaque":true}),
        ),
        (
            ReadKind::CopyPermissions,
            json!({"ok":true,"permission_type":"everyone","opaque":{"keep":1}}),
        ),
    ] {
        load.accept(kind, raw).unwrap();
    }
    load.editor().unwrap()
}
#[test]
fn desired_copy_observation_preserves_every_other_acquired_domain_field() {
    let original = original();
    let before = original.clone();
    let mut current = original.clone();
    current
        .load
        .responses
        .get_mut(&ReadKind::CopyPermissions)
        .unwrap()["permission_type"] = json!("collaborators");
    assert_eq!(
        original.copy_permission_observation(&current, "collaborators"),
        super::super::Observation::Matches
    );
    assert_eq!(original, before);
    for kind in [
        ReadKind::Latest,
        ReadKind::Triggers,
        ReadKind::Preferences,
        ReadKind::CopyPermissions,
    ] {
        let mut changed = current.clone();
        changed.load.responses.get_mut(&kind).unwrap()["opaque"] = json!({"changed":true});
        assert_eq!(
            original.copy_permission_observation(&changed, "collaborators"),
            super::super::Observation::Different
        );
    }
    current
        .load
        .responses
        .get_mut(&ReadKind::CopyPermissions)
        .unwrap()["request_id"] = json!("new envelope");
    assert_eq!(
        original.copy_permission_observation(&current, "collaborators"),
        super::super::Observation::Matches
    );
    assert_eq!(
        original.copy_permission_observation(&original, "collaborators"),
        super::super::Observation::Different
    );
    assert_eq!(
        original.copy_permission_observation(&current, "named_entities"),
        super::super::Observation::Unavailable
    );
    current.load.workflow = "WF2".into();
    assert_eq!(
        original.copy_permission_observation(&current, "collaborators"),
        super::super::Observation::Unavailable
    );
}

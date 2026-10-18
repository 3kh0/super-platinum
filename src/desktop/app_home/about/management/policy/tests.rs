use super::*;

#[test]
fn activity_export_uses_only_actual_nested_assignment_group() {
    for raw in [
        json!({}),
        json!({"assignments": {}}),
        json!({"assignments": {ACTIVITY_EXPORT_EXPERIMENT: "on"}}),
        json!({"assignments": {ACTIVITY_EXPORT_EXPERIMENT: {"enabled": true}}}),
        json!({ACTIVITY_EXPORT_EXPERIMENT: {"group": "on"}}),
        json!({"assignments": {"other": {"group": "on"}}}),
    ] {
        assert_eq!(activity_export_group(&raw), None);
    }
    for value in [
        json!("on"),
        json!("off"),
        json!(true),
        json!(null),
        json!({"group":"on"}),
    ] {
        let raw = json!({"ok":true,"assignments":{ACTIVITY_EXPORT_EXPERIMENT:{"group":value}}});
        assert_eq!(activity_export_group(&raw), Some(value));
    }
}

#[test]
fn ordinary_editor_access_never_implies_export_assignment() {
    let mut s = super::super::tests::shell();
    let mut snapshot = fixture_snapshot(&mut s);
    assert!(snapshot.can_enter());
    assert_eq!(snapshot.activity_export_assignment(), None);
    assert!(!snapshot.can_export_activity());
    for value in [
        json!("off"),
        json!("ON"),
        json!(" on"),
        json!(true),
        json!(null),
        json!({"group":"on"}),
    ] {
        snapshot.activity_export_group = Some(value);
        assert!(!snapshot.can_export_activity());
    }
    snapshot.activity_export_group = Some(json!("on"));
    assert!(snapshot.can_export_activity());
}

#[test]
fn policy_equality_retains_export_assignment_witness() {
    let mut s = super::super::tests::shell();
    let original = fixture_snapshot(&mut s);
    assert!(original.same(&original.clone()));
    for value in [json!("on"), json!("off"), json!(null), json!(true)] {
        let mut changed = original.clone();
        changed.activity_export_group = Some(value);
        assert!(changed.current(&s));
        assert!(!original.same(&changed));
        assert!(!changed.same(&original));
        assert!(changed.same(&changed.clone()));
    }
}

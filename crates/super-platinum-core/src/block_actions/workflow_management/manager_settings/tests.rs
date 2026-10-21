use super::*;

fn editor(ids: &[&str]) -> Editor {
    let mut load = EditorLoad::new("WF1").unwrap();
    for (kind, raw) in [
        (
            ReadKind::Latest,
            json!({"ok":true,"decorated_workflow":{"id":"WF1","app_id":"A1","steps":[],"collaborators":ids,"title":"Original","opaque":{"keep":true}},"available_data":{"keep":1},"expression_data":{"keep":2},"unknown_domain":7}),
        ),
        (ReadKind::Triggers, json!({"ok":true,"triggers":[]})),
        (
            ReadKind::Preferences,
            json!({"ok":true,"opaque":{"keep":3}}),
        ),
        (
            ReadKind::CopyPermissions,
            json!({"ok":true,"permission_type":"collaborators","opaque":4}),
        ),
    ] {
        load.accept(kind, raw).unwrap();
    }
    load.editor().unwrap()
}
fn selected(ids: &[&str]) -> Draft {
    let mut draft = Draft::new(&editor(&["U1", "U2"]), "U1").unwrap();
    draft.select(ids.iter().map(|s| Some((*s).into())).collect());
    draft
}
fn ack(plan: &Plan, index: usize) -> Value {
    let mut raw = plan.original.latest_response().clone();
    raw["decorated_workflow"]["collaborators"] = json!(plan.expected[index]);
    raw["opaque_receipt"] = json!({"preserve":[1,true,"original"]});
    raw
}

#[test]
fn no_op_reordering_and_existing_filtered_grants_do_not_gain_picker_authority() {
    let plan = selected(&["U2", "U1", "U1"]).plan().unwrap();
    assert!(plan.slots().is_empty());
    assert!(plan.complete());
    assert!(!plan.self_removal());
    assert_eq!(
        plan.observation(&editor(&["U1", "U2"]), None, None, None),
        Observation::Matches
    );
    // Eligibility is intentionally not guessed from the ID: original grants
    // stay visible/removable; the host must validate new canonical selections.
    assert_eq!(
        Draft::new(&editor(&["UGUEST"]), "UGUEST")
            .unwrap()
            .plan()
            .unwrap()
            .slots()
            .len(),
        0
    );
}

#[test]
fn unresolved_empty_invalid_refuse_but_no_invented_manager_cap() {
    assert!(selected(&[]).plan().is_err());
    assert!(selected(&[""]).plan().is_err());
    let mut draft = selected(&["U1"]);
    draft.select(vec![Some("U1".into()), None]);
    assert!(draft.plan().is_err());
    draft.select((0..101).map(|i| Some(format!("U{i}"))).collect());
    assert_eq!(draft.plan().unwrap().selected().len(), 101);
}

#[test]
fn source_union_preserves_order_and_removed_manager_run_targets() {
    assert_eq!(
        selected(&["U3", "U1"])
            .union_named_run(&["C1".into(), "U2".into(), "U3".into()])
            .unwrap(),
        vec!["U3", "U1", "C1", "U2"]
    );
}

#[test]
fn add_before_remove_requires_exact_original_ack_and_retains_raw() {
    let mut plan = selected(&["U3", "U2"]).plan().unwrap();
    assert!(plan.self_removal());
    assert_eq!(
        plan.slots()[0].plan().method(),
        "functions.workflows.collaborators.add"
    );
    assert_eq!(
        plan.slots()[1].plan().method(),
        "functions.workflows.collaborators.remove"
    );
    assert_eq!(plan.slots()[1].prerequisite(), Some(0));
    assert!(plan.mark_sent(1).is_err());
    let raw = ack(&plan, 0);
    plan.mark_sent(0).unwrap();
    plan.settle_response(0, raw.clone()).unwrap();
    assert_eq!(plan.responses()[0], Some(raw));
    assert_eq!(plan.next(), Some(1));
    let raw = ack(&plan, 1);
    plan.mark_sent(1).unwrap();
    plan.settle_response(1, raw.clone()).unwrap();
    assert!(plan.complete());
    assert!(plan.mark_sent(0).is_err());
    assert_eq!(
        plan.acknowledged_editor().unwrap().raw_workflow(),
        &raw["decorated_workflow"]
    );
}

#[test]
fn ambiguous_or_wrong_ack_never_opens_remove_and_preserves_original_raw() {
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"error":"contradictory"}),
        json!({"ok":false,"error":"denied"}),
        json!({"ok":true,"decorated_workflow":{"id":"OTHER","app_id":"A1","steps":[],"collaborators":["U1","U2","U3"]}}),
        json!({"ok":true,"decorated_workflow":{"id":"WF1","app_id":"A1","steps":[],"collaborators":["U3","U2"]}}),
    ] {
        let mut plan = selected(&["U3", "U2"]).plan().unwrap();
        plan.mark_sent(0).unwrap();
        plan.settle_response(0, raw.clone()).unwrap();
        assert_eq!(plan.responses()[0], Some(raw));
        assert_eq!(plan.next(), None);
        assert!(!plan.complete());
        assert!(plan.self_removal());
        assert!(plan.mark_sent(0).is_err());
        assert!(plan.mark_sent(1).is_err());
    }
}

#[test]
fn lost_add_matching_desired_readback_does_not_become_ack_or_allow_remove() {
    let mut plan = selected(&["U1", "U2", "U3"]).plan().unwrap();
    plan.mark_sent(0).unwrap();
    plan.settle_failure(0, false).unwrap();
    assert_eq!(
        plan.observation(&editor(&["U3", "U2", "U1"]), None, None, None),
        Observation::Matches
    );
    assert!(!plan.complete());
    assert_eq!(plan.slots()[0].outcome(), &Outcome::Unknown);
    assert_eq!(plan.next(), None);
}

#[test]
fn unrelated_ack_domains_cannot_bless_drift_in_independent_readback() {
    let mut plan = selected(&["U1", "U2", "U3"]).plan().unwrap();
    let mut raw = ack(&plan, 0);
    raw["decorated_workflow"]["date_updated"] = json!(9);
    raw["available_data"] = json!({"new":7});
    plan.mark_sent(0).unwrap();
    plan.settle_response(0, raw).unwrap();
    let current = plan.acknowledged_editor().unwrap();
    assert_eq!(
        plan.observation(&current, None, None, None),
        Observation::Different
    );
    let mut latest = current.latest_response().clone();
    latest["unknown_domain"] = json!(8);
    assert_eq!(
        plan.observation(&rebuild(&current, latest).unwrap(), None, None, None),
        Observation::Different
    );
    let mut latest = current.latest_response().clone();
    latest["decorated_workflow"]["opaque"] = json!(false);
    assert_eq!(
        plan.observation(&rebuild(&current, latest).unwrap(), None, None, None),
        Observation::Different
    );
    assert_eq!(
        plan.observation(&editor(&["U1", "U2", "U3"]), None, None, None),
        Observation::Matches
    );
}

#[test]
fn actual_optional_source_omission_and_partial_ack_remain_distinct() {
    let mut plan = selected(&["U3", "U2"]).plan().unwrap();
    let mut raw = ack(&plan, 0);
    raw.as_object_mut().unwrap().remove("available_data");
    plan.mark_sent(0).unwrap();
    plan.settle_response(0, raw).unwrap();
    assert!(
        plan.acknowledged_editor()
            .unwrap()
            .latest_response()
            .get("available_data")
            .is_none()
    );
    plan.mark_sent(1).unwrap();
    plan.settle_failure(1, true).unwrap();
    assert!(!plan.complete());
    assert_eq!(plan.next(), None);
    assert_eq!(plan.slots()[1].outcome(), &Outcome::NotSent);
}

#[test]
fn exact_collaborator_sets_refuse_extra_missing_and_duplicate_ids() {
    for ids in [
        json!(["U1", "U2"]),
        json!(["U1", "U2", "U3", "U4"]),
        json!(["U1", "U2", "U3", "U3"]),
    ] {
        let mut plan = selected(&["U1", "U2", "U3"]).plan().unwrap();
        let mut raw = ack(&plan, 0);
        raw["decorated_workflow"]["collaborators"] = ids;
        plan.mark_sent(0).unwrap();
        plan.settle_response(0, raw).unwrap();
        assert_eq!(plan.slots()[0].outcome(), &Outcome::Unknown);
        assert!(!plan.complete());
    }
}

#[test]
fn combined_copy_observation_allows_only_declared_change_and_keeps_opaque_preferences() {
    let plan = selected(&["U1", "U2", "U3"]).plan().unwrap();
    let make = |opaque| {
        let original = editor(&["U1", "U2", "U3"]);
        let mut load = EditorLoad::new("WF1").unwrap();
        for kind in [
            ReadKind::Latest,
            ReadKind::Triggers,
            ReadKind::Preferences,
            ReadKind::CopyPermissions,
        ] {
            let mut raw = original.responses()[&kind].clone();
            if kind == ReadKind::CopyPermissions {
                raw["permission_type"] = json!("everyone");
            }
            if kind == ReadKind::Preferences {
                raw["opaque"]["keep"] = json!(opaque);
            }
            load.accept(kind, raw).unwrap();
        }
        load.editor().unwrap()
    };
    assert_eq!(
        plan.observation(&make(3), Some("everyone"), None, None),
        Observation::Matches
    );
    assert_eq!(
        plan.observation(&make(3), None, None, None),
        Observation::Different
    );
    assert_eq!(
        plan.observation(&make(8), Some("everyone"), None, None),
        Observation::Different
    );
    assert!(!plan.complete());
}

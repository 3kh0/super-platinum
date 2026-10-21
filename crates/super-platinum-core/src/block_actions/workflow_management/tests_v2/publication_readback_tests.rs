use super::*;
#[test]
fn publication_readback_uses_desired_sync_not_old_failed_or_unknown_receipt() {
    let mut trigger = shortcut();
    trigger["inputs"] = json!({"removed":{"value":"opaque old"}});
    trigger["available_data"] = json!({"{{data.user}}":{"type":"slack#/types/user_id","name":"user","title":"User","is_customizable":false}});
    let original = acquired(latest(json!([])), Some(trigger.clone()), "original");
    let mut raw = latest(json!([]));
    raw["decorated_workflow"]["input_parameters"] =
        json!({"TR1__user":{"type":"slack#/types/user_id"}});
    for outcome in [
        Outcome::Rejected(json!({"ok":false,"error":"denied","opaque":7})),
        Outcome::Unknown,
    ] {
        let primary = acknowledged(&original, Command::Published(false), raw.clone());
        let mut plan = FollowUpPlan::new(&primary).unwrap();
        let desired = plan.slots()[0].plan().arguments()["inputs"].clone();
        plan.mark_sent(0).unwrap();
        plan.settle(0, outcome.clone()).unwrap();
        let before = plan.clone();
        let unchanged = acquired(raw.clone(), Some(trigger.clone()), "fresh");
        assert_eq!(
            plan.publication_observation(&unchanged),
            Observation::Different
        );
        let mut applied = trigger.clone();
        applied["inputs"] = desired;
        let observed = acquired(raw.clone(), Some(applied), "fresh");
        assert_eq!(
            plan.publication_observation(&observed),
            Observation::Matches
        );
        assert_eq!(plan, before);
        assert!(!plan.fully_succeeded());
        assert_eq!(plan.slots()[0].outcome(), &outcome);
        assert!(plan.ready().is_empty());
    }
}
#[test]
fn publication_noop_and_triggerless_observation_keep_exact_identity_and_boolean() {
    for trigger in [None, Some(shortcut())] {
        let raw = latest(json!([]));
        let editor = acquired(raw.clone(), trigger.clone(), "initial");
        let plan = FollowUpPlan::new(&acknowledged(
            &editor,
            Command::Published(false),
            raw.clone(),
        ))
        .unwrap();
        assert!(plan.fully_succeeded());
        assert_eq!(plan.publication_observation(&editor), Observation::Matches);
        let mut other = raw.clone();
        other["decorated_workflow"]["is_published"] = json!(true);
        assert_eq!(
            plan.publication_observation(&acquired(other, trigger.clone(), "new")),
            Observation::Different
        );
        let mut malformed = raw.clone();
        malformed["decorated_workflow"]["is_published"] = Value::Null;
        assert_eq!(
            plan.publication_observation(&acquired(malformed, trigger.clone(), "new")),
            Observation::Unavailable
        );
        let mut other = raw;
        other["decorated_workflow"]["app_id"] = json!("A_OTHER");
        let trigger = trigger.map(|mut t| {
            t["workflow"]["app_id"] = json!("A_OTHER");
            t
        });
        assert_eq!(
            plan.publication_observation(&acquired(other, trigger, "new")),
            Observation::Unavailable
        );
    }
}

#[test]
fn inconsistent_successful_sync_ack_cannot_change_desired_readback_target() {
    let mut trigger = shortcut();
    trigger["inputs"] = json!({"removed":{"value":"old"}});
    trigger["available_data"] = json!({"{{data.user}}":{"type":"slack#/types/user_id","name":"user","is_customizable":false}});
    let original = acquired(latest(json!([])), Some(trigger.clone()), "initial");
    let mut raw = latest(json!([]));
    raw["decorated_workflow"]["input_parameters"] = json!({"TR1__user":{}});
    let mut plan = FollowUpPlan::new(&acknowledged(
        &original,
        Command::Published(false),
        raw.clone(),
    ))
    .unwrap();
    let desired = plan.slots()[0].plan().arguments()["inputs"].clone();
    plan.mark_sent(0).unwrap();
    plan.settle(0, Outcome::Succeeded(json!({"ok":true,"trigger":trigger})))
        .unwrap();
    plan.advance().unwrap();
    assert!(plan.fully_succeeded());
    assert_eq!(
        plan.publication_observation(&acquired(raw.clone(), Some(trigger.clone()), "wrong")),
        Observation::Different
    );
    trigger["inputs"] = desired;
    assert_eq!(
        plan.publication_observation(&acquired(raw, Some(trigger), "right")),
        Observation::Matches
    );
}

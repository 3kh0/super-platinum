use super::*;

fn never_published(prior: Option<Value>) -> Value {
    let mut w = unpublished_workflow();
    w.as_object_mut()
        .unwrap()
        .remove("last_published_version_id");
    if let Some(value) = prior {
        w["last_published_version_id"] = value;
    }
    w
}

#[test]
fn explicit_first_publish_accepts_only_unpublished_no_prior_baselines() {
    let raw = published();
    for prior in [None, Some(Value::Null), Some(json!(""))] {
        let w = never_published(prior);
        let original = editor(w.clone(), Some(trigger()));
        assert!(original.publication().acknowledges_first_publish(&raw));
        assert!(!original.publication().acknowledges_publish(&raw));
        let mut published_baseline = w.clone();
        published_baseline["is_published"] = json!(true);
        assert!(
            !Publication {
                workflow: &published_baseline
            }
            .acknowledges_first_publish(&raw)
        );
        for value in [Value::Null, json!(0), json!("false"), json!({})] {
            let mut malformed = w.clone();
            malformed["is_published"] = value;
            assert!(
                !Publication {
                    workflow: &malformed
                }
                .acknowledges_first_publish(&raw)
            );
        }
        let mut absent = w;
        absent.as_object_mut().unwrap().remove("is_published");
        assert!(!Publication { workflow: &absent }.acknowledges_first_publish(&raw));
    }
    for prior in [json!("V1"), json!(false), json!(0), json!([]), json!({})] {
        let w = never_published(Some(prior));
        assert!(!Publication { workflow: &w }.acknowledges_first_publish(&raw));
    }
}

#[test]
fn first_publish_keeps_exact_raw_identity_and_shape_boundary() {
    let original = editor(never_published(None), Some(trigger()));
    let raw = published();
    assert!(original.publication().acknowledges_first_publish(&raw));
    for (key, value) in [
        ("id", json!("WfOTHER001")),
        ("app_id", json!("A_OTHER")),
        ("steps", json!({})),
        ("input_parameters", Value::Null),
        ("is_published", json!(false)),
        ("last_published_version_id", json!("")),
        ("unpublished_change_count", json!(-1)),
    ] {
        let mut changed = raw.clone();
        changed["decorated_workflow"][key] = value;
        assert!(
            !original.publication().acknowledges_first_publish(&changed),
            "{key}"
        );
        changed["decorated_workflow"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            !original.publication().acknowledges_first_publish(&changed),
            "missing {key}"
        );
    }
    for (key, value) in [
        ("ok", json!(false)),
        ("error", json!("not_allowed")),
        ("available_data", Value::Null),
    ] {
        let mut changed = raw.clone();
        changed[key] = value;
        assert!(
            !original.publication().acknowledges_first_publish(&changed),
            "{key}"
        );
    }
    for key in ["ok", "available_data", "decorated_workflow"] {
        let mut changed = raw.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert!(
            !original.publication().acknowledges_first_publish(&changed),
            "missing {key}"
        );
    }
}

#[test]
fn first_publish_readback_requires_actual_full_ack_workflow_and_trigger() {
    for prior in [None, Some(Value::Null), Some(json!(""))] {
        let original = editor(never_published(prior), Some(trigger()));
        let raw = published();
        let plan = FollowUpPlan::new(&primary(&original, raw.clone())).unwrap();
        let w = raw["decorated_workflow"].clone();
        let current = editor(w.clone(), Some(trigger()));
        assert_eq!(
            plan.publication_observation_first_publish(&current),
            Observation::Matches
        );
        assert_eq!(
            plan.publication_observation(&current),
            Observation::Unavailable
        );
        assert_eq!(
            plan.publication_observation_first_publish(&original),
            Observation::Different
        );
        for (key, value) in [
            ("is_published", json!(false)),
            ("last_published_version_id", json!("OTHER")),
            ("unpublished_change_count", json!(1)),
            ("title", json!("Drift")),
            ("steps", json!([])),
            ("opaque", json!({"keep":false})),
            ("date_updated", json!(9)),
        ] {
            let mut changed = w.clone();
            changed[key] = value;
            assert_eq!(
                plan.publication_observation_first_publish(&editor(changed, Some(trigger()))),
                Observation::Different,
                "{key}"
            );
        }
        let mut t = trigger();
        t["inputs"] = json!({"unexpected":{"value":true}});
        assert_eq!(
            plan.publication_observation_first_publish(&editor(w, Some(t))),
            Observation::Different
        );
    }
    let original = editor(workflow(), Some(trigger()));
    let raw = published();
    let plan = FollowUpPlan::new(&primary(&original, raw.clone())).unwrap();
    let current = editor(raw["decorated_workflow"].clone(), Some(trigger()));
    assert_eq!(plan.publication_observation(&current), Observation::Matches);
    assert_eq!(
        plan.publication_observation_first_publish(&current),
        Observation::Unavailable
    );
}

#[test]
fn first_publish_matching_readback_does_not_upgrade_unknown() {
    let mut t = trigger();
    t["inputs"] = json!({"remove":{"value":"old"}});
    let original = editor(never_published(None), Some(t.clone()));
    let mut unknown = MutationPlan::new(&original, Command::Published(true)).unwrap();
    unknown.mark_sent(0).unwrap();
    unknown.settle(0, Outcome::Unknown).unwrap();
    assert!(FollowUpPlan::new(&unknown).is_err());
    assert!(unknown.ready().is_empty());
    assert_eq!(unknown.slots()[0].outcome(), &Outcome::Unknown);
    let raw = published();
    let mut plan = FollowUpPlan::new(&primary(&original, raw.clone())).unwrap();
    t["inputs"] = plan.slots()[0].plan().arguments()["inputs"].clone();
    plan.mark_sent(0).unwrap();
    plan.settle(0, Outcome::Unknown).unwrap();
    let before = plan.clone();
    assert_eq!(
        plan.publication_observation_first_publish(&editor(
            raw["decorated_workflow"].clone(),
            Some(t)
        )),
        Observation::Matches
    );
    assert_eq!(plan, before);
    assert!(!plan.fully_succeeded());
    assert!(plan.ready().is_empty());
}

#[test]
fn first_publish_observation_cannot_reinterpret_unpublish_or_malformed_ack() {
    let original = editor(workflow(), Some(trigger()));
    let mut raw = published();
    raw["decorated_workflow"]["is_published"] = json!(false);
    let mut unpublish = MutationPlan::new(&original, Command::Published(false)).unwrap();
    unpublish.mark_sent(0).unwrap();
    unpublish
        .settle(0, Outcome::Succeeded(raw.clone()))
        .unwrap();
    let plan = FollowUpPlan::new(&unpublish).unwrap();
    let current = editor(raw["decorated_workflow"].clone(), Some(trigger()));
    assert_eq!(plan.publication_observation(&current), Observation::Matches);
    assert_eq!(
        plan.publication_observation_first_publish(&current),
        Observation::Unavailable
    );

    let original = editor(never_published(None), Some(trigger()));
    let mut raw = published();
    raw["decorated_workflow"]
        .as_object_mut()
        .unwrap()
        .remove("last_published_version_id");
    assert!(!original.publication().acknowledges_first_publish(&raw));
    let plan = FollowUpPlan::new(&primary(&original, raw)).unwrap();
    let current = editor(published()["decorated_workflow"].clone(), Some(trigger()));
    assert_eq!(
        plan.publication_observation_first_publish(&current),
        Observation::Unavailable
    );
}

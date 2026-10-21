use super::*;
mod first_publish;

fn workflow() -> Value {
    json!({"id":"WfREVIEW01","app_id":"A_REVIEW","title":"Original","description":"Keep","is_published":true,
        "last_published_version_id":"V1","unpublished_change_count":2,"input_parameters":{},"opaque":{"keep":true},
        "steps":[{"id":"STEP1","function":{"id":"FN_REVIEW","type":"builtin","input_parameters":[],
        "product_level_availability":{"is_available":true}},"inputs":{"message":{"value":"literal"}},"errors":[]}]})
}
fn trigger() -> Value {
    json!({"id":"TR_REVIEW","type":"shortcut","trigger_type_id":"TT_SHORTCUT","name":"Original","description":"Keep",
        "workflow":{"workflow_id":"WfREVIEW01","app_id":"A_REVIEW"},"inputs":{},"available_data":{}})
}
fn editor(workflow: Value, trigger: Option<Value>) -> Editor {
    let mut load = EditorLoad::new(workflow["id"].as_str().unwrap()).unwrap();
    load.accept(ReadKind::Latest,json!({"ok":true,"decorated_workflow":workflow,"available_data":{},"expression_data":{"opaque":7}})).unwrap();
    load.accept(ReadKind::Triggers,json!({"ok":true,"triggers":trigger.as_ref().map(|t|vec![json!({"id":t["id"]})]).unwrap_or_default()})).unwrap();
    if let Some(t) = trigger {
        load.accept(ReadKind::TriggerInfo, json!({"ok":true,"trigger":t}))
            .unwrap();
    }
    load.accept(
        ReadKind::Preferences,
        json!({"ok":true,"trippable_by_slack_connected_teams":{"allowed_teams":[]}}),
    )
    .unwrap();
    load.accept(
        ReadKind::CopyPermissions,
        json!({"ok":true,"permission_type":"everyone"}),
    )
    .unwrap();
    if load.responses().contains_key(&ReadKind::TriggerInfo) {
        load.accept(
            ReadKind::RunPermissions,
            json!({"ok":true,"permission_type":"everyone"}),
        )
        .unwrap();
    }
    load.editor().unwrap()
}
fn permissions() -> EditPermissions {
    EditPermissions {
        is_publish_restricted: false,
        apps: json!({}),
    }
}
fn observed(
    w: Value,
    t: Option<Value>,
    dynamic: Value,
    p: EditPermissions,
    past: Option<bool>,
) -> PublicationInputs {
    PublicationInputs::new(&editor(w, t), Some(&dynamic), Some(&p), past)
}
fn primary(original: &Editor, raw: Value) -> MutationPlan {
    let mut p = MutationPlan::new(original, Command::Published(true)).unwrap();
    p.mark_sent(0).unwrap();
    p.settle(0, Outcome::Succeeded(raw)).unwrap();
    p
}
fn published() -> Value {
    let mut w = workflow();
    w["last_published_version_id"] = json!("V2");
    w["unpublished_change_count"] = json!(0);
    w["date_updated"] = json!(2);
    json!({"ok":true,"decorated_workflow":w,"available_data":{},"expression_data":{"opaque":7},"opaque_receipt":["original"]})
}

#[test]
fn acquired_static_proof_is_bound_and_missing_facts_are_unavailable() {
    let original = editor(workflow(), Some(trigger()));
    let inputs = PublicationInputs::static_inputs(&original, &permissions()).unwrap();
    assert!(inputs.assess().unwrap().can_be_published);
    assert!(inputs.current(&original));
    let mut changed = workflow();
    changed["opaque"]["keep"] = json!(false);
    assert!(!inputs.current(&editor(changed, Some(trigger()))));
    let mut t = trigger();
    t["inputs"] = json!({"new":false});
    assert!(!inputs.current(&editor(workflow(), Some(t))));
    assert!(
        PublicationInputs::new(&original, None, Some(&permissions()), None)
            .assess()
            .is_err()
    );
    assert!(
        PublicationInputs::new(&original, Some(&json!({})), None, None)
            .assess()
            .is_err()
    );
}
#[test]
fn all_blocking_errors_preserve_preorder_and_original_opaque_values() {
    let mut w = workflow();
    w["steps"][0]["function"]["type"] = json!("app");
    w["steps"][0]["function"]["app_id"] = json!("APP");
    w["steps"][0]["errors"] =
        json!([{"code":"unknown-future","source_id":"wrong","opaque":[1,true]}]);
    w["steps"][0]["steps"] = json!([{"id":"CHILD","errors":[{"code":"child"}]}]);
    let p = EditPermissions {
        is_publish_restricted: false,
        apps: json!({"APP":{"appResolution":"denied"}}),
    };
    let a = observed(
        w,
        Some(trigger()),
        json!({"STEP1":{"x":{"error":{"code":"dynamic","source_id":"original","opaque":9}}}}),
        p,
        None,
    )
    .assess()
    .unwrap();
    assert_eq!(
        a.blocking_errors
            .iter()
            .map(|e| e["code"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "workflow_has_restricted_app_step",
            "dynamic",
            "unknown-future",
            "child"
        ]
    );
    assert_eq!(a.blocking_errors[1]["source_id"], "original");
    assert_eq!(a.blocking_errors[2]["source_id"], "STEP1");
    assert_eq!(a.blocking_errors[2]["opaque"], json!([1, true]));
    assert!(!a.can_be_published);
}
#[test]
fn source_only_includes_service_and_reaction_trigger_errors() {
    for (kind, subtype, count) in [
        ("shortcut", "", 0),
        ("webhook", "", 0),
        ("event", "other", 0),
        ("event", "slack#/events/reaction_added", 1),
        ("service", "", 2),
    ] {
        let mut t = trigger();
        t["type"] = json!(kind);
        t["subtype"] = json!(subtype);
        t["errors"] = json!([{"code":"server"}]);
        let a = observed(
            workflow(),
            Some(t),
            json!({"TR_REVIEW":{"p":{"error":{"code":"dynamic"}}}}),
            permissions(),
            None,
        )
        .assess()
        .unwrap();
        assert_eq!(a.blocking_errors.len(), count, "{kind}/{subtype}");
    }
}
#[test]
fn scheduled_past_fact_is_required_only_before_first_publication() {
    let mut w = workflow();
    w.as_object_mut()
        .unwrap()
        .remove("last_published_version_id");
    let mut t = trigger();
    t["type"] = json!("scheduled");
    assert!(
        observed(w.clone(), Some(t.clone()), json!({}), permissions(), None)
            .assess()
            .is_err()
    );
    let a = observed(w, Some(t.clone()), json!({}), permissions(), Some(true))
        .assess()
        .unwrap();
    assert_eq!(a.blocking_errors[0]["code"], "invalid_datetime");
    assert!(
        observed(workflow(), Some(t), json!({}), permissions(), None)
            .assess()
            .unwrap()
            .blocking_errors
            .is_empty()
    );
}
#[test]
fn publication_predicate_keeps_all_four_independent_conditions() {
    for kind in 0..4 {
        let mut w = workflow();
        let mut p = permissions();
        match kind {
            0 => w["steps"] = json!([]),
            1 => w["steps"][0]["errors"] = json!([{"code":"failure"}]),
            2 => p.is_publish_restricted = true,
            _ => w["unpublished_change_count"] = json!(0),
        }
        assert!(
            !observed(w, Some(trigger()), json!({}), p, None)
                .assess()
                .unwrap()
                .can_be_published
        );
    }
    let mut w = workflow();
    w["is_published"] = json!(false);
    w["unpublished_change_count"] = json!(0);
    assert!(
        observed(w, Some(trigger()), json!({}), permissions(), None)
            .assess()
            .unwrap()
            .can_be_published
    );
}
#[test]
fn product_availability_is_recursive_and_separate_from_publishability() {
    let mut w = workflow();
    w["steps"][0]["function"]["product_level_availability"] =
        json!({"is_available":false,"available_to":"std"});
    w["steps"][0]["steps"] = json!([{"id":"CHILD","function":{"product_level_availability":{"available_to":"enterprise"}}}]);
    let a = observed(w, Some(trigger()), json!({}), permissions(), None)
        .assess()
        .unwrap();
    assert!(a.can_be_published);
    assert_eq!(a.unavailable_steps.len(), 2);
    assert_eq!(a.unavailable_steps[0]["id"], "CHILD");
}
#[test]
fn malformed_count_and_unrecognized_workflow_do_not_gain_admission() {
    let mut w = workflow();
    w["unpublished_change_count"] = json!({});
    assert!(
        observed(w, Some(trigger()), json!({}), permissions(), None)
            .assess()
            .is_err()
    );
    let mut w = workflow();
    w["id"] = json!("WF_REVIEW");
    let mut t = trigger();
    t["workflow"]["workflow_id"] = w["id"].clone();
    assert!(
        !observed(w, Some(t), json!({}), permissions(), None)
            .assess()
            .unwrap()
            .has_steps
    );
}
#[test]
fn static_proof_refuses_dynamic_wrapped_knowledge_reference_and_channel_demands() {
    for kind in 0..7 {
        let mut w = workflow();
        match kind {
            0 => {
                w["steps"][0]["function"]["input_parameters"] =
                    json!([{"name":"x","nested":{"dynamic_options":{}}}])
            }
            1 => w["steps"][0]["wrapped_function"] = json!({}),
            2 => w["steps"][0]["function"]["id"] = json!("Fn0163"),
            3 => w["steps"][0]["inputs"]["message"]["value"] = json!("{{data.x}}"),
            4 => w["steps"][0]["function"]["type"] = json!("app"),
            5 => {
                w["steps"][0]["function"]
                    .as_object_mut()
                    .unwrap()
                    .remove("input_parameters");
            }
            _ => w["steps"][0]["function"]["callback_id"] = json!("send_message"),
        }
        assert!(
            PublicationInputs::static_inputs(&editor(w, Some(trigger())), &permissions()).is_err(),
            "{kind}"
        );
    }
    let mut t = trigger();
    t["type"] = json!("service");
    assert!(
        PublicationInputs::static_inputs(&editor(workflow(), Some(t)), &permissions()).is_err()
    );
}
#[test]
fn declarations_are_not_consuming_references() {
    let mut t = trigger();
    t["available_data"] = json!({"{{data.channel}}":{"name":"channel"}});
    t["inputs"] = json!({"custom":false,"keep":{"value":"literal","opaque":[1,true]}});
    assert!(PublicationInputs::static_inputs(&editor(workflow(), Some(t)), &permissions()).is_ok());
}
#[test]
fn trigger_types_factory_preserves_raw_nullish_and_missing_match_defaults() {
    let request = TriggerTypes::request(None).unwrap();
    assert_eq!(request.method(), "workflows.triggers.types.list");
    assert_eq!(request.arguments(), &json!({}));
    assert_eq!(request.reason(), "fetching-workflow-trigger-types-list-wfb");
    assert_eq!(
        TriggerTypes::request(Some("A_REVIEW")).unwrap().arguments(),
        &json!({"app_id":"A_REVIEW"})
    );
    assert!(TriggerTypes::request(Some("")).is_err());
    for available in [json!(true), json!(false), Value::Null] {
        let raw = json!({"ok":true,"trigger_types":[{"trigger_type":{"id":"TT_SHORTCUT"},"is_available_to_user":available,"opaque":[1]}]});
        let types = TriggerTypes::decode(raw.clone()).unwrap();
        assert_eq!(types.raw(), &raw);
        assert_eq!(
            types.availability(Some(&trigger())).unwrap(),
            available != json!(false)
        );
        assert!(
            types
                .availability(Some(&json!({"trigger_type_id":"missing"})))
                .unwrap()
        );
    }
    assert!(TriggerTypes::decode(json!({"ok":true})).is_err());
    assert!(TriggerTypes::decode(json!({"ok":true,"trigger_types":[{"trigger_type":{"id":"TT"},"is_available_to_user":{}}]})).is_err());
}
#[test]
fn published_readback_cannot_match_the_old_published_version_or_content_drift() {
    let original = editor(workflow(), Some(trigger()));
    let receipt = published();
    assert!(original.publication().acknowledges_publish(&receipt));
    let plan = FollowUpPlan::new(&primary(&original, receipt.clone())).unwrap();
    assert_eq!(
        plan.publication_observation(&original),
        Observation::Different
    );
    let w = receipt["decorated_workflow"].clone();
    assert_eq!(
        plan.publication_observation(&editor(w.clone(), Some(trigger()))),
        Observation::Matches
    );
    for key in [
        "last_published_version_id",
        "unpublished_change_count",
        "title",
        "opaque",
        "steps",
        "date_updated",
    ] {
        let mut changed = w.clone();
        changed[key] = workflow()[key].clone();
        if changed == w {
            changed[key] = json!({"drift":true});
        }
        // Keep step array structurally valid for actual Editor acquisition.
        if key == "steps" {
            changed[key] = json!([]);
        }
        assert_eq!(
            plan.publication_observation(&editor(changed, Some(trigger()))),
            Observation::Different,
            "{key}"
        );
    }
}
#[test]
fn malformed_or_unchanged_publish_reply_never_resolves() {
    let original = editor(workflow(), Some(trigger()));
    for key in [
        "last_published_version_id",
        "unpublished_change_count",
        "is_published",
    ] {
        let mut raw = published();
        raw["decorated_workflow"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(!original.publication().acknowledges_publish(&raw));
        let plan = FollowUpPlan::new(&primary(&original, raw.clone())).unwrap();
        assert_eq!(
            plan.publication_observation(&editor(
                published()["decorated_workflow"].clone(),
                Some(trigger())
            )),
            Observation::Unavailable
        );
    }
    let raw = json!({"ok":true,"decorated_workflow":workflow(),"available_data":{}});
    assert!(!original.publication().acknowledges_publish(&raw));
}
#[test]
fn matching_publication_readback_does_not_upgrade_unknown_sync() {
    let mut t = trigger();
    t["inputs"] = json!({"remove":{"value":"old"}});
    let original = editor(workflow(), Some(t.clone()));
    let receipt = published();
    let mut plan = FollowUpPlan::new(&primary(&original, receipt.clone())).unwrap();
    let desired = plan.slots()[0].plan().arguments()["inputs"].clone();
    plan.mark_sent(0).unwrap();
    plan.settle(0, Outcome::Unknown).unwrap();
    t["inputs"] = desired;
    let before = plan.clone();
    assert_eq!(
        plan.publication_observation(&editor(receipt["decorated_workflow"].clone(), Some(t))),
        Observation::Matches
    );
    assert_eq!(plan, before);
    assert!(!plan.fully_succeeded());
    assert!(plan.ready().is_empty());
}

fn unpublished_workflow() -> Value {
    let mut w = workflow();
    w["is_published"] = json!(false);
    w["unpublished_change_count"] = json!(0);
    w
}

// Constructed compatibility vectors, not observed Slack endpoint responses.
#[test]
fn republish_same_or_new_version_requires_full_positive_readback() {
    let original = editor(unpublished_workflow(), Some(trigger()));
    for version in ["V1", "V2"] {
        let mut receipt = published();
        receipt["decorated_workflow"]["last_published_version_id"] = json!(version);
        assert!(original.publication().acknowledges_publish(&receipt));
        let primary = primary(&original, receipt.clone());
        assert_eq!(
            primary.slots()[0].outcome(),
            &Outcome::Succeeded(receipt.clone())
        );
        let plan = FollowUpPlan::new(&primary).unwrap();
        assert_eq!(
            plan.publication_observation(&original),
            Observation::Different
        );
        let w = receipt["decorated_workflow"].clone();
        assert_eq!(
            plan.publication_observation(&editor(w.clone(), Some(trigger()))),
            Observation::Matches
        );
        for (key, value) in [
            ("is_published", json!(false)),
            ("last_published_version_id", json!("OTHER_VERSION")),
            ("unpublished_change_count", json!(1)),
            ("title", json!("Unrelated drift")),
            ("opaque", json!({"keep":false})),
            ("steps", json!([])),
            ("date_updated", json!(3)),
        ] {
            let mut changed = w.clone();
            changed[key] = value;
            assert_eq!(
                plan.publication_observation(&editor(changed, Some(trigger()))),
                Observation::Different,
                "{version}/{key}"
            );
        }
    }
}

#[test]
fn republish_zero_changes_static_assessment_is_already_supported() {
    let original = editor(unpublished_workflow(), Some(trigger()));
    let inputs = PublicationInputs::static_inputs(&original, &permissions()).unwrap();
    let assessment = inputs.assess().unwrap();
    assert!(assessment.can_be_published);
    assert!(!assessment.unchanged_published);
    assert!(assessment.blocking_errors.is_empty());
    assert!(assessment.unavailable_steps.is_empty());
    let mut w = unpublished_workflow();
    w["is_published"] = json!(true);
    let current = editor(w, Some(trigger()));
    assert!(!inputs.current(&current));
    assert!(
        !PublicationInputs::static_inputs(&current, &permissions())
            .unwrap()
            .assess()
            .unwrap()
            .can_be_published
    );
}

#[test]
fn republish_ack_does_not_accept_missing_or_coerced_original_facts() {
    for version in ["V1", "V2"] {
        let mut receipt = published();
        receipt["decorated_workflow"]["last_published_version_id"] = json!(version);
        for value in [Value::Null, json!(0), json!("false"), json!({})] {
            let mut w = unpublished_workflow();
            w["is_published"] = value;
            assert!(!Publication { workflow: &w }.acknowledges_publish(&receipt));
        }
        let mut w = unpublished_workflow();
        w.as_object_mut().unwrap().remove("is_published");
        assert!(!Publication { workflow: &w }.acknowledges_publish(&receipt));
        for value in [Value::Null, json!(""), json!(7), json!(false)] {
            let mut w = unpublished_workflow();
            w["last_published_version_id"] = value;
            assert!(!Publication { workflow: &w }.acknowledges_publish(&receipt));
        }
        let mut w = unpublished_workflow();
        w.as_object_mut()
            .unwrap()
            .remove("last_published_version_id");
        assert!(!Publication { workflow: &w }.acknowledges_publish(&receipt));
        assert_eq!(
            Publication {
                workflow: &workflow()
            }
            .acknowledges_publish(&receipt),
            version != "V1"
        );
    }
}

#[test]
fn republish_same_version_keeps_raw_identity_and_shape_guards() {
    let original = editor(unpublished_workflow(), Some(trigger()));
    let mut receipt = published();
    receipt["decorated_workflow"]["last_published_version_id"] = json!("V1");
    for (key, value) in [
        ("id", json!("WfOTHER001")),
        ("app_id", json!("A_OTHER")),
        ("steps", json!({})),
        ("input_parameters", Value::Null),
        ("is_published", json!(false)),
        ("last_published_version_id", json!("")),
        ("unpublished_change_count", json!(-1)),
    ] {
        let mut raw = receipt.clone();
        raw["decorated_workflow"][key] = value;
        assert!(!original.publication().acknowledges_publish(&raw), "{key}");
        raw["decorated_workflow"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            !original.publication().acknowledges_publish(&raw),
            "missing {key}"
        );
    }
    for (key, value) in [
        ("ok", json!(false)),
        ("error", json!("not_allowed")),
        ("available_data", Value::Null),
    ] {
        let mut raw = receipt.clone();
        raw[key] = value;
        assert!(!original.publication().acknowledges_publish(&raw), "{key}");
    }
}

#[test]
fn republish_unknown_primary_never_authorizes_followup() {
    let original = editor(unpublished_workflow(), Some(trigger()));
    let mut plan = MutationPlan::new(&original, Command::Published(true)).unwrap();
    plan.mark_sent(0).unwrap();
    plan.settle(0, Outcome::Unknown).unwrap();
    let mut raw = published();
    raw["decorated_workflow"]["last_published_version_id"] = json!("V1");
    let current = editor(raw["decorated_workflow"].clone(), Some(trigger()));
    assert!(current.publication().is_published());
    let before = plan.clone();
    assert!(FollowUpPlan::new(&plan).is_err());
    assert!(!plan.primary_succeeded());
    assert!(plan.ready().is_empty());
    assert_eq!(plan, before);
    assert_eq!(plan.slots()[0].outcome(), &Outcome::Unknown);
}

#[test]
fn republish_matching_readback_does_not_upgrade_unknown_sync() {
    let mut t = trigger();
    t["inputs"] = json!({"remove":{"value":"old"}});
    let original = editor(unpublished_workflow(), Some(t.clone()));
    let mut raw = published();
    raw["decorated_workflow"]["last_published_version_id"] = json!("V1");
    let mut plan = FollowUpPlan::new(&primary(&original, raw.clone())).unwrap();
    t["inputs"] = plan.slots()[0].plan().arguments()["inputs"].clone();
    plan.mark_sent(0).unwrap();
    plan.settle(0, Outcome::Unknown).unwrap();
    let before = plan.clone();
    assert_eq!(
        plan.publication_observation(&editor(raw["decorated_workflow"].clone(), Some(t))),
        Observation::Matches
    );
    assert!(!plan.fully_succeeded());
    assert!(plan.ready().is_empty());
    assert_eq!(plan, before);
}

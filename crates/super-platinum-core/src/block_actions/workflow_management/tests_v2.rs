use super::*;

fn acquired(latest: Value, trigger: Option<Value>, envelope: &str) -> Editor {
    let mut load = EditorLoad::new("WF1").unwrap();
    load.accept(ReadKind::Latest, latest).unwrap();
    load.accept(ReadKind::Triggers,json!({"ok":true,"triggers":trigger.as_ref().map(|v|vec![json!({"id":v["id"]})]).unwrap_or_default(),"request_id":envelope})).unwrap();
    if let Some(trigger) = trigger {
        load.accept(
            ReadKind::TriggerInfo,
            json!({"ok":true,"trigger":trigger,"request_id":envelope}),
        )
        .unwrap();
    }
    load.accept(ReadKind::Preferences,json!({"ok":true,"trippable_by_slack_connected_teams":{"allowed_teams":[]},"request_id":envelope})).unwrap();
    load.accept(
        ReadKind::CopyPermissions,
        json!({"ok":true,"permission_type":"everyone","request_id":envelope}),
    )
    .unwrap();
    if load.responses().contains_key(&ReadKind::TriggerInfo) {
        load.accept(
            ReadKind::RunPermissions,
            json!({"ok":true,"permission_type":"everyone","request_id":envelope}),
        )
        .unwrap();
    }
    load.editor().unwrap()
}
fn latest(steps: Value) -> Value {
    json!({"ok":true,"decorated_workflow":{"id":"WF1","app_id":"A1","steps":steps,"input_parameters":{},"title":"Before","description":"Original","is_published":false},"available_data":{},"expression_data":{"opaque":42}})
}
fn shortcut() -> Value {
    json!({"id":"TR1","type":"shortcut","name":"Before","description":"Original","workflow":{"workflow_id":"WF1","app_id":"A1"},"inputs":{},"available_data":{}})
}
fn acknowledged(editor: &Editor, command: Command, raw: Value) -> MutationPlan {
    let mut primary = MutationPlan::new(editor, command).unwrap();
    primary.mark_sent(0).unwrap();
    primary.settle(0, Outcome::Succeeded(raw)).unwrap();
    primary
}

#[test]
fn actual_retained_trigger_roundtrips_match_all_golden_wire_arguments() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("trigger_cases.json")).unwrap();
    assert_eq!(cases.len(), 10);
    for case in cases {
        let prepared = TriggerUpdate::capture("WF1", &case["raw"]).unwrap();
        assert_eq!(
            prepared.original(),
            &case["raw"],
            "raw input must remain exact"
        );
        assert_eq!(prepared.arguments(), &case["expected"], "{}", case["label"]);
    }
}
#[test]
fn revision_ignores_envelope_ids_but_preserves_planning_and_unknown_data() {
    let mut before = latest(json!([]));
    before["request_id"] = json!("one");
    before["response_metadata"] = json!({"request_id":"one"});
    let original = acquired(before.clone(), Some(shortcut()), "one");
    before["request_id"] = json!("two");
    before["response_metadata"] = json!({"request_id":"two"});
    let current = acquired(before.clone(), Some(shortcut()), "two");
    assert!(original.same_revision(&current));
    for key in ["available_data", "expression_data"] {
        let mut changed = before.clone();
        changed[key] = json!({"future":77});
        assert!(!original.same_revision(&acquired(changed, Some(shortcut()), "two")));
    }
    before["decorated_workflow"]["future"] = json!(true);
    assert!(!original.same_revision(&acquired(before, Some(shortcut()), "two")));
}
#[test]
fn contradictory_success_never_becomes_a_mutation_receipt() {
    let editor = acquired(latest(json!([])), None, "r");
    for error in [json!(false), json!(""), json!({}), json!([])] {
        let mut plan =
            MutationPlan::new(&editor, Command::CopyPermission("collaborators".into())).unwrap();
        plan.mark_sent(0).unwrap();
        assert!(
            plan.settle(0, Outcome::Succeeded(json!({"ok":true,"error":error})))
                .is_err()
        );
        assert_eq!(plan.slots()[0].outcome(), &Outcome::InFlight);
    }
}
#[test]
fn publish_sync_uses_actual_new_inputs_and_unknown_blocks_progress() {
    let mut trigger = shortcut();
    trigger["available_data"] = json!({"{{data.user}}":{"type":"slack#/types/user_id","name":"user","title":"User","is_customizable":false}});
    let editor = acquired(latest(json!([])), Some(trigger.clone()), "r");
    let mut response = latest(json!([]));
    response["decorated_workflow"]["is_published"] = json!(true);
    response["decorated_workflow"]["input_parameters"] =
        json!({"TR1__user":{"type":"slack#/types/user_id"}});
    let primary = acknowledged(&editor, Command::Published(true), response.clone());
    let mut next = FollowUpPlan::new(&primary).unwrap();
    assert_eq!(next.phase(), FollowUpPhase::TriggerInputSynchronization);
    assert_eq!(next.current_workflow(), &response["decorated_workflow"]);
    assert_eq!(
        next.slots()[0].plan().arguments()["inputs"]["TR1__user"],
        json!({"value":"{{data.user}}"})
    );
    next.mark_sent(0).unwrap();
    next.settle(0, Outcome::Unknown).unwrap();
    let retained = next.clone();
    assert!(next.advance().is_err());
    assert_eq!(next, retained);
    assert!(next.ready().is_empty());
    assert!(!next.fully_succeeded());
}
#[test]
fn failed_restriction_is_retained_and_never_reports_complete_success() {
    let editor = acquired(latest(json!([])), Some(shortcut()), "r");
    let primary = acknowledged(
        &editor,
        Command::RunPermission {
            permission_type: "everyone".into(),
            user_ids: None,
            channel_ids: None,
            team_ids: None,
            org_ids: None,
        },
        json!({"ok":true}),
    );
    let mut next = FollowUpPlan::new(&primary).unwrap();
    assert_eq!(next.phase(), FollowUpPhase::Restriction);
    next.mark_sent(0).unwrap();
    let denied = json!({"ok":false,"error":"not_allowed","opaque":9});
    next.settle(0, Outcome::Rejected(denied.clone())).unwrap();
    next.advance().unwrap();
    assert_eq!(next.phase(), FollowUpPhase::Complete);
    assert!(!next.fully_succeeded());
    assert_eq!(
        next.stages()[0].slots()[0].outcome(),
        &Outcome::Rejected(denied)
    );
}
#[test]
fn observation_never_converts_an_unknown_send_into_acknowledgement() {
    let editor = acquired(latest(json!([])), None, "r");
    let mut plan = MutationPlan::new(&editor, Command::CopyPermission("everyone".into())).unwrap();
    plan.mark_sent(0).unwrap();
    plan.settle(0, Outcome::Unknown).unwrap();
    assert_eq!(plan.observe(&editor), Observation::Matches);
    assert_eq!(plan.slots()[0].outcome(), &Outcome::Unknown);
    assert!(!plan.primary_succeeded());
}
#[test]
fn connector_visibility_requires_actual_observation_and_preserves_hidden_policy() {
    let apps=ConnectorApps::decode(json!({"ok":true,"is_aaa_enabled":true,"apps":[{"id":"A9","app_resolution":"pending"}],"opaque":7})).unwrap();
    let workflow = json!({"steps":[{"id":"S1","function":{"app_id":"A9"}}]});
    assert!(apps.permissions(&workflow, None).is_err());
    assert!(
        !apps
            .permissions(&workflow, Some(&json!("hidden")))
            .unwrap()
            .is_publish_restricted
    );
    assert!(
        apps.permissions(&workflow, Some(&Value::Null))
            .unwrap()
            .is_publish_restricted
    );
    assert_eq!(apps.raw()["opaque"], 7);
}

fn context_primary(user_context: bool) -> (MutationPlan, Value) {
    let parameters = if user_context {
        json!([{"name":"interaction","type":"slack#/types/interactivity"},{"name":"user","type":"slack#/types/user_context"}])
    } else {
        json!([{"name":"interaction","type":"slack#/types/interactivity"}])
    };
    let steps = json!([
        {"id":"S1","function":{"id":"Fn1","input_parameters":parameters},"inputs":{}},
        {"id":"S2","function":{"id":"Fn2","input_parameters":[{"name":"interaction","type":"slack#/types/interactivity"}]},"inputs":{}}
    ]);
    let mut raw = latest(steps);
    raw["available_data"] = json!({"{{steps.S1.interaction}}":{"name":"interaction","type":"slack#/types/interactivity","source":"steps","source_id":"S1"}});
    let mut trigger = shortcut();
    trigger["available_data"] = json!({"{{data.interaction}}":{"name":"interaction","type":"slack#/types/interactivity"},"{{data.user}}":{"name":"user","type":"slack#/types/user_context"}});
    let editor = acquired(raw.clone(), Some(trigger), "r");
    let primary = acknowledged(
        &editor,
        Command::Step(StepAction::Move {
            step_id: "S2".into(),
            after_step_id: None,
        }),
        raw.clone(),
    );
    (primary, raw)
}
#[test]
fn context_receipts_fold_in_request_order_not_completion_order() {
    let (primary, raw) = context_primary(false);
    let mut next = FollowUpPlan::new(&primary).unwrap();
    assert_eq!(next.phase(), FollowUpPhase::Interactivity);
    assert_eq!(next.ready(), [0, 1]);
    assert_eq!(
        next.slots()[0].plan().arguments()["step"]["inputs"]["interaction"]["value"],
        "{{inputs.TR1__interaction}}"
    );
    assert_eq!(
        next.slots()[1].plan().arguments()["step"]["inputs"]["interaction"]["value"],
        "{{steps.S1.interaction}}"
    );
    next.mark_sent(0).unwrap();
    next.mark_sent(1).unwrap();
    let mut first = raw.clone();
    first["decorated_workflow"]["opaque_revision"] = json!("first");
    let mut second = raw;
    second["decorated_workflow"]["opaque_revision"] = json!("second");
    second["expression_data"] = json!({"actual":"second"});
    next.settle(1, Outcome::Succeeded(second.clone())).unwrap();
    next.settle(0, Outcome::Succeeded(first)).unwrap();
    next.advance().unwrap();
    assert!(next.fully_succeeded());
    assert_eq!(next.current_workflow(), &second["decorated_workflow"]);
    assert_eq!(next.current_expression_data(), Some(&json!({"opaque":42})));
    assert_eq!(
        next.stages()[0].slots()[1].outcome(),
        &Outcome::Succeeded(second)
    );
}
#[test]
fn invalid_next_stage_leaves_acknowledged_current_stage_and_raw_receipts_intact() {
    let (primary, raw) = context_primary(true);
    let mut next = FollowUpPlan::new(&primary).unwrap();
    next.mark_sent(0).unwrap();
    next.mark_sent(1).unwrap();
    next.settle(0, Outcome::Succeeded(raw.clone())).unwrap();
    let mut malformed = raw;
    malformed["decorated_workflow"]["steps"][0]["inputs"] = json!([]);
    next.settle(1, Outcome::Succeeded(malformed)).unwrap();
    let retained = next.clone();
    assert!(next.advance().is_err());
    assert_eq!(next, retained);
    assert_eq!(next.phase(), FollowUpPhase::Interactivity);
    assert!(!next.fully_succeeded());
}

mod publication_readback_tests;

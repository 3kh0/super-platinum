use super::*;

fn editor(trigger: bool) -> Editor {
    let mut load = EditorLoad::new("WF1").unwrap();
    load.accept(ReadKind::Latest,json!({"ok":true,"available_data":{"unknown":[7]},"expression_data":{"retained":true},
        "decorated_workflow":{"id":"WF1","app_id":"A1","title":"Before","unknown":{"nested":42},
        "steps":[{"id":"S1","function":{"id":"Fn1"},"inputs":{"CamelUserKey":{"value":{"UserPayload":9}}},"unknown":73}]}})).unwrap();
    load.accept(ReadKind::Triggers,json!({"ok":true,"triggers":if trigger {json!([{"id":"TR1"},{"id":"TR2"}])}else{json!([])},"opaque":"list"})).unwrap();
    if trigger {
        load.accept(ReadKind::TriggerInfo,json!({"ok":true,"trigger":{"id":"TR1","type":"scheduled","workflow":{"workflow_id":"WF1","app_id":"A1"},"inputs":{},"unknown":"kept"}})).unwrap();
    }
    load.accept(
        ReadKind::Preferences,
        json!({"ok":true,"trippable_by_slack_connected_teams":{"allowed_teams":["T1"],"future":7}}),
    )
    .unwrap();
    load.accept(
        ReadKind::CopyPermissions,
        json!({"ok":true,"permission_type":"future_permission","future":"kept"}),
    )
    .unwrap();
    if trigger {
        load.accept(
            ReadKind::RunPermissions,
            json!({"ok":true,"permission_type":"named_entities","user_ids":["U1"],"future":8}),
        )
        .unwrap();
    }
    load.editor().unwrap()
}
fn page(query: ListQuery, workflows: Value, cursor: &str) -> ListPage {
    let workflows=ListWorkflows::decode(query,json!({"ok":true,"workflows":workflows,"response_metadata":{"next_cursor":cursor},"future":{"retained":true}})).unwrap();
    ListPage::decode(
        workflows,
        json!({"ok":true,"triggers":[{"id":"TR1","workflow":{"app":{"id":"A1"}},"opaque":42}]}),
    )
    .unwrap()
}
#[test]
fn acquisition_rejects_wrong_order_cross_workflow_and_duplicate_receipts() {
    let mut load = EditorLoad::new("WF1").unwrap();
    assert_eq!(
        load.requests().iter().map(|r| r.kind).collect::<Vec<_>>(),
        [ReadKind::Restriction, ReadKind::Latest]
    );
    assert!(
        load.accept(ReadKind::Preferences, json!({"ok":true}))
            .is_err()
    );
    let raw = json!({"ok":true,"decorated_workflow":{"id":"WF2","app_id":"A1","steps":[]}});
    assert!(load.accept(ReadKind::Latest, raw).is_err());
    load.accept(
        ReadKind::Latest,
        json!({"ok":true,"decorated_workflow":{"id":"WF1","app_id":"A1","steps":[]}}),
    )
    .unwrap();
    assert!(load.accept(ReadKind::Latest, json!({"ok":true})).is_err());
    load.accept(
        ReadKind::Triggers,
        json!({"ok":true,"triggers":[{"id":"TR1"},{"id":"TR2"}]}),
    )
    .unwrap();
    assert_eq!(
        load.requests().last().unwrap().plan.arguments(),
        &json!({"trigger_id":"TR1"})
    );
    assert!(
        load.accept(
            ReadKind::TriggerInfo,
            json!({"ok":true,"trigger":{"id":"TR2","workflow":{"workflow_id":"WF1","app_id":"A1"}}})
        )
        .is_err()
    );
    assert!(load.editor().is_err());
}
#[test]
fn raw_editor_data_and_unknown_permissions_survive_without_becoming_capabilities() {
    let editor = editor(true);
    assert_eq!(editor.raw_workflow()["unknown"]["nested"], 42);
    assert_eq!(editor.steps()[0]["unknown"], 73);
    assert_eq!(
        editor.latest_response()["available_data"]["unknown"],
        json!([7])
    );
    assert_eq!(editor.trigger().unwrap()["unknown"], "kept");
    assert_eq!(
        editor.copy_permissions()["permission_type"],
        "future_permission"
    );
    assert_eq!(
        editor.external_preferences()["trippable_by_slack_connected_teams"]["future"],
        7
    );
    assert_eq!(editor.run_permissions().unwrap()["future"], 8);
    assert!(editor.restriction().is_none());
}
#[test]
fn editor_collaborators_and_unavailable_nested_steps_are_independent_facts() {
    let mut original = editor(true).responses().clone();
    let latest = original.get_mut(&ReadKind::Latest).unwrap();
    latest["decorated_workflow"]["collaborators"] = json!(["U1"]);
    latest["decorated_workflow"]["steps"] = json!([{"id":"S1","function":{"product_level_availability":{"is_available":true}},"steps":[{"id":"S2","function":{}}]}]);
    let mut load = EditorLoad::new("WF1").unwrap();
    while let Some(kind) = load
        .requests()
        .iter()
        .map(|r| r.kind)
        .find(|kind| original.contains_key(kind))
    {
        load.accept(kind, original.remove(&kind).unwrap()).unwrap();
    }
    let editor = load.editor().unwrap();
    assert!(editor.is_collaborator("U1"));
    assert!(!editor.is_collaborator("U2"));
    assert!(editor.has_unavailable_steps());
    assert!(editor.restriction().is_none());
}
#[test]
fn managed_list_contract_binds_join_and_cursor_without_losing_raw_rows() {
    let query = ListQuery {
        query: "hello world".into(),
        published: Some(false),
        sort: Sort::Ascending,
        cursor: None,
    };
    assert_eq!(
        query.request().unwrap().arguments(),
        &json!({"limit":50,"filter_options":{"name":"hello world","is_published":false},"sort_options":{"column":"date_updated","direction":"asc"},"workflow_builder_only":true})
    );
    let first = page(
        query.clone(),
        json!([{"id":"WF1","app_id":"A1","is_empty":true,"unknown":1},{"id":"WF2","app_id":"A1","unknown":2}]),
        "cursor1",
    );
    assert_eq!(first.rows()[1].trigger().unwrap()["id"], "TR1");
    let mut list = List::default();
    list.append(first).unwrap();
    let next = list.next_query().unwrap();
    assert_eq!(next.cursor.as_deref(), Some("cursor1"));
    list.append(page(next,json!([{"id":"WF1","app_id":"A1","is_empty":false},{"id":"WF2","app_id":"A1","unknown":99},{"id":"WF3","app_id":"A3"}]),"")).unwrap();
    let rows = list.visible_rows();
    assert_eq!(
        rows.iter().map(|r| r.id()).collect::<Vec<_>>(),
        ["WF2", "WF3"]
    );
    assert_eq!(rows[0].raw()["unknown"], 2);
    assert_eq!(list.pages()[0].raw_workflows()["future"]["retained"], true);
    assert!(list.next_query().is_none());
    assert!(list.append(page(query, json!([]), "")).is_err());
}
#[test]
fn empty_lists_still_acquire_triggers_and_bad_continuations_do_not_advance() {
    let workflows =
        ListWorkflows::decode(ListQuery::default(), json!({"ok":true,"workflows":[]})).unwrap();
    assert_eq!(
        workflows.triggers_request().arguments(),
        &json!({"app_ids":""})
    );
    let q = ListQuery {
        cursor: Some("loop".into()),
        ..Default::default()
    };
    assert!(
        ListWorkflows::decode(
            q,
            json!({"ok":true,"workflows":[],"response_metadata":{"next_cursor":"loop"}})
        )
        .is_err()
    );
    let mut list = List::default();
    list.append(page(ListQuery::default(), json!([]), "next"))
        .unwrap();
    let wrong = ListQuery {
        query: "changed".into(),
        cursor: Some("next".into()),
        ..Default::default()
    };
    assert!(list.append(page(wrong, json!([]), "")).is_err());
    assert_eq!(list.pages().len(), 1);
}
#[test]
fn app_resolution_uses_flat_app_identity_and_preserves_actual_trigger() {
    let raw = json!({"ok":true,"triggers":[{"id":"WRONG","workflow":{"app":{"id":"A1"},"workflow_id":"WF0"}},
        {"id":"TR1","workflow":{"app_id":"A1","workflow_id":"WF1"},"unknown":9}]});
    let resolved = AppWorkflow::decode("A1", raw.clone()).unwrap();
    assert_eq!(resolved.workflow_id(), "WF1");
    assert_eq!(resolved.raw(), &raw);
    assert_eq!(resolved.trigger()["unknown"], 9);
    assert!(AppWorkflow::decode("A2", raw).is_err());
    assert!(resolve_app("../bad").is_err());
}
#[test]
fn delete_never_grants_second_request_after_failure_unknown_or_duplicate_receipt() {
    for outcome in [
        Outcome::Unknown,
        Outcome::NotSent,
        Outcome::Rejected(json!({"ok":false,"error":"denied","raw":"retained"})),
    ] {
        let mut plan = MutationPlan::new(&editor(true), Command::Delete).unwrap();
        assert_eq!(plan.ready(), [0]);
        assert!(plan.mark_sent(1).is_err());
        plan.mark_sent(0).unwrap();
        assert!(plan.mark_sent(0).is_err());
        plan.settle(0, outcome.clone()).unwrap();
        assert!(plan.ready().is_empty());
        assert_eq!(plan.slots()[0].outcome(), &outcome);
        assert!(
            plan.settle(0, Outcome::Succeeded(json!({"ok":true})))
                .is_err()
        );
        assert!(!plan.primary_succeeded());
    }
}
#[test]
fn delete_retains_trigger_success_and_workflow_unknown_with_original_input() {
    let original = editor(true);
    let mut plan = MutationPlan::new(&original, Command::Delete).unwrap();
    plan.mark_sent(0).unwrap();
    let receipt = json!({"ok":true,"opaque":"trigger deleted"});
    plan.settle(0, Outcome::Succeeded(receipt.clone())).unwrap();
    assert_eq!(plan.ready(), [1]);
    plan.mark_sent(1).unwrap();
    plan.settle(1, Outcome::Unknown).unwrap();
    assert_eq!(plan.original(), &original);
    assert_eq!(plan.command(), &Command::Delete);
    assert_eq!(plan.slots()[0].outcome(), &Outcome::Succeeded(receipt));
    assert!(!plan.primary_succeeded());
    let triggerless = MutationPlan::new(&editor(false), Command::Delete).unwrap();
    assert_eq!(triggerless.slots().len(), 1);
    assert_eq!(triggerless.ready(), [0]);
    assert_eq!(
        triggerless.slots()[0].plan().method(),
        "functions.workflows.delete"
    );
}
#[test]
fn metadata_keeps_two_concurrent_results_and_rejects_wrong_trigger_authority() {
    let command = Command::Metadata {
        name: "After".into(),
        description: "New".into(),
        icon_url: None,
        trigger_update: Some(
            json!({"trigger_id":"TR1","workflow":"WF1","workflow_app_id":"A1","type":"scheduled","inputs":{},"name":"After","description":"New","schedule":{"unknown":7}}),
        ),
    };
    let mut plan = MutationPlan::new(&editor(true), command.clone()).unwrap();
    assert_eq!(plan.ready(), [0, 1]);
    plan.mark_sent(0).unwrap();
    plan.mark_sent(1).unwrap();
    plan.settle(
        0,
        Outcome::Succeeded(json!({"ok":true,"decorated_workflow":{"id":"WF1","future":42}})),
    )
    .unwrap();
    plan.settle(1, Outcome::Rejected(json!({"ok":false,"error":"denied"})))
        .unwrap();
    assert!(!plan.primary_succeeded());
    assert_eq!(plan.command(), &command);
    let Command::Metadata {
        mut trigger_update,
        name,
        description,
        icon_url,
    } = command
    else {
        unreachable!()
    };
    trigger_update.as_mut().unwrap()["token"] = json!("must never enter body");
    assert!(
        MutationPlan::new(
            &editor(true),
            Command::Metadata {
                trigger_update,
                name,
                description,
                icon_url
            }
        )
        .is_err()
    );
}
#[test]
fn permissions_preserve_missing_vs_empty_and_primary_success_is_not_completion() {
    let command = |teams| Command::RunPermission {
        permission_type: "everyone".into(),
        user_ids: None,
        channel_ids: None,
        team_ids: teams,
        org_ids: None,
    };
    let plain = MutationPlan::new(&editor(true), command(None)).unwrap();
    assert_eq!(
        plain.slots()[0].plan().arguments(),
        &json!({"trigger_id":"TR1","permission_type":"everyone"})
    );
    let empty = MutationPlan::new(&editor(true), command(Some(vec![]))).unwrap();
    assert_eq!(
        empty.slots()[0].plan().arguments(),
        &json!({"trigger_id":"TR1","permission_type":"named_entities","team_ids":[]})
    );
    assert_eq!(empty.follow_ups(), [FollowUp::RestrictionRefresh]);
    for published in [true, false] {
        let mut plan = MutationPlan::new(&editor(true), Command::Published(published)).unwrap();
        plan.mark_sent(0).unwrap();
        plan.settle(
            0,
            Outcome::Succeeded(json!({"ok":true,"decorated_workflow":{"id":"WF1"}})),
        )
        .unwrap();
        assert!(plan.primary_succeeded());
        assert_eq!(plan.follow_ups(), [FollowUp::TriggerInputSynchronization]);
    }
}
#[test]
fn step_input_keys_remain_opaque_and_foreign_step_ids_are_refused() {
    let action = StepAction::Update {
        step: json!({"id":"S1","function_id":"Fn1","inputs":{"CamelUserKey":{"value":{"KeepCase":[1,2]}}}}),
        input_parameters: Some(json!({"UserNamedParameter":{"type":"string"}})),
    };
    let plan = MutationPlan::new(&editor(true), Command::Step(action.clone())).unwrap();
    assert_eq!(
        plan.slots()[0].plan().arguments()["step"]["inputs"]["CamelUserKey"]["value"]["KeepCase"],
        json!([1, 2])
    );
    assert_eq!(
        plan.slots()[0].plan().arguments()["input_parameters"]["UserNamedParameter"]["type"],
        "string"
    );
    assert_eq!(
        plan.follow_ups(),
        [FollowUp::StepInteractivity, FollowUp::StepUserContext]
    );
    assert!(
        MutationPlan::new(
            &editor(true),
            Command::Step(StepAction::Delete {
                step_id: "foreign".into()
            })
        )
        .is_err()
    );
}
#[test]
fn exact_request_metadata_and_captured_form_array_semantics() {
    use crate::slack::client::RequestBody;
    let workspace = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
    };
    let plan = MutationPlan::new(
        &editor(false),
        Command::AddCollaborators(vec!["U1".into(), "U2".into()]),
    )
    .unwrap();
    let request = plan.slots()[0]
        .plan()
        .request(&SlackClient::default(), &workspace);
    assert_eq!(request.method, "POST");
    let url = url::Url::parse(&request.url).unwrap();
    assert_eq!(url.path(), "/api/functions.workflows.collaborators.add");
    assert_eq!(url.origin().ascii_serialization(), "https://example.test");
    assert!(url.fragment().is_none());
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form")
    };
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[0], ("token".into(), "synthetic".into()));
    assert!(fields.contains(&("collaborator_ids".into(), "U1,U2".into())));
    assert!(fields.contains(&(
        "_x_reason".into(),
        "add-hermes-workflow-collaborators".into()
    )));
    assert!(!fields.iter().any(|(key, _)| key == "reason"));
    assert_eq!(form_value(&json!([])), "");
    assert_eq!(form_value(&json!(["U1", null, "U2"])), "U1,,U2");
    assert_eq!(
        form_value(&json!({"camelInput":"untouched"})),
        "{\"camelInput\":\"untouched\"}"
    );
}

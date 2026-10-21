use super::*;

fn trigger_raw() -> Value {
    json!({"ok":true,"trigger":{
        "id":"TR_DETAIL","type":"shortcut","name":"Fresh trigger title","description":"Fresh trigger description",
        "owning_team_id":"T_ORIGINAL","shortcut_url":"https://slack.com/shortcuts/DETAIL",
        "workflow":{"workflow_id":"WF_DETAIL","title":"Nested title","description":"Nested description","app_id":"A_ORIGINAL","app":{"id":"A_APP","icons":{"image_36":"https://example.test/icon.png"},"opaque":[1,true]}},
        "workflow_details":{"collaborators":["U_INLINE"],"steps":[{"id":"INLINE","opaque":"keep"}]},
        "opaque":{"original":[1,2]}
    },"opaque_receipt":{"keep":true}})
}
fn workflow_raw() -> Value {
    json!({"ok":true,"workflow":{"id":"WF_DETAIL","title":"Full title","description":"Full description","collaborators":["U_FULL"],"steps":[{"id":"STEP1","function":{"type":"builtin","title":"Actual function title"},"inputs":{"original":true}}],"opaque":[8,9]},"opaque_receipt":"full"})
}
fn trigger() -> Trigger {
    Trigger::decode("TR_DETAIL", trigger_raw()).unwrap()
}
fn workflow() -> Workflow {
    Workflow::decode("WF_DETAIL", workflow_raw()).unwrap()
}

#[test]
fn cold_reads_pin_exact_method_reason_boolean_and_path_despite_xparams() {
    let session = WorkspaceSession {
        team_id: "T_ORIGINAL".into(),
        enterprise_id: None,
        user_id: "U_ORIGINAL".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    };
    for (read, method, reason, body) in [
        (
            Read::trigger_info("TR_DETAIL").unwrap(),
            "workflows.triggers.info",
            "hermes_triggers_ufm",
            json!({"trigger_id":"TR_DETAIL","include_workflow_details":true}),
        ),
        (
            Read::workflow("WF_DETAIL").unwrap(),
            "functions.workflows.get",
            "hermes_workflows_store_unknown_fetching",
            json!({"workflow_id":"WF_DETAIL"}),
        ),
    ] {
        assert_eq!(read.method(), method);
        assert_eq!(read.reason(), reason);
        assert_eq!(read.arguments(), &body);
        let request = read.request(&SlackClient::default(), &session);
        assert_eq!(request.method, "POST");
        let url = url::Url::parse(&request.url).unwrap();
        assert_eq!(url.host_str(), Some("fixture.test"));
        assert_eq!(url.path(), format!("/api/{method}"));
        assert!(url.query().is_some());
        let crate::slack::client::RequestBody::Form(fields) = request.body else {
            panic!("form required")
        };
        let actual = fields
            .into_iter()
            .filter(|(k, _)| k != "token")
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut expected = body
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        expected.insert("_x_reason".into(), reason.into());
        assert_eq!(actual, expected);
    }
    for invalid in ["", "TR1,TR2", "../WF", "WF?other=1"] {
        assert!(Read::trigger_info(invalid).is_err());
        assert!(Read::workflow(invalid).is_err());
    }
}
#[test]
fn actual_any_kind_identity_and_full_opaque_replies_are_retained_without_run_grant() {
    for kind in [
        "shortcut",
        "event",
        "scheduled",
        "external",
        "webhook",
        "service",
        "future_kind",
    ] {
        let mut raw = trigger_raw();
        raw["trigger"]["type"] = json!(kind);
        let t = Trigger::decode("TR_DETAIL", raw.clone()).unwrap();
        assert_eq!(t.raw(), &raw);
        let p = Projection::new(&t, Some(&workflow())).unwrap();
        assert_eq!(p.trigger_type(), Some(kind));
        assert_eq!(p.trigger().raw(), &raw);
    }
    assert_eq!(workflow().raw(), &workflow_raw());
    let p = Projection::new(&trigger(), Some(&workflow())).unwrap();
    assert_eq!(p.workflow().unwrap().workflow()["opaque"], json!([8, 9]));
    assert_eq!(
        p.raw_steps().unwrap()[0]["inputs"],
        json!({"original":true})
    );
}
#[test]
fn metadata_truthy_fallback_uses_trigger_then_nested_then_acquired_workflow_only() {
    let w = workflow();
    let mut raw = trigger_raw();
    let project = |raw: &Value| {
        Projection::new(
            &Trigger::decode("TR_DETAIL", raw.clone()).unwrap(),
            Some(&w),
        )
        .unwrap()
    };
    let p = project(&raw);
    assert_eq!(p.title(), Some("Fresh trigger title"));
    assert_eq!(p.description(), Some("Fresh trigger description"));
    raw["trigger"]["name"] = json!("");
    raw["trigger"]["description"] = Value::Null;
    let p = project(&raw);
    assert_eq!(p.title(), Some("Nested title"));
    assert_eq!(p.description(), Some("Nested description"));
    raw["trigger"]["workflow"]["title"] = json!("");
    raw["trigger"]["workflow"]["description"] = Value::Null;
    let p = project(&raw);
    assert_eq!(p.title(), Some("Full title"));
    assert_eq!(p.description(), Some("Full description"));
    let t = Trigger::decode("TR_DETAIL", raw.clone()).unwrap();
    let p = Projection::new(&t, None).unwrap();
    assert_eq!(p.title(), None);
    assert_eq!(p.description(), None);
    raw["trigger"]["name"] = json!("  &amp; title  ");
    assert_eq!(project(&raw).title(), Some("  &amp; title  "));
}
#[test]
fn optional_workflow_and_acquired_empty_arrays_do_not_fabricate_missing_facts() {
    let t = trigger();
    let p = Projection::new(&t, None).unwrap();
    assert_eq!(p.collaborators(), Some(&[json!("U_INLINE")][..]));
    assert_eq!(p.raw_steps().unwrap()[0]["id"], "INLINE");
    let mut raw = workflow_raw();
    raw["workflow"]["collaborators"] = json!([]);
    raw["workflow"]["steps"] = json!([]);
    let w = Workflow::decode("WF_DETAIL", raw.clone()).unwrap();
    let p = Projection::new(&t, Some(&w)).unwrap();
    assert_eq!(p.collaborators(), Some(&[][..]));
    assert_eq!(p.raw_steps(), Some(&[][..]));
    raw["workflow"]
        .as_object_mut()
        .unwrap()
        .remove("collaborators");
    raw["workflow"].as_object_mut().unwrap().remove("steps");
    let w = Workflow::decode("WF_DETAIL", raw).unwrap();
    let p = Projection::new(&t, Some(&w)).unwrap();
    assert_eq!(p.collaborators(), Some(&[json!("U_INLINE")][..]));
    assert_eq!(p.raw_steps(), None);
    let t = Trigger::decode("TR_EMPTY", json!({"ok":true,"trigger":{"id":"TR_EMPTY"}})).unwrap();
    assert_eq!(t.workflow_id(), None);
    let p = Projection::new(&t, None).unwrap();
    assert_eq!(p.workflow(), None);
    assert_eq!(p.raw_steps(), None);
    assert_eq!(p.collaborators(), None);
    assert_eq!(p.title(), None);
}
#[test]
fn workflow_correlation_cannot_borrow_other_identity_or_inline_details_id() {
    let mut raw = workflow_raw();
    raw["workflow"]["id"] = json!("WF_OTHER");
    let other = Workflow::decode("WF_OTHER", raw).unwrap();
    let failure = Projection::new(&trigger(), Some(&other)).unwrap_err();
    assert_eq!(failure.kind(), &FailureKind::Malformed);
    assert_eq!(failure.raw(), other.raw());
    let mut raw = trigger_raw();
    raw["trigger"]["workflow"]
        .as_object_mut()
        .unwrap()
        .remove("workflow_id");
    raw["trigger"]["workflow_details"]["id"] = json!("WF_DETAIL");
    let t = Trigger::decode("TR_DETAIL", raw).unwrap();
    assert_eq!(t.workflow_id(), None);
    assert!(Projection::new(&t, Some(&workflow())).is_err());
    assert!(Trigger::decode("OTHER", trigger_raw()).is_err());
    assert!(Workflow::decode("OTHER", workflow_raw()).is_err());
}
#[test]
fn exact_source_errors_remain_errors_with_original_raw_receipts() {
    for (code, kind) in [
        ("access_denied", FailureKind::AccessDenied),
        ("trigger_not_found", FailureKind::NotFound),
        (
            "function_not_found",
            FailureKind::Other("function_not_found".into()),
        ),
        (
            "workflow_not_found",
            FailureKind::Other("workflow_not_found".into()),
        ),
    ] {
        let raw = json!({"ok":false,"error":code,"opaque":{"keep":true}});
        let e = Trigger::decode("TR_DETAIL", raw.clone()).unwrap_err();
        assert_eq!(e.kind(), &kind);
        assert_eq!(e.raw(), &raw);
        let e = Workflow::decode("WF_DETAIL", raw.clone()).unwrap_err();
        assert_eq!(e.kind(), &kind);
        assert_eq!(e.raw(), &raw);
        let raw = json!({"ok":true,"trigger":{"id":"TR_DETAIL","error":code}});
        assert_eq!(Trigger::decode("TR_DETAIL", raw).unwrap_err().kind(), &kind);
    }
}
#[test]
fn malformed_missing_and_sentinel_metadata_never_become_ready() {
    for raw in [
        json!({}),
        json!({"error":"access_denied"}),
        json!({"ok":false}),
        json!({"ok":true}),
        json!({"ok":true,"trigger":false}),
        json!({"ok":true,"trigger":{"id":"TR_DETAIL","isUnknown":true}}),
        json!({"ok":true,"trigger":{"id":"TR_DETAIL","isNonExistent":true}}),
        json!({"ok":true,"trigger":{"id":"TR_DETAIL","name":3}}),
        json!({"ok":true,"trigger":{"id":"TR_DETAIL","workflow":[]}}),
        json!({"ok":true,"trigger":{"id":"TR_DETAIL","workflow_details":{"steps":{}}}}),
    ] {
        let e = Trigger::decode("TR_DETAIL", raw.clone()).unwrap_err();
        assert_eq!(e.kind(), &FailureKind::Malformed);
        assert_eq!(e.raw(), &raw);
    }
    for field in ["title", "collaborators", "steps"] {
        let mut raw = workflow_raw();
        raw["workflow"][field] = json!({"wrong":true});
        assert_eq!(
            Workflow::decode("WF_DETAIL", raw).unwrap_err().kind(),
            &FailureKind::Malformed
        );
    }
}
#[test]
fn owning_team_and_app_are_actual_trigger_facts_without_workspace_inference() {
    let p = Projection::new(&trigger(), Some(&workflow())).unwrap();
    assert_eq!(p.owning_team(), Some("T_ORIGINAL"));
    assert_eq!(p.app_id(), Some("A_ORIGINAL"));
    assert_eq!(p.app().unwrap()["opaque"], json!([1, true]));
    let mut raw = trigger_raw();
    raw["trigger"]["workflow"]["app_id"] = json!("");
    raw["trigger"]["owning_team_id"] = Value::Null;
    let t = Trigger::decode("TR_DETAIL", raw).unwrap();
    let p = Projection::new(&t, Some(&workflow())).unwrap();
    assert_eq!(p.owning_team(), None);
    assert_eq!(p.app_id(), Some("A_APP"));
}

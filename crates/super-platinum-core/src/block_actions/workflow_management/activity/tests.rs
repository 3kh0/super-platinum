use super::*;

#[test]
fn exact_source_requests_keep_undefined_filters_absent_and_empty_values_present() {
    let q = ActivityQuery::new("WF1", ActivityFilters::default()).unwrap();
    assert_eq!(q.limit(), 100);
    assert_eq!(q.request().method(), "workflows.activity.list");
    assert_eq!(q.request().reason(), "multi-click-activity-list-fetcher");
    assert_eq!(
        q.request().arguments(),
        &json!({"workflow_id":"WF1","limit":100,"filters":{}})
    );
    let q = ActivityQuery::new(
        "WF1",
        ActivityFilters {
            status: Some("".into()),
            user_id: Some("U1".into()),
            from_timestamp: Some(0),
            to_timestamp: Some(123),
        },
    )
    .unwrap()
    .with_limit(20)
    .unwrap()
    .with_parent(Some("WEx.1"))
    .unwrap()
    .with_cursor(Some("opaque+/="))
    .unwrap();
    assert_eq!(
        q.request().arguments(),
        &json!({"workflow_id":"WF1","limit":20,"parent_workflow_execution_id":"WEx.1","cursor":"opaque+/=","filters":{"workflow_instance_status":"","user_id":"U1","from_timestamp":0,"to_timestamp":123}})
    );
    let request = ActivityDetailRequest::new("WF1", "WEx.1")
        .unwrap()
        .request();
    assert_eq!(request.method(), "workflows.activity.get");
    assert_eq!(request.reason(), "Load instance steps for activity tab");
    assert_eq!(
        request.arguments(),
        &json!({"workflow_id":"WF1","workflow_execution_id":"WEx.1"})
    );
}

#[test]
fn exact_query_and_opaque_cursor_travel_with_each_bounded_page() {
    let q = ActivityQuery::new("WF1", ActivityFilters::default()).unwrap();
    let raw = json!({"ok":true,"workflow_events":[{"workflow_execution_id":"E2","future":{"kept":[2,1]}},{"workflow_execution_id":"E1","actor_id":""}],"response_metadata":{"next_cursor":"page2"},"future":7});
    let page = ActivityPage::decode(&q, raw.clone()).unwrap();
    assert_eq!(page.raw(), &raw);
    assert_eq!(page.rows()[0]["workflow_execution_id"], "E2");
    assert_eq!(page.query(), &q);
    let next = page.next_query().unwrap();
    assert_eq!(next.cursor(), Some("page2"));
    assert_eq!(next.clone().with_limit(50).unwrap().cursor(), None);
    assert_eq!(next.with_parent(Some("E2")).unwrap().cursor(), None);
    assert!(page.detail_request("foreign").is_err());
    assert_eq!(page.detail_request("E2").unwrap().execution_id(), "E2");
}

#[test]
fn explicit_pages_have_no_total_history_cap_and_empty_success_is_not_absence_proof() {
    let mut q = ActivityQuery::new("WF1", ActivityFilters::default()).unwrap();
    for i in 0..240 {
        let page = ActivityPage::decode(&q, json!({"ok":true,"workflow_events":[],"response_metadata":{"next_cursor":format!("next{i}")}})).unwrap();
        assert!(page.rows().is_empty());
        q = page.next_query().unwrap();
    }
    let end = ActivityPage::decode(&q, json!({"ok":true,"workflow_events":[]})).unwrap();
    assert!(end.next_query().is_none());
    assert!(end.detail_request("not_observed").is_err());
}

#[test]
fn malformed_negative_foreign_and_repeated_cursor_pages_do_not_become_success() {
    let q = ActivityQuery::new("WF1", ActivityFilters::default())
        .unwrap()
        .with_cursor(Some("same"))
        .unwrap();
    for raw in [
        json!({"ok":false,"workflow_events":[]}),
        json!({"workflow_events":[]}),
        json!({"ok":true}),
        json!({"ok":true,"workflow_events":[{}]}),
        json!({"ok":true,"workflow_events":[{"workflow_execution_id":"E1","workflow_id":"WF2"}]}),
        json!({"ok":true,"workflow_events":[],"response_metadata":{"next_cursor":7}}),
        json!({"ok":true,"workflow_events":[],"response_metadata":{"next_cursor":"same"}}),
    ] {
        assert!(ActivityPage::decode(&q, raw).is_err());
    }
    let too_many =
        json!({"ok":true,"workflow_events":vec![json!({"workflow_execution_id":"E1"});101]});
    assert!(ActivityPage::decode(&q, too_many).is_err());
}

#[test]
fn detail_preserves_complete_steps_and_optional_identity_without_fabricating_fields() {
    let key = ActivityDetailRequest::new("WF1", "E1").unwrap();
    let raw = json!({"ok":true,"workflow_event":{"step_events":[{"function":{"id":"Fn1"},"future":[3,2]}],"trace_id":"trace","workflow_status":"ERROR","trigger_message":{"private":"retained"},"started":17,"future":4}});
    let detail = key.decode(raw.clone()).unwrap();
    assert_eq!(detail.raw(), &raw);
    assert_eq!(detail.request_key(), &key);
    assert_eq!(detail.steps()[0]["future"], json!([3, 2]));
    assert_eq!(detail.status(), Some("ERROR"));
    assert_eq!(detail.trace_id(), Some("trace"));
    assert_eq!(detail.started(), Some(&json!(17)));
    let empty = key
        .decode(json!({"ok":true,"workflow_event":{"step_events":[]}}))
        .unwrap();
    assert!(empty.steps().is_empty());
    assert_eq!(empty.status(), None);
    assert!(empty.event().get("workflow_execution_id").is_none());
    for raw in [
        json!({"ok":false}),
        json!({"ok":true,"workflow_event":{}}),
        json!({"ok":true,"workflow_event":{"step_events":[],"workflow_execution_id":"E2"}}),
    ] {
        assert!(key.decode(raw).is_err());
    }
}

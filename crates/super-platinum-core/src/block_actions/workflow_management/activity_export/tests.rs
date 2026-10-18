use super::*;

#[test]
fn exact_export_request_omits_pagination_and_preserves_source_filter_rules() {
    let query = ActivityQuery::new(
        "Wf00000001",
        ActivityFilters {
            status: Some("error".into()),
            user_id: Some("".into()),
            from_timestamp: Some(0),
            to_timestamp: Some(42),
        },
    )
    .unwrap()
    .with_parent(Some("E1"))
    .unwrap()
    .with_limit(20)
    .unwrap()
    .with_cursor(Some("opaque"))
    .unwrap();
    assert_eq!(query.parent_execution_id(), Some("E1"));
    assert_eq!(query.cursor(), Some("opaque"));
    assert_eq!(query.limit(), 20);
    let mut export = ActivityExport::new(&query, Some(&json!("on"))).unwrap();
    let plan = export.take_request().unwrap();
    assert_eq!(plan.method(), "workflows.activity.export");
    assert_eq!(plan.reason(), "export_workflow_activity_csv");
    assert_eq!(
        plan.arguments(),
        &json!({"workflow_id":"Wf00000001","filters":{"workflow_instance_status":"error","user_id":"","from_timestamp":0,"to_timestamp":42}})
    );
    assert!(export.take_request().is_none());
}

#[test]
fn exact_lowercase_export_status_set_does_not_reuse_uppercase_list_statuses() {
    for status in [
        "success",
        "error",
        "in_progress",
        "cancelled",
        "SUCCESS",
        "ERROR",
        "PENDING",
        "CANCELED",
        "PENDING_USER_ACTION",
        "cancelled ",
        " success",
        "",
        "future",
    ] {
        let query = ActivityQuery::new(
            "Wf00000001",
            ActivityFilters {
                status: Some(status.into()),
                ..Default::default()
            },
        )
        .unwrap();
        let export = ActivityExport::new(&query, Some(&json!("on"))).unwrap();
        let expected = matches!(status, "success" | "error" | "in_progress" | "cancelled");
        assert_eq!(
            export.arguments()["filters"]
                .get("workflow_instance_status")
                .is_some(),
            expected
        );
    }
}

#[test]
fn missing_or_non_on_experiment_does_not_create_an_export_request() {
    let query = ActivityQuery::new("Wf00000001", ActivityFilters::default()).unwrap();
    assert!(ActivityExport::new(&query, None).is_err());
    for assignment in [
        Value::Null,
        json!(true),
        json!("off"),
        json!("treatment"),
        json!("ON"),
        json!(""),
        json!(1),
        json!({"group":"on"}),
    ] {
        assert!(ActivityExport::new(&query, Some(&assignment)).is_err());
    }
}

#[test]
fn acknowledgement_retains_raw_initiation_without_inventing_a_file_or_download() {
    let query = ActivityQuery::new("Wf00000001", ActivityFilters::default()).unwrap();
    let mut export = ActivityExport::new(&query, Some(&json!("on"))).unwrap();
    assert!(export.acknowledge(json!({"ok":true})).is_err());
    export.take_request().unwrap();
    for raw in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"ok":false}),
        json!({"ok":"true"}),
        json!({"ok":true,"error":"unexpected"}),
    ] {
        assert!(export.acknowledge(raw).is_err());
    }
    let raw = json!({"ok":true,"future":{"keep":[2,1]}});
    let receipt = export.acknowledge(raw.clone()).unwrap();
    assert_eq!(receipt.workflow_id(), query.workflow_id());
    assert_eq!(receipt.raw(), &raw);
    assert_eq!(receipt.arguments(), export.arguments());
    assert!(receipt.raw().get("file").is_none());
    assert!(export.take_request().is_none());
}

#[test]
fn prepared_export_form_has_only_source_arguments_and_absent_filters_stay_absent() {
    let owner = WorkspaceSession {
        team_id: "T00000001".into(),
        user_id: "U00000001".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let query = ActivityQuery::new("Wf00000001", ActivityFilters::default()).unwrap();
    let mut export = ActivityExport::new(&query, Some(&json!("on"))).unwrap();
    assert_eq!(
        export.arguments(),
        &json!({"workflow_id":"Wf00000001","filters":{}})
    );
    let request = export
        .take_request()
        .unwrap()
        .request(&SlackClient::default(), &owner);
    assert_eq!(request.method, "POST");
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/workflows.activity.export"
    );
    let crate::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
    assert_eq!(
        fields,
        std::collections::BTreeMap::from([
            ("token".into(), "synthetic".into()),
            ("workflow_id".into(), "Wf00000001".into()),
            ("filters".into(), "{}".into()),
            ("_x_reason".into(), "export_workflow_activity_csv".into()),
        ])
    );
}

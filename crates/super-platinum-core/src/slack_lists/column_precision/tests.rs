use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody},
};

fn document(precision: u8) -> Document {
    Document::parse(json!({"id":"F_PRECISION","mode":"list","title":"Plan","created":1,"user":"U1",
        "access":"write","unknown_file":{"keep":true},"list_metadata":{"opaque":{"keep":1},"schema":[
            {"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number","options":{"precision":precision},"opaque":{"keep":2}},
            {"id":"C_FUTURE","key":"future","name":"Future","type":"unknown","options":{"future":1}}
        ],"views":[{"id":"V1","opaque":true}]}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn capture(document: &Document, desired: u8) -> Result<ColumnPrecision, ListError> {
    ColumnPrecision::capture(
        document,
        "C_NUMBER",
        desired,
        Some("full"),
        Some(&ordinary()),
    )
}
fn intent() -> ColumnPrecision {
    capture(&document(0), 2).unwrap()
}

#[test]
fn every_precision_uses_exact_key_not_id_and_options_only_form() {
    for desired in 0..=5 {
        let original = document((desired + 1) % 6);
        let mut attempt = ColumnPrecisionAttempt::new(capture(&original, desired).unwrap());
        let permit = attempt
            .admit(&original, Some("full"), Some(&ordinary()))
            .unwrap();
        let session = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://example.slack.com".into(),
            token: "fixture-token".into(),
        };
        let request = api::list_column_precision(&SlackClient::default(), &session, permit);
        assert_eq!(request.method, "POST");
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/api/lists.columns.update"
        );
        let RequestBody::Form(fields) = request.body else {
            panic!()
        };
        assert_eq!(
            fields,
            vec![
                ("token".into(), "fixture-token".into()),
                ("list_id".into(), "F_PRECISION".into()),
                ("key".into(), "estimate".into()),
                ("options".into(), format!("{{\"precision\":{desired}}}")),
                ("backfill_missing_cells_with_default".into(), "true".into()),
                ("_x_reason".into(), "update-column".into())
            ]
        );
        assert!(
            attempt
                .admit(&original, Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
}
#[test]
fn capture_preserves_unknown_metadata_and_only_changes_expected_precision() {
    let original = document(0);
    let before = original.clone();
    let intent = capture(&original, 2).unwrap();
    assert_eq!(intent.original(), &before);
    assert_eq!(original, before);
    assert_eq!(intent.column_id(), "C_NUMBER");
    assert_eq!(intent.column_key(), "estimate");
    assert_eq!(intent.desired_precision(), 2);
    assert!(intent.observes_desired(&document(2)));
    assert!(!intent.observes_desired(&original));
    let mut raw = document(2).raw;
    raw["list_metadata"]["schema"][0]["options"]["precision"] = json!(0);
    assert_eq!(raw, original.raw);
}
#[test]
fn unsupported_shapes_types_and_invalid_precision_never_capture() {
    for options in [
        Value::Null,
        json!({}),
        json!({"precision":-1}),
        json!({"precision":6}),
        json!({"precision":1.5}),
        json!({"precision":2.0}),
        json!({"precision":"2"}),
        json!({"precision":true}),
        json!({"precision":0,"format":"number"}),
        json!({"precision":0,"default_value":12}),
        json!({"precision":0,"default_value_typed":{"number":[12]}}),
        json!({"precision":0,"future":true}),
    ] {
        let mut raw = document(0).raw;
        raw["list_metadata"]["schema"][0]["options"] = options;
        assert!(capture(&Document::parse(raw).unwrap(), 2).is_err());
    }
    let mut raw = document(0).raw;
    raw["list_metadata"]["schema"][0]
        .as_object_mut()
        .unwrap()
        .remove("options");
    assert!(capture(&Document::parse(raw).unwrap(), 2).is_err());
    for kind in ["currency", "text", "rich_text", "todo_completed", "unknown"] {
        let mut raw = document(0).raw;
        raw["list_metadata"]["schema"][0]["type"] = json!(kind);
        assert!(capture(&Document::parse(raw).unwrap(), 2).is_err());
    }
    for value in [0, 6, 255] {
        assert!(capture(&document(0), value).is_err());
    }
}
#[test]
fn permissions_personal_lists_missing_identity_and_forged_models_refuse() {
    for allow in [None, Some("read_only"), Some("off")] {
        assert!(
            ColumnPrecision::capture(&document(0), "C_NUMBER", 2, allow, Some(&ordinary()))
                .is_err()
        );
    }
    assert!(ColumnPrecision::capture(&document(0), "C_NUMBER", 2, Some("full"), None).is_err());
    for prefs in [
        json!({"task_lists":"[\"F_PRECISION\"]","assigned_to_list":""}),
        json!({"task_lists":"[]","assigned_to_list":"F_PRECISION"}),
    ] {
        let personal = PersonalTaskLists::from_response(&json!({"ok":true,"prefs":prefs})).unwrap();
        assert!(
            ColumnPrecision::capture(&document(0), "C_NUMBER", 2, Some("full"), Some(&personal))
                .is_err()
        );
    }
    assert!(
        ColumnPrecision::capture(&document(0), "missing", 2, Some("full"), Some(&ordinary()))
            .is_err()
    );
    for (key, value) in [
        ("access", json!("read")),
        ("is_locked", json!(true)),
        ("is_locked", json!("bad")),
        ("is_deleted", json!(true)),
        ("is_tombstoned", json!(true)),
    ] {
        let mut doc = document(0);
        doc.raw[key] = value;
        assert!(capture(&doc, 2).is_err());
    }
    let mut forged = document(0);
    forged.columns[0].key = "foreign".into();
    assert!(capture(&forged, 2).is_err());
    assert!(!intent().observes_desired(&forged));
}
#[test]
fn preflight_requires_current_full_schema_and_policy_except_last_read() {
    for path in [
        "/list_metadata/schema/0/id",
        "/list_metadata/schema/0/key",
        "/list_metadata/schema/0/name",
        "/list_metadata/schema/0/opaque/keep",
        "/list_metadata/schema/0/options/precision",
        "/list_metadata/schema/1/name",
        "/list_metadata/views/0/opaque",
        "/unknown_file/keep",
    ] {
        let mut raw = document(0).raw;
        *raw.pointer_mut(path).unwrap() = json!("changed");
        let fresh = Document::parse(raw).unwrap();
        assert!(
            intent()
                .revalidate(&fresh, Some("full"), Some(&ordinary()))
                .is_err(),
            "{path}"
        );
    }
    assert!(
        intent()
            .revalidate(&document(0), None, Some(&ordinary()))
            .is_err()
    );
    assert!(
        intent()
            .revalidate(&document(0), Some("full"), None)
            .is_err()
    );
    let mut raw = document(0).raw;
    raw["last_read"] = json!(123);
    assert!(
        intent()
            .revalidate(
                &Document::parse(raw).unwrap(),
                Some("full"),
                Some(&ordinary())
            )
            .is_ok()
    );
}
#[test]
fn failed_preflight_does_not_mint_permit_or_change_attempt_phase() {
    let mut attempt = ColumnPrecisionAttempt::new(intent());
    assert!(
        attempt
            .admit(&document(1), Some("full"), Some(&ordinary()))
            .is_err()
    );
    assert_eq!(attempt.phase(), ColumnPrecisionPhase::Reviewing);
    assert_eq!(attempt.intent(), &intent());
    assert!(
        attempt
            .admit(&document(0), Some("full"), Some(&ordinary()))
            .is_ok()
    );
    assert_eq!(attempt.phase(), ColumnPrecisionPhase::InFlight);
}
#[test]
fn readback_requires_incarnation_complete_metadata_and_exact_options() {
    for path in [
        "/id",
        "/created",
        "/user",
        "/list_metadata/schema/0/id",
        "/list_metadata/schema/0/key",
        "/list_metadata/schema/0/name",
        "/list_metadata/schema/0/type",
        "/list_metadata/schema/0/opaque/keep",
        "/list_metadata/schema/1/name",
        "/list_metadata/views/0/opaque",
        "/list_metadata/opaque/keep",
    ] {
        let mut raw = document(2).raw;
        *raw.pointer_mut(path).unwrap() = json!("changed");
        assert!(
            !intent().observes_desired(&Document::parse(raw).unwrap()),
            "{path}"
        );
    }
    let mut raw = document(2).raw;
    raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(!intent().observes_desired(&Document::parse(raw).unwrap()));
    let mut raw = document(2).raw;
    raw["list_metadata"]["schema"].as_array_mut().unwrap().pop();
    assert!(!intent().observes_desired(&Document::parse(raw).unwrap()));
    let mut raw = document(2).raw;
    raw["list_metadata"]["schema"][0]["options"]["future"] = json!(true);
    assert!(!intent().observes_desired(&Document::parse(raw).unwrap()));
    let mut raw = document(2).raw;
    raw["updated"] = json!(999);
    raw["access"] = json!("read");
    assert!(intent().observes_desired(&Document::parse(raw).unwrap()));
}
#[test]
fn only_post_dispatch_current_metadata_can_observe_and_no_phase_can_replay() {
    let mut attempt = ColumnPrecisionAttempt::new(intent());
    assert!(!attempt.finish(false));
    assert!(!attempt.observe(&document(2)));
    attempt
        .admit(&document(0), Some("full"), Some(&ordinary()))
        .unwrap();
    assert!(!attempt.observe(&document(2)));
    assert!(attempt.finish(false));
    assert_eq!(attempt.phase(), ColumnPrecisionPhase::Unconfirmed);
    assert!(!attempt.observe(&document(0)));
    assert!(
        attempt
            .admit(&document(0), Some("full"), Some(&ordinary()))
            .is_err()
    );
    assert!(attempt.observe(&document(2)));
    assert_eq!(attempt.phase(), ColumnPrecisionPhase::Observed);
    assert!(!attempt.finish(false));
    assert!(!attempt.observe(&document(2)));
    assert!(
        attempt
            .admit(&document(0), Some("full"), Some(&ordinary()))
            .is_err()
    );
}
#[test]
fn not_requested_requires_new_review_and_cannot_become_observed() {
    let mut attempt = ColumnPrecisionAttempt::new(intent());
    attempt
        .admit(&document(0), Some("full"), Some(&ordinary()))
        .unwrap();
    assert!(attempt.finish(true));
    assert_eq!(attempt.phase(), ColumnPrecisionPhase::NotRequested);
    assert!(!attempt.observe(&document(2)));
    assert!(
        attempt
            .admit(&document(0), Some("full"), Some(&ordinary()))
            .is_err()
    );
}

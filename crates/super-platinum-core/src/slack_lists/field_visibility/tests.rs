use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody},
};
fn document() -> Document {
    Document::parse(json!({"id":"F_FIELDS","mode":"list","access":"write","created":1,"user":"U1","opaque_file":7,"list_metadata":{
        "opaque":{"keep":1},"schema":[
            {"id":"C_NAME","key":"NAME","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true},
            {"id":"C_TEXT","key":"notes","name":"Notes","type":"text","options":{"format":"text"},"opaque":2},
            {"id":"C_NUMBER","key":"estimate","name":"Estimate","type":"number","options":{"precision":2}},
            {"id":"C_GROUP","key":"done","name":"Done","type":"checkbox"}],
        "views":[{"id":"V1","name":"Custom","type":"table","columns":[
            {"key":"NAME","id":"C_NAME","visible":true},{"key":"notes","id":"C_TEXT","visible":true,"width":177,"opaque":3},
            {"key":"estimate","id":"C_NUMBER","visible":false}],"filters":[{"column_id":"C_NUMBER","key":"estimate","operator":"greater_than","values":[0]}],
            "sorts":[{"key":"estimate","column_id":"C_NUMBER","direction":"desc"}],"grouping":{},"opaque_view":4},
            {"id":"V2","name":"Other","type":"board","columns":[{"key":"notes","id":"C_TEXT","visible":true}]}]}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn intent(column: &str, value: bool) -> ViewFieldVisibility {
    ViewFieldVisibility::capture(
        &document(),
        "V1",
        column,
        value,
        Some("full"),
        Some(&ordinary()),
    )
    .unwrap()
}
#[test]
fn hide_and_show_observe_only_one_boolean_preserving_every_other_metadata_member() {
    for (column, index, value) in [("C_TEXT", 1, false), ("C_NUMBER", 2, true)] {
        let i = intent(column, value);
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0]["columns"][index]["visible"] = json!(value);
        let expected = Document::parse(raw.clone()).unwrap();
        assert!(i.observes_desired(&expected));
        assert!(!i.observes_desired(&document()));
        for pointer in [
            "/list_metadata/opaque/keep",
            "/list_metadata/views/0/columns/1/width",
            "/list_metadata/views/0/opaque_view",
            "/list_metadata/schema/1/opaque",
            "/list_metadata/views/1/columns/0/visible",
        ] {
            let mut changed = raw.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("drift");
            if let Ok(doc) = Document::parse(changed) {
                assert!(!i.observes_desired(&doc));
            }
        }
        assert_eq!(i.original(), &document());
    }
}
#[test]
fn exact_json_array_form_keeps_distinct_key_id_and_both_boolean_values() {
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    };
    for (column, key, value) in [("C_TEXT", "notes", false), ("C_NUMBER", "estimate", true)] {
        let mut a = ViewFieldVisibilityAttempt::new(intent(column, value));
        let permit = a
            .admit(&document(), Some("full"), Some(&ordinary()))
            .unwrap();
        let r = api::list_field_visibility(&SlackClient::default(), &session, permit);
        assert_eq!(r.method, "POST");
        assert_eq!(
            url::Url::parse(&r.url).unwrap().path(),
            "/api/lists.views.update"
        );
        let RequestBody::Form(fields) = r.body else {
            panic!()
        };
        assert_eq!(
            fields,
            vec![
                ("token".into(), "fixture-token".into()),
                ("list_id".into(), "F_FIELDS".into()),
                ("id".into(), "V1".into()),
                (
                    "columns".into(),
                    json!([{"key":key,"visible":value,"id":column}]).to_string()
                ),
                ("_x_reason".into(), "update-view".into())
            ]
        );
    }
}
#[test]
fn protected_unsupported_and_ambiguous_columns_cannot_be_selected() {
    assert_eq!(
        ViewFieldVisibility::current(&document(), "V1", "C_NAME"),
        None
    );
    assert_eq!(
        ViewFieldVisibility::current(&document(), "V1", "C_GROUP"),
        None
    );
    assert_eq!(
        ViewFieldVisibility::current(&document(), "V2", "C_TEXT"),
        None
    );
    for (pointer, value) in [
        ("/list_metadata/views/0/is_locked", json!(true)),
        ("/list_metadata/views/0/is_all_items_view", json!(true)),
        ("/list_metadata/views/0/default_view_key", json!("all")),
        ("/list_metadata/views/0/columns/1/id", json!("OTHER")),
        ("/list_metadata/views/0/columns/1/id", Value::Null),
        ("/list_metadata/views/0/columns/1/visible", json!("false")),
        (
            "/list_metadata/views/0/grouping",
            json!({"group_by":"notes"}),
        ),
        (
            "/list_metadata/views/0/grouping",
            json!({"group_by_column_id":"C_TEXT"}),
        ),
        (
            "/list_metadata/schema/1/options",
            json!({"format":"text","default_value":"unknown"}),
        ),
    ] {
        let mut raw = document().raw;
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        raw.pointer_mut(parent).unwrap()[key] = value;
        let d = Document::parse(raw).unwrap();
        assert_eq!(
            ViewFieldVisibility::current(&d, "V1", "C_TEXT"),
            None,
            "{pointer}"
        );
    }
    let mut raw = document().raw;
    let duplicate = raw["list_metadata"]["views"][0]["columns"][1].clone();
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert_eq!(
        ViewFieldVisibility::current(&Document::parse(raw).unwrap(), "V1", "C_TEXT"),
        None
    );
}
#[test]
fn permissions_preferences_and_raw_typed_disagreement_refuse_before_admission() {
    for allow in [None, Some("read_only")] {
        assert!(
            ViewFieldVisibility::capture(
                &document(),
                "V1",
                "C_TEXT",
                false,
                allow,
                Some(&ordinary())
            )
            .is_err()
        );
    }
    assert!(
        ViewFieldVisibility::capture(&document(), "V1", "C_TEXT", false, Some("full"), None)
            .is_err()
    );
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F_FIELDS\"]","assigned_to_list":""}}),
    )
    .unwrap();
    assert!(
        ViewFieldVisibility::capture(
            &document(),
            "V1",
            "C_TEXT",
            false,
            Some("full"),
            Some(&personal)
        )
        .is_err()
    );
    let mut d = document();
    d.columns[1].name = "inconsistent".into();
    assert!(
        ViewFieldVisibility::capture(&d, "V1", "C_TEXT", false, Some("full"), Some(&ordinary()))
            .is_err()
    );
    let mut raw = document().raw;
    raw["access"] = json!("read");
    assert!(
        ViewFieldVisibility::capture(
            &Document::parse(raw).unwrap(),
            "V1",
            "C_TEXT",
            false,
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
}
#[test]
fn admitted_attempt_never_replays_or_accepts_ack_as_observation() {
    let i = intent("C_TEXT", false);
    let mut a = ViewFieldVisibilityAttempt::new(i);
    let mut changed = document().raw;
    changed["list_metadata"]["opaque"]["keep"] = json!(2);
    assert!(
        a.admit(
            &Document::parse(changed).unwrap(),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
    a.admit(&document(), Some("full"), Some(&ordinary()))
        .unwrap();
    assert!(!a.observe(&document()));
    assert!(a.finish(false));
    assert_eq!(a.phase(), ViewFieldVisibilityPhase::Unconfirmed);
    assert!(
        a.admit(&document(), Some("full"), Some(&ordinary()))
            .is_err()
    );
    assert!(!a.observe(&document()));
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]["columns"][1]["visible"] = json!(false);
    assert!(a.observe(&Document::parse(raw).unwrap()));
    assert!(!a.finish(false));
}

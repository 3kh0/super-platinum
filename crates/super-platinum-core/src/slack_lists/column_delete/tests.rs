use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody},
};
use serde_json::json;

fn document() -> Document {
    Document::parse(json!({"id":"F_DELETE","mode":"list","title":"Plan","created":1,"user":"U1",
        "access":"write","unknown_file":{"keep":true},"list_metadata":{"opaque":{"keep":1},"schema":[
            {"id":"C_TITLE","key":"title","name":"Title","type":"text","is_primary_column":true},
            {"id":"C_TARGET","key":"details","name":"Details","type":"text","options":{"future":true},"opaque":2},
            {"id":"C_FUTURE","key":"future","name":"Future","type":"unknown","options":{"future":1}}
        ],"views":[
            {"id":"V1","columns":[{"key":"title","visible":true},{"key":"details","visible":false,"opaque":3},{"key":"future","opaque":4}],
             "sorts":[{"key":"details","ascending":true}],"filters":[{"key":"details","operator":"contains","values":["old"]}],"grouping":{"key":"details"},"opaque":true},
            {"id":"V2","columns":[{"key":"future"},{"key":"details"}],"opaque":"board"},
            {"id":"V3","columns":[{"key":"title"}],"opaque":"target absent"}
        ],"integrations":[{"column_key":"details","opaque":"keep"}]}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn capture(doc: &Document) -> Result<ColumnDelete, ListError> {
    ColumnDelete::capture(
        doc,
        "C_TARGET",
        "U1",
        Some(false),
        Some("full"),
        Some(&ordinary()),
    )
}
fn intent() -> ColumnDelete {
    capture(&document()).unwrap()
}
fn deleted() -> Document {
    let mut raw = document().raw;
    raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    raw["list_metadata"]["views"][1]["columns"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    Document::parse(raw).unwrap()
}

#[test]
fn removal_preserves_all_other_metadata_and_original_document() {
    let original = document();
    let deletion = intent();
    assert_eq!(deletion.original(), &original);
    assert!(deletion.observes_desired(&deleted()));
    assert!(!deletion.observes_desired(&original));
    assert_eq!(document(), original);
}

#[test]
fn known_ordinary_types_share_key_only_delete_and_options_are_not_rewritten() {
    for kind in [
        "text",
        "rich_text",
        "message",
        "number",
        "select",
        "multi_select",
        "date",
        "user",
        "attachment",
        "checkbox",
        "email",
        "phone",
        "channel",
        "currency",
        "rating",
        "created_by",
        "last_edited_by",
        "created_time",
        "last_edited_time",
        "vote",
        "assignee",
        "due_date",
        "completed",
        "canvas",
        "reference",
        "link",
    ] {
        for options in [Value::Null, json!({"future":{"keep":true}})] {
            let mut raw = document().raw;
            raw["list_metadata"]["schema"][1]["type"] = json!(kind);
            raw["list_metadata"]["schema"][1]["options"] = options;
            let doc = Document::parse(raw).unwrap();
            assert!(ColumnDelete::supports(&doc, "C_TARGET"), "{kind}");
            assert!(capture(&doc).is_ok(), "{kind}");
        }
    }
    // +E6Z distinguishes legacy ordinary types above from the three task-bundle types.
    for kind in [
        "todo_completed",
        "todo_assignee",
        "todo_due_date",
        "future_unknown",
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["schema"][1]["type"] = json!(kind);
        assert!(capture(&Document::parse(raw).unwrap()).is_err(), "{kind}");
    }
}

#[test]
fn primary_flags_fallback_and_forged_model_are_refused() {
    assert!(
        ColumnDelete::capture(
            &document(),
            "C_TITLE",
            "U1",
            Some(false),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
    for flag in [json!(true), json!("true"), json!(1), json!({})] {
        let mut raw = document().raw;
        raw["list_metadata"]["schema"][1]["is_primary_column"] = flag;
        assert!(capture(&Document::parse(raw).unwrap()).is_err());
    }
    let mut raw = document().raw;
    raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(capture(&Document::parse(raw).unwrap()).is_err());
    let mut forged = document();
    forged.columns[1].key = "title".into();
    assert!(capture(&forged).is_err());
    assert!(!intent().observes_desired(&forged));
}

#[test]
fn permissions_and_current_personal_preferences_are_required_at_capture_and_admission() {
    assert!(
        ColumnDelete::capture(
            &document(),
            "C_TARGET",
            "U1",
            Some(false),
            None,
            Some(&ordinary())
        )
        .is_err()
    );
    assert!(
        ColumnDelete::capture(
            &document(),
            "C_TARGET",
            "U1",
            Some(false),
            Some("full"),
            None
        )
        .is_err()
    );
    for prefs in [
        json!({"task_lists":"[\"F_DELETE\"]","assigned_to_list":""}),
        json!({"task_lists":"[]","assigned_to_list":"F_DELETE"}),
    ] {
        let personal = PersonalTaskLists::from_response(&json!({"ok":true,"prefs":prefs})).unwrap();
        assert!(
            ColumnDelete::capture(
                &document(),
                "C_TARGET",
                "U1",
                Some(false),
                Some("full"),
                Some(&personal)
            )
            .is_err()
        );
        assert!(
            ColumnDeleteAttempt::new(intent())
                .admit(
                    &document(),
                    "U1",
                    Some(false),
                    Some("full"),
                    Some(&personal)
                )
                .is_err()
        );
    }
    for (key, value) in [
        ("access", json!("read")),
        ("is_locked", json!(true)),
        ("is_deleted", json!(true)),
    ] {
        let mut raw = document().raw;
        raw[key] = value;
        if let Ok(doc) = Document::parse(raw) {
            assert!(capture(&doc).is_err());
        }
    }
}

#[test]
fn every_view_must_have_unambiguous_columns_but_need_not_contain_target() {
    for columns in [
        Value::Null,
        json!({}),
        json!([null]),
        json!([{"key":""}]),
        json!([{"key":"details"},{"key":"details"}]),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0]["columns"] = columns;
        assert!(capture(&Document::parse(raw).unwrap()).is_err());
    }
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]
        .as_object_mut()
        .unwrap()
        .remove("columns");
    assert!(capture(&Document::parse(raw).unwrap()).is_err());
    for views in [None, Some(Value::Null)] {
        let mut raw = document().raw;
        if let Some(views) = views {
            raw["list_metadata"]["views"] = views;
        } else {
            raw["list_metadata"]
                .as_object_mut()
                .unwrap()
                .remove("views");
        }
        assert!(capture(&Document::parse(raw).unwrap()).is_err());
    }
    let mut raw = document().raw;
    raw["list_metadata"]["views"] = json!([]);
    assert!(capture(&Document::parse(raw).unwrap()).is_ok());
}

#[test]
fn preflight_requires_exact_reviewed_metadata_and_file_identity() {
    for path in [
        "/id",
        "/created",
        "/user",
        "/unknown_file/keep",
        "/list_metadata/schema/1/key",
        "/list_metadata/schema/1/options/future",
        "/list_metadata/schema/2/name",
        "/list_metadata/views/0/opaque",
        "/list_metadata/views/0/columns/1/visible",
        "/list_metadata/integrations/0/opaque",
    ] {
        let mut raw = document().raw;
        *raw.pointer_mut(path).unwrap() = json!("changed");
        if let Ok(doc) = Document::parse(raw) {
            let mut attempt = ColumnDeleteAttempt::new(intent());
            assert!(
                attempt
                    .admit(&doc, "U1", Some(false), Some("full"), Some(&ordinary()))
                    .is_err(),
                "{path}"
            );
            assert_eq!(attempt.phase(), ColumnDeletePhase::Reviewing);
        }
    }
    let mut raw = document().raw;
    raw["last_read"] = json!(123);
    assert!(
        intent()
            .revalidate(
                &Document::parse(raw).unwrap(),
                "U1",
                Some(false),
                Some("full"),
                Some(&ordinary())
            )
            .is_ok()
    );
}

#[test]
fn readback_requires_target_absence_in_all_views_and_exact_unrelated_metadata() {
    for path in [
        "/id",
        "/created",
        "/user",
        "/list_metadata/schema/1/name",
        "/list_metadata/opaque/keep",
        "/list_metadata/views/0/columns/1/key",
        "/list_metadata/views/0/sorts/0/ascending",
        "/list_metadata/views/0/filters/0/values/0",
        "/list_metadata/views/0/grouping/key",
        "/list_metadata/views/1/opaque",
        "/list_metadata/integrations/0/opaque",
    ] {
        let mut raw = deleted().raw;
        *raw.pointer_mut(path).unwrap() = json!("changed");
        if let Ok(doc) = Document::parse(raw) {
            assert!(!intent().observes_desired(&doc), "{path}");
        }
    }
    let mut raw = deleted().raw;
    raw["list_metadata"]["views"][1]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"details"}));
    assert!(!intent().observes_desired(&Document::parse(raw).unwrap()));
    let mut raw = deleted().raw;
    raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(!intent().observes_desired(&Document::parse(raw).unwrap()));
    let mut raw = deleted().raw;
    raw["updated"] = json!(999);
    raw["access"] = json!("read");
    assert!(intent().observes_desired(&Document::parse(raw).unwrap()));
}

#[test]
fn absent_integrations_are_preserved_without_claiming_optimistic_normalization() {
    let mut raw = document().raw;
    raw["list_metadata"]
        .as_object_mut()
        .unwrap()
        .remove("integrations");
    let intent = capture(&Document::parse(raw).unwrap()).unwrap();
    let mut observed = deleted().raw;
    observed["list_metadata"]
        .as_object_mut()
        .unwrap()
        .remove("integrations");
    assert!(intent.observes_desired(&Document::parse(observed.clone()).unwrap()));
    observed["list_metadata"]["integrations"] = json!([]);
    assert!(!intent.observes_desired(&Document::parse(observed).unwrap()));
}

#[test]
fn consumed_permit_builds_exact_four_field_form_using_key_not_column_id() {
    let mut attempt = ColumnDeleteAttempt::new(intent());
    let permit = attempt
        .admit(
            &document(),
            "U1",
            Some(false),
            Some("full"),
            Some(&ordinary()),
        )
        .unwrap();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.slack.com".into(),
        token: "fixture-token".into(),
    };
    let request = api::list_column_delete(&SlackClient::default(), &session, permit);
    assert_eq!(request.method, "POST");
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/lists.columns.delete"
    );
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-token".into()),
            ("list_id".into(), "F_DELETE".into()),
            ("key".into(), "details".into()),
            ("_x_reason".into(), "delete-column".into())
        ]
    );
    assert!(
        attempt
            .admit(
                &document(),
                "U1",
                Some(false),
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
    );
}

#[test]
fn acknowledgment_is_not_observation_and_never_enables_replay() {
    let mut attempt = ColumnDeleteAttempt::new(intent());
    assert!(!attempt.finish(false));
    assert!(!attempt.observe(&deleted()));
    attempt
        .admit(
            &document(),
            "U1",
            Some(false),
            Some("full"),
            Some(&ordinary()),
        )
        .unwrap();
    assert!(!attempt.observe(&deleted()));
    assert!(attempt.finish(false));
    assert_eq!(attempt.phase(), ColumnDeletePhase::Unconfirmed);
    assert!(!attempt.observe(&document()));
    assert!(
        attempt
            .admit(
                &document(),
                "U1",
                Some(false),
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
    );
    assert!(attempt.observe(&deleted()));
    assert_eq!(attempt.phase(), ColumnDeletePhase::Observed);
    assert!(!attempt.finish(false));
    assert!(!attempt.observe(&deleted()));
}

#[test]
fn not_requested_is_terminal_and_cannot_be_reclassified_by_readback() {
    let mut attempt = ColumnDeleteAttempt::new(intent());
    attempt
        .admit(
            &document(),
            "U1",
            Some(false),
            Some("full"),
            Some(&ordinary()),
        )
        .unwrap();
    assert!(attempt.finish(true));
    assert_eq!(attempt.phase(), ColumnDeletePhase::NotRequested);
    assert!(!attempt.observe(&deleted()));
    assert!(
        attempt
            .admit(
                &document(),
                "U1",
                Some(false),
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
    );
}

#[test]
fn owner_only_assignment_is_checked_at_review_and_again_at_admission() {
    let doc = document();
    for (user, flag, permitted) in [
        ("U_COLLABORATOR", None, false),
        ("U1", None, false),
        ("U_COLLABORATOR", Some(false), true),
        ("U_COLLABORATOR", Some(true), false),
        ("U1", Some(true), true),
        ("", Some(false), false),
        (" U1 ", Some(false), false),
    ] {
        assert_eq!(
            ColumnDelete::schema_policy_allows(&doc, user, flag),
            permitted
        );
        assert_eq!(
            ColumnDelete::capture(
                &doc,
                "C_TARGET",
                user,
                flag,
                Some("full"),
                Some(&ordinary())
            )
            .is_ok(),
            permitted
        );
    }
    let intent = ColumnDelete::capture(
        &doc,
        "C_TARGET",
        "U_COLLABORATOR",
        Some(false),
        Some("full"),
        Some(&ordinary()),
    )
    .unwrap();
    let mut attempt = ColumnDeleteAttempt::new(intent);
    assert!(
        attempt
            .admit(
                &doc,
                "U_COLLABORATOR",
                Some(true),
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
    );
    assert!(
        attempt
            .admit(
                &doc,
                "U_COLLABORATOR",
                None,
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
    );
    assert!(
        attempt
            .admit(&doc, "U1", Some(false), Some("full"), Some(&ordinary()))
            .is_err()
    );
    assert_eq!(attempt.phase(), ColumnDeletePhase::Reviewing);
    assert!(
        attempt
            .admit(
                &doc,
                "U_COLLABORATOR",
                Some(false),
                Some("full"),
                Some(&ordinary())
            )
            .is_ok()
    );
    let mut owner = doc.raw;
    owner["access"] = json!("owner");
    owner.as_object_mut().unwrap().remove("user");
    let owner = Document::parse(owner).unwrap();
    assert!(
        ColumnDelete::capture(
            &owner,
            "C_TARGET",
            "U_CURRENT",
            Some(true),
            Some("full"),
            Some(&ordinary())
        )
        .is_ok()
    );
    let mut owner = owner.raw;
    owner["access"] = json!("read");
    owner["user"] = json!("U_CURRENT");
    assert!(
        ColumnDelete::capture(
            &Document::parse(owner).unwrap(),
            "C_TARGET",
            "U_CURRENT",
            Some(true),
            Some("full"),
            Some(&ordinary())
        )
        .is_err()
    );
}

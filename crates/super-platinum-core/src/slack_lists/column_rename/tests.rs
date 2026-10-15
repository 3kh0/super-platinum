use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody},
    slack_lists::RecordsPage,
};

fn document() -> Document {
    Document::parse(json!({"id":"F_RENAME","mode":"list","title":"Plan","created":1,"user":"U1",
        "access":"write","unknown_file":{"keep":true},"list_metadata":{"opaque":{"keep":1},"schema":[
            {"id":"C_TEXT","key":"title","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true,"opaque":{"keep":2}},
            {"id":"C_FUTURE","key":"future","name":"Future","type":"unknown","options":{"future":1}}
        ],"views":[{"id":"V1","opaque":true}]}})).unwrap()
}
fn ordinary() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn intent() -> ColumnRename {
    ColumnRename::capture(
        &document(),
        "C_TEXT",
        "  Delivery title  ",
        Some("full"),
        Some(&ordinary()),
    )
    .unwrap()
}
fn renamed() -> Document {
    let mut raw = document().raw;
    raw["list_metadata"]["schema"][0]["name"] = json!("Delivery title");
    Document::parse(raw).unwrap()
}
#[test]
fn rename_preserves_raw_schema_and_never_changes_population_completeness() {
    let before = document();
    let original = before.clone();
    let rename = intent();
    let raw =
        json!({"ok":true,"records":[],"response_metadata":{"next_cursor":"opaque+/=","keep":true}});
    let page = RecordsPage::parse(raw.clone()).unwrap();
    assert_eq!(rename.original(), &before);
    assert_eq!(before, original);
    assert!(rename.observes_desired(&renamed()));
    assert!(!rename.observes_desired(&before));
    assert!(!page.complete());
    assert_eq!(page.raw, raw);
}
#[test]
fn source_backed_form_uses_key_not_id_and_exact_unchanged_text_options() {
    let mut attempt = ColumnRenameAttempt::new(intent());
    let permit = attempt
        .admit(&document(), Some("full"), Some(&ordinary()))
        .unwrap();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.slack.com".into(),
        token: "fixture-token".into(),
    };
    let request = api::list_column_rename(&SlackClient::default(), &session, permit);
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
            ("list_id".into(), "F_RENAME".into()),
            ("key".into(), "title".into()),
            ("name".into(), "Delivery title".into()),
            ("options".into(), "{\"format\":\"text\"}".into()),
            ("backfill_missing_cells_with_default".into(), "true".into()),
            ("_x_reason".into(), "update-column".into())
        ]
    );
}
#[test]
fn unsupported_options_types_and_defaults_cannot_be_silently_rewritten() {
    for options in [
        Value::Null,
        json!({}),
        json!({"format":"rich_text"}),
        json!({"format":"text","default_value":"hello"}),
        json!({"format":"text","future":1}),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["schema"][0]["options"] = options;
        assert!(
            ColumnRename::capture(
                &Document::parse(raw).unwrap(),
                "C_TEXT",
                "New",
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
    }
    for kind in ["rich_text", "number", "todo_completed", "unknown"] {
        let mut raw = document().raw;
        raw["list_metadata"]["schema"][0]["type"] = json!(kind);
        assert!(
            ColumnRename::capture(
                &Document::parse(raw).unwrap(),
                "C_TEXT",
                "New",
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
        );
    }
}
#[test]
fn empty_unchanged_control_and_oversized_names_refuse_before_admission() {
    for name in [
        "",
        " \t ",
        "Title",
        "Title\nOther",
        "Title\0Other",
        &"x".repeat(257),
        &"😀".repeat(129),
    ] {
        assert!(
            ColumnRename::capture(&document(), "C_TEXT", name, Some("full"), Some(&ordinary()))
                .is_err()
        );
    }
    assert!(
        ColumnRename::capture(
            &document(),
            "C_TEXT",
            &"😀".repeat(128),
            Some("full"),
            Some(&ordinary())
        )
        .is_ok()
    );
}
#[test]
fn permission_personal_and_forged_models_fail_closed() {
    assert!(ColumnRename::capture(&document(), "C_TEXT", "New", None, Some(&ordinary())).is_err());
    assert!(ColumnRename::capture(&document(), "C_TEXT", "New", Some("full"), None).is_err());
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F_RENAME\"]","assigned_to_list":""}}),
    )
    .unwrap();
    assert!(
        ColumnRename::capture(&document(), "C_TEXT", "New", Some("full"), Some(&personal)).is_err()
    );
    let mut forged = document();
    forged.columns[0].key = "foreign".into();
    assert!(
        ColumnRename::capture(&forged, "C_TEXT", "New", Some("full"), Some(&ordinary())).is_err()
    );
    let mut raw = document().raw;
    raw["is_locked"] = json!(true);
    assert!(
        intent()
            .revalidate(
                &Document::parse(raw).unwrap(),
                Some("full"),
                Some(&ordinary())
            )
            .is_err()
    );
}
#[test]
fn preflight_rejects_exact_schema_unknown_and_sibling_changes() {
    for path in [
        "/list_metadata/schema/0/id",
        "/list_metadata/schema/0/key",
        "/list_metadata/schema/0/name",
        "/list_metadata/schema/0/opaque/keep",
        "/list_metadata/schema/1/name",
        "/list_metadata/views/0/opaque",
        "/unknown_file/keep",
    ] {
        let mut raw = document().raw;
        *raw.pointer_mut(path).unwrap() = json!("changed");
        if let Ok(fresh) = Document::parse(raw) {
            assert!(
                intent()
                    .revalidate(&fresh, Some("full"), Some(&ordinary()))
                    .is_err()
            );
        }
    }
    let mut raw = document().raw;
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
fn readback_requires_full_metadata_identity_and_only_target_name_change() {
    for path in [
        "/id",
        "/created",
        "/user",
        "/list_metadata/schema/0/id",
        "/list_metadata/schema/0/key",
        "/list_metadata/schema/0/opaque/keep",
        "/list_metadata/schema/1/name",
        "/list_metadata/views/0/opaque",
        "/list_metadata/opaque/keep",
    ] {
        let mut raw = renamed().raw;
        *raw.pointer_mut(path).unwrap() = json!("changed");
        if let Ok(fresh) = Document::parse(raw) {
            assert!(!intent().observes_desired(&fresh));
        }
    }
    let mut raw = renamed().raw;
    raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(!intent().observes_desired(&Document::parse(raw).unwrap()));
    let mut raw = renamed().raw;
    raw["updated"] = json!(999);
    raw["access"] = json!("read");
    assert!(intent().observes_desired(&Document::parse(raw).unwrap()));
}
#[test]
fn original_or_inflight_read_cannot_release_receipt_or_enable_replay() {
    let mut attempt = ColumnRenameAttempt::new(intent());
    attempt
        .admit(&document(), Some("full"), Some(&ordinary()))
        .unwrap();
    assert!(!attempt.observe(&renamed()));
    assert!(attempt.finish(false));
    assert_eq!(attempt.phase(), ColumnRenamePhase::Unconfirmed);
    assert!(!attempt.observe(&document()));
    assert!(
        attempt
            .admit(&document(), Some("full"), Some(&ordinary()))
            .is_err()
    );
    assert!(attempt.observe(&renamed()));
    assert_eq!(attempt.phase(), ColumnRenamePhase::Observed);
    assert!(!attempt.finish(false));
    assert!(!attempt.observe(&renamed()));
}
#[test]
fn not_requested_is_terminal_and_requires_a_new_reviewed_attempt() {
    let mut attempt = ColumnRenameAttempt::new(intent());
    assert!(!attempt.finish(true));
    attempt
        .admit(&document(), Some("full"), Some(&ordinary()))
        .unwrap();
    assert!(attempt.finish(true));
    assert_eq!(attempt.phase(), ColumnRenamePhase::NotRequested);
    assert!(
        attempt
            .admit(&document(), Some("full"), Some(&ordinary()))
            .is_err()
    );
}

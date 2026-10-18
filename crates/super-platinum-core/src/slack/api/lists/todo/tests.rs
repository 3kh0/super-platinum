use super::*;
use serde_json::json;
#[test]
fn saved_lookup_keeps_cursor_and_requires_exact_record_identity() {
    let raw = json!({"ok":true,"saved_items":[{"item_type":"list_record","item_id":"F_TASK","ts":"ROW","date_created":100,"state":"in_progress","date_due":200}],"response_metadata":{"next_cursor":"next"}});
    let (found, cursor) = saved_record_page_result(&raw, "F_TASK", "ROW").unwrap();
    assert_eq!(found.unwrap().date_created, 100);
    assert_eq!(cursor, "next");
    assert!(
        saved_record_page_result(&raw, "F_TASK", "OTHER")
            .unwrap()
            .0
            .is_none()
    );
    let mut duplicate = raw.clone();
    duplicate["saved_items"]
        .as_array_mut()
        .unwrap()
        .push(raw["saved_items"][0].clone());
    assert!(saved_record_page_result(&duplicate, "F_TASK", "ROW").is_err());
}
#[test]
fn malformed_or_unsuccessful_saved_page_is_not_absence() {
    for raw in [
        json!({"ok":true}),
        json!({"ok":false,"saved_items":[]}),
        json!({"ok":true,"saved_items":null}),
        json!({"ok":true,"saved_items":[],"response_metadata":{"next_cursor":false}}),
        json!({"ok":true,"saved_items":[null]}),
    ] {
        assert!(saved_record_page_result(&raw, "F_TASK", "ROW").is_err());
    }
    let raw = json!({"ok":true,"saved_items":[],"response_metadata":{"next_cursor":"next"}});
    let (found, cursor) = saved_record_page_result(&raw, "F_TASK", "ROW").unwrap();
    assert!(found.is_none());
    assert_eq!(cursor, "next");
}

#[test]
fn actual_request_builders_keep_saved_record_identity_and_permission_reason() {
    use crate::config::WorkspaceSession;
    use crate::slack::client::RequestBody;
    use crate::slack_lists::{Document, ListReminderChange, RecordsPage};
    let w = WorkspaceSession {
        team_id: "T_TEST".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "test".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let c = SlackClient::default();
    let request = list_assignee_access(&c, &w, "F_TASK", "U00000002");
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-not-a-token".into()),
            ("file_id".into(), "F_TASK".into()),
            ("user_id".into(), "U00000002".into()),
            ("_x_reason".into(), "canShare".into())
        ]
    );
    let d = Document::parse(
        json!({"id":"F_TASK","mode":"list","access":"read","list_metadata":{"schema":[]}}),
    )
    .unwrap();
    let p = RecordsPage::parse(json!({"ok":true,"records":[{"id":"ROW","fields":[]}]})).unwrap();
    let edit =
        ListReminderEdit::capture(&d, &p, "ROW", true, None, ListReminderChange::Set(300)).unwrap();
    let request = list_reminder_change(&c, &w, &edit);
    assert!(!request.retry_safe());
    assert!(request.url.contains("/api/saved.add?"));
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-not-a-token".into()),
            ("item_type".into(), "list_record".into()),
            ("item_id".into(), "F_TASK".into()),
            ("ts".into(), "ROW".into()),
            ("date_due".into(), "300".into()),
            ("_x_reason".into(), "saved-api/saveListRecord".into())
        ]
    );
}
#[test]
fn reminder_experiment_requires_successful_complete_assignments_and_exact_on() {
    for group in ["on", "off", "control"] {
        let flags = parse_list_experiments(
            &json!({"ok":true,"assignments":{"lists_in_later":{"group":group}}}),
        )
        .unwrap();
        assert_eq!(flags.lists_in_later, group == "on");
    }
    assert!(
        !parse_list_experiments(&json!({"ok":true,"assignments":{}}))
            .unwrap()
            .lists_in_later
    );
    for raw in [
        json!({"ok":true}),
        json!({"ok":false,"assignments":{}}),
        json!({"ok":true,"assignments":{"lists_in_later":true}}),
    ] {
        assert!(parse_list_experiments(&raw).is_err());
    }
}

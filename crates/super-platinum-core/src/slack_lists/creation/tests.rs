use super::*;
use crate::config::WorkspaceSession;
use crate::slack::{
    api,
    client::{RequestBody, SlackClient},
};

fn fixture() -> (Document, PersonalTaskLists) {
    let document = Document::parse(json!({"id":"F_LIST","mode":"list","access":"write",
        "is_locked":false,"list_limits":{"over_row_maximum":false,"row_count":3},
        "list_metadata":{"schema":[
            {"id":"C_NAME","key":"name","name":"Name","type":"text"},
            {"id":"C_DONE","key":"done","name":"Completed","type":"todo_completed"}],
            "views":[{"id":"V_ALL","default_view_key":"all_items"},
                {"id":"V_DONE","default_view_key":"completed_items"}]}}))
    .unwrap();
    let policy = PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{
        "task_lists":"[]","assigned_to_list":""}}))
    .unwrap();
    (document, policy)
}
fn intent(document: &Document, policy: &PersonalTaskLists) -> CreationIntent {
    CreationIntent::capture(document, Some("full"), Some(policy), Some("V_ALL")).unwrap()
}
fn receipt() -> Value {
    json!({"ok":true,"record":{"id":"R_NEW","date_created":123,"created_by":"U1",
        "position":"500","fields":[{"key":"name","rich_text":[]}]}})
}

#[test]
fn creation_requires_fresh_ordinary_writable_below_limit_schema() {
    let (document, policy) = fixture();
    assert!(CreationIntent::capture(&document, None, Some(&policy), None).is_err());
    assert!(CreationIntent::capture(&document, Some("full"), None, None).is_err());
    assert!(CreationIntent::capture(&document, Some("full"), Some(&policy), Some("gone")).is_err());
    let personal = PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{
        "task_lists":"[\"F_LIST\"]","assigned_to_list":""}}))
    .unwrap();
    assert!(CreationIntent::capture(&document, Some("full"), Some(&personal), None).is_err());
    for (path, value) in [
        ("/access", json!("read")),
        ("/is_locked", json!(true)),
        ("/is_locked", json!("false")),
        ("/list_limits", Value::Null),
        ("/list_limits/over_row_maximum", json!(true)),
        ("/list_limits/over_row_maximum", json!("false")),
        ("/list_metadata/schema", json!([])),
    ] {
        let mut raw = document.raw.clone();
        *raw.pointer_mut(path).unwrap() = value;
        let changed = Document::parse(raw).unwrap();
        assert!(
            CreationIntent::capture(&changed, Some("full"), Some(&policy), None).is_err(),
            "{path}"
        );
    }
}

#[test]
fn preflight_rejects_changed_view_schema_limits_and_permissions_but_not_last_read() {
    let (document, policy) = fixture();
    let intent = intent(&document, &policy);
    let mut fresh = document.clone();
    fresh.raw["last_read"] = json!(100);
    assert!(
        intent
            .revalidate(&fresh, Some("full"), Some(&policy), Some("V_ALL"))
            .is_ok()
    );
    assert!(
        intent
            .revalidate(&fresh, Some("full"), Some(&policy), Some("V_DONE"))
            .is_err()
    );
    for (path, value) in [
        ("/list_limits/row_count", json!(4)),
        ("/list_metadata/schema/0/name", json!("Renamed")),
        ("/access", json!("owner")),
    ] {
        let mut raw = fresh.raw.clone();
        *raw.pointer_mut(path).unwrap() = value;
        let changed = Document::parse(raw).unwrap();
        assert!(
            intent
                .revalidate(&changed, Some("full"), Some(&policy), Some("V_ALL"))
                .is_err()
        );
    }
}

#[test]
fn captured_header_request_omits_view_filter_position_and_client_identity() {
    let (document, policy) = fixture();
    let workspace = WorkspaceSession {
        team_id: "T_TEST".into(),
        enterprise_id: None,
        user_id: "U_TEST".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let mut clock = ListClock::new();
    let timestamp = clock
        .tick(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        )
        .unwrap();
    for selected in [None, Some("V_ALL"), Some("V_DONE")] {
        let intent =
            CreationIntent::capture(&document, Some("full"), Some(&policy), selected).unwrap();
        let request =
            api::list_record_create(&SlackClient::default(), &workspace, &intent, &timestamp);
        assert!(!request.retry_safe());
        assert!(request.url.contains("/api/lists.records.create?"));
        let RequestBody::Form(fields) = request.body else {
            panic!("form expected")
        };
        assert!(fields.contains(&("position".into(), "".into())));
        assert!(fields.contains(&("updated_timestamp".into(), timestamp.to_string())));
        assert!(fields.contains(&("_x_reason".into(), "create-record".into())));
        assert!(!fields.iter().any(|(key, _)| matches!(
            key.as_str(),
            "fields" | "view_id" | "parent_record_id" | "duplicated_record_id" | "id" | "client_id"
        )));
        let initial = fields.iter().find(|(key, _)| key == "initial_fields");
        if selected == Some("V_DONE") {
            assert_eq!(
                serde_json::from_str::<Value>(&initial.unwrap().1).unwrap(),
                json!([{"column_id":"C_DONE","checkbox":true}])
            );
        } else {
            assert!(initial.is_none());
        }
    }
}

#[test]
fn response_needs_canonical_owned_top_level_record_and_valid_field_identities() {
    let (document, policy) = fixture();
    let intent = intent(&document, &policy);
    assert_eq!(intent.validate_response(&receipt()).unwrap().id, "R_NEW");
    for (path, value) in [
        ("/ok", json!(false)),
        ("/record/id", json!(" R_NEW")),
        ("/record/fields", json!(null)),
        ("/record/fields/0/key", json!("unknown")),
    ] {
        let mut response = receipt();
        *response.pointer_mut(path).unwrap() = value;
        assert!(intent.validate_response(&response).is_err());
    }
    for (key, value) in [
        ("list_id", json!("F_OTHER")),
        ("archived", json!(true)),
        ("parent_record_id", json!("PARENT")),
        ("is_deleted", json!(true)),
    ] {
        let mut response = receipt();
        response["record"][key] = value;
        assert!(intent.validate_response(&response).is_err());
    }
    for (key, value) in [
        ("column_id", json!("OTHER")),
        ("row_id", json!("OTHER")),
        ("error", json!("failure")),
    ] {
        let mut response = receipt();
        response["record"]["fields"][0][key] = value;
        assert!(intent.validate_response(&response).is_err());
    }
    assert!(intent.validate_response(&json!({"ok":true})).is_err());
}

#[test]
fn readback_uses_receipt_identity_and_creation_metadata_never_blank_title() {
    let (document, policy) = fixture();
    let intent = intent(&document, &policy);
    let record = intent.validate_response(&receipt()).unwrap();
    let page = RecordsPage::parse(json!({"ok":true,"records":[record.raw.clone()]})).unwrap();
    assert_eq!(
        intent
            .readback_record(&document, &page, &record)
            .unwrap()
            .id,
        record.id
    );
    for (key, value) in [
        ("id", json!("R_CONCURRENT")),
        ("date_created", json!(124)),
        ("created_by", json!("U_OTHER")),
        ("archived", json!(true)),
    ] {
        let mut row = record.raw.clone();
        row[key] = value;
        let page = RecordsPage::parse(json!({"ok":true,"records":[row]})).unwrap();
        assert!(intent.readback_record(&document, &page, &record).is_none());
    }
    let mut changed = document.clone();
    changed.id = "F_OTHER".into();
    assert!(intent.readback_record(&changed, &page, &record).is_none());
}

#[test]
fn completed_view_receipt_must_confirm_seed_but_later_edits_preserve_creation_identity() {
    let (document, policy) = fixture();
    let intent =
        CreationIntent::capture(&document, Some("full"), Some(&policy), Some("V_DONE")).unwrap();
    assert!(intent.validate_response(&receipt()).is_err());
    let mut response = receipt();
    response["record"]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"done","checkbox":true}));
    let record = intent.validate_response(&response).unwrap();
    for value in [json!(false), Value::Null, json!("true")] {
        response["record"]["fields"][1]["checkbox"] = value;
        assert!(intent.validate_response(&response).is_err());
    }
    // Another user unchecks the completed item after the successful creation.
    // That change does not erase the receipt's proof that this is our new row.
    response["record"]["fields"][1]["checkbox"] = json!(false);
    let page =
        RecordsPage::parse(json!({"ok":true,"records":[response["record"].clone()]})).unwrap();
    let confirmed = intent.readback_record(&document, &page, &record).unwrap();
    assert_eq!(confirmed.id, record.id);
    assert_eq!(confirmed.fields["done"]["checkbox"], false);
    let mut wrong_creator = page.raw.clone();
    wrong_creator["records"][0]["created_by"] = json!("OTHER_USER");
    let wrong_creator = RecordsPage::parse(wrong_creator).unwrap();
    assert!(
        intent
            .readback_record(&document, &wrong_creator, &record)
            .is_none()
    );
}

#[test]
fn readback_accepts_current_schema_additions_but_rejects_conflicting_field_identity() {
    let (document, policy) = fixture();
    let intent = intent(&document, &policy);
    let receipt = intent.validate_response(&receipt()).unwrap();
    let mut current = document.raw.clone();
    current["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"C_ADDED", "key":"added", "name":"New field", "type":"number"
        }));
    let current = Document::parse(current).unwrap();
    let mut row = receipt.raw.clone();
    row["fields"].as_array_mut().unwrap().push(json!({
        "key":"added", "column_id":"C_ADDED", "row_id":receipt.id, "number":[5]
    }));
    let page = RecordsPage::parse(json!({"ok":true,"records":[row.clone()]})).unwrap();
    let confirmed = intent.readback_record(&current, &page, &receipt).unwrap();
    assert_eq!(confirmed.fields["added"]["number"], json!([5]));
    for (key, value) in [
        ("column_id", json!("WRONG_COLUMN")),
        ("row_id", json!("WRONG_ROW")),
    ] {
        let mut conflict = row.clone();
        conflict["fields"][1][key] = value;
        let page = RecordsPage::parse(json!({"ok":true,"records":[conflict]})).unwrap();
        assert!(intent.readback_record(&current, &page, &receipt).is_none());
    }
    assert!(
        intent
            .revalidate(&current, Some("full"), Some(&policy), Some("V_ALL"))
            .is_err()
    );
}

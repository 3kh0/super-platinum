use super::*;
use serde_json::json;

fn fixture() -> (Document, RecordsPage, PersonalTaskLists) {
    let document = Document::parse(json!({"id":"F_LIST","mode":"list","access":"write","is_locked":false,
        "list_metadata":{"schema":[{"id":"C_NAME","key":"name","name":"Name","type":"text"}],"views":[]}})).unwrap();
    let page = RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"R_ITEM","list_id":"F_LIST","fields":[],"date_created":1},
        {"id":"R_CHILD","parent_record_id":"R_ITEM","fields":[]},
        {"id":"R_OTHER","fields":[]}]}))
    .unwrap();
    let policy = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    (document, page, policy)
}
fn intent(document: &Document, page: &RecordsPage, policy: &PersonalTaskLists) -> DeletionIntent {
    DeletionIntent::capture(document, page, "R_ITEM", Some("full"), Some(policy)).unwrap()
}

#[test]
fn deletion_requires_current_ordinary_permission_and_available_exact_target() {
    let (document, page, policy) = fixture();
    assert!(DeletionIntent::capture(&document, &page, "R_ITEM", None, Some(&policy)).is_err());
    assert!(DeletionIntent::capture(&document, &page, "R_ITEM", Some("full"), None).is_err());
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F_LIST\"]","assigned_to_list":""}}),
    )
    .unwrap();
    assert!(
        DeletionIntent::capture(&document, &page, "R_ITEM", Some("full"), Some(&personal)).is_err()
    );
    assert!(
        DeletionIntent::capture(&document, &page, "missing", Some("full"), Some(&policy)).is_err()
    );
    for (key, value) in [
        ("access", json!("read")),
        ("is_locked", json!(true)),
        ("is_locked", json!("false")),
    ] {
        let mut changed = document.clone();
        changed.raw[key] = value;
        assert!(
            DeletionIntent::capture(&changed, &page, "R_ITEM", Some("full"), Some(&policy))
                .is_err()
        );
    }
    for (key, value) in [
        ("archived", json!(true)),
        ("is_deleted", json!("false")),
        ("list_id", json!("F_OTHER")),
    ] {
        let mut changed = page.clone();
        changed.records[0].raw[key] = value;
        assert!(
            DeletionIntent::capture(&document, &changed, "R_ITEM", Some("full"), Some(&policy))
                .is_err()
        );
    }
}

#[test]
fn preflight_rejects_target_and_descendant_changes_but_not_unrelated_rows_or_last_read() {
    let (document, page, policy) = fixture();
    let captured = intent(&document, &page, &policy);
    let mut fresh_document = document.clone();
    fresh_document.raw["last_read"] = json!(5);
    let mut fresh = page.clone();
    fresh.records[2].raw["updated_timestamp"] = json!("new");
    assert!(
        captured
            .revalidate(&fresh_document, &fresh, Some("full"), Some(&policy))
            .is_ok()
    );
    for index in [0, 1] {
        let mut fresh = page.clone();
        fresh.records[index].raw["updated_timestamp"] = json!("changed");
        assert!(
            captured
                .revalidate(&document, &fresh, Some("full"), Some(&policy))
                .is_err()
        );
    }
    let mut fresh = page.clone();
    fresh.records[1].raw["parent_record_id"] = json!("R_OTHER");
    assert!(
        captured
            .revalidate(&document, &fresh, Some("full"), Some(&policy))
            .is_err()
    );
    let mut fresh = page.clone();
    fresh.next_cursor = Some("more".into());
    assert!(
        captured
            .revalidate(&document, &fresh, Some("full"), Some(&policy))
            .is_err()
    );
    fresh.records[0].raw["parent_record_id"] = json!("R_CHILD");
    assert!(
        DeletionIntent::capture(&document, &fresh, "R_ITEM", Some("full"), Some(&policy)).is_err()
    );
}

#[test]
fn added_descendants_are_detected_independently_of_record_order() {
    let (document, page, policy) = fixture();
    let captured = intent(&document, &page, &policy);
    let mut fresh = page.clone();
    fresh.records.reverse();
    assert!(
        captured
            .revalidate(&document, &fresh, Some("full"), Some(&policy))
            .is_ok()
    );
    fresh.records.push(
        Record::parse(json!({"id":"R_GRANDCHILD","parent_record_id":"R_CHILD","fields":[]}))
            .unwrap(),
    );
    assert!(
        captured
            .revalidate(&document, &fresh, Some("full"), Some(&policy))
            .is_err()
    );
}

#[test]
fn only_exact_record_deleted_envelope_proves_absence() {
    for response in [
        json!({"ok":false,"error":"record_not_found"}),
        json!({"ok":false,"error":"list_not_found"}),
        json!({"ok":false,"error":"access_denied"}),
        json!({"ok":true}),
        json!({"records":[]}),
        json!({"error":"record_deleted"}),
        json!({"ok":true,"error":"record_deleted"}),
        json!({"ok":false,"error":"record_deleted","record":{"id":"R_ITEM","fields":[]}}),
    ] {
        assert!(
            ListRecordReadback::from_response("F_LIST", "R_ITEM", &response).is_err(),
            "{response}"
        );
    }
    let deleted = ListRecordReadback::from_response(
        "F_LIST",
        "R_ITEM",
        &json!({"ok":false,"error":"record_deleted"}),
    )
    .unwrap();
    assert!(deleted.is_deleted());
    assert!(deleted.record().is_none());
    let present = ListRecordReadback::from_response("F_LIST", "R_ITEM", &json!({"ok":true,"list":{"id":"F_LIST"},"record":{"id":"R_ITEM","fields":[],"is_deleted":true}})).unwrap();
    assert!(!present.is_deleted());
}

#[test]
fn readback_and_accepted_receipts_cannot_settle_another_identity() {
    let (document, page, policy) = fixture();
    let captured = intent(&document, &page, &policy);
    let accepted = captured.validate_response(&json!({"ok":true})).unwrap();
    for (list, record) in [("F_OTHER", "R_ITEM"), ("F_LIST", "R_OTHER")] {
        let deleted = ListRecordReadback::from_response(
            list,
            record,
            &json!({"ok":false,"error":"record_deleted"}),
        )
        .unwrap();
        assert!(!captured.confirms_deleted(&deleted));
        assert!(!accepted.confirms_deleted(&deleted));
    }
    let deleted = ListRecordReadback::from_response(
        "F_LIST",
        "R_ITEM",
        &json!({"ok":false,"error":"record_deleted"}),
    )
    .unwrap();
    assert!(captured.confirms_deleted(&deleted));
    assert!(accepted.confirms_deleted(&deleted));
    for response in [
        json!({"ok":true,"record":{"id":"R_OTHER","fields":[]}}),
        json!({"ok":false,"error":"record_deleted","list_id":"F_OTHER"}),
        json!({"ok":false,"error":"record_deleted","list":{"id":"F_OTHER"}}),
    ] {
        assert!(ListRecordReadback::from_response("F_LIST", "R_ITEM", &response).is_err());
    }
}

#[test]
fn delete_ack_requires_success_and_rejects_conflicting_optional_identity() {
    let (document, page, policy) = fixture();
    let captured = intent(&document, &page, &policy);
    for response in [
        json!({}),
        json!({"ok":false}),
        json!({"ok":true,"error":"failed"}),
        json!({"ok":true,"id":"R_OTHER"}),
        json!({"ok":true,"list_id":"F_OTHER"}),
    ] {
        assert!(captured.validate_response(&response).is_err());
    }
    assert_eq!(
        captured
            .validate_response(&json!({"ok":true}))
            .unwrap()
            .record_id(),
        "R_ITEM"
    );
}

#[test]
fn immediate_delete_requires_complete_childless_page() {
    let (doc, mut page, policy) = fixture();
    assert!(!intent(&doc, &page, &policy).can_skip_delete_confirmation());
    page.records.retain(|record| record.id != "R_CHILD");
    assert!(intent(&doc, &page, &policy).can_skip_delete_confirmation());
    page.next_cursor = Some("next-page".into());
    let incomplete = intent(&doc, &page, &policy);
    assert!(!incomplete.has_known_subtasks());
    assert!(!incomplete.can_skip_delete_confirmation());
}

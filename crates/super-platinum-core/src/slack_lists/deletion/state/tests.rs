use super::*;
use serde_json::json;

fn setup() -> (
    Document,
    RecordsPage,
    PersonalTaskLists,
    DeletionIntent,
    ListRecordReadback,
) {
    let document = Document::parse(
        json!({"id":"F_LIST","mode":"list","access":"write","is_locked":false,
        "list_metadata":{"schema":[],"views":[]}}),
    )
    .unwrap();
    let page = RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"R_ITEM","list_id":"F_LIST","date_created":10,"created_by":"U1","fields":[]},
        {"id":"R_CHILD","parent_record_id":"R_ITEM","fields":[]}]}))
    .unwrap();
    let policy = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    let original =
        DeletionIntent::capture(&document, &page, "R_ITEM", Some("full"), Some(&policy)).unwrap();
    let deleted = ListRecordReadback::from_response(
        "F_LIST",
        "R_ITEM",
        &json!({"ok":false,"error":"record_deleted"}),
    )
    .unwrap();
    (document, page, policy, original, deleted)
}
fn present(record: Value) -> ListRecordReadback {
    ListRecordReadback::from_response("F_LIST", "R_ITEM", &json!({"ok":true,"record":record}))
        .unwrap()
}

#[test]
fn undo_requires_exact_tombstone_and_fresh_permission_not_page_absence() {
    let (doc, page, policy, original, deleted) = setup();
    assert!(original.has_known_subtasks());
    let existing = present(page.records[0].raw.clone());
    assert!(
        original
            .deletion_state_intent(&doc, &page, &existing, false, Some("full"), Some(&policy))
            .is_err()
    );
    let other = ListRecordReadback::from_response(
        "F_LIST",
        "R_OTHER",
        &json!({"ok":false,"error":"record_deleted"}),
    )
    .unwrap();
    assert!(
        original
            .deletion_state_intent(&doc, &page, &other, false, Some("full"), Some(&policy))
            .is_err()
    );
    assert!(
        original
            .deletion_state_intent(&doc, &page, &deleted, false, Some("full"), None)
            .is_err()
    );
    let undo = original
        .deletion_state_intent(&doc, &page, &deleted, false, Some("full"), Some(&policy))
        .unwrap();
    let mut changed = doc.clone();
    changed.raw["last_read"] = json!(44);
    assert!(
        undo.revalidate(&changed, &page, &deleted, Some("full"), Some(&policy))
            .is_ok()
    );
    assert!(
        undo.revalidate(&doc, &page, &existing, Some("full"), Some(&policy))
            .is_err()
    );
    assert!(
        undo.revalidate(&doc, &page, &deleted, Some("read"), Some(&policy))
            .is_err()
    );
    changed.raw["title"] = json!("Changed");
    assert!(
        undo.revalidate(&changed, &page, &deleted, Some("full"), Some(&policy))
            .is_err()
    );
}

#[test]
fn restore_receipt_matches_stable_original_identity_but_not_old_fields() {
    let (doc, page, policy, original, deleted) = setup();
    let undo = original
        .deletion_state_intent(&doc, &page, &deleted, false, Some("full"), Some(&policy))
        .unwrap();
    let accepted = undo.validate_response(&json!({"ok":true})).unwrap();
    assert!(!accepted.deleted());
    assert!(!accepted.confirms_readback(&deleted));
    let mut record = page.records[0].raw.clone();
    record["fields"] = json!([{"column_id":"C_NAME","key":"name","rich_text":[]}]);
    record["updated_timestamp"] = json!(99);
    assert!(accepted.confirms_readback(&present(record.clone())));
    for (key, value) in [
        ("date_created", json!(11)),
        ("created_by", json!("U2")),
        ("parent_record_id", json!("R_OTHER")),
        ("is_deleted", json!(true)),
        ("archived", json!(true)),
    ] {
        let mut altered = record.clone();
        altered[key] = value;
        assert!(!undo.confirms_readback(&present(altered)), "{key}");
    }
    for response in [
        json!({"ok":false}),
        json!({"ok":true,"id":"R_OTHER"}),
        json!({"ok":true,"list_id":"F_OTHER"}),
        json!({"ok":true,"deleted":true}),
        json!({"ok":true,"deleted":"false"}),
        json!({"ok":true,"error":"failed"}),
    ] {
        assert!(undo.validate_response(&response).is_err());
    }
}

#[test]
fn redo_pins_fresh_restored_target_and_descendants_then_requires_exact_tombstone() {
    let (doc, page, policy, original, deleted) = setup();
    assert!(
        original
            .deletion_state_intent(&doc, &page, &deleted, true, Some("full"), Some(&policy))
            .is_err()
    );
    let current = present(page.records[0].raw.clone());
    let redo = original
        .deletion_state_intent(&doc, &page, &current, true, Some("full"), Some(&policy))
        .unwrap();
    assert!(redo.deleted());
    let accepted = redo
        .validate_response(&json!({"ok":true,"deleted":true}))
        .unwrap();
    assert!(accepted.confirms_readback(&deleted));
    assert!(!accepted.confirms_readback(&current));
    let mut changed = page.clone();
    changed.records[1].raw["fields"] = json!([{"key":"name","value":"changed"}]);
    assert!(
        redo.revalidate(&doc, &changed, &current, Some("full"), Some(&policy))
            .is_err()
    );
    let mut changed = page.clone();
    changed.records[0].raw["date_created"] = json!(999);
    let replacement = present(changed.records[0].raw.clone());
    assert!(
        original
            .deletion_state_intent(
                &doc,
                &changed,
                &replacement,
                true,
                Some("full"),
                Some(&policy)
            )
            .is_err()
    );
    let mut changed = page.clone();
    changed.records[0].raw["fields"] = json!([{"key":"name","value":"new"}]);
    assert!(
        original
            .deletion_state_intent(&doc, &changed, &current, true, Some("full"), Some(&policy))
            .is_err()
    );
}

#[test]
fn restored_root_accepts_source_nullish_parent_shapes_without_accepting_reparenting() {
    let (doc, page, policy, original, deleted) = setup();
    let undo = original
        .deletion_state_intent(&doc, &page, &deleted, false, Some("full"), Some(&policy))
        .unwrap();
    for parent in [Value::Null, json!("")] {
        let mut record = page.records[0].raw.clone();
        record["parent_record_id"] = parent;
        assert!(undo.confirms_readback(&present(record)));
    }
    for parent in [
        json!("R_OTHER"),
        json!(false),
        json!([]),
        json!("bad parent"),
    ] {
        let mut record = page.records[0].raw.clone();
        record["parent_record_id"] = parent;
        assert!(!undo.confirms_readback(&present(record)));
    }
    let mut childpage = page.clone();
    childpage.records[0].raw["parent_record_id"] = json!("R_PARENT");
    let child =
        DeletionIntent::capture(&doc, &childpage, "R_ITEM", Some("full"), Some(&policy)).unwrap();
    let restore = child
        .deletion_state_intent(
            &doc,
            &childpage,
            &deleted,
            false,
            Some("full"),
            Some(&policy),
        )
        .unwrap();
    assert!(restore.confirms_readback(&present(childpage.records[0].raw.clone())));
    assert!(!restore.confirms_readback(&present(page.records[0].raw.clone())));
}

#[test]
fn redo_accepts_info_subscription_metadata_but_not_content_or_unknown_drift() {
    let (doc, page, policy, original, _) = setup();
    let mut raw = page.records[0].raw.clone();
    raw["is_subscribed"] = json!(false);
    raw["parent_record_id"] = Value::Null;
    let info = present(raw.clone());
    let redo = original
        .deletion_state_intent(&doc, &page, &info, true, Some("full"), Some(&policy))
        .unwrap();
    assert!(
        redo.revalidate(&doc, &page, &info, Some("full"), Some(&policy))
            .is_ok()
    );
    // Only page/info comparison ignores subscription. The fresh preflight's
    // exact captured info is still pinned, including its read-state metadata.
    let mut subscribed = raw.clone();
    subscribed["is_subscribed"] = json!(true);
    assert!(
        redo.revalidate(
            &doc,
            &page,
            &present(subscribed),
            Some("full"),
            Some(&policy)
        )
        .is_err()
    );
    for (key, value) in [
        ("fields", json!([{"key":"name","value":"changed"}])),
        ("position", json!("different")),
        ("unknown_metadata", json!(true)),
        ("parent_record_id", json!("R_PARENT")),
    ] {
        let mut changed = raw.clone();
        changed[key] = value;
        assert!(
            original
                .deletion_state_intent(
                    &doc,
                    &page,
                    &present(changed),
                    true,
                    Some("full"),
                    Some(&policy)
                )
                .is_err(),
            "{key}"
        );
    }
}

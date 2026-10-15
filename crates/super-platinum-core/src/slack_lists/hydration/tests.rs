use super::*;
use serde_json::json;

fn document() -> Document {
    Document::parse(json!({"id":"F_LIST","mode":"list","access":"write",
        "list_limits":{"over_row_maximum":false},
        "list_metadata":{"schema":[{"id":"C_NAME","key":"name","name":"Name","type":"text"}],"views":[]}}))
    .unwrap()
}
fn row(id: &str, parent: Option<&str>) -> Value {
    let mut value = json!({"id":id,"list_id":"F_LIST","fields":[],
        "date_created":1,"created_by":"U_TEST","position":"a1",
        "view_positions":{"V":"b2"},"future":{"keep":true}});
    if let Some(parent) = parent {
        value["parent_record_id"] = json!(parent);
    }
    value
}
fn page() -> RecordsPage {
    RecordsPage::parse(json!({"ok":true,"records":[row("R_PARENT",None),
        row("R_OLD",Some("R_PARENT")),row("R_OTHER",None)],
        "response_metadata":{"next_cursor":"opaque+/=","future":7},"retained":"yes"}))
    .unwrap()
}
fn response() -> Value {
    json!({"ok":true,"error":null,"list":document().raw,"record":row("R_PARENT",None),
        "subtasks":[row("R_NEW",Some("R_PARENT"))]})
}
fn parse(raw: &Value) -> Result<ListRecordHydration, ListError> {
    ListRecordHydration::from_response("F_LIST", "R_PARENT", raw)
}

#[test]
fn merges_positive_children_without_deleting_absent_rows_or_changing_cursor() {
    let before = page();
    let raw = response();
    let (doc, merged) = parse(&raw).unwrap().merge(&document(), &before).unwrap();
    assert_eq!(doc, document());
    assert_eq!(merged.records.len(), 4);
    assert_eq!(merged.record("R_OLD"), before.record("R_OLD"));
    assert_eq!(merged.record("R_OTHER"), before.record("R_OTHER"));
    assert_eq!(merged.record("R_NEW").unwrap().raw, raw["subtasks"][0]);
    assert_eq!(
        merged.raw["response_metadata"],
        before.raw["response_metadata"]
    );
    assert_eq!(merged.raw["retained"], "yes");
    assert_eq!(merged.next_cursor, before.next_cursor);
    assert!(!merged.complete());
    let personal = PersonalTaskLists::from_response(&json!({"ok":true,
        "prefs":{"task_lists":"[]","assigned_to_list":""}}))
    .unwrap();
    let deletion =
        DeletionIntent::capture(&doc, &merged, "R_PARENT", Some("full"), Some(&personal)).unwrap();
    assert!(!deletion.can_skip_delete_confirmation());
    let prior = DeletionIntent::capture(
        &document(),
        &before,
        "R_PARENT",
        Some("full"),
        Some(&personal),
    )
    .unwrap();
    assert!(
        prior
            .revalidate(&doc, &merged, Some("full"), Some(&personal))
            .is_err()
    );
    assert_eq!(
        CreationIntent::duplicate_eligible(
            &doc,
            &merged,
            "R_PARENT",
            Some("full"),
            Some(&personal),
            None
        )
        .unwrap_err()
        .0,
        "Load all List items before duplicating an item."
    );
}

#[test]
fn wrong_owner_parent_and_contradictory_envelopes_are_rejected() {
    for (pointer, value) in [
        ("/ok", json!(false)),
        ("/error", json!("denied")),
        ("/list/id", json!("F_OTHER")),
        ("/record/id", json!("R_OTHER")),
        ("/record/list_id", json!("F_OTHER")),
        ("/subtasks/0/list_id", json!("F_OTHER")),
        ("/subtasks/0/parent_record_id", json!("R_OTHER")),
    ] {
        let mut raw = response();
        *raw.pointer_mut(pointer).unwrap() = value;
        assert!(parse(&raw).is_err(), "{pointer}");
    }
    for (key, value) in [("id", json!("R_OTHER")), ("list_id", json!("F_OTHER"))] {
        let mut raw = response();
        raw[key] = value;
        assert!(parse(&raw).is_err());
    }
}

#[test]
fn unavailable_malformed_and_duplicate_subtasks_are_not_adopted() {
    for value in [json!({}), json!(false), json!("partial")] {
        let mut raw = response();
        raw["subtasks"] = value;
        assert!(parse(&raw).is_err());
    }
    for pointer in ["/record", "/subtasks/0"] {
        for marker in ["archived", "is_deleted"] {
            let mut raw = response();
            raw.pointer_mut(pointer).unwrap()[marker] = json!(true);
            assert!(parse(&raw).is_err());
        }
    }
    let mut raw = response();
    raw["list"]["mode"] = json!("tombstone");
    assert!(parse(&raw).is_err());
    let mut raw = response();
    raw["subtasks"] = json!([
        row("R_NEW", Some("R_PARENT")),
        row("R_NEW", Some("R_PARENT"))
    ]);
    assert!(parse(&raw).is_err());
    raw["subtasks"] = json!([row("R_PARENT", Some("R_PARENT"))]);
    assert!(parse(&raw).is_err());
    raw["subtasks"] = json!([row("R_NEW", None)]);
    assert!(parse(&raw).is_err());
}

#[test]
fn missing_or_null_subtasks_never_prove_absent_children_deleted() {
    for absent in [true, false] {
        let mut raw = response();
        if absent {
            raw.as_object_mut().unwrap().remove("subtasks");
        } else {
            raw["subtasks"] = Value::Null;
        }
        let (_, merged) = parse(&raw).unwrap().merge(&document(), &page()).unwrap();
        assert_eq!(merged, page());
    }
}

#[test]
fn changed_incarnation_or_existing_other_parent_collision_refuses_entire_merge() {
    for (pointer, value) in [
        ("/record/date_created", json!(2)),
        ("/record/created_by", json!("OTHER")),
    ] {
        let mut raw = response();
        *raw.pointer_mut(pointer).unwrap() = value;
        assert!(parse(&raw).unwrap().merge(&document(), &page()).is_err());
    }
    let mut raw = response();
    raw["subtasks"][0]["id"] = json!("R_OTHER");
    assert!(parse(&raw).unwrap().merge(&document(), &page()).is_err());
    let mut raw = response();
    raw["record"]["parent_record_id"] = json!("R_OTHER");
    assert!(parse(&raw).unwrap().merge(&document(), &page()).is_err());
}

#[test]
fn complete_unknown_parent_and_wrong_document_cannot_use_hydration() {
    let hydration = parse(&response()).unwrap();
    let mut raw = page().raw;
    raw["response_metadata"]["next_cursor"] = json!("");
    assert!(
        hydration
            .merge(&document(), &RecordsPage::parse(raw).unwrap())
            .is_err()
    );
    let mut raw = page().raw;
    raw["records"] = json!([row("R_OTHER", None)]);
    assert!(
        hydration
            .merge(&document(), &RecordsPage::parse(raw).unwrap())
            .is_err()
    );
    let mut doc = document();
    doc.id = "OTHER".into();
    assert!(hydration.merge(&doc, &page()).is_err());
}

#[test]
fn positive_list_metadata_and_record_fields_are_preserved_losslessly() {
    let mut raw = response();
    raw["list"]["access"] = json!("read");
    raw["list"]["future"] = json!({"opaque":true});
    raw["record"]["fields"] = json!([{"key":"future","unknown":[1,2]}]);
    let (doc, merged) = parse(&raw).unwrap().merge(&document(), &page()).unwrap();
    assert_eq!(doc.raw, raw["list"]);
    assert_eq!(merged.record("R_PARENT").unwrap().raw, raw["record"]);
    assert!(!doc.editable(Some("full")));
}

#[test]
fn missing_ancestor_cannot_be_adopted_as_child_and_create_cycle() {
    let mut before = page().raw;
    before["records"][0]["parent_record_id"] = json!("R_NEW");
    let before = RecordsPage::parse(before).unwrap();
    let mut raw = response();
    raw["record"]["parent_record_id"] = json!("R_NEW");
    assert!(parse(&raw).unwrap().merge(&document(), &before).is_err());
}

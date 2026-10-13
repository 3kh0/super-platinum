use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{
        api,
        client::{RequestBody, SlackClient},
    },
};

fn policy() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn fixture(format: Option<Value>, stored: Option<Value>) -> (Document, RecordsPage) {
    let mut column = json!({"id":"COL","key":"reference","name":"Channel","type":"channel"});
    if let Some(format) = format {
        column["options"] = json!({"format":format});
    }
    let doc = Document::parse(
        json!({"id":"F_LIST","mode":"list","access":"write","list_metadata":{"schema":[column]}}),
    )
    .unwrap();
    let mut fields = vec![json!({"key":"unrelated","number":[17],"future":{"retain":true}})];
    if let Some(value) = stored {
        fields.push(json!({"key":"reference","column_id":"COL","channel":value}));
    }
    let page=RecordsPage::parse(json!({"ok":true,"records":[{"id":"ROW","created_by":"U00000001","date_created":1,"fields":fields}]})).unwrap();
    (doc, page)
}
fn channels(ids: &[&str]) -> CellEditValue {
    CellEditValue::Channels(ids.iter().map(|id| (*id).into()).collect())
}
fn capture(doc: &Document, page: &RecordsPage, ids: &[&str]) -> Result<CellEdit, ListError> {
    CellEdit::capture(
        doc,
        page,
        "ROW",
        "COL",
        Some("full"),
        Some(&policy()),
        channels(ids),
    )
}
fn read(doc: &Document, page: &RecordsPage) -> Result<CellEditValue, ListError> {
    doc.columns[0].edit_value(&page.records[0], &doc.id, Some(&policy()))
}

#[test]
fn channel_default_single_clear_and_wire_are_reference_only() {
    for format in [None, Some(Value::Null), Some(json!("single_entity"))] {
        for empty in [None, Some(Value::Null), Some(json!([]))] {
            let (doc, page) = fixture(format.clone(), empty);
            assert!(!doc.columns[0].is_multi_channel());
            assert_eq!(read(&doc, &page).unwrap(), channels(&[]));
            assert!(capture(&doc, &page, &[]).is_err());
            let edit = capture(&doc, &page, &["C00000001"]).unwrap();
            assert_eq!(
                edit.cells(),
                json!([{"row_id":"ROW","column_id":"COL","channel":["C00000001"]}])
            );
            assert!(capture(&doc, &page, &["C00000001", "G00000002"]).is_err());
        }
    }
    let (doc, page) = fixture(None, Some(json!(["G00000002"])));
    let original = page.clone();
    let clear = capture(&doc, &page, &[]).unwrap();
    assert_eq!(
        clear.cells(),
        json!([{"row_id":"ROW","column_id":"COL","channel":[]}])
    );
    assert_eq!(page, original);
    let workspace = WorkspaceSession {
        team_id: "T_TEST".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "test".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = api::list_cell_update(&SlackClient::default(), &workspace, &clear);
    assert!(!request.retry_safe());
    assert!(request.url.contains("/api/lists.cells.update?"));
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    let wire: Value =
        serde_json::from_str(&fields.iter().find(|(k, _)| k == "cells").unwrap().1).unwrap();
    assert_eq!(wire, clear.cells());
    assert_eq!(
        wire[0].as_object().unwrap().len(),
        3,
        "no create/join/share arguments"
    );
}

#[test]
fn channel_canonical_prefixes_and_retained_unknown_references_are_distinct_from_users() {
    let ids = ["C00000001", "G00000002", "D00000003", "Z00000004"];
    let (doc, page) = fixture(Some(json!("multi_entity")), Some(json!(ids)));
    assert!(doc.columns[0].is_multi_channel());
    assert!(!doc.columns[0].is_multi_user());
    assert_eq!(read(&doc, &page).unwrap(), channels(&ids));
    assert!(
        capture(&doc, &page, &["Z00000004"]).is_ok(),
        "removal requires no candidate-cache membership"
    );
    for bad in [
        "",
        "C1",
        "C0000000",
        "c00000001",
        "C0000000é",
        "C0000000_",
        "U00000001",
        "W00000001",
        "#C00000001",
        "C00000001|G00000002",
        " C00000001",
    ] {
        assert!(capture(&doc, &page, &[bad]).is_err(), "{bad:?}");
        let (doc, page) = fixture(Some(json!("multi_entity")), Some(json!([bad])));
        assert!(read(&doc, &page).is_err());
    }
    assert!(capture(&doc, &page, &[ids[0], ids[0]]).is_err());
    assert!(
        CellEdit::capture(
            &doc,
            &page,
            "ROW",
            "COL",
            Some("full"),
            Some(&policy()),
            CellEditValue::Users(vec!["U00000001".into()])
        )
        .is_err()
    );
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["schema"][0]["type"] = json!("user");
    let users = Document::parse(raw).unwrap();
    assert!(
        read(&users, &page).is_err(),
        "shared helper retains distinct user prefixes"
    );
}

#[test]
fn channel_malformed_cardinality_and_schema_cannot_be_overwritten() {
    for value in [
        json!("C00000001"),
        json!([1]),
        json!([null]),
        json!([{"id":"C00000001"}]),
        json!(["C00000001", "G00000002"]),
    ] {
        let (doc, page) = fixture(None, Some(value));
        assert!(read(&doc, &page).is_err());
        assert!(capture(&doc, &page, &[]).is_err());
    }
    for format in [
        json!("future"),
        json!("multi_select"),
        json!(false),
        json!([]),
        json!(""),
    ] {
        let (doc, page) = fixture(Some(format), None);
        assert!(capture(&doc, &page, &["C00000001"]).is_err());
    }
    let (doc, page) = fixture(None, None);
    for options in [json!(false), json!("multi_entity"), json!([])] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["schema"][0]["options"] = options;
        assert!(capture(&Document::parse(raw).unwrap(), &page, &["C00000001"]).is_err());
    }
}

#[test]
fn channel_preflight_receipts_and_clear_readback_preserve_existing_guards() {
    let (doc, page) = fixture(Some(json!("multi_entity")), Some(json!(["C00000001"])));
    let edit = capture(&doc, &page, &["G00000002", "C00000001"]).unwrap();
    assert!(
        edit.revalidate(&doc, &page, Some("full"), Some(&policy()))
            .is_ok()
    );
    assert!(
        edit.revalidate(&doc, &page, Some("read"), Some(&policy()))
            .is_err()
    );
    assert!(edit.revalidate(&doc, &page, Some("full"), None).is_err());
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F_LIST\"]","assigned_to_list":""}}),
    )
    .unwrap();
    assert!(
        edit.revalidate(&doc, &page, Some("full"), Some(&personal))
            .is_err()
    );
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["schema"][0]["options"]["format"] = json!("single_entity");
    assert!(
        edit.revalidate(
            &Document::parse(raw).unwrap(),
            &page,
            Some("full"),
            Some(&policy())
        )
        .is_err()
    );
    let (_, fresh) = fixture(
        Some(json!("multi_entity")),
        Some(json!(["G00000002", "C00000001"])),
    );
    assert!(
        edit.revalidate(&doc, &fresh, Some("full"), Some(&policy()))
            .is_err()
    );
    assert!(edit.readback_matches(&doc, &fresh));
    let receipt = json!({"ok":true,"cells":[{"row_id":"ROW","column_id":"COL","key":"reference","channel":["G00000002","C00000001"]}]});
    assert!(edit.validate_response(&receipt).is_ok());
    for value in [
        json!([]),
        json!(null),
        json!("G00000002"),
        json!(["C00000001", "G00000002"]),
    ] {
        let mut wrong = receipt.clone();
        wrong["cells"][0]["channel"] = value;
        assert!(edit.validate_response(&wrong).is_err());
    }
    for key in ["row_id", "column_id", "key"] {
        let mut wrong = receipt.clone();
        wrong["cells"][0][key] = json!("other");
        assert!(edit.validate_response(&wrong).is_err());
    }
    assert!(edit.validate_response(&json!({"ok":true})).is_err());
    let mut replaced = fresh.clone();
    replaced.records[0].raw["created_by"] = json!("U00000002");
    assert!(edit.readback_value(&doc, &replaced).is_none());
    let clear = capture(&doc, &page, &[]).unwrap();
    let (_, empty) = fixture(Some(json!("multi_entity")), Some(json!([])));
    assert!(clear.readback_matches(&doc, &empty));
    assert!(
        clear
            .validate_response(
                &json!({"ok":true,"cells":[{"row_id":"ROW","column_id":"COL","channel":[]}]})
            )
            .is_ok()
    );
}

use super::*;
use crate::config::WorkspaceSession;
use crate::slack::{
    api,
    client::{RequestBody, SlackClient},
};

const FIRST: &str = "U00000001";
const SECOND: &str = "W00000002";

fn policy() -> PersonalTaskLists {
    PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{
        "task_lists":"[]","assigned_to_list":""
    }}))
    .unwrap()
}
fn fixture(format: Option<Value>, field: Option<Value>) -> (Document, RecordsPage) {
    let mut column = json!({"id":"COL_USER","key":"reviewers","name":"Reviewers","type":"user"});
    if let Some(format) = format {
        column["options"] = json!({"format":format});
    }
    let document = Document::parse(json!({"id":"F_USERS","mode":"list","access":"write",
        "list_metadata":{"schema":[column]}}))
    .unwrap();
    let fields: Vec<_> = field
        .into_iter()
        .map(|mut field| {
            field["key"] = json!("reviewers");
            field["column_id"] = json!("COL_USER");
            field["row_id"] = json!("ROW");
            field
        })
        .collect();
    let page = RecordsPage::parse(json!({"ok":true,"records":[{
        "id":"ROW","date_created":1,"created_by":FIRST,"fields":fields
    }]}))
    .unwrap();
    (document, page)
}
fn users(ids: &[&str]) -> CellEditValue {
    CellEditValue::Users(ids.iter().map(|id| (*id).into()).collect())
}
fn capture(doc: &Document, page: &RecordsPage, ids: &[&str]) -> Result<CellEdit, ListError> {
    CellEdit::capture(
        doc,
        page,
        "ROW",
        "COL_USER",
        Some("full"),
        Some(&policy()),
        users(ids),
    )
}
fn read(doc: &Document, page: &RecordsPage) -> Result<CellEditValue, ListError> {
    doc.columns[0].edit_value(&page.records[0], &doc.id, Some(&policy()))
}

#[test]
fn user_empty_seed_single_default_and_exact_wire_do_not_touch_sibling_fields() {
    for format in [None, Some(Value::Null), Some(json!("single_entity"))] {
        for field in [
            None,
            Some(json!({})),
            Some(json!({"user":null})),
            Some(json!({"user":[]})),
        ] {
            let (doc, page) = fixture(format.clone(), field);
            assert!(!doc.columns[0].is_multi_user());
            assert_eq!(read(&doc, &page).unwrap(), users(&[]));
            assert!(capture(&doc, &page, &[]).is_err(), "no-op is not a write");
            let original = page.clone();
            let edit = capture(&doc, &page, &[FIRST]).unwrap();
            assert_eq!(
                edit.cells(),
                json!([{"row_id":"ROW","column_id":"COL_USER","user":[FIRST]}])
            );
            assert_eq!(page, original);
            assert!(capture(&doc, &page, &[FIRST, SECOND]).is_err());
        }
    }
    let (doc, mut page) = fixture(
        None,
        Some(json!({"user":[FIRST],"future_metadata":{"preserve":true}})),
    );
    page.records[0].raw["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"other","number":[17]}));
    page.records[0] = Record::parse(page.records[0].raw.clone()).unwrap();
    let original = page.clone();
    let edit = capture(&doc, &page, &[]).unwrap();
    assert_eq!(
        edit.cells(),
        json!([{"row_id":"ROW","column_id":"COL_USER","user":[]}])
    );
    assert_eq!(
        page, original,
        "capturing does not mutate raw cells or unknown metadata"
    );
    let workspace = WorkspaceSession {
        team_id: "T_TEST".into(),
        enterprise_id: None,
        user_id: FIRST.into(),
        name: "test".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = api::list_cell_update(&SlackClient::default(), &workspace, &edit);
    assert!(!request.retry_safe());
    assert!(request.url.contains("/api/lists.cells.update?"));
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    let wire: Value =
        serde_json::from_str(&fields.iter().find(|(key, _)| key == "cells").unwrap().1).unwrap();
    assert_eq!(wire, edit.cells());
    assert!(wire[0].get("row_id_to_create").is_none());
}

#[test]
fn multi_user_retains_unknown_canonical_ids_and_order_but_rejects_duplicate_or_noncanonical_ids() {
    let (doc, page) = fixture(
        Some(json!("multi_entity")),
        Some(json!({"user":[FIRST,SECOND]})),
    );
    assert!(doc.columns[0].is_multi_user());
    assert_eq!(read(&doc, &page).unwrap(), users(&[FIRST, SECOND]));
    assert!(
        capture(&doc, &page, &[SECOND]).is_ok(),
        "no directory lookup is needed to remove an unknown user"
    );
    assert!(capture(&doc, &page, &[SECOND, FIRST]).is_ok());
    for ids in [
        vec![FIRST, FIRST],
        vec![""],
        vec!["U1"],
        vec!["U0000000"],
        vec!["u00000001"],
        vec!["U0000000é"],
        vec!["U0000000_"],
        vec!["C00000001"],
        vec!["@U00000001"],
        vec!["USLACK"],
        vec!["U00000001|W00000002"],
        vec![" U00000001"],
    ] {
        assert!(
            capture(&doc, &page, &ids).is_err(),
            "invalid desired IDs: {ids:?}"
        );
        let (bad_doc, bad_page) = fixture(Some(json!("multi_entity")), Some(json!({"user":ids})));
        assert!(read(&bad_doc, &bad_page).is_err(), "invalid stored IDs");
    }
}

#[test]
fn malformed_stored_values_formats_and_specialized_types_stay_read_only() {
    for value in [
        json!(FIRST),
        json!([null]),
        json!([1]),
        json!([{"id":FIRST}]),
        json!([FIRST, SECOND]),
    ] {
        let (doc, page) = fixture(None, Some(json!({"user":value})));
        assert!(read(&doc, &page).is_err());
        assert!(capture(&doc, &page, &[]).is_err());
    }
    for format in [
        json!("future"),
        json!("multi_select"),
        json!(""),
        json!(1),
        json!([]),
    ] {
        let (doc, page) = fixture(Some(format), None);
        assert!(read(&doc, &page).is_err());
        assert!(capture(&doc, &page, &[FIRST]).is_err());
    }
    let (doc, page) = fixture(None, None);
    for options in [json!(true), json!("single_entity"), json!([])] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["schema"][0]["options"] = options;
        assert!(read(&Document::parse(raw).unwrap(), &page).is_err());
    }
    for kind in ["todo_assignee", "assignee", "created_by", "last_edited_by"] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["schema"][0]["type"] = json!(kind);
        assert!(capture(&Document::parse(raw).unwrap(), &page, &[FIRST]).is_err());
    }
}

#[test]
fn user_edit_requires_verified_ordinary_policy_permission_and_exact_preflight_snapshot() {
    let (doc, page) = fixture(Some(json!("multi_entity")), Some(json!({"user":[FIRST]})));
    let edit = capture(&doc, &page, &[SECOND]).unwrap();
    assert!(
        edit.revalidate(&doc, &page, Some("full"), Some(&policy()))
            .is_ok()
    );
    assert!(
        CellEdit::capture(
            &doc,
            &page,
            "ROW",
            "COL_USER",
            Some("full"),
            None,
            users(&[SECOND])
        )
        .is_err()
    );
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F_USERS\"]","assigned_to_list":""}}),
    )
    .unwrap();
    assert!(
        CellEdit::capture(
            &doc,
            &page,
            "ROW",
            "COL_USER",
            Some("full"),
            Some(&personal),
            users(&[SECOND])
        )
        .is_err()
    );
    assert!(
        edit.revalidate(&doc, &page, Some("read"), Some(&policy()))
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
    let (_, changed) = fixture(Some(json!("multi_entity")), Some(json!({"user":[SECOND]})));
    assert!(
        edit.revalidate(&doc, &changed, Some("full"), Some(&policy()))
            .is_err()
    );
    let mut raw = doc.raw.clone();
    raw["last_read"] = json!(123);
    assert!(
        edit.revalidate(
            &Document::parse(raw).unwrap(),
            &page,
            Some("full"),
            Some(&policy())
        )
        .is_ok()
    );
}

#[test]
fn user_receipts_and_readback_require_exact_target_typed_array_and_immutable_identity() {
    let (doc, page) = fixture(Some(json!("multi_entity")), Some(json!({"user":[FIRST]})));
    let edit = capture(&doc, &page, &[SECOND, FIRST]).unwrap();
    let receipt = json!({"ok":true,"cells":[{"row_id":"ROW","column_id":"COL_USER","key":"reviewers","user":[SECOND,FIRST]}]});
    assert!(edit.validate_response(&receipt).is_ok());
    for value in [
        json!(null),
        json!(SECOND),
        json!([]),
        json!([FIRST, SECOND]),
        json!([SECOND, SECOND]),
    ] {
        let mut wrong = receipt.clone();
        wrong["cells"][0]["user"] = value;
        assert!(edit.validate_response(&wrong).is_err());
    }
    for key in ["row_id", "column_id", "key"] {
        let mut wrong = receipt.clone();
        wrong["cells"][0][key] = json!("OTHER");
        assert!(edit.validate_response(&wrong).is_err());
    }
    assert!(edit.validate_response(&json!({"ok":true})).is_err());
    let (_, canonical) = fixture(
        Some(json!("multi_entity")),
        Some(json!({"user":[SECOND,FIRST]})),
    );
    assert!(edit.readback_matches(&doc, &canonical));
    let (_, different) = fixture(Some(json!("multi_entity")), Some(json!({"user":[SECOND]})));
    assert!(!edit.readback_matches(&doc, &different));
    assert_eq!(
        edit.readback_value(&doc, &different),
        Some(users(&[SECOND]))
    );
    let mut replaced = canonical.clone();
    replaced.records[0].raw["date_created"] = json!(2);
    assert!(edit.readback_value(&doc, &replaced).is_none());
    let clear = capture(&doc, &page, &[]).unwrap();
    assert!(
        clear
            .validate_response(
                &json!({"ok":true,"cells":[{"row_id":"ROW","column_id":"COL_USER","user":[]}]})
            )
            .is_ok()
    );
    let (_, empty) = fixture(Some(json!("multi_entity")), Some(json!({"user":[]})));
    assert!(clear.readback_matches(&doc, &empty));
}

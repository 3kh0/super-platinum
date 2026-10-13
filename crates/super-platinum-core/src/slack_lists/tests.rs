use super::*;
use serde_json::json;

fn file() -> Value {
    json!({"id":"F_LIST","mode":"list","title":"Release plan","access":"write","is_locked":false,"unknown_file":{"keep":1},"list_metadata":{"schema":[
    {"id":"ColTitle","key":"name","name":"Name","type":"rich_text","is_primary_column":true},
    {"id":"ColDone","key":"done","name":"Done","type":"checkbox"},
    {"id":"ColCount","key":"count","name":"Count","type":"number"},
    {"id":"ColFuture","key":"future","name":"Future","type":"future_kind"}
],"views":[{"id":"ViewMain","type":"table","unknown_view":true}]}})
}
fn row() -> Value {
    json!({"id":"RecOne","date_created":100,"created_by":"U1","position":"a","updated_timestamp":"100.0","unknown_record":true,"fields":[
        {"key":"name","column_id":"ColTitle","rich_text":[],"text":"Fallback"},
        {"key":"done","column_id":"ColDone","checkbox":false},
        {"key":"count","column_id":"ColCount","number":[0]},
        {"key":"future","column_id":"ColFuture","future_value":{"raw":1},"text":"Future fallback"}
    ]})
}
fn page_with(rows: Vec<Value>) -> RecordsPage {
    RecordsPage::parse(json!({"ok":true,"records":rows})).unwrap()
}
fn fixture() -> (Document, RecordsPage) {
    (Document::parse(file()).unwrap(), page_with(vec![row()]))
}

#[test]
fn preserves_unknown_data_and_distinguishes_empty_false_zero_and_unsupported() {
    let (doc, page) = fixture();
    let record = &page.records[0];
    assert_eq!(doc.raw["unknown_file"]["keep"], 1);
    assert_eq!(doc.views[0]["unknown_view"], true);
    assert_eq!(record.raw["unknown_record"], true);
    assert_eq!(record.fallback("name"), Some("Fallback"));
    assert_eq!(
        doc.column("ColTitle").unwrap().cell(record),
        CellValue::RichText(vec![])
    );
    assert_eq!(
        doc.column("ColDone").unwrap().cell(record),
        CellValue::Boolean(false)
    );
    assert_eq!(
        doc.column("ColCount").unwrap().cell(record),
        CellValue::Number(0.into())
    );
    assert!(matches!(
        doc.column("ColFuture").unwrap().cell(record),
        CellValue::Unsupported(_)
    ));
    assert_eq!(
        doc.column("ColFuture").unwrap().display_text(record),
        "Future fallback"
    );
    let mut missing = row();
    missing["fields"].as_array_mut().unwrap().remove(1);
    assert_eq!(
        doc.column("ColDone")
            .unwrap()
            .cell(&Record::parse(missing).unwrap()),
        CellValue::Empty
    );
}

#[test]
fn recognition_and_editing_are_conservative() {
    let doc = Document::parse(file()).unwrap();
    assert!(is_list(&serde_json::from_value(file()).unwrap()));
    for pref in [None, Some("read_only"), Some("true"), Some("FULL")] {
        assert!(!doc.editable(pref));
    }
    assert!(doc.editable(Some("full")));
    for (key, value) in [
        ("is_locked", json!(true)),
        ("is_locked", json!("false")),
        ("access", json!("read")),
        ("access", Value::Null),
    ] {
        let mut raw = file();
        raw[key] = value;
        assert!(!Document::parse(raw).unwrap().editable(Some("full")));
    }
    let mut poll = file();
    poll["subtype"] = json!("poll");
    assert!(Document::parse(poll.clone()).is_err());
    assert!(!is_list(&serde_json::from_value(poll).unwrap()));
    let mut deleted = file();
    deleted["is_deleted"] = json!(true);
    assert!(Document::parse(deleted).is_err());
    let mut wrong = file();
    wrong["mode"] = json!("hosted");
    assert!(Document::parse(wrong).is_err());
}

#[test]
fn canonical_tombstones_cannot_be_loaded_captured_revalidated_or_read_back() {
    let (doc, page) = fixture();
    let edit = CheckboxEdit::capture(&doc, &page, "RecOne", "ColDone", Some("full"), true).unwrap();
    for (key, value) in [("mode", json!("tombstone")), ("is_tombstoned", json!(true))] {
        let mut raw = file();
        // Retained filetype and schema must not make a deleted file editable.
        raw["filetype"] = json!("list");
        raw[key] = value;
        let file: File = serde_json::from_value(raw.clone()).unwrap();
        assert!(crate::file_actions::deleted(&file));
        assert!(Document::parse(raw.clone()).is_err());
        assert!(Document::from_file(&file).is_err());

        // A retained public domain object must also fail the dispatch boundary.
        let mut retained = doc.clone();
        retained.raw = raw;
        assert!(!retained.editable(Some("full")));
        assert!(
            CheckboxEdit::capture(&retained, &page, "RecOne", "ColDone", Some("full"), true)
                .is_err()
        );
        assert!(edit.revalidate(&retained, &page, Some("full")).is_err());
        assert_eq!(edit.readback_value(&retained, &page), None);
    }
}

#[test]
fn rejects_ambiguous_and_malformed_identity_structures() {
    let mut duplicate = file();
    let col = duplicate["list_metadata"]["schema"][0].clone();
    duplicate["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .push(col);
    assert!(Document::parse(duplicate).is_err());
    let mut duplicate_key = file();
    duplicate_key["list_metadata"]["schema"][1]["key"] = json!("name");
    assert!(Document::parse(duplicate_key).is_err());
    assert!(RecordsPage::parse(json!({"ok":true,"records":[row(),row()]})).is_err());
    let mut duplicate_field = row();
    let field = duplicate_field["fields"][0].clone();
    duplicate_field["fields"]
        .as_array_mut()
        .unwrap()
        .push(field);
    assert!(Record::parse(duplicate_field).is_err());
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"records":{}}),
        json!({"ok":false,"records":[]}),
        json!({"ok":true,"records":[{"id":"RecBad"}]}),
        json!({"ok":true,"records":[],"response_metadata":{"next_cursor":4}}),
    ] {
        assert!(RecordsPage::parse(raw).is_err());
    }
}

#[test]
fn incomplete_cursor_is_never_silently_discarded() {
    let page = RecordsPage::parse(
        json!({"ok":true,"records":[],"response_metadata":{"next_cursor":"opaque continuation"}}),
    )
    .unwrap();
    assert!(!page.complete());
    assert_eq!(page.next_cursor.as_deref(), Some("opaque continuation"));
}

#[test]
fn unexpected_typed_value_is_not_presented_as_empty_or_editable() {
    let (doc, _) = fixture();
    let mut malformed = row();
    malformed["fields"][1]
        .as_object_mut()
        .unwrap()
        .remove("checkbox");
    malformed["fields"][1]["future_checkbox"] = json!(true);
    let record = Record::parse(malformed).unwrap();
    let column = doc.column("ColDone").unwrap();
    assert!(matches!(column.cell(&record), CellValue::Unsupported(_)));
    assert!(!column.editable_checkbox(&record));
    let mut malformed = row();
    malformed["fields"][1]["row_id"] = json!("Other");
    assert!(!column.editable_checkbox(&Record::parse(malformed).unwrap()));
}

#[test]
fn ordering_uses_view_positions_then_default_and_created_time() {
    let mut a = row();
    a["id"] = json!("RecA");
    a["position"] = json!("b");
    a["date_created"] = json!(1);
    a["view_positions"] = json!({"VIEWMAIN":"0"});
    let mut b = row();
    b["id"] = json!("RecB");
    b["position"] = json!("a");
    b["date_created"] = json!(2);
    let mut c = b.clone();
    c["id"] = json!("RecC");
    c["date_created"] = json!(3);
    let page = page_with(vec![c, b, a]);
    assert_eq!(
        page.ordered(None)
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["RecB", "RecC", "RecA"]
    );
    assert_eq!(
        page.ordered(Some("viewmain"))
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["RecA", "RecB", "RecC"]
    );
}

#[test]
fn captured_checkbox_rejects_stale_owner_schema_cell_and_permissions() {
    let (doc, page) = fixture();
    let edit = CheckboxEdit::capture(&doc, &page, "RecOne", "ColDone", Some("full"), true).unwrap();
    assert!(edit.revalidate(&doc, &page, Some("full")).is_ok());
    assert!(edit.revalidate(&doc, &page, None).is_err());
    assert!(CheckboxEdit::capture(&doc, &page, "RecOne", "ColDone", Some("full"), false).is_err());
    let mut changed = row();
    changed["updated_timestamp"] = json!("101.0");
    assert!(
        edit.revalidate(&doc, &page_with(vec![changed]), Some("full"))
            .is_err()
    );
    let mut changed = file();
    changed["list_metadata"]["schema"][1]["key"] = json!("other");
    assert!(
        edit.revalidate(&Document::parse(changed).unwrap(), &page, Some("full"))
            .is_err()
    );
    let mut changed = row();
    changed["fields"][1]["column_id"] = json!("ColOther");
    assert!(
        CheckboxEdit::capture(
            &doc,
            &page_with(vec![changed]),
            "RecOne",
            "ColDone",
            Some("full"),
            true
        )
        .is_err()
    );
    let mut changed = file();
    changed["list_metadata"]["schema"][1]["type"] = json!("todo_completed");
    assert!(
        CheckboxEdit::capture(
            &Document::parse(changed).unwrap(),
            &page,
            "RecOne",
            "ColDone",
            Some("full"),
            true
        )
        .is_err()
    );
}

#[test]
fn update_response_requires_exact_target_and_concrete_checked_value() {
    let (doc, page) = fixture();
    let edit = CheckboxEdit::capture(&doc, &page, "RecOne", "ColDone", Some("full"), true).unwrap();
    let valid = json!({"ok":true,"cells":edit.cells()});
    assert!(edit.validate_response(&valid).is_ok());
    for response in [
        json!({"ok":true}),
        json!({"ok":true,"cells":[]}),
        json!({"ok":true,"cells":[{"row_id":"Other","column_id":"ColDone","checkbox":true}]}),
        json!({"ok":true,"cells":[{"row_id":"RecOne","column_id":"ColDone","checkbox":"true"}]}),
        json!({"ok":true,"cells":[{"row_id":"RecOne","column_id":"ColDone","checkbox":false}]}),
    ] {
        assert!(edit.validate_response(&response).is_err());
    }
    let mut duplicate = valid.clone();
    duplicate["cells"]
        .as_array_mut()
        .unwrap()
        .push(valid["cells"][0].clone());
    assert!(edit.validate_response(&duplicate).is_err());
    let client = crate::slack::client::SlackClient::default();
    let workspace = crate::config::WorkspaceSession {
        team_id: "T_LIST".into(),
        enterprise_id: None,
        user_id: "U_LIST".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = crate::slack::api::list_checkbox_update(&client, &workspace, &edit);
    assert!(!request.retry_safe());
    let crate::slack::client::RequestBody::Form(fields) = request.body else {
        panic!()
    };
    let cells: Value =
        serde_json::from_str(&fields.iter().find(|(key, _)| key == "cells").unwrap().1).unwrap();
    assert_eq!(
        cells,
        json!([{"row_id":"RecOne","column_id":"ColDone","checkbox":true}])
    );
}

#[test]
fn readback_accepts_updated_timestamp_but_not_replaced_identity() {
    let (doc, page) = fixture();
    let edit = CheckboxEdit::capture(&doc, &page, "RecOne", "ColDone", Some("full"), true).unwrap();
    assert_eq!(edit.readback_value(&doc, &page), Some(false));
    assert!(!edit.readback_matches(&doc, &page));
    let mut changed = row();
    changed["fields"][1]["checkbox"] = json!(true);
    changed["updated_timestamp"] = json!("101.0");
    assert!(edit.readback_matches(&doc, &page_with(vec![changed.clone()])));
    changed["created_by"] = json!("OTHER");
    assert_eq!(edit.readback_value(&doc, &page_with(vec![changed])), None);
    let mut changed = row();
    changed["fields"][1]["checkbox"] = json!("true");
    assert_eq!(edit.readback_value(&doc, &page_with(vec![changed])), None);
}

#[test]
fn ordinary_completion_uses_existing_boolean_cells_and_rechecks_personal_exclusion() {
    let ordinary = PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{
        "task_lists":"[\"F_PERSONAL\"]","assigned_to_list":"F_ASSIGNED"
    }}))
    .unwrap();
    let became_personal = PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{
        "task_lists":"[\"F_LIST\"]","assigned_to_list":""
    }}))
    .unwrap();
    let became_assigned = PersonalTaskLists::from_response(&json!({"ok":true,"prefs":{
        "task_lists":"[]","assigned_to_list":"F_LIST"
    }}))
    .unwrap();
    for kind in ["completed", "todo_completed"] {
        let mut raw = file();
        raw["list_metadata"]["schema"][1]["type"] = json!(kind);
        raw["list_metadata"]["todo_mode"] = json!("default");
        let doc = Document::parse(raw).unwrap();
        let page = page_with(vec![row()]);
        let edit = CheckboxEdit::capture_with_policy(
            &doc,
            &page,
            "RecOne",
            "ColDone",
            Some("full"),
            Some(&ordinary),
            true,
        )
        .unwrap();
        assert_eq!(
            edit.cells(),
            json!([{"row_id":"RecOne","column_id":"ColDone","checkbox":true}])
        );
        assert!(
            edit.revalidate_with_policy(&doc, &page, Some("full"), Some(&ordinary))
                .is_ok()
        );
        for personal in [&became_personal, &became_assigned] {
            assert!(
                !doc.column("ColDone")
                    .unwrap()
                    .editable_checkbox_with_policy(&page.records[0], &doc.id, Some(personal))
            );
            assert!(
                CheckboxEdit::capture_with_policy(
                    &doc,
                    &page,
                    "RecOne",
                    "ColDone",
                    Some("full"),
                    Some(personal),
                    true
                )
                .is_err()
            );
            assert!(
                edit.revalidate_with_policy(&doc, &page, Some("full"), Some(personal))
                    .is_err()
            );
        }
        assert!(
            edit.revalidate_with_policy(&doc, &page, None, Some(&ordinary))
                .is_err()
        );
        assert!(edit.revalidate(&doc, &page, Some("full")).is_err());
        assert!(
            CheckboxEdit::capture(&doc, &page, "RecOne", "ColDone", Some("full"), true).is_err()
        );
        let mut checked = row();
        checked["fields"][1]["checkbox"] = json!(true);
        let checked_page = page_with(vec![checked]);
        let clear = CheckboxEdit::capture_with_policy(
            &doc,
            &checked_page,
            "RecOne",
            "ColDone",
            Some("full"),
            Some(&ordinary),
            false,
        )
        .unwrap();
        assert_eq!(clear.cells()[0]["checkbox"], false);
        let mut missing = row();
        missing["fields"][1]
            .as_object_mut()
            .unwrap()
            .remove("checkbox");
        assert!(
            CheckboxEdit::capture_with_policy(
                &doc,
                &page_with(vec![missing]),
                "RecOne",
                "ColDone",
                Some("full"),
                Some(&ordinary),
                true
            )
            .is_err()
        );
    }
}

#[test]
fn checkbox_preflight_ignores_only_top_level_read_marker() {
    let (mut doc, page) = fixture();
    doc.raw["last_read"] = json!("100.0");
    let edit = CheckboxEdit::capture(&doc, &page, "RecOne", "ColDone", Some("full"), true).unwrap();
    let original = edit.clone();
    for marker in [Some(json!("101.0")), Some(Value::Null), None] {
        let mut current = doc.clone();
        if let Some(marker) = marker {
            current.raw["last_read"] = marker;
        } else {
            current.raw.as_object_mut().unwrap().remove("last_read");
        }
        assert!(edit.revalidate(&current, &page, Some("full")).is_ok());
    }
    let mut absent = doc.clone();
    absent.raw.as_object_mut().unwrap().remove("last_read");
    let absent_edit =
        CheckboxEdit::capture(&absent, &page, "RecOne", "ColDone", Some("full"), true).unwrap();
    assert!(absent_edit.revalidate(&doc, &page, Some("full")).is_ok());
    for (key, value) in [
        ("access", json!("read")),
        ("is_locked", json!(true)),
        ("updated", json!(101)),
        ("unknown_file", json!({"keep":2})),
    ] {
        let mut current = doc.clone();
        current.raw["last_read"] = json!("101.0");
        current.raw[key] = value;
        assert!(
            edit.revalidate(&current, &page, Some("full")).is_err(),
            "{key}"
        );
    }
    for key in ["last_read", "schema", "views"] {
        let mut current = doc.clone();
        current.raw["list_metadata"][key] = json!([]);
        assert!(
            edit.revalidate(&current, &page, Some("full")).is_err(),
            "nested {key}"
        );
    }
    let mut current = doc.clone();
    current.columns[1].raw["last_read"] = json!("101.0");
    assert!(edit.revalidate(&current, &page, Some("full")).is_err());
    let mut current_page = page.clone();
    current_page.records[0].raw["last_read"] = json!("101.0");
    assert!(edit.revalidate(&doc, &current_page, Some("full")).is_err());
    assert!(edit.revalidate(&doc, &page, None).is_err());
    assert_eq!(edit, original);
    assert_eq!(doc.raw["last_read"], "100.0");
}

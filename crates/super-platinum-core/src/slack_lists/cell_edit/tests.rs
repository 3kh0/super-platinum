use super::*;
use crate::config::WorkspaceSession;
use crate::slack::{
    api,
    client::{RequestBody, SlackClient},
};

fn policy(id: &str) -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":json!([id]).to_string(),"assigned_to_list":""}}),
    )
    .unwrap()
}
fn fixture(kind: &str, stored: Option<(&str, Value)>) -> (Document, RecordsPage) {
    let doc = Document::parse(
        json!({"id":"F_LIST","mode":"list","access":"write","list_metadata":{"schema":[{
            "id":"COL","key":"field","name":"Field","type":kind,"options":{"choices":[
                {"value":"a","label":"Alpha"},{"value":"b","label":"Beta"}
            ]}
        }]}}),
    )
    .unwrap();
    let mut fields = vec![];
    if let Some((member, value)) = stored {
        let mut field = json!({"key":"field","column_id":"COL","row_id":"ROW"});
        field[member] = value;
        fields.push(field);
    }
    let page = RecordsPage::parse(json!({"ok":true,"records":[{"id":"ROW","date_created":1,"created_by":"U1","fields":fields}]})).unwrap();
    (doc, page)
}
fn capture(doc: &Document, page: &RecordsPage, value: CellEditValue) -> CellEdit {
    CellEdit::capture(
        doc,
        page,
        "ROW",
        "COL",
        Some("full"),
        Some(&policy("F_PERSONAL")),
        value,
    )
    .unwrap()
}
fn rich(text: &str) -> Vec<Value> {
    vec![
        json!({"type":"rich_text","block_id":"original","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text,"style":{"bold":true}}]}]}),
    ]
}

#[test]
fn absent_existing_text_cell_can_be_written_without_creating_row_or_column() {
    for kind in ["text", "rich_text"] {
        let (doc, page) = fixture(kind, None);
        let column = doc.column("COL").unwrap();
        assert_eq!(
            column
                .edit_value(&page.records[0], &doc.id, Some(&policy("F_PERSONAL")))
                .unwrap(),
            CellEditValue::RichText(vec![])
        );
        let blocks = rich("New name");
        let edit = capture(&doc, &page, CellEditValue::RichText(blocks.clone()));
        assert_eq!(
            edit.cells(),
            json!([{"row_id":"ROW","column_id":"COL","rich_text":blocks}])
        );
        let workspace = WorkspaceSession {
            team_id: "T_TEST".into(),
            enterprise_id: None,
            user_id: "U_TEST".into(),
            name: "test".into(),
            url: "https://example.test".into(),
            token: "fixture-not-a-token".into(),
        };
        let req = api::list_cell_update(&SlackClient::default(), &workspace, &edit);
        assert!(!req.retry_safe());
        assert!(req.url.contains("/api/lists.cells.update?"));
        let RequestBody::Form(fields) = req.body else {
            panic!()
        };
        let wire: Value =
            serde_json::from_str(&fields.iter().find(|(key, _)| key == "cells").unwrap().1)
                .unwrap();
        assert_eq!(wire, edit.cells());
        assert!(wire[0].get("row_id_to_create").is_none());
        assert!(wire[0].get("column_id_to_create").is_none());
        assert!(
            CellEdit::capture(
                &doc,
                &page,
                "MISSING",
                "COL",
                Some("full"),
                Some(&policy("F_PERSONAL")),
                CellEditValue::RichText(rich("x"))
            )
            .is_err()
        );
    }
}
#[test]
fn rich_edits_preserve_original_opaque_nodes_and_reject_fabricated_or_flattened_shapes() {
    let mut blocks = rich("Before");
    let opaque = json!({"type":"future_inline","value":{"important":true}});
    blocks[0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(opaque.clone());
    let (doc, page) = fixture("rich_text", Some(("rich_text", json!(blocks))));
    assert_eq!(
        doc.columns[0]
            .edit_value(&page.records[0], &doc.id, Some(&policy("F_OTHER")))
            .unwrap(),
        CellEditValue::RichText(blocks.clone())
    );
    blocks[0]["elements"][0]["elements"][0]["text"] = json!("After");
    let edit = capture(&doc, &page, CellEditValue::RichText(blocks.clone()));
    assert_eq!(
        edit.cells()[0]["rich_text"][0]["elements"][0]["elements"][1],
        opaque
    );
    blocks[0]["elements"][0]["elements"][1]["value"] = json!("changed");
    assert!(
        CellEdit::capture(
            &doc,
            &page,
            "ROW",
            "COL",
            Some("full"),
            Some(&policy("F_OTHER")),
            CellEditValue::RichText(blocks)
        )
        .is_err()
    );
    let mut malformed = rich("x");
    malformed[0]["elements"] = json!("not children");
    assert!(
        CellEdit::capture(
            &doc,
            &page,
            "ROW",
            "COL",
            Some("full"),
            Some(&policy("F_OTHER")),
            CellEditValue::RichText(malformed)
        )
        .is_err()
    );
    let clear = capture(&doc, &page, CellEditValue::RichText(vec![]));
    assert_eq!(clear.cells()[0]["rich_text"], json!([]));
    let (doc, mut page) = fixture("rich_text", Some(("rich_text", json!([]))));
    page.records[0].fields.get_mut("field").unwrap()["text"] =
        json!("Visible fallback without original blocks");
    assert!(!doc.columns[0].editable_value(&page.records[0], &doc.id, Some(&policy("F_OTHER"))));
}
#[test]
fn number_currency_zero_and_clear_are_exact_and_malformed_arrays_are_not_truncated() {
    for kind in ["number", "currency"] {
        let (doc, page) = fixture(kind, None);
        let zero = capture(&doc, &page, CellEditValue::Number(Some(0.into())));
        assert_eq!(zero.cells()[0]["number"], json!([0]));
        let (doc, page) = fixture(kind, Some(("number", json!([12.5]))));
        let clear = capture(&doc, &page, CellEditValue::Number(None));
        assert_eq!(clear.cells()[0]["number"], json!([]));
        let huge = CellEditValue::Number(Some(9_007_199_254_740_992u64.into()));
        assert!(
            CellEdit::capture(
                &doc,
                &page,
                "ROW",
                "COL",
                Some("full"),
                Some(&policy("F_OTHER")),
                huge
            )
            .is_err()
        );
        for bad in [json!([1, 2]), json!(["12"]), json!(0)] {
            let (doc, page) = fixture(kind, Some(("number", bad)));
            assert!(!doc.columns[0].editable_value(
                &page.records[0],
                &doc.id,
                Some(&policy("F_OTHER"))
            ));
        }
    }
}
#[test]
fn selections_validate_current_unique_options_and_single_vs_multi_cardinality() {
    for kind in ["select", "multi_select"] {
        let (doc, page) = fixture(kind, Some(("select", json!(["a"]))));
        let clear = capture(&doc, &page, CellEditValue::Select(vec![]));
        assert_eq!(clear.cells()[0]["select"], json!([]));
        for ids in [vec!["missing".into()], vec!["b".into(), "b".into()]] {
            assert!(
                CellEdit::capture(
                    &doc,
                    &page,
                    "ROW",
                    "COL",
                    Some("full"),
                    Some(&policy("F_OTHER")),
                    CellEditValue::Select(ids)
                )
                .is_err()
            );
        }
        let both = CellEdit::capture(
            &doc,
            &page,
            "ROW",
            "COL",
            Some("full"),
            Some(&policy("F_OTHER")),
            CellEditValue::Select(vec!["a".into(), "b".into()]),
        );
        assert_eq!(both.is_ok(), kind == "multi_select");
        let mut duplicate = doc.raw.clone();
        duplicate["list_metadata"]["schema"][0]["options"]["choices"][1]["value"] = json!("a");
        let duplicate = Document::parse(duplicate).unwrap();
        assert!(!duplicate.columns[0].editable_value(
            &page.records[0],
            &duplicate.id,
            Some(&policy("F_OTHER"))
        ));
    }
}

#[test]
fn current_select_schema_uses_options_format_for_multiple_values() {
    let (doc, page) = fixture("select", Some(("select", json!(["a"]))));
    let mut raw = doc.raw;
    raw["list_metadata"]["schema"][0]["options"]["format"] = json!("multi_select");
    let doc = Document::parse(raw.clone()).unwrap();
    assert!(doc.columns[0].is_multi_select());
    let edit = capture(
        &doc,
        &page,
        CellEditValue::Select(vec!["a".into(), "b".into()]),
    );
    assert_eq!(edit.cells()[0]["select"], json!(["a", "b"]));
    raw["list_metadata"]["schema"][0]["options"]["format"] = json!("future_format");
    let unknown = Document::parse(raw).unwrap();
    assert!(!unknown.columns[0].editable_value(
        &page.records[0],
        &unknown.id,
        Some(&policy("F_OTHER"))
    ));
}
#[test]
fn typed_preflight_rejects_stale_rows_schema_permissions_and_personal_routing() {
    let (doc, page) = fixture("number", Some(("number", json!([1]))));
    let edit = capture(&doc, &page, CellEditValue::Number(Some(2.into())));
    assert!(
        edit.revalidate(&doc, &page, Some("full"), Some(&policy("F_OTHER")))
            .is_ok()
    );
    assert!(
        edit.revalidate(&doc, &page, None, Some(&policy("F_OTHER")))
            .is_err()
    );
    for personal in [None, Some(policy("F_LIST"))] {
        assert!(
            edit.revalidate(&doc, &page, Some("full"), personal.as_ref())
                .is_err()
        );
    }
    let mut changed = page.clone();
    changed.records[0].raw["updated_timestamp"] = json!("new");
    assert!(
        edit.revalidate(&doc, &changed, Some("full"), Some(&policy("F_OTHER")))
            .is_err()
    );
    let mut changed = doc.clone();
    changed.raw["access"] = json!("read");
    assert!(
        edit.revalidate(&changed, &page, Some("full"), Some(&policy("F_OTHER")))
            .is_err()
    );
    let mut changed = page.clone();
    changed.records[0].fields.get_mut("field").unwrap()["column_id"] = json!("OTHER");
    assert!(
        CellEdit::capture(
            &doc,
            &changed,
            "ROW",
            "COL",
            Some("full"),
            Some(&policy("F_OTHER")),
            CellEditValue::Number(Some(2.into()))
        )
        .is_err()
    );
}
#[test]
fn typed_receipts_and_readback_require_exact_target_and_concrete_value() {
    let (doc, page) = fixture("number", Some(("number", json!([1]))));
    let edit = capture(&doc, &page, CellEditValue::Number(Some(2.into())));
    assert!(
        edit.validate_response(&json!({"ok":true,"cells":edit.cells()}))
            .is_ok()
    );
    for response in [
        json!({"ok":true}),
        json!({"ok":true,"cells":[]}),
        json!({"ok":true,"cells":[{"row_id":"ROW","column_id":"COL","number":[1]}]}),
        json!({"ok":true,"cells":[{"row_id":"OTHER","column_id":"COL","number":[2]}]}),
    ] {
        assert!(edit.validate_response(&response).is_err());
    }
    let (_, readback) = fixture("number", Some(("number", json!([2]))));
    assert!(edit.readback_matches(&doc, &readback));
    assert_eq!(
        edit.readback_value(&doc, &page),
        Some(CellEditValue::Number(Some(1.into())))
    );
    let mut replaced = readback.clone();
    replaced.records[0].raw["created_by"] = json!("OTHER");
    assert_eq!(edit.readback_value(&doc, &replaced), None);
    let (doc, page) = fixture("checkbox", Some(("checkbox", json!(false))));
    let checkbox = CheckboxEdit::capture(&doc, &page, "ROW", "COL", Some("full"), true).unwrap();
    let generic: CellEdit = checkbox.clone().into();
    assert_eq!(generic.cells(), checkbox.cells());
    assert_eq!(generic.value(), CellEditValue::Boolean(true));
}

#[test]
fn rich_receipts_and_readback_ignore_only_observed_root_block_ids() {
    let (doc, page) = fixture("text", None);
    let mut desired = rich("Native editor verification");
    desired[0].as_object_mut().unwrap().remove("block_id");
    let edit = capture(&doc, &page, CellEditValue::RichText(desired.clone()));
    let mut received = desired.clone();
    received[0]["block_id"] = json!("bfz4L");
    assert!(
        edit.validate_response(
            &json!({"ok":true,"cells":[{"row_id":"ROW","column_id":"COL","rich_text":received}]})
        )
        .is_ok()
    );
    let (_, read) = fixture("text", Some(("rich_text", json!(received))));
    assert!(edit.readback_matches(&doc, &read));
    assert_eq!(edit.cells()[0]["rich_text"], json!(desired));
    let mut text = received.clone();
    text[0]["elements"][0]["elements"][0]["text"] = json!("Different");
    let mut style = received.clone();
    style[0]["elements"][0]["elements"][0]["style"]["bold"] = json!(false);
    let mut nested = received.clone();
    nested[0]["elements"][0]["block_id"] = json!("nested-id");
    let mut malformed = received.clone();
    malformed[0]["block_id"] = json!(7);
    let mut extra = received.clone();
    extra[0]["other_metadata"] = json!("changed");
    for changed in [text, style, nested, malformed, extra] {
        assert!(
            edit.validate_response(
                &json!({"ok":true,"cells":[{"row_id":"ROW","column_id":"COL","rich_text":changed}]})
            )
            .is_err()
        );
        let (_, read) = fixture("text", Some(("rich_text", json!(changed))));
        assert!(!edit.readback_matches(&doc, &read));
    }
}
#[test]
fn root_id_normalization_preserves_link_and_opaque_payload_identity() {
    let original = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"link","url":"https://example.test/old","text":"Link"},
            {"type":"future_token","payload":{"id":"opaque"}}
        ]}]}),
    ];
    let (doc, page) = fixture("rich_text", Some(("rich_text", json!(original))));
    let mut desired = original.clone();
    desired[0]["elements"][0]["elements"][0]["text"] = json!("New label");
    let edit = capture(&doc, &page, CellEditValue::RichText(desired.clone()));
    let mut received = desired.clone();
    received[0]["block_id"] = json!("server-id");
    let (_, read) = fixture("rich_text", Some(("rich_text", json!(received))));
    assert!(edit.readback_matches(&doc, &read));
    let mut link = received.clone();
    link[0]["elements"][0]["elements"][0]["url"] = json!("https://example.test/changed");
    let mut opaque = received.clone();
    opaque[0]["elements"][0]["elements"][1]["payload"]["id"] = json!("other");
    for changed in [link, opaque] {
        let (_, read) = fixture("rich_text", Some(("rich_text", json!(changed))));
        assert!(!edit.readback_matches(&doc, &read));
    }
}

#[test]
fn typed_preflight_ignores_only_top_level_last_read() {
    let (mut doc, page) = fixture("number", Some(("number", json!([0]))));
    doc.raw["last_read"] = json!("100.0");
    let edit = capture(
        &doc,
        &page,
        CellEditValue::Number(Some("12.5".parse().unwrap())),
    );
    let original = edit.clone();
    let original_document = doc.raw.clone();
    let payload = edit.cells();
    for last_read in [Some(json!("101.0")), Some(Value::Null), None] {
        let mut changed = doc.clone();
        match last_read {
            Some(value) => {
                changed.raw["last_read"] = value;
            }
            None => {
                changed.raw.as_object_mut().unwrap().remove("last_read");
            }
        }
        assert!(
            edit.revalidate(&changed, &page, Some("full"), Some(&policy("F_OTHER")))
                .is_ok()
        );
    }
    let mut absent = doc.clone();
    absent.raw.as_object_mut().unwrap().remove("last_read");
    let absent_edit = capture(&absent, &page, CellEditValue::Number(Some(2.into())));
    assert!(
        absent_edit
            .revalidate(&doc, &page, Some("full"), Some(&policy("F_OTHER")))
            .is_ok()
    );
    for (key, value) in [
        ("access", json!("read")),
        ("is_locked", json!(true)),
        ("updated", json!(101)),
        ("unknown_metadata", json!(true)),
        ("list_metadata", json!({"schema":[],"last_read":"101.0"})),
    ] {
        let mut changed = doc.clone();
        changed.raw["last_read"] = json!("101.0");
        changed.raw[key] = value;
        assert!(
            edit.revalidate(&changed, &page, Some("full"), Some(&policy("F_OTHER")))
                .is_err(),
            "{key}"
        );
    }
    for key in ["last_read", "schema", "views"] {
        let mut changed = doc.clone();
        changed.raw["list_metadata"][key] = json!([]);
        assert!(
            edit.revalidate(&changed, &page, Some("full"), Some(&policy("F_OTHER")))
                .is_err(),
            "nested {key}"
        );
    }
    let mut changed_column = doc.clone();
    changed_column.columns[0].raw["last_read"] = json!("101.0");
    assert!(
        edit.revalidate(
            &changed_column,
            &page,
            Some("full"),
            Some(&policy("F_OTHER"))
        )
        .is_err()
    );
    let mut changed_record = page.clone();
    changed_record.records[0].raw["last_read"] = json!("101.0");
    assert!(
        edit.revalidate(
            &doc,
            &changed_record,
            Some("full"),
            Some(&policy("F_OTHER"))
        )
        .is_err()
    );
    assert!(
        edit.revalidate(&doc, &page, None, Some(&policy("F_OTHER")))
            .is_err()
    );
    assert!(
        edit.revalidate(&doc, &page, Some("full"), Some(&policy("F_LIST")))
            .is_err()
    );
    assert_eq!(edit, original);
    assert_eq!(edit.cells(), payload);
    assert_eq!(doc.raw, original_document);
}

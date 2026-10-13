use super::*;

#[test]
fn link_modal_name_and_wire_follow_captured_synthetic_oracle() {
    let oracle: Value = serde_json::from_str(include_str!("link/detector/oracle.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let normalized = &case["normalized"];
        let name = normalized.get("displayName").and_then(Value::as_str);
        let actual = LinkValue::from_input(input, name).unwrap();
        let expected = if input.is_empty() {
            json!([])
        } else {
            let mut item = json!({"original_url":normalized["originalUrl"],"display_as_url":false});
            if let Some(name) = name.filter(|v| !v.is_empty()) {
                item["display_name"] = json!(name);
            }
            json!([item])
        };
        assert_eq!(link::wire(actual.as_ref()), expected, "input {input:?}");
    }
}

fn policy() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn fixture(stored: Value) -> (Document, RecordsPage) {
    let doc=Document::parse(json!({"id":"F_LIST","mode":"list","access":"write","list_metadata":{"schema":[{"id":"COL","key":"reference","name":"Link","type":"link"}]}})).unwrap();
    let page=RecordsPage::parse(json!({"ok":true,"records":[{"id":"ROW","created_by":"U00000001","date_created":1,"fields":[{"column_id":"COL","key":"reference","link":stored},{"key":"other","number":[17]}]}]})).unwrap();
    (doc, page)
}
fn capture(
    doc: &Document,
    page: &RecordsPage,
    value: Option<LinkValue>,
) -> Result<CellEdit, ListError> {
    CellEdit::capture(
        doc,
        page,
        "ROW",
        "COL",
        Some("full"),
        Some(&policy()),
        CellEditValue::Link(value),
    )
}

#[test]
fn link_wire_clear_preserves_siblings_and_only_serializes_editable_members() {
    let stored = json!([{"original_url":"https://example.com/old","display_name":"Old","display_as_url":true,"attachment":{"https://example.com/old":{"title":"Server title"}}}]);
    let (doc, page) = fixture(stored);
    let original = page.clone();
    let edit = capture(
        &doc,
        &page,
        LinkValue::from_input("example.org", Some("New")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        edit.cells(),
        json!([{"row_id":"ROW","column_id":"COL","link":[{"original_url":"http://example.org","display_name":"New","display_as_url":false}]}])
    );
    assert_eq!(
        capture(&doc, &page, None).unwrap().cells(),
        json!([{"row_id":"ROW","column_id":"COL","link":[]}])
    );
    assert_eq!(page, original);
}

#[test]
fn link_scalar_aliases_and_malformed_values_preserve_canonical_guards() {
    for stored in [
        json!({"originalUrl":"https://example.com","displayAsUrl":true,"displayName":""}),
        json!([{"original_url":"https://example.com","display_as_url":true}]),
    ] {
        let (doc, page) = fixture(stored);
        assert_eq!(
            doc.columns[0]
                .edit_value(&page.records[0], &doc.id, Some(&policy()))
                .unwrap(),
            CellEditValue::Link(Some(LinkValue {
                original_url: "https://example.com".into(),
                display_name: None,
                display_as_url: true
            }))
        );
        let edit = capture(&doc, &page, None).unwrap();
        let mut changed = page.raw.clone();
        changed["records"][0]["fields"][0]["link"] =
            json!([{"original_url":"https://example.com","display_as_url":true}]);
        if changed != page.raw {
            assert!(
                edit.revalidate(
                    &doc,
                    &RecordsPage::parse(changed).unwrap(),
                    Some("full"),
                    Some(&policy())
                )
                .is_err()
            );
        }
    }
    for value in [
        json!("https://example.com"),
        json!([{}, {}]),
        json!({"original_url":"x","originalUrl":"y"}),
        json!({"original_url":"x","display_as_url":"true"}),
        json!({"original_url":"x","display_name":17}),
        json!({"original_url":"x","future":true}),
        json!({"original_url":"x","attachment":17}),
    ] {
        let (doc, page) = fixture(value);
        assert!(capture(&doc, &page, None).is_err());
    }
}

#[test]
fn link_receipt_allows_only_attachment_enrichment_and_exact_target() {
    let (doc, page) = fixture(json!([]));
    let edit = capture(
        &doc,
        &page,
        LinkValue::from_input("https://example.com", Some("Name")).unwrap(),
    )
    .unwrap();
    let mut cell = edit.cells()[0].clone();
    cell["link"][0]["attachment"] = json!({"https://example.com":{"title":"Server title"}});
    assert!(
        edit.validate_response(&json!({"ok":true,"cells":[cell.clone()]}))
            .is_ok()
    );
    for (key, value) in [
        ("original_url", json!("https://other.example")),
        ("display_name", json!("Changed")),
        ("display_as_url", json!(true)),
        ("unknown", json!(1)),
    ] {
        let mut bad = cell.clone();
        bad["link"][0][key] = value;
        assert!(
            edit.validate_response(&json!({"ok":true,"cells":[bad]}))
                .is_err()
        );
    }
    let mut bad = cell.clone();
    bad["row_id"] = json!("OTHER");
    assert!(
        edit.validate_response(&json!({"ok":true,"cells":[bad]}))
            .is_err()
    );
    let mut updated = page.raw.clone();
    updated["records"][0]["fields"][0] = cell;
    updated["records"][0]["fields"][0]["key"] = json!("reference");
    assert!(edit.readback_matches(&doc, &RecordsPage::parse(updated.clone()).unwrap()));
    updated["records"][0]["created_by"] = json!("U00000002");
    assert!(!edit.readback_matches(&doc, &RecordsPage::parse(updated).unwrap()));
}

#[test]
fn link_storage_does_not_authorize_navigation_or_silently_trim_name() {
    let value = LinkValue::from_input("javascript:alert(1)", Some("  "))
        .unwrap()
        .unwrap();
    assert_eq!(value.original_url, "javascript:alert(1)");
    assert_eq!(value.display_name.as_deref(), Some("  "));
    assert_eq!(LinkValue::from_input("", Some("ignored")).unwrap(), None);
    let (doc, page) = fixture(json!([]));
    assert!(
        CellEdit::capture(
            &doc,
            &page,
            "ROW",
            "COL",
            Some("full"),
            None,
            CellEditValue::Link(Some(value))
        )
        .is_err()
    );
}

use super::super::{CellEdit, CellEditValue, CellValue};
use super::*;
fn baseline() -> (Document, RecordsPage, PersonalTaskLists) {
    let d=Document::parse(json!({"id":"F_DATE_CREATE","mode":"list","user":"U1","access":"owner","created":1,"list_limits":{"over_column_maximum":false},"list_metadata":{"schema":[{"id":"TITLE","key":"NAME","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true}],"views":[{"id":"V1","name":"Custom","type":"table","columns":[{"id":"TITLE","key":"NAME","visible":true}],"sorts":[],"filters":[],"info_column_filters":[],"opaque":{"keep":1}}],"opaque":{"keep":2}}})).unwrap();
    let p=RecordsPage::parse(json!({"ok":true,"records":[{"id":"R1","fields":[{"key":"NAME","text":"Keep"}]},{"id":"R2","parent_record_id":"R1","fields":[]}],"response_metadata":{"next_cursor":"NEXT"}})).unwrap();
    let policy = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    (d, p, policy)
}
fn intent(d: &Document, p: &RecordsPage, policy: &PersonalTaskLists) -> ColumnCreate {
    ColumnCreate::capture_kind(
        d,
        p,
        "V1",
        "When",
        ColumnCreateKind::DateSingleDefault,
        "U1",
        Some("full"),
        Some(policy),
    )
    .unwrap()
}
fn added(d: &Document) -> Document {
    let mut raw = d.raw.clone();
    raw["list_metadata"]["schema"].as_array_mut().unwrap().insert(1,json!({"id":"SERVER_DATE","key":"actual_date","name":"When","type":"date","options":{"format":"single","date_format":"default"},"opaque":{"server":true}}));
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"SERVER_DATE","key":"actual_date","visible":true}));
    Document::parse(raw).unwrap()
}
#[test]
fn date_observation_accepts_only_exact_wire_options_and_ordinary_type() {
    let (d, p, policy) = baseline();
    let i = intent(&d, &p, &policy);
    let fresh = added(&d);
    let c = i.matching_addition(&fresh, &p).unwrap();
    assert_eq!(
        (&c.id, &c.key),
        (&"SERVER_DATE".to_owned(), &"actual_date".to_owned())
    );
    assert_eq!(c.raw["opaque"], json!({"server":true}));
    for options in [
        None,
        Some(Value::Null),
        Some(json!({})),
        Some(json!({"format":"single","dateFormat":"default"})),
        Some(json!({"format":"range","date_format":"default"})),
        Some(json!({"format":"single","date_format":"default","timezone":"UTC"})),
        Some(json!({"format":"single","date_format":"YYYY-MM-DD"})),
    ] {
        let mut raw = fresh.raw.clone();
        let column = raw["list_metadata"]["schema"][1].as_object_mut().unwrap();
        if let Some(v) = options {
            column.insert("options".into(), v);
        } else {
            column.remove("options");
        }
        assert!(
            i.matching_addition(&Document::parse(raw).unwrap(), &p)
                .is_none()
        );
    }
    for kind in ["due_date", "todo_due_date", "timestamp", "text", "number"] {
        let mut raw = fresh.raw.clone();
        raw["list_metadata"]["schema"][1]["type"] = json!(kind);
        assert!(
            i.matching_addition(&Document::parse(raw).unwrap(), &p)
                .is_none()
        );
    }
}
#[test]
fn date_creation_observation_preserves_population_metadata_and_refuses_orphan_revival() {
    let (d, p, policy) = baseline();
    let i = intent(&d, &p, &policy);
    let fresh = added(&d);
    let c = i.matching_addition(&fresh, &p).unwrap();
    assert!(
        p.records
            .iter()
            .all(|r| c.cell(r) == CellValue::Empty && !r.fields.contains_key(&c.key))
    );
    assert_eq!(i.original_records(), &p);
    assert_eq!(p.next_cursor.as_deref(), Some("NEXT"));
    for change in 0..2 {
        let mut raw = fresh.raw.clone();
        match change {
            0 => raw["list_metadata"]["opaque"]["keep"] = json!(3),
            _ => raw["list_metadata"]["views"][0]["opaque"]["keep"] = json!(2),
        }
        assert!(
            i.matching_addition(&Document::parse(raw).unwrap(), &p)
                .is_none()
        );
    }
    let mut raw = p.raw.clone();
    raw["response_metadata"]["next_cursor"] = json!("OTHER");
    assert!(
        i.matching_addition(&fresh, &RecordsPage::parse(raw).unwrap())
            .is_none()
    );
    let mut raw = d.raw.clone();
    raw["list_metadata"]["opaque"]["actual_date"] = json!(true);
    let orphan = Document::parse(raw).unwrap();
    assert!(
        intent(&orphan, &p, &policy)
            .matching_addition(&added(&orphan), &p)
            .is_none()
    );
}
#[test]
fn observed_missing_date_uses_existing_calendar_set_clear_and_strict_readback() {
    use super::super::saved_views::{ProjectionContext, catalog, project};
    let (d, p, policy) = baseline();
    let i = intent(&d, &p, &policy);
    let fresh = added(&d);
    let c = i.matching_addition(&fresh, &p).unwrap();
    let projection = project(
        &fresh,
        &p,
        &catalog(&fresh).unwrap()[0],
        &ProjectionContext::default(),
    )
    .unwrap();
    assert_eq!(projection.column_ids, vec!["TITLE", "SERVER_DATE"]);
    assert_eq!(
        c.edit_value(&p.records[0], &fresh.id, Some(&policy))
            .unwrap(),
        CellEditValue::Date(None)
    );
    let edit = CellEdit::capture(
        &fresh,
        &p,
        "R1",
        &c.id,
        Some("full"),
        Some(&policy),
        CellEditValue::Date(Some("2028-02-29".into())),
    )
    .unwrap();
    assert_eq!(
        edit.cells(),
        json!([{"row_id":"R1","column_id":"SERVER_DATE","date":["2028-02-29"]}])
    );
    assert!(!edit.readback_matches(&fresh, &p));
    assert!(
        CellEdit::capture(
            &fresh,
            &p,
            "R1",
            &c.id,
            Some("full"),
            Some(&policy),
            CellEditValue::Date(None)
        )
        .is_err()
    );
    let mut stored = p.raw.clone();
    stored["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":c.key,"column_id":c.id,"row_id":"R1","date":["2028-02-29"]}));
    let stored = RecordsPage::parse(stored).unwrap();
    assert!(edit.readback_matches(&fresh, &stored));
    let clear = CellEdit::capture(
        &fresh,
        &stored,
        "R1",
        &c.id,
        Some("full"),
        Some(&policy),
        CellEditValue::Date(None),
    )
    .unwrap();
    assert_eq!(
        clear.cells(),
        json!([{"row_id":"R1","column_id":"SERVER_DATE","date":[]}])
    );
    let mut cleared = stored.raw.clone();
    cleared["records"][0]["fields"][1]["date"] = json!([]);
    assert!(clear.readback_matches(&fresh, &RecordsPage::parse(cleared).unwrap()));
    assert!(
        i.matching_addition(&fresh, &stored).is_none(),
        "creation observation never backfills cells"
    );
    let mut changed = fresh.raw.clone();
    changed["list_metadata"]["schema"][1]["options"]["format"] = json!("range");
    assert!(!edit.readback_matches(&Document::parse(changed).unwrap(), &stored));
}

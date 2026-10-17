use super::super::{CellEdit, CellEditValue, CellValue};
use super::*;
fn baseline() -> (Document, RecordsPage, PersonalTaskLists) {
    let document=Document::parse(json!({"id":"F_PEOPLE","mode":"list","user":"U1","access":"owner","list_limits":{"over_column_maximum":false},"list_metadata":{"schema":[{"id":"TITLE","key":"NAME","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true}],"views":[{"id":"V1","name":"Custom","type":"table","columns":[{"id":"TITLE","key":"NAME","visible":true}],"filters":[],"info_column_filters":[],"opaque":{"keep":1}}],"opaque":{"keep":2}}})).unwrap();
    let records=RecordsPage::parse(json!({"ok":true,"records":[{"id":"R1","fields":[{"key":"NAME","text":"Keep"}]}],"response_metadata":{"next_cursor":"NEXT"}})).unwrap();
    let policy = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    (document, records, policy)
}
fn intent(d: &Document, p: &RecordsPage, policy: &PersonalTaskLists) -> ColumnCreate {
    ColumnCreate::capture_kind(
        d,
        p,
        "V1",
        " Reviewer ",
        ColumnCreateKind::PeopleSingleDefault,
        "U1",
        Some("full"),
        Some(policy),
    )
    .unwrap()
}
fn added(d: &Document) -> Document {
    let mut raw = d.raw.clone();
    raw["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"SERVER_PERSON","key":"reviewer","name":"Reviewer","type":"user","options":{"format":"single_entity","show_member_name":true,"default_value":null},"opaque":{"server":1}}));
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"SERVER_PERSON","key":"reviewer","visible":true}));
    Document::parse(raw).unwrap()
}
#[test]
fn people_requires_exact_no_default_options_and_observes_only_isolated_addition() {
    let (d, p, policy) = baseline();
    let i = intent(&d, &p, &policy);
    let fresh = added(&d);
    assert_eq!(
        i.options(),
        Some(json!({"format":"single_entity","show_member_name":true,"default_value":null}))
    );
    let c = i.matching_addition(&fresh, &p).unwrap();
    assert_eq!(
        (&c.id, &c.key),
        (&"SERVER_PERSON".to_owned(), &"reviewer".to_owned())
    );
    assert_eq!(c.cell(&p.records[0]), CellValue::Empty);
    assert!(!p.records[0].fields.contains_key("reviewer"));
    for options in [
        None,
        Some(Value::Null),
        Some(json!({})),
        Some(json!({"format":"single_entity","show_member_name":true})),
        Some(json!({"format":"single_entity","showMemberName":true,"defaultValue":null})),
        Some(json!({"format":"multi_entity","show_member_name":true,"default_value":null})),
        Some(json!({"format":"single_entity","show_member_name":false,"default_value":null})),
        Some(json!({"format":"single_entity","show_member_name":true,"default_value":"U00000001"})),
        Some(
            json!({"format":"single_entity","show_member_name":true,"default_value":null,"default_value_typed":null}),
        ),
    ] {
        let mut raw = fresh.raw.clone();
        let col = raw["list_metadata"]["schema"][1].as_object_mut().unwrap();
        if let Some(v) = options {
            col.insert("options".into(), v);
        } else {
            col.remove("options");
        }
        assert!(
            i.matching_addition(&Document::parse(raw).unwrap(), &p)
                .is_none()
        );
    }
    for kind in ["assignee", "todo_assignee", "channel", "text"] {
        let mut raw = fresh.raw.clone();
        raw["list_metadata"]["schema"][1]["type"] = json!(kind);
        assert!(
            i.matching_addition(&Document::parse(raw).unwrap(), &p)
                .is_none()
        );
    }
    let mut changed = fresh.raw.clone();
    changed["list_metadata"]["opaque"]["keep"] = json!(3);
    assert!(
        i.matching_addition(&Document::parse(changed).unwrap(), &p)
            .is_none()
    );
    let mut changed = p.raw.clone();
    changed["response_metadata"]["next_cursor"] = json!("OTHER");
    assert!(
        i.matching_addition(&fresh, &RecordsPage::parse(changed).unwrap())
            .is_none()
    );
    let mut raw = d.raw.clone();
    raw["list_metadata"]["opaque"]["reviewer"] = json!(true);
    let orphan = Document::parse(raw).unwrap();
    assert!(
        intent(&orphan, &p, &policy)
            .matching_addition(&added(&orphan), &p)
            .is_none()
    );
    assert_eq!(i.original(), &d);
    assert_eq!(i.original_records(), &p);
}
#[test]
fn missing_people_is_known_empty_and_existing_single_person_set_clear_remains_strict() {
    let (d, p, policy) = baseline();
    let i = intent(&d, &p, &policy);
    let fresh = added(&d);
    let c = i.matching_addition(&fresh, &p).unwrap();
    assert_eq!(
        c.edit_value(&p.records[0], &fresh.id, Some(&policy))
            .unwrap(),
        CellEditValue::Users(vec![])
    );
    let capture = |page: &RecordsPage, ids: Vec<String>| {
        CellEdit::capture(
            &fresh,
            page,
            "R1",
            &c.id,
            Some("full"),
            Some(&policy),
            CellEditValue::Users(ids),
        )
    };
    assert!(capture(&p, vec![]).is_err());
    for ids in [
        vec!["U00000001".into(), "W00000002".into()],
        vec!["invalid".into()],
        vec!["C00000001".into()],
    ] {
        assert!(capture(&p, ids).is_err());
    }
    let edit = capture(&p, vec!["U00000001".into()]).unwrap();
    assert_eq!(
        edit.cells(),
        json!([{"row_id":"R1","column_id":"SERVER_PERSON","user":["U00000001"]}])
    );
    assert!(!edit.readback_matches(&fresh, &p));
    assert_eq!(
        edit.readback_value(&fresh, &p),
        Some(CellEditValue::Users(vec![]))
    );
    let mut raw = p.raw.clone();
    raw["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"reviewer","column_id":"SERVER_PERSON","user":["U00000001"]}));
    let stored = RecordsPage::parse(raw).unwrap();
    assert!(edit.readback_matches(&fresh, &stored));
    let clear = capture(&stored, vec![]).unwrap();
    assert_eq!(
        clear.cells(),
        json!([{"row_id":"R1","column_id":"SERVER_PERSON","user":[]}])
    );
    let mut raw = stored.raw.clone();
    raw["records"][0]["fields"][1]["user"] = json!([]);
    let cleared = RecordsPage::parse(raw).unwrap();
    assert!(clear.readback_matches(&fresh, &cleared));
    assert!(capture(&cleared, vec![]).is_err());
    assert!(!p.records[0].fields.contains_key("reviewer"));
    assert_eq!(cleared.records[0].fields["reviewer"]["user"], json!([]));
    assert!(i.matching_addition(&fresh, &stored).is_none());
    let mut raw = fresh.raw.clone();
    raw["list_metadata"]["schema"][1]["options"]["format"] = json!("multi_entity");
    assert!(!edit.readback_matches(&Document::parse(raw).unwrap(), &stored));
}

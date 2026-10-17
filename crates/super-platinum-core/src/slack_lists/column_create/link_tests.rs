use super::super::{CellEdit, CellEditValue, CellValue, LinkValue};
use super::*;

fn baseline() -> (Document, RecordsPage, PersonalTaskLists) {
    let d = Document::parse(json!({"id":"F_LINK","mode":"list","user":"U1","access":"owner","list_limits":{"over_column_maximum":false},"list_metadata":{"schema":[{"id":"TITLE","key":"NAME","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true}],"views":[{"id":"V1","name":"Custom","type":"table","columns":[{"id":"TITLE","key":"NAME","visible":true}],"filters":[],"info_column_filters":[]}],"opaque":{"keep":1}}})).unwrap();
    let p = RecordsPage::parse(json!({"ok":true,"records":[{"id":"R1","fields":[{"key":"NAME","text":"Keep"}]}],"response_metadata":{"next_cursor":"NEXT"}})).unwrap();
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
        " Reference ",
        ColumnCreateKind::Link,
        "U1",
        Some("full"),
        Some(policy),
    )
    .unwrap()
}
fn added(d: &Document, options: Option<Value>) -> Document {
    let mut raw = d.raw.clone();
    let mut column = json!({"id":"SERVER_LINK","key":"reference","name":"Reference","type":"link","opaque":{"server":1}});
    if let Some(options) = options {
        column["options"] = options;
    }
    raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .push(column);
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"SERVER_LINK","key":"reference","visible":true}));
    Document::parse(raw).unwrap()
}

#[test]
fn link_observation_accepts_two_distinct_raw_no_configuration_shapes_only() {
    let (d, p, policy) = baseline();
    let i = intent(&d, &p, &policy);
    assert_eq!(i.options(), None);
    for options in [None, Some(json!({}))] {
        let fresh = added(&d, options.clone());
        let snapshot = fresh.raw.clone();
        let c = i.matching_addition(&fresh, &p).unwrap();
        assert_eq!((&*c.id, &*c.key), ("SERVER_LINK", "reference"));
        assert_eq!(c.raw.get("options"), options.as_ref());
        assert_eq!(fresh.raw, snapshot);
        assert_eq!(c.cell(&p.records[0]), CellValue::Empty);
        assert_eq!(
            c.edit_value(&p.records[0], &fresh.id, Some(&policy))
                .unwrap(),
            CellEditValue::Link(None)
        );
        assert!(!p.records[0].fields.contains_key("reference"));
    }
    assert_ne!(added(&d, None), added(&d, Some(json!({}))));
    for options in [
        Value::Null,
        json!([]),
        json!(false),
        json!(""),
        json!({"format":"link"}),
        json!({"default_value":null}),
    ] {
        assert!(i.matching_addition(&added(&d, Some(options)), &p).is_none());
    }
    assert_eq!(i.original(), &d);
    assert_eq!(i.original_records(), &p);
}

#[test]
fn link_observation_keeps_identity_metadata_page_owner_and_orphan_guards() {
    let (d, p, policy) = baseline();
    let i = intent(&d, &p, &policy);
    let fresh = added(&d, None);
    for (pointer, value) in [
        ("/id", json!("OTHER")),
        ("/user", json!("U2")),
        ("/list_metadata/opaque/keep", json!(2)),
        ("/list_metadata/schema/1/type", json!("text")),
        ("/list_metadata/schema/1/name", json!("Other")),
        ("/list_metadata/schema/1/is_primary_column", json!(true)),
        ("/list_metadata/views/0/columns/1/key", json!("other")),
    ] {
        let mut raw = fresh.raw.clone();
        // is_primary_column is intentionally absent from the baseline.
        if pointer.ends_with("is_primary_column") {
            raw["list_metadata"]["schema"][1]["is_primary_column"] = value;
        } else {
            *raw.pointer_mut(pointer).unwrap() = value;
        }
        if let Ok(changed) = Document::parse(raw) {
            assert!(i.matching_addition(&changed, &p).is_none());
        }
    }
    for (pointer, value) in [
        ("/response_metadata/next_cursor", json!("OTHER")),
        ("/records/0/fields/0/text", json!("Changed")),
    ] {
        let mut raw = p.raw.clone();
        *raw.pointer_mut(pointer).unwrap() = value;
        let changed = RecordsPage::parse(raw).unwrap();
        assert!(i.matching_addition(&fresh, &changed).is_none());
        assert!(
            i.revalidate(&d, &changed, "U1", Some("full"), Some(&policy))
                .is_err()
        );
    }
    assert!(
        i.revalidate(&d, &p, "U2", Some("full"), Some(&policy))
            .is_err()
    );
    let mut raw = d.raw.clone();
    raw["list_metadata"]["opaque"]["reference"] = json!(true);
    let orphan = Document::parse(raw).unwrap();
    assert!(
        intent(&orphan, &p, &policy)
            .matching_addition(&added(&orphan, None), &p)
            .is_none()
    );
}

#[test]
fn observed_link_reuses_set_display_remove_and_raw_shape_preflight() {
    let (d, p, policy) = baseline();
    for options in [None, Some(json!({}))] {
        let fresh = added(&d, options.clone());
        let i = intent(&d, &p, &policy);
        let c = i.matching_addition(&fresh, &p).unwrap();
        let desired = CellEditValue::Link(Some(LinkValue {
            original_url: "https://example.com/spec".into(),
            display_name: Some("Spec".into()),
            display_as_url: false,
        }));
        let capture = |page: &RecordsPage, value| {
            CellEdit::capture(
                &fresh,
                page,
                "R1",
                &c.id,
                Some("full"),
                Some(&policy),
                value,
            )
        };
        assert!(capture(&p, CellEditValue::Link(None)).is_err());
        let edit = capture(&p, desired.clone()).unwrap();
        assert_eq!(
            edit.cells(),
            json!([{"row_id":"R1","column_id":"SERVER_LINK","link":[{"original_url":"https://example.com/spec","display_name":"Spec","display_as_url":false}]}])
        );
        assert_eq!(
            edit.readback_value(&fresh, &p),
            Some(CellEditValue::Link(None))
        );
        assert!(!edit.readback_matches(&fresh, &p));
        let mut raw = p.raw.clone();
        raw["records"][0]["fields"].as_array_mut().unwrap().push(json!({"key":"reference","column_id":"SERVER_LINK","link":[{"original_url":"https://example.com/spec","display_name":"Spec","display_as_url":false,"attachment":{"opaque":1}}]}));
        let stored = RecordsPage::parse(raw).unwrap();
        assert!(edit.readback_matches(&fresh, &stored));
        let mut url_display = desired.clone();
        if let CellEditValue::Link(Some(v)) = &mut url_display {
            v.display_as_url = true;
        }
        let toggle = capture(&stored, url_display).unwrap();
        assert_eq!(toggle.cells()[0]["link"][0]["display_as_url"], json!(true));
        assert!(toggle.cells()[0]["link"][0].get("attachment").is_none());
        let clear = capture(&stored, CellEditValue::Link(None)).unwrap();
        assert_eq!(
            clear.cells(),
            json!([{"row_id":"R1","column_id":"SERVER_LINK","link":[]} ])
        );
        assert!(clear.readback_matches(&fresh, &p));
        let other = added(
            &d,
            if options.is_none() {
                Some(json!({}))
            } else {
                None
            },
        );
        assert!(
            edit.revalidate(&other, &p, Some("full"), Some(&policy))
                .is_err()
        );
        assert!(!edit.readback_matches(&other, &stored));
        assert!(
            edit.revalidate(&fresh, &p, Some("full"), Some(&policy))
                .is_ok()
        );
        assert_eq!(p.raw["response_metadata"]["next_cursor"], "NEXT");
    }
}

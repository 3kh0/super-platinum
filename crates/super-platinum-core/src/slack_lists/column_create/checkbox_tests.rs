use super::*;
fn fixture() -> (Document, RecordsPage, PersonalTaskLists) {
    let d=Document::parse(json!({"id":"F1","mode":"list","access":"owner","user":"U1","created":1,"list_limits":{"over_column_maximum":false},"list_metadata":{"schema":[{"id":"TITLE","key":"NAME","name":"Title","type":"text","options":{"format":"text"},"is_primary_column":true}],"views":[{"id":"V1","name":"Custom","type":"table","columns":[{"id":"TITLE","key":"NAME","visible":true}],"sorts":[],"filters":[],"info_column_filters":[]}],"opaque":{"keep":1}}})).unwrap();
    let p=RecordsPage::parse(json!({"ok":true,"records":[{"id":"R1","fields":[{"key":"NAME","text":"Keep"}],"opaque":7},{"id":"R2","parent_record_id":"R1","fields":[]}],"response_metadata":{"next_cursor":"opaque+/="}})).unwrap();
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
        "Done",
        ColumnCreateKind::Checkbox,
        "U1",
        Some("full"),
        Some(policy),
    )
    .unwrap()
}
fn added(d: &Document, options: Option<Value>) -> Document {
    let mut raw = d.raw.clone();
    let mut c = json!({"id":"NEW_CHECK","key":"new_check","name":"Done","type":"checkbox","opaque":{"server":9}});
    if let Some(options) = options {
        c["options"] = options;
    }
    raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .insert(0, c);
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"NEW_CHECK","key":"new_check","visible":true,"opaque":{"width":8}}));
    Document::parse(raw).unwrap()
}
#[test]
fn both_no_option_observations_preserve_actual_raw_shape_without_backfill_or_default() {
    let (d, p, policy) = fixture();
    let original = p.clone();
    let intent = intent(&d, &p, &policy);
    assert_eq!(intent.kind().options(), None);
    for options in [None, Some(json!({}))] {
        let fresh = added(&d, options.clone());
        let c = intent.matching_addition(&fresh, &p).unwrap();
        assert_eq!(c.raw.get("options"), options.as_ref());
        assert_eq!(
            (&c.id, &c.key),
            (&"NEW_CHECK".to_owned(), &"new_check".to_owned())
        );
        for record in &p.records {
            assert_eq!(c.cell(record), super::super::CellValue::Empty);
            assert_eq!(
                c.checkbox_input_value_with_policy(record, &fresh.id, Some(&policy)),
                Some(false)
            );
            assert!(!c.editable_value(record, &fresh.id, Some(&policy)));
        }
        assert_eq!(p, original);
        assert_eq!(fresh.primary_column().unwrap().id, "TITLE");
        let mut restored = fresh.raw.clone();
        restored["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        restored["list_metadata"]["views"][0]["columns"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert_eq!(restored, d.raw);
    }
}
#[test]
fn unknown_options_type_metadata_or_population_never_become_checkbox_observation() {
    let (d, p, policy) = fixture();
    let i = intent(&d, &p, &policy);
    for options in [
        Value::Null,
        json!([]),
        json!(false),
        json!(0),
        json!(""),
        json!({"markAsDoneWhenChecked":false}),
        json!({"precision":0}),
    ] {
        assert!(i.matching_addition(&added(&d, Some(options)), &p).is_none());
    }
    for kind in ["completed", "todo_completed", "text", "number"] {
        let mut raw = added(&d, None).raw;
        raw["list_metadata"]["schema"][0]["type"] = json!(kind);
        assert!(
            i.matching_addition(&Document::parse(raw).unwrap(), &p)
                .is_none()
        );
    }
    let mut raw = added(&d, None).raw;
    raw["list_metadata"]["opaque"]["keep"] = json!(2);
    assert!(
        i.matching_addition(&Document::parse(raw).unwrap(), &p)
            .is_none()
    );
    for member in ["records", "response_metadata"] {
        let mut raw = p.raw.clone();
        if member == "records" {
            raw["records"][0]["fields"]
                .as_array_mut()
                .unwrap()
                .push(json!({"key":"new_check","checkbox":false}));
        } else {
            raw["response_metadata"]["next_cursor"] = json!("other");
        }
        assert!(
            i.matching_addition(&added(&d, None), &RecordsPage::parse(raw).unwrap())
                .is_none()
        );
    }
}
#[test]
fn orphan_identity_or_second_addition_refuses_and_admission_remains_once_owned() {
    let (d, p, policy) = fixture();
    for orphan in ["NEW_CHECK", "new_check"] {
        let mut raw = p.raw.clone();
        raw["records"][0]["opaque"] = json!({"old_reference":orphan});
        let orphaned = RecordsPage::parse(raw).unwrap();
        assert!(
            intent(&d, &orphaned, &policy)
                .matching_addition(&added(&d, None), &orphaned)
                .is_none()
        );
    }
    let mut fresh = added(&d, None).raw;
    fresh["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"SECOND","key":"second","name":"Done","type":"checkbox"}));
    assert!(
        intent(&d, &p, &policy)
            .matching_addition(&Document::parse(fresh).unwrap(), &p)
            .is_none()
    );
    let mut attempt = ColumnCreateAttempt::new(intent(&d, &p, &policy));
    assert!(
        attempt
            .admit(&d, &p, "other", Some("full"), Some(&policy))
            .is_err()
    );
    assert!(
        attempt
            .admit(&d, &p, "U1", Some("full"), Some(&policy))
            .is_ok()
    );
    assert!(
        attempt
            .admit(&d, &p, "U1", Some("full"), Some(&policy))
            .is_err()
    );
}

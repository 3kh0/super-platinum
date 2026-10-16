use super::*;
use crate::slack_lists::{CellEdit, CellEditValue};
fn document(options: Option<Value>) -> Document {
    let mut c = json!({"id":"C1","key":"done","name":"Done","type":"checkbox"});
    if let Some(o) = options {
        c["options"] = o;
    }
    Document::parse(
        json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[c],"views":[]}}),
    )
    .unwrap()
}
fn page(field: Option<Value>) -> RecordsPage {
    RecordsPage::parse(json!({"ok":true,"records":[{"id":"R1","date_created":1,"created_by":"U1","fields":field.into_iter().collect::<Vec<_>>(),"opaque":{"keep":1}}],"response_metadata":{"next_cursor":"NEXT"}})).unwrap()
}
fn capture(d: &Document, p: &RecordsPage, checked: bool) -> Result<CheckboxEdit, ListError> {
    CheckboxEdit::capture(d, p, "R1", "C1", Some("full"), checked)
}
#[test]
fn missing_first_true_keeps_raw_empty_and_generic_editor_refuses() {
    for options in [None, Some(json!({}))] {
        let d = document(options);
        let p = page(None);
        let original = p.clone();
        let c = d.column("C1").unwrap();
        let r = p.record("R1").unwrap();
        assert_eq!(c.cell(r), CellValue::Empty);
        assert_eq!(
            c.checkbox_input_value_with_policy(r, "F1", None),
            Some(false)
        );
        assert!(!c.editable_checkbox(r));
        assert!(!c.editable_value(r, "F1", None));
        assert!(c.edit_value(r, "F1", None).is_err());
        assert!(
            CellEdit::capture(
                &d,
                &p,
                "R1",
                "C1",
                Some("full"),
                None,
                CellEditValue::Boolean(true)
            )
            .is_err()
        );
        assert!(capture(&d, &p, false).is_err());
        let e = capture(&d, &p, true).unwrap();
        assert_eq!(
            e.cells(),
            json!([{"row_id":"R1","column_id":"C1","checkbox":true}])
        );
        assert_eq!(e.readback_value(&d, &p), None);
        assert!(e.revalidate(&d, &p, Some("full")).is_ok());
        assert_eq!(p, original);
    }
}
#[test]
fn malformed_present_special_or_inactive_missing_are_not_input_defaults() {
    let d = document(None);
    for f in [
        json!({"key":"done"}),
        json!({"key":"done","checkbox":null}),
        json!({"key":"done","checkbox":[]}),
        json!({"key":"done","checkbox":"false"}),
        json!({"key":"done","checkbox":false,"column_id":"WRONG"}),
        json!({"key":"done","checkbox":false,"row_id":"WRONG"}),
        json!({"key":"other","column_id":"C1","checkbox":false}),
    ] {
        let p = page(Some(f));
        assert_eq!(
            d.column("C1")
                .unwrap()
                .checkbox_input_value_with_policy(&p.records[0], "F1", None),
            None
        );
        assert!(capture(&d, &p, true).is_err());
    }
    for options in [
        Value::Null,
        json!([]),
        json!(false),
        json!({"markAsDoneWhenChecked":true}),
        json!({"unknown":false}),
    ] {
        assert!(capture(&document(Some(options)), &page(None), true).is_err());
    }
    for kind in ["completed", "todo_completed", "text"] {
        let mut raw = d.raw.clone();
        raw["list_metadata"]["schema"][0]["type"] = json!(kind);
        assert!(capture(&Document::parse(raw).unwrap(), &page(None), true).is_err());
    }
    for key in ["archived", "is_deleted"] {
        let mut raw = page(None).raw;
        raw["records"][0][key] = json!(true);
        assert!(capture(&d, &RecordsPage::parse(raw).unwrap(), true).is_err());
    }
    let mut raw = d.raw.clone();
    raw["access"] = json!("read");
    assert!(capture(&Document::parse(raw).unwrap(), &page(None), true).is_err());
}
#[test]
fn exact_preflight_and_readback_keep_absent_and_empty_options_distinct() {
    for options in [None, Some(json!({}))] {
        let d = document(options.clone());
        let p = page(None);
        let e = capture(&d, &p, true).unwrap();
        let changed = document(if options.is_none() {
            Some(json!({}))
        } else {
            None
        });
        assert!(e.revalidate(&changed, &p, Some("full")).is_err());
        assert_eq!(
            e.readback_value(&changed, &page(Some(json!({"key":"done","checkbox":true})))),
            None
        );
        for f in [
            json!({"key":"done"}),
            json!({"key":"done","checkbox":null}),
            json!({"key":"done","checkbox":true}),
            json!({"key":"done","checkbox":false}),
        ] {
            assert!(e.revalidate(&d, &page(Some(f)), Some("full")).is_err());
        }
        assert_eq!(e.readback_value(&d, &p), None);
        for f in [
            json!({"key":"done"}),
            json!({"key":"done","checkbox":null}),
            json!({"key":"done","checkbox":true,"row_id":"OTHER"}),
            json!({"key":"done","checkbox":true,"column_id":"OTHER"}),
        ] {
            assert_eq!(e.readback_value(&d, &page(Some(f))), None);
        }
        for value in [true, false] {
            let current = page(Some(
                json!({"key":"done","column_id":"C1","row_id":"R1","checkbox":value}),
            ));
            assert_eq!(e.readback_value(&d, &current), Some(value));
            assert_eq!(e.readback_matches(&d, &current), value);
            let next = capture(&d, &current, !value).unwrap();
            assert_eq!(next.checked(), !value);
            assert!(
                CellEdit::capture(
                    &d,
                    &current,
                    "R1",
                    "C1",
                    Some("full"),
                    None,
                    CellEditValue::Boolean(!value)
                )
                .is_ok()
            );
        }
        assert!(e.validate_response(&json!({"ok":true})).is_err());
        assert!(
            e.validate_response(
                &json!({"ok":true,"cells":[{"row_id":"R1","column_id":"C1","checkbox":true}]})
            )
            .is_ok()
        );
        assert!(
            e.validate_response(
                &json!({"ok":true,"cells":[{"row_id":"R1","column_id":"C1","checkbox":false}]})
            )
            .is_err()
        );
    }
}
#[test]
fn existing_boolean_eligibility_is_preserved_even_outside_missing_subset() {
    for options in [None, Some(json!({})), Some(json!({"legacy":true}))] {
        let d = document(options);
        for value in [false, true] {
            let p = page(Some(json!({"key":"done","checkbox":value})));
            let c = d.column("C1").unwrap();
            assert!(c.editable_checkbox(&p.records[0]));
            assert_eq!(
                c.checkbox_input_value_with_policy(&p.records[0], "F1", None),
                Some(value)
            );
            assert!(capture(&d, &p, !value).is_ok());
            assert!(capture(&d, &p, value).is_err());
        }
    }
}

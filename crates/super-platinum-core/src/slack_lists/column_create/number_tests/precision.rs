use super::*;
#[test]
fn all_six_precisions_capture_exact_readback_and_default_wrapper_identity() {
    let d = document();
    let p = records();
    let policy = ordinary();
    for n in 0..=5 {
        let precision = NumberCreatePrecision::capture(n).unwrap();
        let intent = ColumnCreate::capture_number(
            &d,
            &p,
            "V1",
            "Amount",
            precision,
            "U1",
            Some("full"),
            Some(&policy),
        )
        .unwrap();
        assert_eq!(intent.options(), Some(json!({"precision":n})));
        if n == 0 {
            assert_eq!(intent, capture(&d, &p, ColumnCreateKind::NumberDefault0));
        }
        let mut raw = added(&d, ColumnCreateKind::NumberDefault0).raw;
        raw["list_metadata"]["schema"][1]["options"] = precision.wire();
        let fresh = Document::parse(raw).unwrap();
        assert_eq!(
            intent.matching_addition(&fresh, &p).unwrap().id,
            "SERVER_NEW"
        );
        for bad in [
            Value::Null,
            json!({}),
            json!({"precision":(n+1)%6}),
            json!({"precision":n,"format":"currency"}),
        ] {
            let mut raw = fresh.raw.clone();
            raw["list_metadata"]["schema"][1]["options"] = bad;
            assert!(
                intent
                    .matching_addition(&Document::parse(raw).unwrap(), &p)
                    .is_none()
            );
        }
        let mut page = p.raw.clone();
        page["response_metadata"]["next_cursor"] = json!("OTHER");
        assert!(
            intent
                .matching_addition(&fresh, &RecordsPage::parse(page).unwrap())
                .is_none()
        );
        assert_eq!(intent.original(), &d);
        assert_eq!(intent.original_records(), &p);
    }
}
#[test]
fn precision_wire_is_closed_and_never_coerces_unknown_shapes() {
    for n in 0..=5 {
        let p = NumberCreatePrecision::capture(n).unwrap();
        assert_eq!(NumberCreatePrecision::from_wire(&p.wire()).unwrap(), p);
    }
    assert!(NumberCreatePrecision::capture(6).is_err());
    assert!(NumberCreatePrecision::capture(255).is_err());
    for value in [
        Value::Null,
        json!({}),
        json!({"precision":null}),
        json!({"precision":-1}),
        json!({"precision":6}),
        json!({"precision":1.5}),
        json!({"precision":"2"}),
        json!({"precision":true}),
        json!({"precision":2,"default_value":null}),
    ] {
        assert!(NumberCreatePrecision::from_wire(&value).is_err());
    }
}
#[test]
fn chosen_precision_preserves_exact_cell_value_and_known_empty_readback() {
    use super::super::super::{CellEdit, CellEditValue};
    let d = document();
    let p = records();
    let policy = ordinary();
    let create = ColumnCreate::capture_number(
        &d,
        &p,
        "V1",
        "Amount",
        NumberCreatePrecision::capture(3).unwrap(),
        "U1",
        Some("full"),
        Some(&policy),
    )
    .unwrap();
    let mut raw = added(&d, ColumnCreateKind::NumberDefault0).raw;
    raw["list_metadata"]["schema"][1]["options"] = json!({"precision":3});
    let fresh = Document::parse(raw).unwrap();
    assert!(create.matching_addition(&fresh, &p).is_some());
    let value = CellEditValue::Number(Some("-12.3456".parse().unwrap()));
    let edit = CellEdit::capture(
        &fresh,
        &p,
        "R1",
        "SERVER_NEW",
        Some("full"),
        Some(&policy),
        value.clone(),
    )
    .unwrap();
    assert_eq!(
        edit.cells(),
        json!([{"row_id":"R1","column_id":"SERVER_NEW","number":[-12.3456]}])
    );
    assert_eq!(
        edit.readback_value(&fresh, &p),
        Some(CellEditValue::Number(None))
    );
    assert!(!edit.readback_matches(&fresh, &p));
    let mut raw = p.raw.clone();
    raw["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"new_amount","column_id":"SERVER_NEW","number":[-12.3456]}));
    let stored = RecordsPage::parse(raw).unwrap();
    assert!(edit.readback_matches(&fresh, &stored));
    let clear = CellEdit::capture(
        &fresh,
        &stored,
        "R1",
        "SERVER_NEW",
        Some("full"),
        Some(&policy),
        CellEditValue::Number(None),
    )
    .unwrap();
    assert!(clear.readback_matches(&fresh, &p));
    let mut empty = stored.raw.clone();
    empty["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap()["number"] = json!([]);
    let empty = RecordsPage::parse(empty).unwrap();
    assert!(clear.readback_matches(&fresh, &empty));
    assert_ne!(p.raw, empty.raw);
    let mut drift = fresh.raw.clone();
    drift["list_metadata"]["schema"][1]["options"]["precision"] = json!(2);
    assert!(
        edit.readback_value(&Document::parse(drift).unwrap(), &stored)
            .is_none()
    );
}

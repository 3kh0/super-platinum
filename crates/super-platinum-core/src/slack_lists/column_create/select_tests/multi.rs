use super::*;

fn configuration() -> SelectCreateOptions {
    SelectCreateOptions::capture_format(SelectCreateFormat::Multiple, choices().choices().to_vec())
        .unwrap()
}
fn document(d: &Document) -> Document {
    let mut raw = added(d).raw;
    raw["list_metadata"]["schema"][1]["options"] = configuration().wire();
    Document::parse(raw).unwrap()
}
fn page(p: &RecordsPage, values: Value) -> RecordsPage {
    let mut raw = p.raw.clone();
    raw["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "key":"actual_select","column_id":"SELECT_SERVER","row_id":"R1","select":values,
            "opaque_cell":{"untouched":true}
        }));
    RecordsPage::parse(raw).unwrap()
}

#[test]
fn closed_cardinality_preserves_single_and_exact_multi_options_without_defaults() {
    assert_eq!(choices().format(), SelectCreateFormat::Single);
    assert_eq!(choices().wire()["format"], "single_select");
    let multi = configuration();
    assert_eq!(multi.format(), SelectCreateFormat::Multiple);
    assert_eq!(multi.choices(), choices().choices());
    assert_eq!(multi.wire()["format"], "multi_select");
    assert_eq!(multi.wire().as_object().unwrap().len(), 2);
    assert_eq!(
        SelectCreateOptions::from_wire(&multi.wire()).unwrap(),
        multi
    );
    for value in [
        Value::Null,
        json!(true),
        json!("multiple"),
        json!("multi_select "),
    ] {
        let mut raw = multi.wire();
        raw["format"] = value;
        assert!(SelectCreateOptions::from_wire(&raw).is_err());
    }
    for field in [
        "default_value",
        "default_value_typed",
        "position",
        "unknown",
    ] {
        let mut raw = multi.wire();
        raw[field] = Value::Null;
        assert!(SelectCreateOptions::from_wire(&raw).is_err());
    }
    let mut duplicate = multi.wire();
    duplicate["choices"][1] = duplicate["choices"][0].clone();
    assert!(SelectCreateOptions::from_wire(&duplicate).is_err());
    assert!(SelectCreateOptions::capture_format(SelectCreateFormat::Multiple, vec![]).is_err());
}

#[test]
fn captured_multi_addition_requires_original_format_order_ids_and_raw_population() {
    let (d, p, policy) = baseline();
    let original = d.raw.clone();
    let intent = ColumnCreate::capture_select(
        &d,
        &p,
        "V1",
        "Phase",
        configuration(),
        "U1",
        Some("full"),
        Some(&policy),
    )
    .unwrap();
    let fresh = document(&d);
    let field = intent.matching_addition(&fresh, &p).unwrap();
    assert!(field.is_multi_select());
    assert_eq!(field.raw["opaque"], json!({"keep":9}));
    assert_eq!(d.raw, original);
    assert_eq!(intent.options(), Some(configuration().wire()));
    for change in 0..7 {
        let mut raw = fresh.raw.clone();
        let options = &mut raw["list_metadata"]["schema"][1]["options"];
        match change {
            0 => options["format"] = json!("single_select"),
            1 => options["choices"].as_array_mut().unwrap().reverse(),
            2 => options["choices"][0]["value"] = json!("OptSERVER00"),
            3 => options["choices"][0]["color"] = json!("blue"),
            4 => options["default_value"] = json!([]),
            5 => raw["list_metadata"]["schema"][1]["type"] = json!("multi_select"),
            _ => raw["list_metadata"]["opaque"] = json!({"changed":true}),
        }
        assert!(
            intent
                .matching_addition(&Document::parse(raw).unwrap(), &p)
                .is_none()
        );
    }
    assert!(
        intent
            .matching_addition(&fresh, &page(&p, json!([])))
            .is_none()
    );
    let mut attempt = ColumnCreateAttempt::new(intent);
    attempt
        .admit(&d, &p, "U1", Some("full"), Some(&policy))
        .unwrap();
    assert!(attempt.finish(false));
    assert!(
        attempt
            .admit(&d, &p, "U1", Some("full"), Some(&policy))
            .is_err()
    );
}

#[test]
fn existing_multi_cell_path_preserves_submission_order_remove_one_clear_and_missing() {
    let (d, p, policy) = baseline();
    let fresh = document(&d);
    let capture = |records: &RecordsPage, values: Vec<String>| {
        CellEdit::capture(
            &fresh,
            records,
            "R1",
            "SELECT_SERVER",
            Some("full"),
            Some(&policy),
            CellEditValue::Select(values),
        )
    };
    let ids = vec!["Opt12345678".into(), "OptABCDEFGH".into()];
    let edit = capture(&p, ids.clone()).unwrap();
    assert_eq!(
        edit.cells(),
        json!([{"row_id":"R1","column_id":"SELECT_SERVER","select":ids}])
    );
    assert_eq!(
        edit.readback_value(&fresh, &p),
        Some(CellEditValue::Select(vec![]))
    );
    let stored = page(&p, json!(ids));
    assert!(edit.readback_matches(&fresh, &stored));
    let reordered = page(&p, json!(["OptABCDEFGH", "Opt12345678"]));
    assert!(!edit.readback_matches(&fresh, &reordered));
    let one = vec!["Opt12345678".into()];
    let remove = capture(&stored, one.clone()).unwrap();
    assert_eq!(remove.cells()[0]["select"], json!(one));
    let single = page(&p, json!(one));
    assert!(remove.readback_matches(&fresh, &single));
    let clear = capture(&single, vec![]).unwrap();
    assert_eq!(clear.cells()[0]["select"], json!([]));
    assert!(clear.readback_matches(&fresh, &p));
    assert!(clear.readback_matches(&fresh, &page(&p, json!([]))));
    assert_ne!(p.raw, page(&p, json!([])).raw);
    assert!(capture(&p, vec![]).is_err());
    assert!(capture(&p, vec!["OptUNKNOWN0".into()]).is_err());
    assert!(capture(&p, vec!["Opt12345678".into(), "Opt12345678".into()]).is_err());
    assert!(
        CellEdit::capture(
            &added(&d),
            &p,
            "R1",
            "SELECT_SERVER",
            Some("full"),
            Some(&policy),
            CellEditValue::Select(ids)
        )
        .is_err()
    );
}

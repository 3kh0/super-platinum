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

fn choices() -> SelectCreateOptions {
    SelectCreateOptions::capture(vec![
        SelectCreateOption::capture("Ready", "OptABCDEFGH", "cyan").unwrap(),
        SelectCreateOption::capture("Équipe 東京", "Opt12345678", "green").unwrap(),
    ])
    .unwrap()
}
fn intent(d: &Document, p: &RecordsPage, policy: &PersonalTaskLists) -> ColumnCreate {
    ColumnCreate::capture_select(
        d,
        p,
        "V1",
        "Phase",
        choices(),
        "U1",
        Some("full"),
        Some(policy),
    )
    .unwrap()
}
fn added(d: &Document) -> Document {
    let mut raw = d.raw.clone();
    raw["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"SELECT_SERVER","key":"actual_select","name":"Phase","type":"select","options":choices().wire(),"opaque":{"keep":9}}));
    raw["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"SELECT_SERVER","key":"actual_select","visible":true}));
    Document::parse(raw).unwrap()
}
#[test]
fn labels_identity_palette_count_and_exact_wire_are_bounded() {
    assert_eq!(
        SelectCreateOption::capture(" Ready ", "OptABCDEFGH", "cyan")
            .unwrap()
            .label(),
        "Ready"
    );
    for label in ["", " ", "bad\nlabel", "x\0", &"😀".repeat(129)] {
        assert!(!SelectCreateOption::valid_label(label));
    }
    assert!(SelectCreateOption::valid_label(&"😀".repeat(128)));
    for (value, color) in [
        ("Optabcdef12", "cyan"),
        ("OptABCDEFGH", "red"),
        ("OptABCDE|GH", "cyan"),
        ("Opt123", "green"),
    ] {
        assert!(SelectCreateOption::capture("Ready", value, color).is_err());
    }
    let option = SelectCreateOption::generate("Ready").unwrap();
    assert_eq!(option.value().len(), 11);
    assert!(SelectCreateOptions::capture(vec![]).is_err());
    assert!(SelectCreateOptions::capture(vec![option.clone(), option]).is_err());
    let eight = (0..8)
        .map(|n| SelectCreateOption::capture("Same label", &format!("Opt{n:08}"), "blue").unwrap())
        .collect::<Vec<_>>();
    assert!(SelectCreateOptions::capture(eight.clone()).is_ok());
    let mut nine = eight;
    nine.push(SelectCreateOption::capture("Ninth", "OptABCDEFGH", "cyan").unwrap());
    assert!(SelectCreateOptions::capture(nine).is_err());
    assert_eq!(
        SelectCreateOptions::from_wire(&choices().wire()).unwrap(),
        choices()
    );
    let mut padded = choices().wire();
    padded["choices"][0]["label"] = json!(" Ready ");
    assert!(SelectCreateOptions::from_wire(&padded).is_err());
    for extra in ["default_value", "default_value_typed", "unknown"] {
        let mut raw = choices().wire();
        raw[extra] = json!(null);
        assert!(SelectCreateOptions::from_wire(&raw).is_err());
    }
}
#[test]
fn observed_select_is_exact_ordered_config_without_defaults_or_backfill() {
    let (d, p, policy) = baseline();
    let intent = intent(&d, &p, &policy);
    let fresh = added(&d);
    let column = intent.matching_addition(&fresh, &p).unwrap();
    assert_eq!(column.raw["opaque"], json!({"keep":9}));
    assert!(p.records.iter().all(|r| column.cell(r) == CellValue::Empty));
    for change in 0..6 {
        let mut raw = fresh.raw.clone();
        let opts = &mut raw["list_metadata"]["schema"][1]["options"];
        match change {
            0 => opts["choices"].as_array_mut().unwrap().reverse(),
            1 => opts["choices"][0]["value"] = json!("OptSERVER00"),
            2 => opts["choices"][0]["color"] = json!("blue"),
            3 => opts["default_value"] = json!("OptABCDEFGH"),
            4 => *opts = Value::Null,
            _ => raw["list_metadata"]["opaque"]["keep"] = json!(3),
        }
        assert!(
            intent
                .matching_addition(&Document::parse(raw).unwrap(), &p)
                .is_none()
        );
    }
    let mut changed = p.raw.clone();
    changed["response_metadata"]["next_cursor"] = json!("OTHER");
    assert!(
        intent
            .matching_addition(&fresh, &RecordsPage::parse(changed).unwrap())
            .is_none()
    );
    assert!(
        ColumnCreate::capture_kind(
            &d,
            &p,
            "V1",
            "Phase",
            ColumnCreateKind::Select,
            "U1",
            Some("full"),
            Some(&policy)
        )
        .is_err()
    );
    let mut attempt = ColumnCreateAttempt::new(intent);
    assert!(
        attempt
            .admit(&d, &p, "OTHER", Some("full"), Some(&policy))
            .is_err()
    );
    assert!(
        attempt
            .admit(&d, &p, "U1", Some("read"), Some(&policy))
            .is_err()
    );
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
fn missing_select_is_known_empty_and_uses_existing_first_selection_clear_noop() {
    let (d, p, policy) = baseline();
    let fresh = added(&d);
    let c = fresh.column("SELECT_SERVER").unwrap();
    assert_eq!(
        c.edit_value(&p.records[0], &fresh.id, Some(&policy))
            .unwrap(),
        CellEditValue::Select(vec![])
    );
    let edit = CellEdit::capture(
        &fresh,
        &p,
        "R1",
        &c.id,
        Some("full"),
        Some(&policy),
        CellEditValue::Select(vec!["OptABCDEFGH".into()]),
    )
    .unwrap();
    assert_eq!(
        edit.cells(),
        json!([{"row_id":"R1","column_id":"SELECT_SERVER","select":["OptABCDEFGH"]}])
    );
    assert_eq!(
        edit.readback_value(&fresh, &p),
        Some(CellEditValue::Select(vec![]))
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
            CellEditValue::Select(vec![])
        )
        .is_err()
    );
    assert!(
        CellEdit::capture(
            &fresh,
            &p,
            "R1",
            &c.id,
            Some("full"),
            Some(&policy),
            CellEditValue::Select(vec!["OptUNKNOWN0".into()])
        )
        .is_err()
    );
    let mut raw = p.raw.clone();
    raw["records"][0]["fields"].as_array_mut().unwrap().push(json!({"key":"actual_select","column_id":"SELECT_SERVER","row_id":"R1","select":["OptABCDEFGH"]}));
    let stored = RecordsPage::parse(raw).unwrap();
    assert!(edit.readback_matches(&fresh, &stored));
    let clear = CellEdit::capture(
        &fresh,
        &stored,
        "R1",
        &c.id,
        Some("full"),
        Some(&policy),
        CellEditValue::Select(vec![]),
    )
    .unwrap();
    assert!(clear.readback_matches(&fresh, &p));
}

mod multi;

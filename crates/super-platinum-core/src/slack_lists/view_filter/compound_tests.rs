use super::*;
fn document() -> Document {
    let schema=(0..10).map(|i|json!({"id":format!("C{i}"),"key":format!("k{i}"),"name":format!("Field {i}"),"type":if i==0{"checkbox"}else{"number"},"options":if i==0{json!({})}else{json!({"precision":2})}})).collect::<Vec<_>>();
    Document::parse(json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":schema,"views":[{"id":"V1","name":"Custom","type":"table","columns":[],"filters":[],"info_column_filters":[],"sorts":[{"key":"k1","ascending":true}],"grouping":{"group_by":"k0","group_by_column_id":"C0"},"opaque":{"preserve":1}}]}})).unwrap()
}
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn number(i: usize, v: &str) -> ViewFilterChoice {
    ViewFilterChoice::Number {
        column_id: format!("C{i}"),
        operator: NumberFilterOperator::LessThan,
        value: Some(v.into()),
    }
}
fn capture(doc: &Document, choice: ViewFilterChoice) -> Result<ViewFilter, ListError> {
    ViewFilter::capture(doc, "V1", choice, Some("full"), Some(&personal()))
}
fn canonical(intent: &ViewFilter) -> Document {
    let mut raw = intent.original().raw.clone();
    raw["list_metadata"]["views"][0]["filters"] = intent.filters().clone();
    raw["list_metadata"]["views"][0]["info_column_filters"] = json!([]);
    Document::parse(raw).unwrap()
}
#[test]
fn ordered_complete_sets_keep_lexical_values_grouping_and_exact_observation() {
    let doc = document();
    let desired = ViewFilterChoice::All(vec![
        ViewFilterChoice::Checkbox {
            column_id: "C0".into(),
            checked: false,
        },
        number(1, "12.50"),
        number(2, "-0"),
    ]);
    let intent = capture(&doc, desired.clone()).unwrap();
    assert_eq!(
        intent.filters(),
        &json!([{"key":"k0","operator":"is_empty","values":[]},{"key":"k1","operator":"less_than","values":["12.50"]},{"key":"k2","operator":"less_than","values":["-0"]}])
    );
    let observed = canonical(&intent);
    assert_eq!(ViewFilter::current(&observed, "V1"), Some(desired));
    assert!(intent.observes_desired(&observed));
    for member in ["grouping", "sorts", "opaque", "columns"] {
        assert_eq!(observed.views[0][member], doc.views[0][member]);
    }
    let mut changed = observed.raw.clone();
    changed["list_metadata"]["views"][0]["filters"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert!(!intent.observes_desired(&Document::parse(changed).unwrap()));
    let mut changed = observed.raw.clone();
    changed["list_metadata"]["views"][0]["filters"][1]["values"] = json!(["12.5"]);
    assert!(!intent.observes_desired(&Document::parse(changed).unwrap()));
    let clear = capture(&observed, ViewFilterChoice::None).unwrap();
    assert_eq!(clear.filters(), &json!([]));
    assert!(clear.observes_desired(&doc));
}
#[test]
fn bounds_flatness_duplicates_and_unsupported_loaded_sets_refuse_even_clear() {
    let doc = document();
    let eight = ViewFilterChoice::All((1..=8).map(|i| number(i, "1")).collect());
    assert!(capture(&doc, eight).is_ok());
    for value in [
        ViewFilterChoice::All((1..=9).map(|i| number(i, "1")).collect()),
        ViewFilterChoice::All(vec![number(1, "1"), number(1, "2")]),
        ViewFilterChoice::All(vec![ViewFilterChoice::None, number(1, "1")]),
        ViewFilterChoice::All(vec![
            ViewFilterChoice::All(vec![number(1, "1"), number(2, "2")]),
            number(3, "3"),
        ]),
        ViewFilterChoice::All(vec![number(1, "1")]),
        ViewFilterChoice::All(vec![]),
    ] {
        assert!(capture(&doc, value).is_err());
    }
    let valid = capture(
        &doc,
        ViewFilterChoice::All(vec![number(1, "12.50"), number(2, "-2")]),
    )
    .unwrap();
    let base = canonical(&valid);
    let mut invalid = Vec::new();
    let mut raw = base.raw.clone();
    raw["list_metadata"]["views"][0]["filters"] = json!(
        (1..=9)
            .map(|i| json!({"key":format!("k{i}"),"operator":"is_empty","values":[]}))
            .collect::<Vec<_>>()
    );
    invalid.push(raw);
    for key in ["filters", "info_column_filters"] {
        let mut raw = base.raw.clone();
        raw["list_metadata"]["views"][0][key] = Value::Null;
        invalid.push(raw);
    }
    let mut raw = base.raw.clone();
    raw["list_metadata"]["views"][0]["filter_set"] = json!({});
    invalid.push(raw);
    for extra in [
        json!({"key":"k1","operator":"less_than","values":["1"],"column_id":"C1"}),
        json!({"key":"k1","operator":"less_than","values":[1]}),
        json!({"key":"k1","operator":"unknown","values":[]}),
        base.views[0]["filters"][0].clone(),
    ] {
        let mut raw = base.raw.clone();
        raw["list_metadata"]["views"][0]["filters"][1] = extra;
        invalid.push(raw);
    }
    for raw in invalid {
        let bad = Document::parse(raw).unwrap();
        assert!(ViewFilter::current(&bad, "V1").is_none());
        assert!(capture(&bad, ViewFilterChoice::None).is_err());
    }
}
#[test]
fn absent_empty_remains_noop_but_changed_clear_requires_explicit_vectors() {
    let mut raw = document().raw;
    for key in ["filters", "info_column_filters"] {
        raw["list_metadata"]["views"][0]
            .as_object_mut()
            .unwrap()
            .remove(key);
    }
    let empty = Document::parse(raw).unwrap();
    assert!(capture(&empty, ViewFilterChoice::None).is_err());
    let intent = capture(
        &empty,
        ViewFilterChoice::All(vec![number(1, "1"), number(2, "2")]),
    )
    .unwrap();
    let current = canonical(&intent);
    let clear = capture(&current, ViewFilterChoice::None).unwrap();
    assert!(!clear.observes_desired(&empty));
    assert!(clear.observes_desired(&document()));
}

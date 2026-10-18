use super::*;
fn document(format: &str) -> Document {
    Document::parse(json!({"id":"F1","mode":"list","access":"write","list_metadata":{"schema":[
 {"id":"S","key":"status","name":"Status","type":"select","options":{"format":format,"choices":[{"value":"z","label":"Same","color":"red"},{"value":"é","label":"Same","color":"blue"}]}},
 {"id":"N","key":"estimate","name":"Estimate","type":"number","options":{"precision":2}},
 {"id":"C","key":"done","name":"Done","type":"checkbox","options":{}}],
 "views":[{"id":"V","name":"Shared","type":"table","columns":[],"filters":[],"info_column_filters":[],"sorts":[{"key":"estimate","ascending":false}],"opaque":{"keep":true}}]}})).unwrap()
}
fn capture(doc: &Document, desired: ViewFilterChoice) -> Result<ViewFilter, ListError> {
    let personal = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    ViewFilter::capture(doc, "V", desired, Some("full"), Some(&personal))
}
fn choice(op: SelectFilterOperator, values: &[&str]) -> ViewFilterChoice {
    ViewFilterChoice::Select {
        column_id: "S".into(),
        operator: op,
        values: values.iter().map(|s| (*s).into()).collect(),
    }
}
#[test]
fn mixed_set_preserves_order_lexical_values_and_exact_readback_for_both_cardinalities() {
    for format in ["single_select", "multi_select"] {
        let doc = document(format);
        let desired = ViewFilterChoice::All(vec![
            ViewFilterChoice::Checkbox {
                column_id: "C".into(),
                checked: false,
            },
            choice(SelectFilterOperator::Contains, &["é", "z"]),
            ViewFilterChoice::Number {
                column_id: "N".into(),
                operator: NumberFilterOperator::LessThan,
                value: Some("12.50".into()),
            },
        ]);
        let intent = capture(&doc, desired.clone()).unwrap();
        assert_eq!(
            intent.filters(),
            &json!([{"key":"done","operator":"is_empty","values":[]},{"key":"status","operator":"contains","values":["é","z"]},{"key":"estimate","operator":"less_than","values":["12.50"]}])
        );
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["views"][0]["filters"] = intent.filters().clone();
        let observed = Document::parse(raw.clone()).unwrap();
        assert_eq!(ViewFilter::current(&observed, "V"), Some(desired));
        assert!(intent.observes_desired(&observed));
        for key in ["columns", "sorts", "opaque"] {
            assert_eq!(observed.views[0][key], doc.views[0][key]);
        }
        raw["list_metadata"]["views"][0]["filters"][1]["values"] = json!(["z", "é"]);
        assert!(!intent.observes_desired(&Document::parse(raw).unwrap()));
        assert_eq!(
            capture(&observed, ViewFilterChoice::None)
                .unwrap()
                .filters(),
            &json!([])
        );
    }
}
#[test]
fn four_source_operators_neutral_membership_and_unknown_loaded_rules_are_closed() {
    let doc = document("single_select");
    for op in [
        SelectFilterOperator::Contains,
        SelectFilterOperator::DoesNotContain,
        SelectFilterOperator::IsEmpty,
        SelectFilterOperator::IsNotEmpty,
    ] {
        let intent = capture(&doc, choice(op, &[])).unwrap();
        assert_eq!(
            intent.filters(),
            &json!([{"key":"status","operator":op.as_str(),"values":[]}])
        );
    }
    for values in [vec!["missing"], vec!["z", "z"], vec!["z|é"]] {
        assert!(capture(&doc, choice(SelectFilterOperator::Contains, &values)).is_err());
    }
    assert!(capture(&doc, choice(SelectFilterOperator::IsEmpty, &["z"])).is_err());
    for rule in [
        json!({"key":"status","operator":"equal","values":["z"]}),
        json!({"key":"status","operator":"contains","values":["gone"]}),
        json!({"key":"status","operator":"contains","values":["z"],"extra":true}),
        json!({"key":"status","operator":"contains","values":null}),
    ] {
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["views"][0]["filters"] = json!([rule]);
        let loaded = Document::parse(raw).unwrap();
        assert_eq!(ViewFilter::current(&loaded, "V"), None);
        assert!(capture(&loaded, ViewFilterChoice::None).is_err());
    }
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["schema"][0]["options"]["format"] = json!("future");
    assert!(
        capture(
            &Document::parse(raw).unwrap(),
            choice(SelectFilterOperator::Contains, &[])
        )
        .is_err()
    );
}

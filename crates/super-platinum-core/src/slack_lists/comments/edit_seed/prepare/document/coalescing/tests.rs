use super::*;
fn vectors() -> Value {
    serde_json::from_str(include_str!("vectors.json")).unwrap()
}

#[test]
fn actual_eachline_and_identity_compose_preserve_attribute_representation() {
    for (index, case) in vectors()["results"].as_array().unwrap().iter().enumerate() {
        let input = case["input"].as_array().unwrap();
        let mut document = Document::capture(input).unwrap();
        let mut lines = vec![];
        for (_, mut line, newline) in document.lines() {
            line.0.push(Unit::new(Insert::Unit(10), newline.clone()));
            for op in line.ops().unwrap() {
                lines.push(json!({"op":op,"line":{"insert":"\n","attributes":newline}}));
            }
        }
        assert_eq!(json!(lines), case["lines"], "line case {index}");
        document.apply(&Change::default()).unwrap();
        assert_eq!(
            json!(document.ops().unwrap()),
            case["composed"],
            "compose case {index}"
        );
    }
}
#[test]
fn later_loose_equal_attributes_win_without_rewriting_original_input() {
    let original = vec![
        json!({"insert":"a","attributes":{"x":true}}),
        json!({"insert":"b","attributes":{"x":1}}),
    ];
    let snapshot = original.clone();
    let document = Document::capture(&original).unwrap();
    assert_eq!(
        document.ops().unwrap(),
        vec![json!({"insert":"ab","attributes":{"x":1}})]
    );
    assert_eq!(original, snapshot);
    let reversed = vec![original[1].clone(), original[0].clone()];
    assert_eq!(
        Document::capture(&reversed).unwrap().ops().unwrap(),
        vec![json!({"insert":"ba","attributes":{"x":true}})]
    );
}
#[test]
fn actual_symbol_change_and_inherited_compose_use_distinct_constructor_rules() {
    for case in vectors()["symbols"].as_array().unwrap() {
        let input = case["input"].as_array().unwrap();
        assert_eq!(
            json!(
                super::super::super::symbols::add_text_presentation_selectors(input, false)
                    .unwrap()
            ),
            case["output"]
        );
    }
    for case in vectors()["inherited"].as_array().unwrap() {
        let mut document = Document::capture(case["input"].as_array().unwrap()).unwrap();
        document.apply(&Change::default()).unwrap();
        assert_eq!(json!(document.ops().unwrap()), case["output"]);
    }
    let original = serde_json::Map::from_iter([
        ("constructor".into(), json!(1)),
        ("ordinary".into(), Value::Null),
    ]);
    let update = serde_json::Map::from_iter([("constructor".into(), json!(2))]);
    assert_eq!(
        compose(&original, &update).unwrap(),
        serde_json::Map::from_iter([
            ("constructor".into(), json!(2)),
            ("ordinary".into(), Value::Null)
        ])
    );
}
#[test]
fn unsupported_coercion_and_prototype_identity_refuse_before_unit_expansion() {
    for attributes in [
        json!({"x":{"toString":null}}),
        json!({"prototype":{}}),
        serde_json::from_str::<Value>("{\"__proto__\":{\"x\":1}}").unwrap(),
    ] {
        assert!(Document::capture(&[json!({"insert":"x","attributes":attributes})]).is_err());
    }
}

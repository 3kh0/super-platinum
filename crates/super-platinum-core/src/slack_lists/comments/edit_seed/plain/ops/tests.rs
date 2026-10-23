use super::*;

#[test]
fn retained_f66b_push_vectors_apply_after_delta_insert() {
    let fixture: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    let cases = fixture["results"].as_array().unwrap();
    assert_eq!(cases.len(), 36);
    for case in cases {
        let original = case["input"].clone();
        let mut delta = Delta::default();
        for op in original.as_array().unwrap() {
            delta.insert(op["insert"].clone(), op["attributes"].clone());
        }
        assert_eq!(
            json!(delta.finish().unwrap()),
            case["pushed"],
            "pair {}:{}",
            case["a"],
            case["b"]
        );
        assert_eq!(case["input"], original);
    }
}

#[test]
fn loose_equal_adjacent_maps_retain_right_attributes_in_both_orders() {
    for (left, right) in [(json!(true), json!(1)), (json!(1), json!(true))] {
        let mut delta = Delta::default();
        delta.insert(json!("a"), json!({"x":left}));
        delta.insert(json!("b"), json!({"x":right}));
        assert_eq!(
            delta.finish().unwrap(),
            vec![json!({"insert":"ab","attributes":{"x":right}})]
        );
    }
}

#[test]
fn insert_omits_null_empty_maps_and_empty_strings_but_preserves_embed_barriers() {
    let mut delta = Delta::default();
    delta.insert(json!(""), json!({"x":1}));
    delta.insert(json!("a"), Value::Null);
    delta.insert(json!("b"), json!({}));
    delta.insert(json!({"slackemoji":{"text":":wave:"}}), Value::Null);
    delta.insert(json!("c"), json!({}));
    assert_eq!(
        delta.finish().unwrap(),
        vec![
            json!({"insert":"ab"}),
            json!({"insert":{"slackemoji":{"text":":wave:"}}}),
            json!({"insert":"c"})
        ]
    );
}

#[test]
fn unsupported_attribute_identity_or_coercion_refuses_even_without_a_merge() {
    for attributes in [
        json!({"x":{"toString":null}}),
        json!({"prototype":{}}),
        json!({"__proto__":{"x":1}}),
    ] {
        let mut delta = Delta::default();
        delta.insert(json!("x"), attributes);
        assert!(delta.finish().is_err());
    }
}

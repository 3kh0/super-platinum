use super::*;

#[test]
fn actual_agze_and_delta_diff_cases_match_supported_json_domain() {
    let vectors: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    for (index, v) in vectors["results"].as_array().unwrap().iter().enumerate() {
        let a = v["left"].as_array().unwrap();
        let b = v["right"].as_array().unwrap();
        if index == 11 {
            assert!(
                equal(a, b).is_err(),
                "object identity requires its own witness"
            );
        } else {
            assert_eq!(
                equal(a, b).unwrap(),
                v["equal"].as_bool().unwrap(),
                "case {index}"
            );
        }
    }
}

#[test]
fn ihc0_raw_or_prepared_shortcut_uses_unfurled_truthiness() {
    let original = vec![json!({"insert":"hello"})];
    let edited = vec![json!({"insert":"changed"})];
    for unfurled in [Value::Null, json!(false), json!(0), json!("")] {
        let compared = EditComparison::capture(&original, &edited, &original, &unfurled).unwrap();
        assert!(compared.unchanged());
        assert!(compared.matches_inputs(&original, &edited, &original, &unfurled));
        assert!(!compared.matches_inputs(&original, &original, &original, &unfurled));
        assert!(
            EditComparison::capture(&original, &original, &edited, &unfurled)
                .unwrap()
                .unchanged()
        );
        assert!(
            !EditComparison::capture(&original, &edited, &edited, &unfurled)
                .unwrap()
                .unchanged()
        );
    }
    for unfurled in [json!([]), json!({}), json!(true), json!(1), json!("files")] {
        assert!(
            !EditComparison::capture(&original, &original, &original, &unfurled)
                .unwrap()
                .unchanged()
        );
    }
}

#[test]
fn unsupported_reference_and_coercion_shapes_do_not_become_noop() {
    let a = vec![json!({"insert":"x","attributes":{"x":{"prototype":{}}}})];
    assert!(EditComparison::capture(&a, &a, &a, &Value::Null).is_err());
    let original = vec![json!({"insert":"x","attributes":{"x":false}})];
    let edited = vec![json!({"insert":"x","attributes":{"x":{"toString":null}}})];
    assert!(EditComparison::capture(&original, &edited, &edited, &Value::Null).is_err());
    assert!(equal(&[json!({"retain":1})], &[]).is_err());
}

#[test]
fn inherited_attributes_and_explicit_boundary_match_actual_source() {
    let vectors: Value = serde_json::from_str(include_str!("boundary-vectors.json")).unwrap();
    for v in vectors["results"].as_array().unwrap() {
        let left = v["left"].as_array().unwrap();
        let right = v["right"].as_array().unwrap();
        if v["supported"] == true {
            assert_eq!(
                equal(left, right).unwrap(),
                v["equal"].as_bool().unwrap(),
                "{}",
                v["name"]
            );
        } else {
            assert!(equal(left, right).is_err(), "{}", v["name"]);
        }
    }
}

#[test]
fn deep_keys_use_descending_javascript_utf16_order() {
    let left = json!({"a":1,"z":{"toString":null}});
    let right = json!({"a":2,"z":false});
    assert!(equality::deep(Some(&left), Some(&right)).is_err());
    let left = json!({"a":{"toString":null},"z":1});
    let right = json!({"a":false,"z":2});
    assert!(!equality::deep(Some(&left), Some(&right)).unwrap());
    // UTF-16 puts E000 after the surrogate pair for U+10000; scalar sort does not.
    let left = json!({"\u{10000}":{"toString":null},"\u{e000}":1});
    let right = json!({"\u{10000}":false,"\u{e000}":2});
    assert!(!equality::deep(Some(&left), Some(&right)).unwrap());
}

#[test]
fn lost_attribute_order_cannot_hide_error_behind_processed_noop() {
    let original = vec![json!({"insert":"x","attributes":{"a":1,"z":{"toString":null}}})];
    let edited = vec![json!({"insert":"x","attributes":{"a":2,"z":false}})];
    assert!(EditComparison::capture(&original, &edited, &original, &Value::Null).is_err());
    let original = vec![json!({"insert":"x","attributes":{"constructor":null}})];
    let edited = vec![json!({"insert":"x"})];
    assert!(
        !EditComparison::capture(&original, &edited, &edited, &Value::Null)
            .unwrap()
            .unchanged()
    );
    assert!(
        EditComparison::capture(&original, &edited, &original, &Value::Null)
            .unwrap()
            .unchanged()
    );
}

#[test]
fn later_embed_throw_cannot_be_masked_by_text_or_processed_fallback() {
    let original = vec![
        json!({"insert":"x"}),
        json!({"insert":{"a":{"toString":null}}}),
    ];
    let edited = vec![json!({"insert":"y"}), json!({"insert":{"a":false}})];
    assert!(EditComparison::capture(&original, &edited, &original, &Value::Null).is_err());
    // This clone is source-equal but deliberately outside the supported domain.
    assert!(EditComparison::capture(&original, &original, &original, &Value::Null).is_err());
}

#[test]
fn inherited_function_string_requires_explicit_observation_not_false_equality() {
    for key in ["constructor", "toString", "valueOf", "hasOwnProperty"] {
        let original = vec![json!({"insert":"x","attributes":{key:"function native() {}"}})];
        let edited = vec![json!({"insert":"x"})];
        assert!(EditComparison::capture(&original, &edited, &original, &Value::Null).is_err());
        // Two positively owned JSON strings use ordinary comparison.
        assert!(
            EditComparison::capture(&original, &original, &edited, &Value::Null)
                .unwrap()
                .unchanged()
        );
    }
}

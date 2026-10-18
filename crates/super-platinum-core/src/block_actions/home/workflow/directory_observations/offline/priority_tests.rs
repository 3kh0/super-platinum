use super::*;
use serde_json::json;

#[test]
fn processed_priority_matches_retained_method_replay() {
    let oracle: Value = serde_json::from_str(include_str!("priority-oracle.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let raw: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let keys: Option<Vec<String>> = serde_json::from_value(case["keys"].clone()).unwrap();
        let expected: Vec<String> = serde_json::from_value(case["expected"].clone()).unwrap();
        let result = Offline::decode_ordered(&raw, keys.as_deref()).unwrap();
        assert_eq!(
            result.processed_top_channels().unwrap(),
            expected,
            "{}",
            case["name"]
        );
    }
}

#[test]
fn unknown_property_order_cannot_silently_sort_tied_channels_by_id() {
    let raw = json!({"ok":true,"channel_priority_90d":{"CZ":2,"CA":2}});
    let unordered = Offline::decode(&raw).unwrap();
    assert_eq!(unordered.priority("CZ"), 2.0);
    assert!(unordered.processed_top_channels().is_err());
    assert!(Offline::decode_ordered(&raw, None).is_err());
    let ordered = Offline::decode_ordered(&raw, Some(&["CZ".into(), "CA".into()])).unwrap();
    assert_eq!(ordered.processed_top_channels().unwrap(), ["CZ", "CA"]);
}

#[test]
fn only_complete_same_object_order_supplies_hint() {
    let raw = json!({"ok":true,"channel_priority_90d":{"CZ":2,"CA":2}});
    for keys in [
        vec![],
        vec!["CZ".into()],
        vec!["CZ".into(), "CZ".into()],
        vec!["CZ".into(), "CC".into()],
    ] {
        assert!(Offline::decode_ordered(&raw, Some(&keys)).is_err());
    }
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"channel_priority_90d":null}),
    ] {
        assert!(Offline::decode_ordered(&raw, Some(&[])).is_err());
    }
    assert!(
        Offline::default()
            .processed_top_channels()
            .unwrap()
            .is_empty()
    );
    assert!(
        Offline::decode(&json!({"ok":true,"channel_priority_90d":{}}))
            .unwrap()
            .processed_top_channels()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn malformed_priority_scores_are_not_cold_observations() {
    for invalid in [json!("2"), json!(true), json!([]), json!({})] {
        let raw = json!({"ok":true,"channel_priority_90d":{"CZ":invalid}});
        assert!(Offline::decode_ordered(&raw, Some(&["CZ".into()])).is_err());
    }
}

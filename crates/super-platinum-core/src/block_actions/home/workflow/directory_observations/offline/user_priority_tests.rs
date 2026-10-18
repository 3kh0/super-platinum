use super::*;
use serde_json::json;
#[test]
fn processed_users_matches_original_priority_method_all_eight_cases() {
    let oracle: Value = serde_json::from_str(include_str!("user-priority-oracle.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let raw: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let keys: Option<Vec<String>> = serde_json::from_value(case["keys"].clone()).unwrap();
        let expected: Vec<String> = serde_json::from_value(case["expected"].clone()).unwrap();
        let decoded = Offline::decode_priorities(&raw, None, keys.as_deref()).unwrap();
        assert_eq!(
            decoded.processed_top_users().unwrap(),
            expected,
            "{}",
            case["name"]
        );
        assert!(decoded.processed_top_channels().unwrap().is_empty());
    }
}
#[test]
fn user_affinity_is_not_top_users_ctr_and_both_orders_remain_independent() {
    let raw = json!({"ok":true,"user_affinity":{"UZ":4,"UA":4,"UM":9},"channel_priority_90d":{"CA":1,"CZ":1},"top_users":{"UOTHER":[2,3]}});
    let channels = vec!["CZ".into(), "CA".into()];
    let users = vec!["UZ".into(), "UA".into(), "UM".into()];
    let decoded = Offline::decode_priorities(&raw, Some(&channels), Some(&users)).unwrap();
    assert_eq!(decoded.processed_top_channels().unwrap(), ["CZ", "CA"]);
    assert_eq!(decoded.processed_top_users().unwrap(), ["UM", "UZ", "UA"]);
    assert_eq!(decoded.affinity("UZ"), 4.0);
    let channel_only = Offline::decode_ordered(&raw, Some(&channels)).unwrap();
    assert_eq!(channel_only.processed_top_channels().unwrap(), ["CZ", "CA"]);
    assert!(channel_only.processed_top_users().is_err());
}
#[test]
fn missing_or_mismatched_user_order_never_creates_sorted_id_hint() {
    let raw = json!({"ok":true,"user_affinity":{"UZ":2,"UA":2}});
    assert!(
        Offline::decode(&raw)
            .unwrap()
            .processed_top_users()
            .is_err()
    );
    assert!(Offline::decode_priorities(&raw, None, None).is_err());
    for keys in [
        vec![],
        vec!["UZ".into()],
        vec!["UZ".into(), "UZ".into()],
        vec!["UZ".into(), "UX".into()],
    ] {
        assert!(Offline::decode_priorities(&raw, None, Some(&keys)).is_err());
    }
    assert!(Offline::decode_priorities(&json!({"ok":true}), None, Some(&[])).is_err());
    assert!(Offline::default().processed_top_users().unwrap().is_empty());
    assert!(
        Offline::decode(&json!({"ok":true,"user_affinity":{}}))
            .unwrap()
            .processed_top_users()
            .unwrap()
            .is_empty()
    );
    for invalid in [json!("2"), json!(true), json!([]), json!({})] {
        assert!(
            Offline::decode_priorities(
                &json!({"ok":true,"user_affinity":{"UZ":invalid}}),
                None,
                Some(&["UZ".into()])
            )
            .is_err()
        );
    }
}

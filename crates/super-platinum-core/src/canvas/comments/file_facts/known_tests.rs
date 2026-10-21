use super::*;
use serde_json::json;

#[test]
fn known_refresh_eligibility_preserves_original_facts_and_skips_source_tombstones() {
    for (extra, expected) in [
        (json!({}), true),
        (json!({"isUnknown":true}), true),
        (json!({"file_access":"access_denied"}), true),
        (json!({"is_salesforce_file":false}), true),
        (json!({"is_salesforce_file":"yes"}), false),
        (json!({"mode":"tombstone"}), false),
        (json!({"isNonExistent":true}), false),
        (json!({"is_deleted":true}), false),
        (json!({"is_tombstoned":true}), false),
    ] {
        let mut raw = json!({"id":"F12345678","name":"Canonical","opaque":{"x":false}});
        raw.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let mut store = Store::default();
        store.observe_batch(&[raw.clone()]).unwrap();
        let row = store.get("F12345678").unwrap();
        assert_eq!(row.refreshable(), expected, "{extra}");
        assert_eq!(row.source(), &raw);
        assert!(row.current(&store));
        assert_eq!(store.revision(), 1);
    }
}

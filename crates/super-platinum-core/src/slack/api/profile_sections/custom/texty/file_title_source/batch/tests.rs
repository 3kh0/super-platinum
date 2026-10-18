use super::*;
use serde_json::json;

fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    }
}
fn file(id: &str) -> Value {
    json!({"id":id,"mode":"quip","filetype":"quip","created":101,"title":"Title"})
}
fn apply(store: &mut Store, rows: Vec<Value>) -> BatchReceipt {
    store
        .ingest_batch(&json!({"ok":true,"files":rows}), &rows, &owner())
        .unwrap()
}
#[test]
fn original_four_oy_three_batch_oracle_matches_supported_projection() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    assert_eq!(oracle["status"], "PASS");
    assert_eq!(oracle["cases"], 14);
    for case in oracle["tests"].as_array().unwrap() {
        let mut store = Store::new();
        for step in case["steps"].as_array().unwrap() {
            let rows = step["rows"].as_array().unwrap().clone();
            let receipt = apply(&mut store, rows);
            let actual: Vec<_> = receipt
                .entries()
                .iter()
                .map(|row| row.effective().cloned().unwrap())
                .collect();
            assert_eq!(json!(actual), step["processed"], "case {}", case["name"]);
            let collection = store.candidates();
            assert!(
                collection.unavailable_ids().is_empty(),
                "case {}",
                case["name"]
            );
            let actual: Vec<_> = collection
                .rows()
                .iter()
                .map(|row| row.effective().clone())
                .collect();
            assert_eq!(json!(actual), step["final"], "case {}", case["name"]);
        }
    }
}
#[test]
fn exact_carrier_and_all_preimages_survive_duplicate_assignment() {
    let mut store = Store::new();
    let mut first = file("F1");
    first["actions"] = json!([{"ts":"20"}]);
    apply(&mut store, vec![first.clone()]);
    let prior = store.candidate("F1").unwrap();
    let rows = vec![
        json!({"id":"F1","actions":[{"ts":"30"}]}),
        json!({"id":"F1","actions":[{"ts":"10"}]}),
    ];
    let carrier = json!({"ok":true,"files":rows,"opaqueCarrier":{"keep":true}});
    let receipt = store.ingest_batch(&carrier, &rows, &owner()).unwrap();
    assert_eq!(receipt.carrier(), &carrier);
    assert_eq!(receipt.entries().len(), 2);
    for row in receipt.entries() {
        assert!(Arc::ptr_eq(&row.projected.receipt, &receipt.carrier));
        assert_eq!(row.preimage(), Some(&first));
        assert_eq!(row.preimage_effective(), Some(prior.effective()));
    }
    assert_eq!(
        receipt.entries()[0].effective().unwrap()["actions"][0]["ts"],
        "30"
    );
    assert_eq!(
        store.candidate("F1").unwrap().effective()["actions"][0]["ts"],
        "20"
    );
    assert!(!prior.current(&store));
}
#[test]
fn embedded_input_has_original_root_position_and_one_level_only() {
    let mut store = Store::new();
    let mut root = file("F1");
    let mut child = file("F2");
    child["canvas_metadata"] = json!({"items":[{"file":file("F3")}]});
    root["canvas_metadata"] = json!({"items":[{"file":child,"opaque":true}]});
    let receipt = apply(&mut store, vec![root.clone()]);
    assert_eq!(receipt.entries().len(), 2);
    assert_eq!(receipt.entries()[1].root_index(), 0);
    assert_eq!(receipt.entries()[1].embedded_index(), Some(0));
    assert_eq!(
        receipt.entries()[1].source(),
        &root["canvas_metadata"]["items"][0]["file"]
    );
    assert!(store.candidate("F3").is_err());
    assert_eq!(
        store.candidate("F1").unwrap().effective()["canvas_metadata"]["items"][0]["file"],
        json!({"id":"F2"})
    );
    assert_eq!(
        store.candidate("F2").unwrap().effective()["canvas_metadata"]["items"][0]["file"],
        json!({"id":"F3"})
    );
    assert!(
        store
            .row("F1")
            .unwrap()
            .effective()
            .get("canvas_metadata")
            .is_none()
    );
}
#[test]
fn malformed_foreign_carrier_or_late_bad_row_is_atomic() {
    let mut store = Store::new();
    apply(&mut store, vec![file("F1")]);
    let before = store.candidates();
    assert!(
        store
            .ingest_batch(
                &json!({"ok":true,"files":[file("F2")]}),
                &[file("F1")],
                &owner()
            )
            .is_err()
    );
    assert!(before.current(&store));
    let rows = vec![file("F2"), json!({"id":"wrong","title":"invalid"})];
    assert!(
        store
            .ingest_batch(&json!({"ok":true,"files":rows}), &rows, &owner())
            .is_err()
    );
    assert!(before.current(&store));
    assert!(store.candidate("F2").is_err());
}
#[test]
fn unsupported_resets_poison_only_that_identity_and_are_not_healed() {
    let mut store = Store::new();
    apply(&mut store, vec![file("F1")]);
    let mut unsupported = file("F1");
    unsupported["restriction_type"] = json!(1);
    apply(&mut store, vec![unsupported.clone(), file("F2")]);
    let snapshot = store.candidates();
    assert_eq!(snapshot.unavailable_ids(), &["F1"]);
    assert_eq!(snapshot.rows()[0].id(), "F2");
    assert_eq!(store.source("F1"), Some(&unsupported));
    apply(&mut store, vec![file("F1")]);
    assert!(store.candidate("F1").is_err());
    assert!(store.row("F1").is_err());
}
#[test]
fn candidate_collection_is_incomplete_and_has_exact_mutation_witness() {
    let mut store = Store::new();
    let empty = store.candidates();
    assert!(empty.rows().is_empty());
    assert!(empty.current(&store));
    apply(&mut store, vec![file("F2"), file("F1")]);
    assert!(!empty.current(&store));
    let snapshot = store.candidates();
    let row = store.candidate("F2").unwrap();
    assert_eq!(
        snapshot.rows().iter().map(|r| r.id()).collect::<Vec<_>>(),
        vec!["F2", "F1"]
    );
    apply(&mut store, vec![json!({"id":"F1","title":"new"})]);
    assert!(!snapshot.current(&store));
    assert!(row.current(&store));
    assert!(!row.current(&Store::new()));
}
#[test]
fn singleton_title_authority_does_not_claim_candidate_or_membership_facts() {
    let mut store = Store::new();
    let mut raw = file("F1");
    raw["user"] = json!("U1");
    raw["shares"] = json!({"public":{"C1":[{"ts":"5"}]}});
    raw["facts"] = json!({"latestAction":"forged"});
    raw["actions"] = json!([{"ts":"3"}]);
    store.ingest(&json!({"ok":true,"file":raw})).unwrap();
    let title = store.row("F1").unwrap();
    let candidate = store.candidate("F1").unwrap();
    assert!(title.effective().get("shares").is_none());
    assert!(title.effective().get("actions").is_none());
    assert_eq!(candidate.effective()["shares"], raw["shares"]);
    assert_eq!(candidate.effective()["actions"], raw["actions"]);
    assert!(candidate.effective().get("facts").is_none());
    assert_eq!(candidate.source(), &raw);
}
#[test]
fn unavailable_string_channel_map_order_is_not_fabricated() {
    let mut store = Store::new();
    let mut raw = file("F1");
    raw["channels"] = json!([]);
    raw["channel_map"] = json!({"z":{"id":"C1","team":"T1"},"a":{"id":"C2","team":"T1"}});
    apply(&mut store, vec![raw.clone()]);
    assert!(store.candidate("F1").is_err());
    assert_eq!(store.source("F1"), Some(&raw));
    assert_eq!(store.candidates().unavailable_ids(), &["F1"]);
}
#[test]
fn action_decimal_prefix_uses_js_whitespace_not_rust_whitespace() {
    let mut store = Store::new();
    let mut raw = file("F1");
    raw["actions"] = json!([{"ts":"10"}]);
    apply(&mut store, vec![raw]);
    apply(
        &mut store,
        vec![json!({"id":"F1","actions":[{"ts":"\u{85}20"}]})],
    );
    assert_eq!(
        store.candidate("F1").unwrap().effective()["actions"][0]["ts"],
        "10"
    );
    apply(
        &mut store,
        vec![json!({"id":"F1","actions":[{"ts":"\u{feff}20suffix"}]})],
    );
    assert_eq!(
        store.candidate("F1").unwrap().effective()["actions"][0]["ts"],
        "\u{feff}20suffix"
    );
}

use super::*;
use crate::{config::WorkspaceSession, slack::api::custom::texty::files::OrderedResponse};
use serde_json::json;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn apply(store: &mut Store, bytes: &[u8]) -> BatchReceipt {
    let response = OrderedResponse::decode(bytes).unwrap();
    store
        .ingest_batch_ordered(
            response.raw(),
            response.rows(),
            &owner(),
            response.channel_map_order(),
        )
        .unwrap()
}
#[test]
fn original_string_numeric_and_duplicate_property_order_drives_channels() {
    let bytes=br#"{"ok":true,"files":[{"id":"F1","mode":"quip","created":1,"channels":["CZ","CTAIL"],"channel_map":{"z":{"id":"old","team":"T1"},"10":{"id":"C10","team":"T1"},"2":{"id":"C2","team":"T1"},"a":{"id":"CA","team":"T1"},"01":{"id":"C01","team":"T1"},"4294967295":{"id":"CLAST","team":"T1"},"z":{"id":"CZ","team":"T1"},"foreign":{"id":"NO","team":"T2"},"org":{"id":"CORG","team":"T2","is_org_shared":true}}}]}"#;
    let mut store = Store::new();
    apply(&mut store, bytes);
    let row = store.candidate("F1").unwrap();
    assert_eq!(
        row.channel_map_keys().unwrap(),
        ["2", "10", "z", "a", "01", "4294967295", "foreign", "org"]
    );
    assert_eq!(
        row.effective()["channels"],
        json!(["C2", "C10", "CZ", "CA", "C01", "CLAST", "CORG", "CTAIL"])
    );
    assert_eq!(row.source()["channel_map"]["z"]["id"], "CZ");
}
#[test]
fn duplicate_file_occurrences_have_distinct_order_and_same_prebatch_rows() {
    let mut store = Store::new();
    apply(
        &mut store,
        br#"{"ok":true,"files":[{"id":"F1","mode":"quip","created":1,"channels":["CPRIOR"]}]}"#,
    );
    let receipt=apply(&mut store,br#"{"ok":true,"files":[{"id":"F1","channels":[],"channel_map":{"z":{"id":"CZ","team":"T1"},"a":{"id":"CA","team":"T1"}}},{"id":"F1","channels":[],"channel_map":{"a":{"id":"CA","team":"T1"},"z":{"id":"CZ","team":"T1"}}}]}"#);
    assert_eq!(
        receipt.entries()[0].effective().unwrap()["channels"],
        json!(["CZ", "CA"])
    );
    assert_eq!(
        receipt.entries()[1].effective().unwrap()["channels"],
        json!(["CA", "CZ"])
    );
    for entry in receipt.entries() {
        assert_eq!(
            entry.preimage_effective().unwrap()["channels"],
            json!(["CPRIOR"])
        );
    }
    assert_eq!(
        store.candidate("F1").unwrap().channel_map_keys().unwrap(),
        ["a", "z"]
    );
}
#[test]
fn duplicate_envelope_metadata_and_embedded_file_properties_keep_last_value_order() {
    let bytes=br#"{"ok":true,"files":[{"id":"DISCARDED"}],"files":[{"id":"FROOT","mode":"quip","created":1,"canvas_metadata":{"items":[]},"canvas_metadata":{"items":[{"file":{"id":"IGNORED"},"file":{"id":"F1","mode":"list","created":1,"channels":[],"channel_map":{"ignored":{"id":"NO","team":"T1"}},"channel_map":{"z":{"id":"CZ","team":"T1"},"a":{"id":"CA","team":"T1"}}}}]}},{"id":"F1","mode":"list","created":1,"channels":[],"channel_map":{"a":{"id":"CA","team":"T1"},"z":{"id":"CZ","team":"T1"}}}]}"#;
    let mut store = Store::new();
    let receipt = apply(&mut store, bytes);
    assert_eq!(receipt.entries().len(), 3);
    assert_eq!(receipt.entries()[2].root_index(), 0);
    assert_eq!(receipt.entries()[2].embedded_index(), Some(0));
    assert_eq!(
        receipt.entries()[1].effective().unwrap()["channels"],
        json!(["CA", "CZ"])
    );
    assert_eq!(
        receipt.entries()[2].effective().unwrap()["channels"],
        json!(["CZ", "CA"])
    );
    assert_eq!(
        store.candidate("F1").unwrap().effective()["channels"],
        json!(["CZ", "CA"])
    );
    assert!(store.source("IGNORED").is_none());
    assert!(store.source("DISCARDED").is_none());
}
#[test]
fn inherited_map_retains_provenance_but_omitted_incoming_map_does_not_drive_channels() {
    let mut store = Store::new();
    apply(&mut store,br#"{"ok":true,"files":[{"id":"F1","mode":"quip","created":1,"channels":[],"channel_map":{"z":{"id":"CZ","team":"T1"},"a":{"id":"CA","team":"T1"}}}]}"#);
    apply(
        &mut store,
        br#"{"ok":true,"files":[{"id":"F1","title":"Partial"}]}"#,
    );
    let retained = store.candidate("F1").unwrap();
    assert_eq!(retained.channel_map_keys().unwrap(), ["z", "a"]);
    assert_eq!(retained.effective()["channels"], json!(["CZ", "CA"]));
    apply(
        &mut store,
        br#"{"ok":true,"files":[{"id":"F1","channels":["CONLY"]}]}"#,
    );
    let changed = store.candidate("F1").unwrap();
    assert_eq!(changed.channel_map_keys().unwrap(), ["z", "a"]);
    assert_eq!(changed.effective()["channels"], json!(["CONLY"]));
    apply(
        &mut store,
        br#"{"ok":true,"files":[{"id":"F1","channel_map":null}]}"#,
    );
    assert!(store.candidate("F1").unwrap().channel_map_keys().is_none());
}
#[test]
fn unordered_map_cannot_borrow_old_equal_value_order_and_foreign_carrier_refuses() {
    let response=OrderedResponse::decode(br#"{"ok":true,"files":[{"id":"F1","mode":"quip","created":1,"channels":[],"channel_map":{"z":{"id":"CZ","team":"T1"},"a":{"id":"CA","team":"T1"}}}]}"#).unwrap();
    let mut store = Store::new();
    store
        .ingest_batch_ordered(
            response.raw(),
            response.rows(),
            &owner(),
            response.channel_map_order(),
        )
        .unwrap();
    let old = store.candidate("F1").unwrap();
    let mut foreign = response.raw().clone();
    foreign["files"][0]["title"] = json!("Different");
    assert!(
        store
            .ingest_batch_ordered(
                &foreign,
                foreign["files"].as_array().unwrap(),
                &owner(),
                response.channel_map_order()
            )
            .is_err()
    );
    assert!(old.current(&store));
    store
        .ingest_batch(response.raw(), response.rows(), &owner())
        .unwrap();
    assert!(store.candidate("F1").is_err());
    assert_eq!(store.candidates().unavailable_ids(), ["F1"]);
}
#[test]
fn same_prebatch_omitted_map_inherits_original_order_not_first_duplicate() {
    let mut store = Store::new();
    apply(&mut store,br#"{"ok":true,"files":[{"id":"F1","mode":"quip","created":1,"channels":[],"channel_map":{"z":{"id":"CZ","team":"T1"},"a":{"id":"CA","team":"T1"}}}]}"#);
    apply(&mut store,br#"{"ok":true,"files":[{"id":"F1","channels":[],"channel_map":{"a":{"id":"CA","team":"T1"},"z":{"id":"CZ","team":"T1"}}},{"id":"F1","title":"Last"}]}"#);
    let row = store.candidate("F1").unwrap();
    assert_eq!(row.channel_map_keys().unwrap(), ["z", "a"]);
    assert_eq!(row.effective()["channels"], json!(["CZ", "CA"]));
}
#[test]
fn decoder_refuses_invalid_utf8_trailing_json_and_excessive_bytes_nodes_depth() {
    for bytes in [
        b"{\"ok\":true,\"files\":[]} {}".as_slice(),
        b"{\"ok\":true,\"files\":[{\"id\":\"F\xff\"}]}".as_slice(),
        br#"{"ok":false,"files":[]}"#,
        br#"{"ok":true,"files":{}}"#,
    ] {
        assert!(OrderedResponse::decode(bytes).is_err());
    }
    assert!(OrderedResponse::decode(&vec![b' '; MAX_TOTAL_BYTES + 1]).is_err());
    let nodes = json!({"ok":true,"files":[],"extra":vec![Value::Null;MAX_NODES]});
    assert!(OrderedResponse::decode(nodes.to_string().as_bytes()).is_err());
    let mut depth = Value::Null;
    for _ in 0..MAX_DEPTH + 1 {
        depth = json!([depth]);
    }
    let raw = json!({"ok":true,"files":[],"extra":depth});
    assert!(OrderedResponse::decode(raw.to_string().as_bytes()).is_err());
    assert!(OrderedResponse::decode(br#"{"ok":true,"files":[],"empty":""}"#).is_ok());
}

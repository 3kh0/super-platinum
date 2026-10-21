use super::*;
use serde_json::json;

const SHARED: &[u8] = br#"{"ok":true,"file":{"id":"F1","mode":"quip","filetype":"quip","created":101,"title":"Canonical","is_deleted":true,"channels":["CZ","CTAIL"],"channel_map":{"z":{"id":"CZ","team":"T1"},"10":{"id":"C10","team":"T1"},"2":{"id":"C2","team":"T1"},"a":{"id":"CA","team":"T1"},"foreign":{"id":"CFOREIGN","team":"T2"},"org":{"id":"CORG","team":"T2","is_org_shared":true}},"opaque":{"keep":1}},"comments":{"original":true}}"#;

fn apply(store: &mut Store, bytes: &[u8]) -> Result<(), Error> {
    store.ingest_info("T1", &OrderedFileInfo::decode(bytes)?)
}

#[test]
fn genuine_singleton_preserves_order_team_filter_and_positive_info_mapping() {
    let received = OrderedFileInfo::decode(SHARED).unwrap();
    let mut store = Store::new();
    store.ingest_info("T1", &received).unwrap();
    let row = store.candidate("F1").unwrap();
    assert_eq!(
        row.channel_map_keys().unwrap(),
        ["2", "10", "z", "a", "foreign", "org"]
    );
    assert_eq!(
        row.effective()["channels"],
        json!(["C2", "C10", "CZ", "CA", "CORG", "CTAIL"])
    );
    assert_eq!(row.effective()["is_deleted"], false);
    assert_eq!(row.source(), received.file());
    assert_eq!(row.source()["is_deleted"], true);
    assert_eq!(row.receipt(), received.raw());
    assert!(row.receipt().get("files").is_none());
    assert_eq!(store.row("F1").unwrap().receipt(), received.raw());
    let mut other = Store::new();
    other.ingest_info("T2", &received).unwrap();
    assert_eq!(
        other.candidate("F1").unwrap().effective()["channels"],
        json!(["CFOREIGN", "CORG", "CZ", "CTAIL"])
    );
}

#[test]
fn partial_receipts_keep_predecessor_title_blocks_and_map_provenance() {
    let mut store = Store::new();
    apply(&mut store, SHARED).unwrap();
    apply(
        &mut store,
        br#"{"ok":true,"file":{"id":"F1","title_blocks":[],"title":"Renamed"}}"#,
    )
    .unwrap();
    let before = store.candidate("F1").unwrap();
    assert_eq!(
        before.effective()["channels"],
        json!(["C2", "C10", "CZ", "CA", "CORG", "CTAIL"])
    );
    assert_eq!(
        before.channel_map_keys().unwrap(),
        ["2", "10", "z", "a", "foreign", "org"]
    );
    apply(
        &mut store,
        br#"{"ok":true,"file":{"id":"F1","title":"Again","channels":["CONLY"]}}"#,
    )
    .unwrap();
    assert!(!before.current(&store));
    assert_eq!(
        store.row("F1").unwrap().effective()["title_blocks"],
        json!([])
    );
    assert_eq!(
        store.candidate("F1").unwrap().effective()["channels"],
        json!(["CONLY"])
    );
    apply(
        &mut store,
        br#"{"ok":true,"file":{"id":"F1","channel_map":null}}"#,
    )
    .unwrap();
    assert!(store.candidate("F1").unwrap().channel_map_keys().is_none());
}

#[test]
fn valid_unsupported_singleton_poisons_old_title_without_losing_original_receipt() {
    let mut store = Store::new();
    apply(&mut store, SHARED).unwrap();
    let old = store.row("F1").unwrap();
    let hidden = OrderedFileInfo::decode(
        br#"{"ok":true,"file":{"id":"F1","is_hidden_by_limit":true,"opaque":"retained"}}"#,
    )
    .unwrap();
    assert!(store.ingest_info("T1", &hidden).is_err());
    assert!(!old.current(&store));
    assert_eq!(store.source("F1"), Some(hidden.file()));
    assert!(store.candidate("F1").is_err());
    assert!(apply(&mut store, SHARED).is_err());
    assert!(store.row("F1").is_err());
}

#[test]
fn singleton_decoder_refuses_non_envelopes_errors_and_missing_identity() {
    for bytes in [
        b"null".as_slice(),
        br#"{"id":"F1"}"#,
        br#"{"ok":true,"files":[]}"#,
        br#"{"ok":false,"error":"not_found","file":{"id":"F1"}}"#,
        br#"{"ok":true,"error":"bad","file":{"id":"F1"}}"#,
        br#"{"ok":true,"file":{"id":"bad"}}"#,
        br#"{"ok":true,"file":[]}"#,
    ] {
        assert!(OrderedFileInfo::decode(bytes).is_err());
    }
    let mut store = Store::new();
    let received = OrderedFileInfo::decode(SHARED).unwrap();
    assert!(store.ingest_info("", &received).is_err());
    assert!(store.source("F1").is_none());
    // Legacy Value intake remains unchanged and cannot claim string-key order.
    store.ingest(received.raw()).unwrap();
    assert!(store.candidate("F1").is_err());
    assert!(store.row("F1").is_ok());
}

#[test]
fn duplicate_envelope_file_and_map_properties_use_last_value_original_order() {
    let received = OrderedFileInfo::decode(br#"{"ok":true,"file":{"id":"FOLD"},"file":{"id":"F1","mode":"list","created":1,"channels":[],"channel_map":{"ignored":{}},"channel_map":{"z":{"id":"old","team":"T1"},"a":{"id":"CA","team":"T1"},"z":{"id":"CZ","team":"T1"}}}}"#).unwrap();
    let mut store = Store::new();
    store.ingest_info("T1", &received).unwrap();
    assert!(store.source("FOLD").is_none());
    assert_eq!(
        store.candidate("F1").unwrap().channel_map_keys().unwrap(),
        ["z", "a"]
    );
    assert_eq!(
        store.candidate("F1").unwrap().effective()["channels"],
        json!(["CZ", "CA"])
    );
}

use super::*;
use serde_json::json;

fn file(id: &str) -> Value {
    json!({"id":id,"mode":"quip","filetype":"quip","created":101,"title":"First"})
}

#[test]
fn genuine_empty_cache_accepts_singleton_raw_or_positive_info_receipt() {
    let mut store = Store::new();
    assert!(store.row("F1").is_err());
    let raw = file("F1");
    store.ingest(&json!({"ok":true,"file":raw})).unwrap();
    let row = store.row("F1").unwrap();
    assert_eq!(row.source(), &raw);
    assert_eq!(row.effective()["title"], "First");
    assert_eq!(row.effective()["is_tombstoned"], false);
    assert_eq!(row.effective()["isUnknown"], false);
    assert_eq!(row.effective()["isNonExistent"], false);
    assert!(row.current(&store));
    let mut list = file("F2");
    list["mode"] = json!("list");
    list["filetype"] = json!("list");
    store.ingest(&list).unwrap();
    assert!(row.current(&store));
    assert_eq!(store.row("F2").unwrap().source(), &list);
}

#[test]
fn positive_info_mapping_clears_deleted_without_rewriting_actual_receipt() {
    let mut raw = file("F1");
    raw["is_deleted"] = json!(true);
    let receipt = json!({"ok":true,"file":raw,"comments":[{"unknown":true}],"content":"outer"});
    let mut store = Store::new();
    store.ingest(&receipt).unwrap();
    let row = store.row("F1").unwrap();
    assert_eq!(row.source(), &raw);
    assert_eq!(row.receipt(), &receipt);
    assert_eq!(row.effective()["is_deleted"], false);
    assert!(row.effective().get("content").is_none());
    // The raw-input API represents an already processed upsert input.
    let mut other = Store::new();
    assert!(other.ingest(&raw).is_err());
    assert!(other.row("F1").is_err());
}

#[test]
fn ordinary_omitted_blocks_inherit_and_only_explicit_empty_clears() {
    let mut store = Store::new();
    let mut raw = file("F1");
    raw["title_blocks"] = json!([{"type":"rich_text","opaque":true}]);
    raw["name"] = json!("original name");
    store.ingest(&raw).unwrap();
    let original = store.row("F1").unwrap();
    let next = json!({"id":"F1","title":"Plain new title"});
    store.ingest(&next).unwrap();
    let retained = store.row("F1").unwrap();
    assert!(!original.current(&store));
    assert_eq!(retained.source(), &next);
    assert_eq!(retained.effective()["title_blocks"], raw["title_blocks"]);
    assert_eq!(retained.effective()["name"], "original name");
    assert_eq!(retained.effective()["mode"], "quip");
    store.ingest(&json!({"id":"F1","title_blocks":[]})).unwrap();
    assert_eq!(
        store.row("F1").unwrap().effective()["title_blocks"],
        json!([])
    );
    store
        .ingest(&json!({"id":"F1","title_blocks":null,"title":null}))
        .unwrap();
    let cleared = store.row("F1").unwrap();
    assert!(cleared.effective()["title_blocks"].is_null());
    assert!(cleared.effective()["title"].is_null());
}

#[test]
fn incoming_lock_and_permalink_replace_or_remove_instead_of_inherit() {
    let mut store = Store::new();
    let mut raw = file("F1");
    raw["is_locked"] = json!(true);
    raw["permalink"] = json!("https://fixture.invalid/original");
    store.ingest(&raw).unwrap();
    assert_eq!(store.row("F1").unwrap().effective()["is_locked"], true);
    store.ingest(&json!({"id":"F1","title":"New"})).unwrap();
    let row = store.row("F1").unwrap();
    assert!(row.effective().get("is_locked").is_none());
    assert!(row.effective().get("permalink").is_none());
    store
        .ingest(&json!({"id":"F1","is_locked":null,"permalink":null}))
        .unwrap();
    let row = store.row("F1").unwrap();
    assert_eq!(row.effective().get("is_locked"), Some(&Value::Null));
    assert_eq!(row.effective().get("permalink"), Some(&Value::Null));
}

#[test]
fn timestamp_not_envelope_success_controls_known_flag_reset() {
    for timestamp in [
        json!(101),
        json!("101"),
        json!(false),
        json!(0),
        Value::Null,
    ] {
        let mut store = Store::new();
        let raw = json!({"id":"F1","filetype":"quip","isUnknown":true,"created":timestamp});
        let accepted = store.ingest(&json!({"ok":true,"file":raw})).is_ok();
        assert_eq!(accepted, truth(&timestamp));
        if accepted {
            assert_eq!(store.row("F1").unwrap().effective()["isUnknown"], false);
        }
    }
    let mut store = Store::new();
    store
        .ingest(&json!({"id":"F1","mode":"list","title":"Known without timestamp"}))
        .unwrap();
    let row = store.row("F1").unwrap();
    assert!(row.effective().get("isUnknown").is_none());
    assert!(row.effective().get("isNonExistent").is_none());
}

#[test]
fn malformed_and_error_receipts_leave_good_cache_and_receipt_unchanged() {
    let mut store = Store::new();
    store.ingest(&file("F1")).unwrap();
    let old = store.row("F1").unwrap();
    for raw in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"id":""}),
        json!({"id":1}),
        json!({"id":"U1","mode":"quip"}),
        json!({"ok":false,"error":"offline"}),
        json!({"ok":true}),
        json!({"ok":true,"file":[]}),
        json!({"ok":true,"error":"contradictory","file":file("F1")}),
        json!({"file":file("F1")}),
    ] {
        assert!(store.ingest(&raw).is_err());
        assert!(old.current(&store), "{raw}");
        assert_eq!(store.source("F1"), Some(old.source()));
    }
}

#[test]
fn unsupported_positive_poison_is_per_id_and_cannot_guess_history_recovery() {
    for update in [
        json!({"canvas_template_mode":"published"}),
        json!({"list_template_mode":"draft"}),
        json!({"restriction_type":1}),
        json!({"is_tombstoned":true}),
        json!({"mode":"tombstone"}),
        json!({"mode":"not_found"}),
        json!({"mode":"template_not_visible"}),
        json!({"file_access":"access_denied"}),
        json!({"isNonExistent":true}),
        json!({"is_deleted":true}),
        json!({"is_hidden_by_limit":true}),
        json!({"is_external":true}),
        json!({"is_skill":true}),
        json!({"is_huddle_canvas":true}),
        json!({"canvas_metadata":{"items":[{"file":{"id":"F2"}}]}}),
        json!({"title":{"unported":true}}),
        json!({"title_blocks":false}),
        json!({"isMigratedFromLegacyPost":true,"docsId":"F2"}),
        json!({"mode":"list","filetype":"list","subtype":"poll"}),
    ] {
        let mut store = Store::new();
        store.ingest(&file("F1")).unwrap();
        store.ingest(&file("F2")).unwrap();
        let old = store.row("F1").unwrap();
        let other = store.row("F2").unwrap();
        let mut raw = update;
        raw["id"] = json!("F1");
        assert!(store.ingest(&raw).is_err(), "{raw}");
        assert!(!old.current(&store));
        assert!(other.current(&store));
        assert!(store.row("F1").is_err());
        assert_eq!(store.source("F1"), Some(&raw));
        assert!(store.ingest(&file("F1")).is_err());
        assert!(store.row("F1").is_err());
    }
}

#[test]
fn observations_require_original_store_and_original_row_receipt() {
    let mut one = Store::new();
    let mut two = Store::new();
    let raw = file("F1");
    one.ingest(&raw).unwrap();
    two.ingest(&raw).unwrap();
    let row = one.row("F1").unwrap();
    assert!(!row.current(&two));
    assert!(row.current(&one.clone()));
    one.ingest(&raw).unwrap();
    assert!(!row.current(&one));
}

#[test]
fn opaque_raw_is_retained_without_claiming_other_file_projection() {
    let mut store = Store::new();
    let mut raw = file("F1");
    raw["shares"] = json!({"opaque":{"unknown":[1,2]}});
    raw["actions"] = json!([{"ts":"1","future":true}]);
    raw["future"] = json!({"preserved":true});
    store.ingest(&raw).unwrap();
    let row = store.row("F1").unwrap();
    assert_eq!(row.source(), &raw);
    for key in ["shares", "actions", "future", "localUpdated"] {
        assert!(row.effective().get(key).is_none());
    }
}

#[test]
fn bounds_fail_without_discarding_existing_receipts() {
    let mut store = Store::new();
    store.ingest(&file("F1")).unwrap();
    let old = store.row("F1").unwrap();
    let mut large = file("F1");
    large["title"] = json!("x".repeat(MAX_ROW_BYTES));
    assert!(store.ingest(&large).is_err());
    assert!(old.current(&store));
    let mut deep = Value::Null;
    for _ in 0..MAX_DEPTH + 1 {
        deep = json!([deep]);
    }
    let mut raw = file("F1");
    raw["future"] = deep;
    assert!(store.ingest(&raw).is_err());
    assert!(old.current(&store));
    for id in 2..=MAX_ROWS {
        store.ingest(&file(&format!("F{id}"))).unwrap();
    }
    assert!(store.ingest(&file("F999")).is_err());
    assert!(old.current(&store));
}

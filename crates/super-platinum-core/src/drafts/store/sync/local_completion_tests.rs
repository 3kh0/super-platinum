use super::tests::{Database, scope};
use super::*;
use crate::drafts::{codec, sync::ConsumeKind};

fn completed(store: &mut DraftStore) -> (Stored, crate::drafts::sync::ConsumeIntent) {
    let scope = scope();
    let mut state = DraftSync::new(scope.clone());
    state.edit(&scope, Some(codec::encode(&scope.key, "Original").unwrap()));
    let intent = state
        .request_consume(&scope, state.local_revision(), ConsumeKind::Send)
        .unwrap();
    let op = state.begin_operation(&scope).unwrap();
    assert!(state.confirm_sent(&scope, &op.id, &op.client_msg_id, None, "200.000001"));
    let Commit::Applied(stored) = store
        .commit_synced(&scope, None, &LocalDraft::default(), &state, None)
        .unwrap()
    else {
        panic!()
    };
    (stored, intent)
}

fn replace_raw(store: &DraftStore, raw: &serde_json::Value) {
    store
        .0
        .execute(
            "UPDATE composer_draft_sync SET state=?1",
            [serde_json::to_string(raw).unwrap()],
        )
        .unwrap();
}

#[test]
fn legacy_v5_completed_row_without_marker_loads_without_post_ack_authority() {
    let db = Database::new();
    let mut store = db.open();
    let (row, intent) = completed(&mut store);
    let mut raw = serde_json::to_value(&row.state).unwrap();
    raw.as_object_mut().unwrap().remove("local_completion");
    replace_raw(&store, &raw);
    drop(store);
    let loaded = db
        .open()
        .load_synced(&scope().account_id, LoadMode::AfterRestart)
        .unwrap();
    let restored = &loaded.synced[&scope().key];
    assert_eq!(restored.revision, row.revision);
    assert!(restored.state.local_preservation_intent().is_none());
    assert!(!restored.state.accepts_direct_local(&intent));
}

#[test]
fn v5_positive_completion_roundtrip_keeps_successor_and_exact_local_capability() {
    let db = Database::new();
    let mut store = db.open();
    let (row, intent) = completed(&mut store);
    drop(store);
    let loaded = db
        .open()
        .load_synced(&scope().account_id, LoadMode::AfterRestart)
        .unwrap();
    let restored = &loaded.synced[&scope().key];
    assert_eq!(restored.state, row.state);
    assert_eq!(restored.state.local_preservation_intent(), Some(&intent));
    assert_ne!(restored.state.client_msg_id(), intent.client_msg_id);
}

#[test]
fn malformed_completion_is_rejected_without_changing_paired_local_row() {
    let db = Database::new();
    let mut store = db.open();
    let (row, intent) = completed(&mut store);
    let mut raw = serde_json::to_value(&row.state).unwrap();
    raw["local_completion"]["successor_client_msg_id"] = serde_json::json!(intent.client_msg_id);
    replace_raw(&store, &raw);
    let before = store.load().unwrap();
    assert!(
        store
            .load_synced(&scope().account_id, LoadMode::Current)
            .is_err()
    );
    let after = store.load().unwrap();
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(after).unwrap()
    );
    let retained: String = store
        .0
        .query_row("SELECT state FROM composer_draft_sync", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&retained).unwrap(),
        raw
    );
}

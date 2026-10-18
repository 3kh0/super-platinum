//! Actual isolated Store/Source custody; no websocket or renderer acceptance claim.
use super::super::super::source::inventory::tests::Fixture;
use super::*;
use serde_json::json;

#[tokio::test]
async fn ingress_reuses_exact_source_and_retires_only_closed_empty_queue() {
    let f = Fixture::new().await;
    let queue = f.store.account.open_ingress(f.source.clone()).unwrap();
    assert!(Arc::ptr_eq(
        &queue,
        &f.store.account.open_ingress(f.source.clone()).unwrap()
    ));
    let inventory = f.store.account.ownership_inventory().unwrap();
    assert_eq!(
        f.store.account.retire_ingress(&queue),
        Err(durable::Error::Pending)
    );
    inventory.validate(&f.store.account).unwrap();
    f.source.gap(durable::GapReason::Disconnect);
    f.store.account.retire_ingress(&queue).unwrap();
    assert!(inventory.validate(&f.store.account).is_err());
    assert!(f.store.account.ingress.lock().unwrap().is_empty());
    assert!(f.store.account.retire_ingress(&queue).is_err());
}

#[tokio::test]
async fn raw_frame_and_failed_queue_are_not_evicted_after_disconnect() {
    let f = Fixture::new().await;
    let queue = f.store.account.open_ingress(f.source.clone()).unwrap();
    let raw = Arc::new(
        serde_json::from_value::<RawEvent>(
            json!({"type":"draft_delete", "draft_id":"D1", "event_ts":"20.000001"}),
        )
        .unwrap(),
    );
    {
        let mut state = queue.state.lock().unwrap();
        queue.source.inventory.clock.change();
        state.raw = Some(raw.clone());
    }
    let snapshot = queue.inventory().unwrap();
    assert!(snapshot.has_raw_frame());
    f.source.gap(durable::GapReason::Disconnect);
    f.store.account.retire_closed_ingress();
    assert_eq!(f.store.account.ingress.lock().unwrap().len(), 1);
    assert!(Arc::ptr_eq(
        queue.state.lock().unwrap().raw.as_ref().unwrap(),
        &raw
    ));
    {
        let mut state = queue.state.lock().unwrap();
        queue.source.inventory.clock.change();
        state.raw = None;
        state.failed = true;
    }
    assert_eq!(
        f.store.account.retire_ingress(&queue),
        Err(durable::Error::Pending)
    );
}

#[tokio::test]
async fn raw_scope_lookup_uses_actual_managed_identity_and_never_visible_selection() {
    let f = Fixture::new().await;
    let loaded = f.store.account.inventory().await.unwrap();
    let review = loaded.managed().get(&f.store.owner.scope().key).unwrap();
    let raw: RawEvent =
        serde_json::from_value(json!({"type":"draft_update", "draft_id":"Dfixture",
        "client_msg_id":review.row().state().client_msg_id(), "event_ts":"20.000001"}))
        .unwrap();
    assert_eq!(
        key_for(&loaded, "T1", &raw).unwrap(),
        Some(f.store.owner.scope().key.clone())
    );
    assert_eq!(key_for(&loaded, "TOTHER", &raw).unwrap(), None);
    let unrelated: RawEvent = serde_json::from_value(
        json!({"type":"draft_delete", "draft_id":"DUNRELATED", "event_ts":"20.000001"}),
    )
    .unwrap();
    assert_eq!(key_for(&loaded, "T1", &unrelated).unwrap(), None);
    let malformed: RawEvent =
        serde_json::from_value(json!({"type":"draft_delete", "draft_id":7})).unwrap();
    assert!(key_for(&loaded, "T1", &malformed).is_err());
    // Key selection alone creates no Prepared or reservation; full actual
    // Source decoding still rejects the incomplete update above.
    assert!(
        f.source
            .prepare(&f.shell, &f.store.owner.scope().key, &raw)
            .is_err()
    );
}

//! Actual isolated owned jobs; final HTTP closure remains synthetic, never live.
use super::*;

#[tokio::test]
async fn actual_unsupported_route_boundary_invalidates_component_without_legacy_submission() {
    let f = Box::pin(Fixture::new(false)).await;
    let before = f.account.ownership_inventory().unwrap();
    let jobs = f.ops().retained().len();
    assert!(matches!(
        f.account.client(),
        Err(durable::Error::Unsupported)
    ));
    assert_eq!(before.validate(&f.account), Err(ownership::Error::Changed));
    assert_eq!(f.ops().retained().len(), jobs);
    // A new observation may retain the unsupported state; it is not clearance.
    f.account
        .ownership_inventory()
        .unwrap()
        .validate(&f.account)
        .unwrap();
}

#[tokio::test]
async fn component_tracks_actual_owned_checkpoint_actor_and_original_holder() {
    let f = Box::pin(Fixture::new(false)).await;
    let before = f.account.ownership_inventory().unwrap();
    let original = f.editor.owned_write.lock().unwrap().clone().unwrap();
    f.editor.submit(&local(NEWER)).unwrap();
    f.editor.flush().await.unwrap();
    assert_eq!(before.validate(&f.account), Err(ownership::Error::Changed));
    let current = f.editor.owned_write.lock().unwrap().clone().unwrap();
    assert!(!Arc::ptr_eq(&original, &current));
    assert!(same_local(&original.original.local, &local(ORIGINAL)));
    assert!(same_local(&current.original.local, &local(NEWER)));
    assert_eq!(current.checkpoint_state(), Some(o::JobState::Ready));
    f.account
        .ownership_inventory()
        .unwrap()
        .validate(&f.account)
        .unwrap();
}

#[tokio::test]
async fn nested_send_holder_is_stamped_by_real_prepare_claim_and_adapter_attachment() {
    let f = Box::pin(Fixture::new(false)).await;
    let (parent, _, held) = Box::pin(f.consume()).await;
    let before = f.account.ownership_inventory().unwrap();
    let stamp = held.ownership_stamp().unwrap();
    let binding = crate::drafts::network::Binding::retained_route_test(f.account.id());
    let captured = parent.clone();
    held.dispatch_with(&parent, &binding, move |_| {
        let receipt = captured
            .record()
            .evidence
            .lock()
            .unwrap()
            .receipt
            .clone()
            .unwrap();
        std::future::ready(Ok(json!({"ok":true,"channel":"C1","ts":"200.000001",
            "message":{"channel":"C1","ts":"200.000001","user":"U1",
            "client_msg_id":receipt.operation().client_msg_id}})))
    })
    .await
    .unwrap();
    assert_eq!(
        held.validate_ownership(&stamp),
        Err(ownership::Error::Changed)
    );
    assert_eq!(before.validate(&f.account), Err(ownership::Error::Changed));
    assert!(Arc::ptr_eq(
        &held,
        parent
            .record()
            .evidence
            .lock()
            .unwrap()
            .owned_send
            .as_ref()
            .unwrap()
    ));
    assert!(parent.record().evidence.lock().unwrap().recorded.is_some());
}

#[tokio::test]
async fn direct_local_fifo_input_invalidates_component_even_without_editor_mutation() {
    let (f, parent, result, intent) = Box::pin(sent()).await;
    f.editor
        .release_completion_owned(result.review, intent, parent)
        .await
        .unwrap();
    let writer = f.editor.state.lock().unwrap().released.clone().unwrap();
    let serial = f.editor.snapshot().serial;
    let before = f.account.ownership_inventory().unwrap();
    writer
        .submit(&local("Input directly owned by existing local FIFO"))
        .unwrap();
    writer.flush().await.unwrap();
    assert_eq!(f.editor.snapshot().serial, serial);
    assert_eq!(before.validate(&f.account), Err(ownership::Error::Changed));
    assert!(Arc::ptr_eq(
        &writer,
        f.editor.state.lock().unwrap().released.as_ref().unwrap()
    ));
    assert_eq!(
        f.owner
            .local_inventory()
            .await
            .unwrap()
            .local()
            .composer
            .text,
        "Input directly owned by existing local FIFO"
    );
}

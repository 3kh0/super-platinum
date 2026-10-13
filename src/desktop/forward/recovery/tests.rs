use super::*;
use super_platinum_core::forwarding::Status;
fn storage() -> Storage {
    Storage {
        account: "00000000-0000-4000-8000-000000000071".into(),
        team: "T1".into(),
        user: "U1".into(),
        fixture_path: Some(std::env::temp_dir().join(format!(
                "forward-lifecycle-{}.sqlite",
                api::ForwardMessage::new("C1".into(), "100.000001".into(), "C2".into(), vec![])
                    .client_msg_id
            ))),
    }
}
async fn pending(storage: &Storage) -> Receipt {
    let request = api::ForwardMessage::new(
        "C1".into(),
        "100.000001".into(),
        "C2".into(),
        vec![
            serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original 日本語 🦀"}]}]}),
        ],
    );
    match storage.begin(request, "destination".into()).await.unwrap() {
        Begin::Started(r) => r,
        _ => panic!("unexpected blocked begin"),
    }
}
fn cleanup(storage: &Storage) {
    std::fs::remove_file(storage.fixture_path.as_ref().unwrap()).unwrap();
}

#[tokio::test]
async fn outcome_uses_captured_account_after_view_disappears() {
    let original = storage();
    let receipt = pending(&original).await;
    let replacement = storage();
    let updated = original
        .finish(
            receipt,
            Ok(api::ForwardedMessage {
                channel: "C2".into(),
                ts: "101.000001".into(),
            }),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(updated.delivery.status, Status::Sent(_)));
    assert!(replacement.unresolved().await.unwrap().is_empty());
    assert!(original.unresolved().await.unwrap().is_empty());
    cleanup(&original);
    cleanup(&replacement);
}

#[tokio::test]
async fn reopened_record_keeps_payload_and_requires_explicit_resend() {
    let original = storage();
    let receipt = pending(&original).await;
    let reopened = original.clone();
    let rows = reopened.unresolved().await.unwrap();
    assert_eq!(rows, vec![receipt.clone()]);
    let newer = match reopened.resend(receipt.clone()).await.unwrap().unwrap() {
        Begin::Started(r) => r,
        _ => panic!("blocked"),
    };
    assert_ne!(newer.attempt_id, receipt.attempt_id);
    assert_eq!(newer.delivery.request, receipt.delivery.request);
    let stale = original
        .finish(
            receipt,
            Ok(api::ForwardedMessage {
                channel: "C2".into(),
                ts: "101.000001".into(),
            }),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stale.attempt_id, newer.attempt_id);
    assert!(stale.unresolved());
    cleanup(&original);
}

#[tokio::test]
async fn second_view_observes_durable_check_delay_and_retirement() {
    let store = storage();
    let receipt = pending(&store).await;
    let delayed = store.defer(receipt.clone(), 60).await.unwrap().unwrap();
    let refreshed = store.refresh(&receipt).await.unwrap().unwrap();
    assert_eq!(refreshed, delayed);
    assert!(refreshed.delivery.retry_at() > delivery::now());
    let stopped = store.retire(receipt).await.unwrap().unwrap();
    assert!(stopped.retired);
    assert_eq!(stopped.delivery.retry_at(), delayed.delivery.retry_at());
    assert!(store.unresolved().await.unwrap().is_empty());
    cleanup(&store);
}

#[tokio::test]
async fn unwritable_journal_never_returns_dispatch_permission() {
    let mut store = storage();
    store.fixture_path = Some(std::env::temp_dir());
    let result = store
        .begin(
            api::ForwardMessage::new("C1".into(), "100.000001".into(), "C2".into(), vec![]),
            "destination".into(),
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn mixed_note_load_stays_blocked_after_the_first_note_is_resolved() {
    let store = storage();
    let first = pending(&store).await;
    let mut second_request = first.delivery.request.clone();
    second_request.destination = "C3".into();
    second_request.client_msg_id = "00000000-0000-4000-8000-000000000081".into();
    // Equal visible text can still be a different authoritative note.
    second_request.blocks[0]["elements"][0]["elements"][0]["style"] =
        serde_json::json!({"bold": true});
    let second = match store.begin(second_request, "second".into()).await.unwrap() {
        Begin::Started(receipt) => receipt,
        _ => panic!("unexpected blocked begin"),
    };
    let receipts = [first.clone(), second.clone()]
        .into_iter()
        .map(|receipt| (receipt.scope.destination.clone(), receipt))
        .collect();
    let mut saved = Saved {
        loaded: true,
        mixed_at_load: notes_differ(&receipts),
        receipts,
        ..Default::default()
    };
    assert!(saved.mixed_notes());

    let stopped = store.retire(first).await.unwrap().unwrap();
    saved
        .receipts
        .insert(stopped.scope.destination.clone(), stopped);
    assert!(!notes_differ(&saved.receipts)); // Only one uncertain note remains.
    assert!(saved.mixed_notes()); // The mounted editor still holds the first note.

    let confirmed = store
        .finish(
            second,
            Ok(api::ForwardedMessage {
                channel: "C3".into(),
                ts: "101.000002".into(),
            }),
        )
        .await
        .unwrap()
        .unwrap();
    saved
        .receipts
        .insert(confirmed.scope.destination.clone(), confirmed);
    assert!(!saved.mixed_notes()); // No frozen notes remain; editing is safe.
    cleanup(&store);
}

#[tokio::test]
async fn identical_pending_notes_do_not_latch_and_terminal_notes_do_not_mix() {
    let store = storage();
    let first = pending(&store).await;
    let mut second_request = first.delivery.request.clone();
    second_request.destination = "C3".into();
    second_request.client_msg_id = "00000000-0000-4000-8000-000000000081".into();
    let second = match store.begin(second_request, "second".into()).await.unwrap() {
        Begin::Started(receipt) => receipt,
        _ => panic!("unexpected blocked begin"),
    };
    let receipts = [first.clone(), second]
        .into_iter()
        .map(|receipt| (receipt.scope.destination.clone(), receipt))
        .collect();
    let mut saved = Saved {
        loaded: true,
        mixed_at_load: notes_differ(&receipts),
        receipts,
        ..Default::default()
    };
    assert!(!saved.mixed_notes());
    saved
        .receipts
        .get_mut("C3")
        .unwrap()
        .delivery
        .request
        .blocks
        .clear();
    assert!(saved.mixed_notes()); // New differences remain guarded without a latch.
    let stopped = store.retire(first).await.unwrap().unwrap();
    saved
        .receipts
        .insert(stopped.scope.destination.clone(), stopped);
    assert!(!saved.mixed_notes()); // A terminal note does not freeze a live composer.
    cleanup(&store);
}

#[test]
fn captured_storage_rejects_a_different_account_with_the_same_workspace() {
    let store = storage();
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(store.account.clone());
    shell.core.active_team = Some(store.team.clone());
    shell
        .core
        .workspaces
        .get_mut(&store.team)
        .unwrap()
        .self_user_id = store.user.clone();
    assert!(store.current(&shell));
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000072".into());
    assert!(!store.current(&shell));
    shell.core.active_account = Some(store.account.clone());
    shell
        .core
        .workspaces
        .get_mut(&store.team)
        .unwrap()
        .self_user_id = "OTHER".into();
    assert!(!store.current(&shell));
}

use super::*;
use crate::drafts::dispatch::Outcome;

async fn acknowledged_editor() -> (Fixture, Arc<Editor>) {
    let (fixture, _) = Fixture::new(ACCOUNT).await;
    let row = fixture.owner.load().await.unwrap().stored.unwrap();
    fixture
        .owner
        .dispatch(row.revision, |operation| async move {
            Outcome::Draft(RemoteDraft {
                scope: operation.scope,
                id: "DrOwned".into(),
                client_msg_id: operation.client_msg_id,
                last_updated_ts: "100.000001".into(),
                snapshot: operation.snapshot.unwrap(),
            })
        })
        .await
        .unwrap();
    let editor = Editor::new(fixture.owner.clone(), fixture.owner.load().await.unwrap()).unwrap();
    (fixture, editor)
}

fn completed(operation: &super_platinum_core::drafts::sync::Operation) -> Outcome {
    match operation.kind {
        super_platinum_core::drafts::sync::OperationKind::Send => Outcome::Sent {
            client_msg_id: operation.client_msg_id.clone(),
            draft_id: operation.base.as_ref().map(|base| base.id.clone()),
            ts: "200.000001".into(),
        },
        super_platinum_core::drafts::sync::OperationKind::Schedule { post_at } => {
            Outcome::Scheduled {
                draft: RemoteDraft {
                    scope: operation.scope.clone(),
                    id: operation.base.as_ref().unwrap().id.clone(),
                    client_msg_id: operation.client_msg_id.clone(),
                    last_updated_ts: "200.000001".into(),
                    snapshot: operation.snapshot.clone().unwrap(),
                },
                post_at,
            }
        }
        _ => panic!("terminal operation required"),
    }
}

#[tokio::test]
async fn acknowledged_send_and_schedule_preserve_arbitrary_newer_input_through_receipt_and_release()
{
    for kind in [ConsumeKind::Send, ConsumeKind::Schedule { post_at: 500 }] {
        let (fixture, editor) = acknowledged_editor().await;
        let frozen = editor.consume(local("Original"), kind).await.unwrap();
        let intent = frozen.state.consumption().unwrap().clone();
        let writer = editor.clone();
        let expected = intent.clone();
        let receipt = fixture
            .owner
            .dispatch(frozen.revision, move |operation| async move {
                writer.submit(unsupported_local(true));
                let saved = writer.flush().await.unwrap();
                assert!(same_local(&saved.local, &unsupported_local(true)));
                assert!(saved.blocked.is_none());
                let state = saved.stored.unwrap().state;
                assert_eq!(state.pending(), Some(&operation));
                assert_eq!(state.consumption(), Some(&expected));
                assert_eq!(operation.base.as_ref().unwrap().id, "DrOwned");
                completed(&operation)
            })
            .await
            .unwrap();
        assert!(receipt.applied);
        editor.submit(unsupported_local(false));
        editor.flush().await.unwrap();
        editor.release().await.unwrap();
        let saved = fixture.owner.load().await.unwrap();
        assert!(saved.stored.is_none());
        assert!(same_local(&saved.local, &unsupported_local(false)));
        assert!(saved.blocked.is_none());
    }
}

#[tokio::test]
async fn restarted_uncertain_acknowledged_send_and_schedule_never_replay_or_overwrite_intent() {
    for kind in [ConsumeKind::Send, ConsumeKind::Schedule { post_at: 500 }] {
        let (fixture, editor) = acknowledged_editor().await;
        let frozen = editor.consume(local("Original"), kind).await.unwrap();
        let receipt = fixture
            .owner
            .dispatch(frozen.revision, |_| async { Outcome::Uncertain })
            .await
            .unwrap();
        let restarted =
            Account::new(fixture.account.backend.clone()).owner(fixture.owner.scope.key.clone());
        let restored = Editor::new(restarted.clone(), restarted.load().await.unwrap()).unwrap();
        restored.submit(unsupported_local(true));
        let saved = restored.flush().await.unwrap();
        assert!(same_local(&saved.local, &unsupported_local(true)));
        assert!(saved.blocked.is_none());
        let mut state = saved.stored.unwrap().state;
        assert_eq!(state.pending(), Some(&receipt.operation));
        assert_eq!(state.consumption(), frozen.state.consumption());
        assert!(state.begin_operation(&fixture.owner.scope).is_none());
        assert!(matches!(restored.release().await, Err(Fault::Pending)));
    }
}

#[tokio::test]
async fn restarted_positive_receipt_recovers_only_its_exact_persisted_local_capability() {
    for kind in [ConsumeKind::Send, ConsumeKind::Schedule { post_at: 500 }] {
        let (fixture, editor) = acknowledged_editor().await;
        let frozen = editor.consume(local("Original"), kind).await.unwrap();
        fixture
            .owner
            .dispatch(
                frozen.revision,
                |operation| async move { completed(&operation) },
            )
            .await
            .unwrap();
        let restarted =
            Account::new(fixture.account.backend.clone()).owner(fixture.owner.scope.key.clone());
        let restored = Editor::new(restarted.clone(), restarted.load().await.unwrap()).unwrap();
        restored.submit(unsupported_local(true));
        let saved = restored.flush().await.unwrap();
        assert!(same_local(&saved.local, &unsupported_local(true)));
        assert!(saved.blocked.is_none());
        restored.release().await.unwrap();
        assert!(same_local(
            &restarted.load().await.unwrap().local,
            &unsupported_local(true)
        ));
    }
}

#[tokio::test]
async fn stale_editor_intent_cannot_admit_writes_after_release_and_reenrollment() {
    let (fixture, editor) = acknowledged_editor().await;
    let frozen = editor
        .consume(local("Original"), ConsumeKind::Send)
        .await
        .unwrap();
    let old = frozen.state.consumption().unwrap().clone();
    fixture
        .owner
        .dispatch(
            frozen.revision,
            |operation| async move { completed(&operation) },
        )
        .await
        .unwrap();
    editor.submit(local("Replacement"));
    editor.flush().await.unwrap();
    editor.release().await.unwrap();
    let new = fixture
        .owner
        .enroll(local("Replacement"), None)
        .await
        .unwrap();
    assert!(matches!(
        fixture
            .owner
            .checkpoint_direct_local(new.revision, unsupported_local(true), &old)
            .await,
        Err(Fault::Pending)
    ));
    let saved = fixture.owner.load().await.unwrap();
    assert!(same_local(&saved.local, &local("Replacement")));
    let saved = saved.stored.unwrap();
    assert_eq!(saved.revision, new.revision);
    assert_eq!(saved.state, new.state);
}

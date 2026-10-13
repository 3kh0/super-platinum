use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use super_platinum_core::drafts::{
    store::DraftStore,
    sync::{ConsumeKind, Workflow},
};

struct Fixture {
    owner: Owner,
    path: std::path::PathBuf,
}
impl Fixture {
    async fn new() -> (Self, Stored) {
        let uuid = super_platinum_core::slack::api::ForwardMessage::new(
            "C1".into(),
            "1.000001".into(),
            "C2".into(),
            vec![],
        )
        .client_msg_id;
        let path = std::env::temp_dir().join(format!("draft-control-{uuid}.sqlite"));
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        };
        let initial = local("Frozen 日本語 🚀");
        DraftStore::open(&path)
            .unwrap()
            .save([(key.clone(), initial.clone())])
            .unwrap();
        let account = Account::new(Backend::Isolated {
            account: "00000000-0000-4000-8000-000000000085".into(),
            path: path.clone(),
        });
        let owner = account.owner(key);
        let row = owner.enroll(initial, None).await.unwrap();
        (Self { owner, path }, row)
    }
    async fn synced() -> (Self, Stored) {
        let (fixture, initial) = Self::new().await;
        let row = fixture
            .owner
            .dispatch(initial.revision, |op| async move {
                Outcome::Draft(RemoteDraft {
                    scope: op.scope,
                    id: "DrControl".into(),
                    client_msg_id: op.client_msg_id,
                    last_updated_ts: "1719800001000".into(),
                    snapshot: op.snapshot.unwrap(),
                })
            })
            .await
            .unwrap()
            .stored;
        (fixture, row)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: super_platinum_core::ComposerState::with_text(text),
        attachments: vec![],
    }
}

#[tokio::test]
async fn clean_remote_deletion_atomically_clears_local_and_does_not_dispatch() {
    let (f, row) = Fixture::synced().await;
    assert!(matches!(
        f.owner.observe_deleted(row.revision - 1, "DrControl").await,
        Err(Fault::Stale)
    ));
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Frozen 日本語 🚀"
    );
    let deleted = f
        .owner
        .observe_deleted(row.revision, "DrControl")
        .await
        .unwrap();
    let view = f.owner.load().await.unwrap();
    assert!(view.local.empty());
    assert!(deleted.state.desired().is_none());
    assert!(deleted.state.acknowledged().is_none());
    assert_ne!(deleted.state.client_msg_id(), row.state.client_msg_id());
    let calls = AtomicUsize::new(0);
    assert!(matches!(
        f.owner
            .dispatch(deleted.revision, |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Outcome::Uncertain
            })
            .await,
        Err(Fault::Pending)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn draft_deletion_during_send_preserves_uncertainty_intent_and_newer_typing() {
    let (f, row) = Fixture::synced().await;
    let consumed = f
        .owner
        .consume(row.revision, ConsumeKind::Send)
        .await
        .unwrap();
    let frozen = consumed.state.consumption().unwrap().clone();
    let pending = f
        .owner
        .dispatch(consumed.revision, |_| async { Outcome::Uncertain })
        .await
        .unwrap();
    let typed = f
        .owner
        .checkpoint(pending.stored.revision, local("Next message"))
        .await
        .unwrap();
    let deleted = f
        .owner
        .observe_deleted(typed.revision, "DrControl")
        .await
        .unwrap();
    assert!(matches!(deleted.state.workflow(), Workflow::Uncertain(_)));
    assert_eq!(deleted.state.pending(), Some(&pending.operation));
    assert_eq!(deleted.state.consumption(), Some(&frozen));
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Next message"
    );
    assert!(matches!(
        f.owner
            .retry_consumption(deleted.revision, &frozen.id)
            .await,
        Err(Fault::Pending)
    ));
}

#[tokio::test]
async fn rejected_consumption_requires_explicit_current_revision_retry() {
    let (f, row) = Fixture::new().await;
    let consumed = f
        .owner
        .consume(row.revision, ConsumeKind::Send)
        .await
        .unwrap();
    let intent = consumed.state.consumption().unwrap().clone();
    let rejected = f
        .owner
        .dispatch(consumed.revision, |_| async { Outcome::Rejected })
        .await
        .unwrap();
    let calls = AtomicUsize::new(0);
    assert!(matches!(
        f.owner
            .dispatch(rejected.stored.revision, |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Outcome::Uncertain
            })
            .await,
        Err(Fault::Pending)
    ));
    assert!(matches!(
        f.owner
            .retry_consumption(consumed.revision, &intent.id)
            .await,
        Err(Fault::Stale)
    ));
    let retriable = f
        .owner
        .retry_consumption(rejected.stored.revision, &intent.id)
        .await
        .unwrap();
    assert_eq!(retriable.state.workflow(), &Workflow::Idle);
    assert_eq!(retriable.state.consumption(), Some(&intent));
    assert!(f.owner.load().await.unwrap().local.empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let retry = f
        .owner
        .dispatch(retriable.revision, |_| async { Outcome::Uncertain })
        .await
        .unwrap();
    assert_ne!(retry.operation.id, rejected.operation.id);
    assert_eq!(retry.operation.snapshot, rejected.operation.snapshot);
}

#[tokio::test]
async fn cancellation_restores_frozen_local_only_without_newer_typing() {
    let (f, row) = Fixture::new().await;
    let consumed = f
        .owner
        .consume(row.revision, ConsumeKind::Send)
        .await
        .unwrap();
    let intent = consumed.state.consumption().unwrap().clone();
    assert!(matches!(
        f.owner.cancel_consumption(row.revision, &intent.id).await,
        Err(Fault::Stale)
    ));
    let restored = f
        .owner
        .cancel_consumption(consumed.revision, &intent.id)
        .await
        .unwrap();
    assert!(restored.state.consumption().is_none());
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Frozen 日本語 🚀"
    );
    let consumed = f
        .owner
        .consume(restored.revision, ConsumeKind::Send)
        .await
        .unwrap();
    let intent = consumed.state.consumption().unwrap().clone();
    let newer = f
        .owner
        .checkpoint(consumed.revision, local("Do not replace this"))
        .await
        .unwrap();
    assert!(matches!(
        f.owner.cancel_consumption(newer.revision, &intent.id).await,
        Err(Fault::Pending)
    ));
    let view = f.owner.load().await.unwrap();
    assert_eq!(view.local.composer.text, "Do not replace this");
    assert_eq!(view.stored.unwrap().state.consumption(), Some(&intent));
}

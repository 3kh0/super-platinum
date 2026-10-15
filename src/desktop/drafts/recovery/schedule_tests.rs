use super::*;
use super_platinum_core::{ComposerState, drafts::store::DraftStore};

pub(super) struct Fixture {
    pub(super) owner: Owner,
    path: std::path::PathBuf,
}
pub(super) fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
impl Fixture {
    pub(super) async fn new() -> (Self, Operation) {
        let id = super_platinum_core::slack::api::ForwardMessage::new(
            "C1".into(),
            "1.000001".into(),
            "C2".into(),
            vec![],
        )
        .client_msg_id;
        let path = std::env::temp_dir().join(format!("draft-schedule-check-{id}.sqlite"));
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        };
        DraftStore::open(&path)
            .unwrap()
            .save([(key.clone(), local("Original"))])
            .unwrap();
        let account = Account::new(Backend::Isolated {
            account: id.clone(),
            path: path.clone(),
        });
        let owner = account.owner(key);
        let stored = owner.enroll(local("Original"), None).await.unwrap();
        let created = owner
            .dispatch(stored.revision, |op| async move {
                Outcome::Draft(RemoteDraft {
                    scope: op.scope,
                    client_msg_id: op.client_msg_id,
                    id: "Dr1".into(),
                    last_updated_ts: "100.000001".into(),
                    snapshot: op.snapshot.unwrap(),
                })
            })
            .await
            .unwrap();
        let edited = owner
            .consume(
                created.stored.revision,
                super_platinum_core::drafts::sync::ConsumeKind::Schedule { post_at: 500 },
            )
            .await
            .unwrap();
        let receipt = owner
            .dispatch(edited.revision, |_| async { Outcome::Uncertain })
            .await
            .unwrap();
        assert_eq!(
            receipt.operation.kind,
            OperationKind::Schedule { post_at: 500 }
        );
        (Self { owner, path }, receipt.operation)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
pub(super) fn positive(op: &Operation) -> Readback {
    let draft = RemoteDraft {
        scope: op.scope.clone(),
        id: op.base.as_ref().unwrap().id.clone(),
        client_msg_id: op.client_msg_id.clone(),
        last_updated_ts: "100.000002".into(),
        snapshot: op.snapshot.clone().unwrap(),
    };
    Readback {
        raw: Some(serde_json::json!({"ok":true,"draft":{
            "id":draft.id,"client_msg_id":draft.client_msg_id,
            "last_updated_ts":draft.last_updated_ts,"date_scheduled":500,
            "date_created":100,"is_deleted":false,"is_sent":false,
            "blocks":draft.snapshot.blocks,"file_ids":draft.snapshot.file_ids,
            "destinations":draft.snapshot.destinations}})),
        outcome: Some(Outcome::Scheduled {
            draft,
            post_at: 500,
        }),
        issue: None,
        cooldown_until_ms: None,
        requested: true,
        has_more: false,
    }
}

#[tokio::test]
async fn schedule_read_completion_preserves_typing_and_requires_exact_one_use_ticket() {
    let (f, op) = Fixture::new().await;
    let ticket = f.owner.reserve_check(&op, 100).await.unwrap();
    let copied = ticket.clone();
    let mut changed = op.clone();
    changed.local_revision += 1;
    assert!(matches!(
        f.owner
            .finish_check_with(changed, ticket.clone(), || async {
                panic!("wrong operation read")
            })
            .await,
        Err(Fault::InvalidReceipt)
    ));
    let during = f.owner.clone();
    let read_op = op.clone();
    let checked = f
        .owner
        .finish_check_with(op.clone(), ticket, || async move {
            let current = during.load().await.unwrap().stored.unwrap();
            during
                .checkpoint(current.revision, local("Typed while checking"))
                .await
                .unwrap();
            positive(&read_op)
        })
        .await
        .unwrap();
    assert!(checked.confirmed);
    assert_eq!(checked.view.local.composer.text, "Typed while checking");
    let stored = checked.view.stored.unwrap();
    assert!(matches!(stored.state.workflow(), Workflow::Idle));
    assert!(stored.state.acknowledged().is_none());
    assert_eq!(
        stored.state.desired(),
        local_snapshot(&op.scope.key, &local("Typed while checking"))
            .unwrap()
            .as_ref()
    );
    assert!(stored.state.consumption().is_none());
    assert!(matches!(
        f.owner
            .finish_check_with(op, copied, || async { panic!("ticket replay") })
            .await,
        Err(Fault::Pending)
    ));
}

#[tokio::test]
async fn unresolved_schedule_ticket_survives_restart_and_elapsed_floor() {
    let (f, op) = Fixture::new().await;
    let ticket = f.owner.reserve_check(&op, 100).await.unwrap();
    // A new process has no issued capability, even though SQLite has the ticket.
    issued().lock().unwrap().remove(ticket.id());
    let restarted = Account::new(f.owner.account.backend.clone()).owner(op.scope.key.clone());
    assert_eq!(
        restarted.reserve_check(&op, u64::MAX).await,
        Err(CheckError::Pending)
    );
    assert!(matches!(
        restarted
            .finish_check_with(op, ticket, || async { panic!("restarted read") })
            .await,
        Err(Fault::Pending)
    ));
}

#[tokio::test]
async fn failed_completion_storage_retains_ticket_and_retries_only_storage() {
    let (f, op) = Fixture::new().await;
    let ticket = f.owner.reserve_check(&op, 100).await.unwrap();
    let held = f.path.with_extension("held");
    let path = f.path.clone();
    let hold = held.clone();
    let read_op = op.clone();
    let failed = f
        .owner
        .finish_check_with(op.clone(), ticket, || async move {
            std::fs::rename(&path, &hold).unwrap();
            std::fs::create_dir(&path).unwrap();
            positive(&read_op)
        })
        .await;
    assert!(matches!(failed, Err(Fault::Storage)));
    std::fs::remove_dir(&f.path).unwrap();
    std::fs::rename(&held, &f.path).unwrap();
    let restarted = Account::new(f.owner.account.backend.clone()).owner(op.scope.key.clone());
    assert_eq!(
        restarted.reserve_check(&op, u64::MAX).await,
        Err(CheckError::Pending)
    );
    let checked = f.owner.retry_check_commit(&op).await.unwrap().unwrap();
    assert!(checked.confirmed);
    assert!(f.owner.retry_check_commit(&op).await.unwrap().is_none());
    assert!(
        f.owner
            .load()
            .await
            .unwrap()
            .stored
            .unwrap()
            .state
            .pending_check(&op)
            .is_none()
    );
}

#[tokio::test]
async fn negative_read_never_resolves_schedule_and_saved_cooldown_survives_restart() {
    let (f, op) = Fixture::new().await;
    let ticket = f.owner.reserve_check(&op, 100).await.unwrap();
    let checked = f
        .owner
        .finish_check_with(op.clone(), ticket, || async {
            Readback {
                outcome: None,
                raw: None,
                issue: Some(network::Issue::RateLimited),
                cooldown_until_ms: Some(120100),
                requested: true,
                has_more: false,
            }
        })
        .await
        .unwrap();
    assert!(!checked.confirmed);
    assert!(matches!(
        checked.view.stored.unwrap().state.workflow(),
        Workflow::Uncertain(_)
    ));
    let restarted = Account::new(f.owner.account.backend.clone()).owner(op.scope.key.clone());
    assert_eq!(
        restarted.reserve_check(&op, 120099).await,
        Err(CheckError::Cooldown(120100))
    );
}

#[tokio::test]
async fn canceled_read_leaves_durable_pending_without_replay() {
    let (f, op) = Fixture::new().await;
    let ticket = f.owner.reserve_check(&op, 100).await.unwrap();
    let owner = f.owner.clone();
    let operation = op.clone();
    let (started, waiting) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        owner
            .finish_check_with(operation, ticket, || async move {
                started.send(()).unwrap();
                std::future::pending::<Readback>().await
            })
            .await
    });
    waiting.await.unwrap();
    task.abort();
    let _ = task.await;
    let restarted = Account::new(f.owner.account.backend.clone()).owner(op.scope.key.clone());
    assert_eq!(
        restarted.reserve_check(&op, u64::MAX).await,
        Err(CheckError::Pending)
    );
    assert!(restarted.retry_check_commit(&op).await.unwrap().is_none());
}

#[tokio::test]
async fn typing_during_receipt_wait_is_preserved_by_atomic_pair_commit() {
    let (f, op) = Fixture::new().await;
    let ticket = f.owner.reserve_check(&op, 100).await.unwrap();
    let gate = f.owner.gate.lock().await;
    let owner = f.owner.clone();
    let operation = op.clone();
    let task =
        tokio::spawn(async move { owner.complete_check(ticket, positive(&operation)).await });
    tokio::task::yield_now().await;
    // Simulate a separately committed latest paired edit while the receipt is
    // waiting. The receipt must load this version only after acquiring its gate.
    let view = f.owner.current().await.unwrap();
    let mut stored = view.stored.unwrap();
    stored.state.edit(
        &op.scope,
        local_snapshot(&op.scope.key, &local("Typing during persistence")).unwrap(),
    );
    f.owner
        .save(
            Some(stored.revision),
            local("Typing during persistence"),
            stored.state,
        )
        .await
        .unwrap();
    drop(gate);
    let checked = task.await.unwrap().unwrap();
    assert!(checked.confirmed);
    assert_eq!(
        checked.view.local.composer.text,
        "Typing during persistence"
    );
    assert_eq!(
        checked.view.stored.unwrap().state.desired(),
        local_snapshot(&op.scope.key, &local("Typing during persistence"))
            .unwrap()
            .as_ref()
    );
}

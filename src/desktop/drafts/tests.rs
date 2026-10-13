use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use super_platinum_core::{
    ComposerState,
    drafts::{
        store::DraftStore,
        sync::{ConsumeKind, OperationKind, Workflow},
    },
};

struct Fixture {
    account: Arc<Account>,
    owner: Owner,
    path: std::path::PathBuf,
    local: LocalDraft,
}
impl Fixture {
    fn new() -> Self {
        Self::with_account("00000000-0000-4000-8000-000000000081")
    }
    fn with_account(account_id: &str) -> Self {
        let id = super_platinum_core::slack::api::ForwardMessage::new(
            "C1".into(),
            "100.000001".into(),
            "C2".into(),
            vec![],
        )
        .client_msg_id;
        let path = std::env::temp_dir().join(format!("draft-owner-{id}.sqlite"));
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        };
        let local = local("Original 日本語 🚀");
        DraftStore::open(&path)
            .unwrap()
            .save([(key.clone(), local.clone())])
            .unwrap();
        let account = Account::new(Backend::Isolated {
            account: account_id.into(),
            path: path.clone(),
        });
        let owner = account.owner(key);
        Self {
            account,
            owner,
            path,
            local,
        }
    }
    async fn enroll(&self) -> Stored {
        self.owner.enroll(self.local.clone(), None).await.unwrap()
    }
    fn restart(&self) -> Owner {
        Account::new(self.account.backend.clone()).owner(self.owner.scope.key.clone())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn remote(op: &Operation, ts: &str) -> RemoteDraft {
    RemoteDraft {
        scope: op.scope.clone(),
        id: op
            .base
            .as_ref()
            .map(|b| b.id.clone())
            .unwrap_or_else(|| "DrOwned".into()),
        client_msg_id: op.client_msg_id.clone(),
        last_updated_ts: ts.into(),
        snapshot: op.snapshot.clone().unwrap(),
    }
}
fn sent(op: &Operation) -> Outcome {
    Outcome::Sent {
        client_msg_id: op.client_msg_id.clone(),
        draft_id: op.base.as_ref().map(|b| b.id.clone()),
        ts: "1719800001.000001".into(),
    }
}

#[tokio::test]
async fn intent_is_durable_before_dispatch_and_newer_typing_survives_receipt() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let owner = &f.owner;
    let result = f
        .owner
        .dispatch(initial.revision, |op| async move {
            let durable = owner.load().await.unwrap();
            assert_eq!(durable.stored.as_ref().unwrap().state.pending(), Some(&op));
            assert_eq!(op.kind, OperationKind::Create);
            owner
                .checkpoint(durable.stored.unwrap().revision, local("Newer input"))
                .await
                .unwrap();
            Outcome::Draft(remote(&op, "1719800001000"))
        })
        .await
        .unwrap();
    assert!(result.applied);
    let restored = f.owner.load().await.unwrap();
    assert_eq!(restored.local.composer.text, "Newer input");
    assert_eq!(
        result.stored.state.desired(),
        Some(&codec::encode(&f.owner.scope.key, "Newer input").unwrap())
    );
    assert_ne!(
        result.stored.state.desired(),
        result.stored.state.acknowledged().map(|r| &r.snapshot)
    );
}

#[tokio::test]
async fn direct_send_has_no_extra_create_and_uncertainty_never_replays_after_restart() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let consumed = f
        .owner
        .consume(initial.revision, ConsumeKind::Send)
        .await
        .unwrap();
    assert!(f.owner.load().await.unwrap().local.empty());
    let frozen = consumed.state.consumption().unwrap().clone();
    let result = f
        .owner
        .dispatch(consumed.revision, |op| async move {
            assert_eq!(op.kind, OperationKind::Send);
            assert!(op.base.is_none());
            assert_eq!(op.snapshot, Some(frozen.snapshot.clone()));
            Outcome::Uncertain
        })
        .await
        .unwrap();
    let restarted = f.restart();
    let loaded = restarted.load().await.unwrap();
    let stored = loaded.stored.unwrap();
    assert!(matches!(stored.state.workflow(), Workflow::Uncertain(_)));
    assert_eq!(stored.state.pending(), Some(&result.operation));
    let calls = AtomicUsize::new(0);
    assert!(matches!(
        restarted
            .dispatch(stored.revision, |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Outcome::Uncertain
            })
            .await,
        Err(Fault::Pending)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn send_during_create_waits_then_uses_the_owned_id_and_preserves_new_input() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let owner = &f.owner;
    let created = f
        .owner
        .dispatch(initial.revision, |op| async move {
            let current = owner.load().await.unwrap().stored.unwrap();
            let consumed = owner
                .consume(current.revision, ConsumeKind::Send)
                .await
                .unwrap();
            assert_eq!(consumed.state.pending(), Some(&op));
            owner
                .checkpoint(consumed.revision, local("Next message"))
                .await
                .unwrap();
            Outcome::Draft(remote(&op, "1719800001000"))
        })
        .await
        .unwrap();
    let original = created.stored.state.client_msg_id().to_owned();
    let delivered = f
        .owner
        .dispatch(created.stored.revision, |op| async move {
            assert_eq!(op.kind, OperationKind::Send);
            assert_eq!(op.base.as_ref().unwrap().id, "DrOwned");
            assert_eq!(
                op.snapshot,
                Some(codec::encode(&op.scope.key, "Original 日本語 🚀").unwrap())
            );
            sent(&op)
        })
        .await
        .unwrap();
    assert!(delivered.applied);
    assert!(delivered.stored.state.consumption().is_none());
    assert_ne!(delivered.stored.state.client_msg_id(), original);
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Next message"
    );
    assert_eq!(
        delivered.stored.state.desired(),
        Some(&codec::encode(&f.owner.scope.key, "Next message").unwrap())
    );
}

#[tokio::test]
async fn scheduling_reuses_adopted_identity_and_rotates_only_after_positive_receipt() {
    let f = Fixture::new();
    let snapshot = codec::encode(&f.owner.scope.key, &f.local.composer.text).unwrap();
    let remote = RemoteDraft {
        scope: f.owner.scope.clone(),
        id: "DrExisting".into(),
        client_msg_id: "00000000-0000-4000-8000-000000000091".into(),
        last_updated_ts: "1719800000000".into(),
        snapshot,
    };
    let initial = f
        .owner
        .enroll(f.local.clone(), Some(remote.clone()))
        .await
        .unwrap();
    let consumed = f
        .owner
        .consume(
            initial.revision,
            ConsumeKind::Schedule {
                post_at: 1_800_000_000,
            },
        )
        .await
        .unwrap();
    let result = f
        .owner
        .dispatch(consumed.revision, |op| async move {
            assert_eq!(
                op.kind,
                OperationKind::Schedule {
                    post_at: 1_800_000_000
                }
            );
            assert_eq!(op.base.as_ref().unwrap().id, remote.id);
            let mut acknowledged = remote;
            acknowledged.last_updated_ts = "1719800002000".into();
            Outcome::Scheduled {
                draft: acknowledged,
                post_at: 1_800_000_000,
            }
        })
        .await
        .unwrap();
    assert!(result.applied);
    assert!(result.stored.state.consumption().is_none());
    assert!(result.stored.state.acknowledged().is_none());
}

#[tokio::test]
async fn invalid_send_receipt_keeps_exact_operation_uncertain() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let consumed = f
        .owner
        .consume(initial.revision, ConsumeKind::Send)
        .await
        .unwrap();
    let result = f
        .owner
        .dispatch(consumed.revision, |op| async move {
            Outcome::Sent {
                client_msg_id: "another-request".into(),
                draft_id: None,
                ts: match op.kind {
                    OperationKind::Send => "1719800001.000001".into(),
                    _ => panic!("not send"),
                },
            }
        })
        .await
        .unwrap();
    assert!(!result.applied);
    assert!(matches!(
        result.stored.state.workflow(),
        Workflow::Uncertain(_)
    ));
    assert!(result.stored.state.consumption().is_some());
}

#[tokio::test]
async fn concurrent_views_dispatch_only_once_and_stale_edits_do_not_overwrite() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let second = f.account.owner(f.owner.scope.key.clone());
    let calls = AtomicUsize::new(0);
    let send = |op: Operation| {
        calls.fetch_add(1, Ordering::SeqCst);
        async move { Outcome::Draft(remote(&op, "1719800001000")) }
    };
    let (a, b) = tokio::join!(
        f.owner.dispatch(initial.revision, send),
        second.dispatch(initial.revision, send)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(matches!(
        second
            .checkpoint(initial.revision, local("Stale edit"))
            .await,
        Err(Fault::Stale)
    ));
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        f.local.composer.text
    );
}

#[tokio::test]
async fn canceled_dispatch_future_is_uncertain_on_restart() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let owner = f.owner.clone();
    let (entered, observed) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        owner
            .dispatch(initial.revision, |op| async move {
                entered.send(op).unwrap();
                std::future::pending::<Outcome>().await
            })
            .await
    });
    let operation = observed.await.unwrap();
    task.abort();
    let _ = task.await;
    let restarted = f.restart();
    let stored = restarted.load().await.unwrap().stored.unwrap();
    assert!(matches!(stored.state.workflow(), Workflow::Uncertain(_)));
    assert_eq!(stored.state.pending(), Some(&operation));
}

#[tokio::test]
async fn unsupported_remote_content_is_retained_as_conflict_without_flattening() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let created = f
        .owner
        .dispatch(initial.revision, |op| async move {
            Outcome::Draft(remote(&op, "1719800001000"))
        })
        .await
        .unwrap();
    let mut changed = created.stored.state.acknowledged().unwrap().clone();
    changed.last_updated_ts = "1719800002000".into();
    changed.snapshot.blocks[0]["block_id"] = "opaque-server-metadata".into();
    changed.snapshot.file_ids.push("FKeepMe".into());
    let stored = f.owner.observe(changed.clone()).await.unwrap();
    assert!(matches!(stored.state.workflow(),Workflow::Conflict{remote:Some(r),..} if r==&changed));
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        f.local.composer.text
    );
    let calls = AtomicUsize::new(0);
    assert!(matches!(
        f.owner
            .dispatch(stored.revision, |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Outcome::Uncertain
            })
            .await,
        Err(Fault::Pending)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn storage_failure_and_unsupported_content_authorize_no_dispatch() {
    let f = Fixture::new();
    assert!(matches!(
        f.owner
            .enroll(local("Different stale local text"), None)
            .await,
        Err(Fault::Stale)
    ));
    let initial = f.enroll().await;
    assert!(matches!(
        f.owner
            .checkpoint(initial.revision, local("<@U123> unsupported entity"))
            .await,
        Err(Fault::Unsupported)
    ));
    std::fs::remove_file(&f.path).unwrap();
    std::fs::create_dir(&f.path).unwrap();
    let calls = AtomicUsize::new(0);
    assert!(matches!(
        f.owner
            .dispatch(initial.revision, |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Outcome::Uncertain
            })
            .await,
        Err(Fault::Storage)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    std::fs::remove_dir(&f.path).unwrap();
}

#[tokio::test]
async fn outcomes_cannot_be_recorded_against_another_account_owner() {
    let f = Fixture::new();
    let other = Fixture::new();
    let initial = f.enroll().await;
    other.enroll().await;
    let result = f
        .owner
        .dispatch(initial.revision, |_| async move { Outcome::Uncertain })
        .await
        .unwrap();
    // Same destination can exist in an entirely different account database.
    let mut operation = result.operation;
    operation.scope.account_id = "00000000-0000-4000-8000-000000000082".into();
    assert!(matches!(
        other.owner.record(operation, Outcome::Rejected).await,
        Err(Fault::InvalidReceipt)
    ));
    assert!(matches!(
        other
            .owner
            .load()
            .await
            .unwrap()
            .stored
            .unwrap()
            .state
            .workflow(),
        Workflow::Idle
    ));
}

#[tokio::test]
async fn unsupported_schedule_response_survives_restart_as_raw_conflict() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let consumed = f
        .owner
        .consume(
            initial.revision,
            ConsumeKind::Schedule {
                post_at: 1_800_000_000,
            },
        )
        .await
        .unwrap();
    let frozen = consumed.state.consumption().unwrap().clone();
    let mut acknowledged = RemoteDraft {
        scope: f.owner.scope.clone(),
        id: "DrNewSchedule".into(),
        client_msg_id: frozen.client_msg_id.clone(),
        last_updated_ts: "1719800002000".into(),
        snapshot: frozen.snapshot.clone(),
    };
    acknowledged.snapshot.blocks[0]["block_id"] = "server-added-metadata".into();
    acknowledged.snapshot.file_ids.push("FServerAdded".into());
    let expected = acknowledged.clone();
    let result = f
        .owner
        .dispatch(consumed.revision, |_op| async move {
            Outcome::Scheduled {
                draft: acknowledged,
                post_at: 1_800_000_000,
            }
        })
        .await
        .unwrap();
    assert!(!result.applied);
    assert_eq!(result.stored.state.consumption(), Some(&frozen));
    assert!(
        matches!(result.stored.state.workflow(), Workflow::Conflict {remote: Some(remote), ..} if remote == &expected)
    );
    let restarted = f.restart();
    let view = restarted.load().await.unwrap();
    assert!(view.local.empty());
    let stored = view.stored.unwrap();
    assert_eq!(stored.state.consumption(), Some(&frozen));
    assert!(
        matches!(stored.state.workflow(), Workflow::Conflict {remote: Some(remote), pending: Some(op)} if remote == &expected && op == &result.operation)
    );
    let calls = AtomicUsize::new(0);
    assert!(matches!(
        restarted
            .dispatch(stored.revision, |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Outcome::Uncertain
            })
            .await,
        Err(Fault::Pending)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn lost_receipt_persistence_retains_original_operation_without_restart_replay() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let backup = f.path.with_extension("pending.sqlite");
    let observed = Mutex::new(None::<Operation>);
    let result = f
        .owner
        .dispatch(initial.revision, |operation| {
            *observed.lock().unwrap() = Some(operation.clone());
            // The operation has committed. Make only its subsequent receipt save
            // unavailable, then restore the original DB before process recovery.
            std::fs::rename(&f.path, &backup).unwrap();
            std::fs::create_dir(&f.path).unwrap();
            async move { Outcome::Draft(remote(&operation, "1719800001000")) }
        })
        .await;
    std::fs::remove_dir(&f.path).unwrap();
    std::fs::rename(&backup, &f.path).unwrap();
    assert!(matches!(result, Err(Fault::Storage)));
    let operation = observed.into_inner().unwrap().unwrap();
    let restarted = f.restart();
    let view = restarted.load().await.unwrap();
    assert_eq!(view.local.composer.text, f.local.composer.text);
    let stored = view.stored.unwrap();
    assert_eq!(stored.state.pending(), Some(&operation));
    assert!(matches!(stored.state.workflow(), Workflow::Uncertain(_)));
    let calls = AtomicUsize::new(0);
    assert!(matches!(
        restarted
            .dispatch(stored.revision, |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Outcome::Uncertain
            })
            .await,
        Err(Fault::Pending)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn delayed_outcome_stays_in_original_account_while_another_account_is_edited() {
    let first = Fixture::with_account("00000000-0000-4000-8000-000000000081");
    let second = Fixture::with_account("00000000-0000-4000-8000-000000000082");
    let a = first.enroll().await;
    let b = second.enroll().await;
    assert_eq!(first.owner.scope.key, second.owner.scope.key);
    assert_ne!(first.owner.scope.account_id, second.owner.scope.account_id);
    let other = &second.owner;
    let result = first
        .owner
        .dispatch(a.revision, |operation| async move {
            other
                .checkpoint(b.revision, local("Only account B changes"))
                .await
                .unwrap();
            Outcome::Draft(remote(&operation, "1719800001000"))
        })
        .await
        .unwrap();
    assert!(result.applied);
    let original = first.restart().load().await.unwrap();
    let different = second.restart().load().await.unwrap();
    assert_eq!(original.local.composer.text, first.local.composer.text);
    assert_eq!(
        original
            .stored
            .unwrap()
            .state
            .acknowledged()
            .unwrap()
            .scope
            .account_id,
        first.owner.scope.account_id
    );
    assert_eq!(different.local.composer.text, "Only account B changes");
    let other = different.stored.unwrap();
    assert!(other.state.acknowledged().is_none());
    assert!(other.state.pending().is_none());
    assert_eq!(
        other.state.desired(),
        Some(&codec::encode(&second.owner.scope.key, "Only account B changes").unwrap())
    );
}

#[tokio::test]
async fn another_owner_on_same_account_never_normalizes_a_live_operation_as_restart() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let account = f.account.clone();
    let key = f.owner.scope.key.clone();
    let result = f
        .owner
        .dispatch(initial.revision, |operation| async move {
            let alternate = account.owner(key);
            let view = alternate.load().await.unwrap();
            let live = view.stored.unwrap();
            assert_eq!(
                live.state.workflow(),
                &Workflow::InFlight(operation.clone())
            );
            let newer = alternate
                .checkpoint(live.revision, local("Edited in another view"))
                .await
                .unwrap();
            assert_eq!(
                newer.state.workflow(),
                &Workflow::InFlight(operation.clone())
            );
            Outcome::Draft(remote(&operation, "1719800001000"))
        })
        .await
        .unwrap();
    assert!(result.applied);
    assert_eq!(result.stored.state.workflow(), &Workflow::Idle);
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Edited in another view"
    );
}

#[tokio::test]
async fn clearing_during_create_waits_then_deletes_only_its_confirmed_remote_id() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let owner = &f.owner;
    let created = f
        .owner
        .dispatch(initial.revision, |operation| async move {
            let current = owner.load().await.unwrap().stored.unwrap();
            let cleared = owner.clear(current.revision).await.unwrap();
            assert!(owner.load().await.unwrap().local.empty());
            assert_eq!(cleared.state.pending(), Some(&operation));
            assert!(cleared.state.tombstone());
            Outcome::Draft(remote(&operation, "1719800001000"))
        })
        .await
        .unwrap();
    assert!(created.stored.state.desired().is_none());
    let removed = f
        .owner
        .dispatch(created.stored.revision, |operation| async move {
            assert_eq!(operation.kind, OperationKind::Delete);
            assert!(operation.snapshot.is_none());
            let base = operation.base.as_ref().unwrap();
            assert_eq!(base.id, "DrOwned");
            assert_eq!(base.scope, operation.scope);
            Outcome::Deleted {
                draft_id: base.id.clone(),
            }
        })
        .await
        .unwrap();
    assert!(removed.applied);
    assert!(removed.stored.state.acknowledged().is_none());
    assert!(removed.stored.state.pending().is_none());
    assert!(f.owner.load().await.unwrap().local.empty());
}

#[tokio::test]
async fn supported_clean_remote_observation_updates_raw_and_local_in_one_pair() {
    let f = Fixture::new();
    let initial = f.enroll().await;
    let created = f
        .owner
        .dispatch(initial.revision, |operation| async move {
            Outcome::Draft(remote(&operation, "1719800001000"))
        })
        .await
        .unwrap();
    let mut updated = created.stored.state.acknowledged().unwrap().clone();
    updated.last_updated_ts = "1719800002000".into();
    updated.snapshot = codec::encode(&f.owner.scope.key, "*Remote* 日本語\nsecond line").unwrap();
    let row = f.owner.observe(updated.clone()).await.unwrap();
    let restored = f.restart().load().await.unwrap();
    assert_eq!(restored.local.composer.text, "*Remote* 日本語\nsecond line");
    let restored = restored.stored.unwrap();
    assert_eq!(restored.revision, row.revision);
    assert_eq!(restored.state.acknowledged(), Some(&updated));
    assert_eq!(restored.state.desired(), Some(&updated.snapshot));
    assert_eq!(restored.state.workflow(), &Workflow::Idle);
    let calls = AtomicUsize::new(0);
    assert!(matches!(
        f.owner
            .dispatch(row.revision, |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Outcome::Uncertain
            })
            .await,
        Err(Fault::Pending)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

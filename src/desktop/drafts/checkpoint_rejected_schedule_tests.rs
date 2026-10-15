use super::*;
use crate::drafts::dispatch::Outcome;
use serde_json::json;
use super_platinum_core::{
    ComposerState,
    drafts::store::{DraftStore, LocalWriter},
};

struct Fixture {
    path: std::path::PathBuf,
    editor: Arc<Editor>,
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn newer() -> LocalDraft {
    let mut draft = local("<@U123> newer unsupported text");
    draft
        .attachments
        .push(super_platinum_core::domain::ComposerAttachment {
            id: 91,
            path: "C:/isolated/not-uploaded.txt".into(),
            name: "not-uploaded.txt".into(),
            bytes: 42,
            uploading: false,
            upload_started: None,
            upload_cancel: None,
            upload_progress: None,
            preview_path: None,
        });
    draft
}
impl Fixture {
    async fn new(thread: bool) -> Self {
        let account = LocalWriter::default().id().to_owned();
        let path = std::env::temp_dir().join(format!("schedule-editor-{account}.sqlite"));
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: thread.then(|| "100.000001".into()),
        };
        let scope = Scope {
            account_id: account.clone(),
            key: key.clone(),
        };
        let mut original = local("Original");
        original.composer.reply_broadcast = thread;
        let remote = RemoteDraft {
            scope: scope.clone(),
            id: "Dr1".into(),
            client_msg_id: LocalWriter::default().id().into(),
            last_updated_ts: "100.000001".into(),
            snapshot: codec::encode_composer(&key, &original.composer).unwrap(),
        };
        let state = DraftSync::adopt_remote(&scope, remote).unwrap();
        let mut store = DraftStore::open(&path).unwrap();
        store.save([(key.clone(), original.clone())]).unwrap();
        assert!(matches!(
            store
                .commit_synced(&scope, None, &original, &state, None)
                .unwrap(),
            Commit::Applied(_)
        ));
        drop(store);
        let owner = Account::new(Backend::Isolated {
            account,
            path: path.clone(),
        })
        .owner(key);
        let editor = Editor::new(owner.clone(), owner.load().await.unwrap()).unwrap();
        Self { path, editor }
    }
    async fn lease(&self) -> (ScheduleLease, ScheduleVisit, RawSnapshot) {
        let visit = ScheduleVisit::new();
        let before = self.editor.snapshot().view.local;
        let lease = self
            .editor
            .prepare_reviewed_schedule(before, visit.clone())
            .await
            .unwrap();
        let mut raw = lease
            .base
            .stored
            .as_ref()
            .unwrap()
            .state
            .desired()
            .unwrap()
            .clone();
        raw.blocks = vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_quote","elements":[{"type":"text","text":"Dialog exact rich payload","style":{"bold":true,"italic":true}}]}]}),
        ];
        assert!(codec::decode(&lease.inventory.scope.key, &raw).is_err());
        (lease, visit, raw)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
async fn editor_gate_held(editor: &Editor) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while editor.gate.try_lock().is_ok() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn positive(operation: &Operation) -> Outcome {
    Outcome::Scheduled {
        draft: RemoteDraft {
            scope: operation.scope.clone(),
            id: operation.base.as_ref().unwrap().id.clone(),
            client_msg_id: operation.client_msg_id.clone(),
            last_updated_ts: "200.000001".into(),
            snapshot: operation.snapshot.clone().unwrap(),
        },
        post_at: 500,
    }
}

async fn rejected_fixture() -> Fixture {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    let frozen = f
        .editor
        .consume_reviewed_schedule(lease, visit, raw, 500)
        .await
        .unwrap();
    f.editor
        .owner
        .dispatch(frozen.revision, |_| async { Outcome::Rejected })
        .await
        .unwrap();
    f.editor.refresh_receipt().await.unwrap();
    f
}
fn replacement(lease: &ScheduleLease) -> RejectedScheduleChoice {
    let mut raw = lease
        .before()
        .stored
        .as_ref()
        .unwrap()
        .state
        .consumption()
        .unwrap()
        .snapshot
        .clone();
    raw.blocks[0]["elements"][0]["elements"][0]["text"] = json!("Explicit changed rich text");
    RejectedScheduleChoice::Replace {
        snapshot: raw,
        post_at: 900,
    }
}

#[tokio::test]
async fn retry_preserves_exact_rich_intent_and_unsupported_paired_local() {
    let f = rejected_fixture().await;
    f.editor.submit(newer());
    f.editor.flush().await.unwrap();
    let visit = ScheduleVisit::new();
    let lease = f
        .editor
        .prepare_rejected_schedule_review(visit.clone())
        .await
        .unwrap();
    let old = lease
        .before()
        .stored
        .as_ref()
        .unwrap()
        .state
        .consumption()
        .unwrap()
        .clone();
    let row = f
        .editor
        .apply_rejected_schedule_review(lease, visit, RejectedScheduleChoice::Retry, 200)
        .await
        .unwrap();
    assert_eq!(row.state.consumption(), Some(&old));
    assert!(same_local(
        &f.editor.owner.load().await.unwrap().local,
        &newer()
    ));
    assert!(row.state.pending().is_none());
    let next = f
        .editor
        .owner
        .dispatch(row.revision, |op| async move { positive(&op) })
        .await
        .unwrap();
    assert!(next.applied);
    f.editor.release().await.unwrap();
    assert!(same_local(
        &f.editor.owner.load().await.unwrap().local,
        &newer()
    ));
}

#[tokio::test]
async fn expired_rejection_can_change_schedule_without_clearing_current_local() {
    let f = rejected_fixture().await;
    f.editor.submit(newer());
    f.editor.flush().await.unwrap();
    let visit = ScheduleVisit::new();
    let lease = f
        .editor
        .prepare_rejected_schedule_review(visit.clone())
        .await
        .unwrap();
    let before = lease.before().stored.as_ref().unwrap().state.clone();
    assert!(matches!(
        f.editor
            .apply_rejected_schedule_review(
                lease.clone(),
                visit.clone(),
                RejectedScheduleChoice::Retry,
                600
            )
            .await,
        Err(Fault::Unsupported)
    ));
    assert!(f.editor.rejected_schedule_review_current(&lease, &visit));
    let change = replacement(&lease);
    let stored = f
        .editor
        .apply_rejected_schedule_review(lease, visit, change, 600)
        .await
        .unwrap();
    let next = stored.state.consumption().unwrap();
    assert_ne!(next.id, before.consumption().unwrap().id);
    assert_eq!(next.base, before.consumption().unwrap().base);
    assert_eq!(stored.state.local_revision(), before.local_revision());
    assert_eq!(stored.state.desired(), before.desired());
    assert!(same_local(
        &f.editor.owner.load().await.unwrap().local,
        &newer()
    ));
    assert!(stored.state.pending().is_none());
}

#[tokio::test]
async fn newer_typing_during_replacement_and_canceled_waiter_survive_paired_commit() {
    let f = rejected_fixture().await;
    let visit = ScheduleVisit::new();
    let lease = f
        .editor
        .prepare_rejected_schedule_review(visit.clone())
        .await
        .unwrap();
    let choice = replacement(&lease);
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (finish, proceed) = tokio::sync::oneshot::channel();
    let editor = f.editor.clone();
    let task = tokio::spawn(async move {
        editor
            .apply_rejected_schedule_using(
                lease,
                visit,
                choice,
                600,
                |backend, expected, choice, now| async move {
                    entered.send(()).unwrap();
                    proceed.await.unwrap();
                    backend
                        .review_rejected_schedule(expected, choice, now)
                        .await
                },
            )
            .await
    });
    waiting.await.unwrap();
    task.abort();
    f.editor.submit(newer());
    finish.send(()).unwrap();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::timeout(std::time::Duration::from_secs(5), f.editor.flush())
        .await
        .unwrap()
        .unwrap();
    let disk = f.editor.owner.load().await.unwrap();
    assert!(same_local(&disk.local, &newer()));
    assert!(disk.blocked.is_none());
    assert_eq!(
        disk.stored.unwrap().state.consumption().unwrap().kind,
        super_platinum_core::drafts::sync::ConsumeKind::Schedule { post_at: 900 }
    );
}

#[tokio::test]
async fn lost_replacement_receipt_preserves_new_intent_without_replaying_old_review() {
    let f = rejected_fixture().await;
    let visit = ScheduleVisit::new();
    let lease = f
        .editor
        .prepare_rejected_schedule_review(visit.clone())
        .await
        .unwrap();
    let old = lease
        .before()
        .stored
        .as_ref()
        .unwrap()
        .state
        .consumption()
        .unwrap()
        .clone();
    let choice = replacement(&lease);
    let stale = lease.clone();
    let result = f
        .editor
        .apply_rejected_schedule_using(
            lease,
            visit.clone(),
            choice.clone(),
            600,
            |backend, expected, choice, now| async move {
                assert!(matches!(
                    backend
                        .review_rejected_schedule(expected, choice, now)
                        .await
                        .unwrap(),
                    RejectedScheduleCommit::Applied { .. }
                ));
                Err(Fault::Storage)
            },
        )
        .await;
    assert!(matches!(result, Err(Fault::Storage)));
    assert!(matches!(
        f.editor
            .apply_rejected_schedule_review(stale, visit, choice, 600)
            .await,
        Err(Fault::Storage)
    ));
    let row = f.editor.owner.load().await.unwrap().stored.unwrap();
    assert_ne!(row.state.consumption().unwrap().id, old.id);
    assert!(row.state.pending().is_none());
    let restarted = Account::new(f.editor.owner.account.backend.clone())
        .owner(f.editor.owner.scope.key.clone());
    let editor = Editor::new(restarted.clone(), restarted.load().await.unwrap()).unwrap();
    assert!(matches!(
        editor
            .prepare_rejected_schedule_review(ScheduleVisit::new())
            .await,
        Err(Fault::Stale)
    ));
    assert_eq!(
        restarted.load().await.unwrap().stored.unwrap().state,
        row.state
    );
}

#[tokio::test]
async fn late_blocked_version_prevents_replacement_and_keeps_old_raw_intent() {
    let f = rejected_fixture().await;
    let visit = ScheduleVisit::new();
    let lease = f
        .editor
        .prepare_rejected_schedule_review(visit.clone())
        .await
        .unwrap();
    let old = lease
        .before()
        .stored
        .as_ref()
        .unwrap()
        .state
        .consumption()
        .unwrap()
        .clone();
    let choice = replacement(&lease);
    let path = f.path.clone();
    let result = f
        .editor
        .apply_rejected_schedule_using(
            lease,
            visit,
            choice,
            600,
            move |backend, expected, choice, now| async move {
                // A stale conditional handoff records a plain blocked edit
                // after the owner read without mutating the managed pair.
                assert!(
                    !DraftStore::open(&path)
                        .unwrap()
                        .save_local_if_unchanged(&expected.scope, &expected.local, &newer())
                        .unwrap()
                );
                let injected = DraftStore::open(&path)
                    .unwrap()
                    .read_local_review(&expected.scope)
                    .unwrap();
                assert!(same_local(&injected.local, &expected.local));
                let injected_stored = injected.stored.as_ref().unwrap();
                let expected_stored = expected.stored.as_ref().unwrap();
                assert_eq!(injected_stored.revision, expected_stored.revision);
                assert_eq!(injected_stored.state, expected_stored.state);
                assert!(same_local(&injected.blocked.unwrap().draft, &newer()));
                assert!(injected.alternates.is_empty());
                backend
                    .review_rejected_schedule(expected, choice, now)
                    .await
            },
        )
        .await;
    assert!(matches!(result, Err(Fault::Stale)));
    let disk = f.editor.owner.load().await.unwrap();
    assert_eq!(disk.stored.unwrap().state.consumption(), Some(&old));
    assert!(same_local(&disk.blocked.unwrap().draft, &newer()));
}

#[tokio::test]
async fn uncertain_or_foreign_editor_cannot_offer_rejected_retry_or_change() {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    let frozen = f
        .editor
        .consume_reviewed_schedule(lease, visit, raw, 500)
        .await
        .unwrap();
    f.editor
        .owner
        .dispatch(frozen.revision, |_| async { Outcome::Uncertain })
        .await
        .unwrap();
    f.editor.refresh_receipt().await.unwrap();
    assert!(matches!(
        f.editor
            .prepare_rejected_schedule_review(ScheduleVisit::new())
            .await,
        Err(Fault::Stale)
    ));

    let f = rejected_fixture().await;
    let visit = ScheduleVisit::new();
    let lease = f
        .editor
        .prepare_rejected_schedule_review(visit.clone())
        .await
        .unwrap();
    let other = Editor::new(f.editor.owner.clone(), f.editor.owner.load().await.unwrap()).unwrap();
    assert!(matches!(
        other
            .apply_rejected_schedule_review(lease, visit, RejectedScheduleChoice::Retry, 200)
            .await,
        Err(Fault::Stale)
    ));
}

#[path = "rejected_schedule_guard_tests.rs"]
mod guard_tests;

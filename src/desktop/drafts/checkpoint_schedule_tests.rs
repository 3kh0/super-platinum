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

#[tokio::test]
async fn channel_and_thread_freeze_exact_dialog_rich_payload_through_shared_owner() {
    for thread in [false, true] {
        let f = Fixture::new(thread).await;
        let (lease, visit, raw) = f.lease().await;
        let expected = lease.base.stored.as_ref().unwrap().state.clone();
        let replay = lease.clone();
        let frozen = f
            .editor
            .consume_reviewed_schedule(lease, visit.clone(), raw.clone(), 500)
            .await
            .unwrap();
        let intent = frozen.state.consumption().unwrap();
        assert_eq!(intent.snapshot, raw);
        assert_eq!(intent.base.as_ref(), expected.acknowledged());
        assert_eq!(intent.local_revision, expected.local_revision());
        assert!(!f.editor.reviewed_schedule_current(&replay, &visit));
        let disk = f.editor.owner.load().await.unwrap();
        assert!(same_local(&disk.local, &LocalDraft::default()));
        assert_eq!(
            disk.stored.as_ref().unwrap().state.consumption(),
            Some(intent)
        );
        let writer = f.editor.clone();
        let receipt = f
            .editor
            .owner
            .dispatch(frozen.revision, move |operation| async move {
                assert_eq!(operation.snapshot.as_ref(), Some(&raw));
                writer.submit(newer());
                let paired = writer.flush().await.unwrap();
                assert!(same_local(&paired.local, &newer()));
                assert!(paired.blocked.is_none());
                positive(&operation)
            })
            .await
            .unwrap();
        assert!(receipt.applied);
        f.editor.release().await.unwrap();
        let disk = f.editor.owner.load().await.unwrap();
        assert!(disk.stored.is_none());
        assert!(same_local(&disk.local, &newer()));
    }
}

#[tokio::test]
async fn serial_edit_revert_replacement_dialog_and_writer_changes_reject_old_lease() {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    f.editor.submit(local("Changed"));
    f.editor.flush().await.unwrap();
    f.editor.submit(local("Original"));
    f.editor.flush().await.unwrap();
    assert!(same_local(
        &f.editor.snapshot().view.local,
        &lease.base.local
    ));
    assert!(matches!(
        f.editor
            .consume_reviewed_schedule(lease, visit, raw, 500)
            .await,
        Err(Fault::Stale)
    ));

    let (lease, visit, raw) = f.lease().await;
    assert!(matches!(
        f.editor
            .consume_reviewed_schedule(lease.clone(), ScheduleVisit::new(), raw.clone(), 500)
            .await,
        Err(Fault::Stale)
    ));
    visit.invalidate();
    assert!(matches!(
        f.editor
            .consume_reviewed_schedule(lease, visit, raw, 500)
            .await,
        Err(Fault::Stale)
    ));

    let (lease, visit, raw) = f.lease().await;
    f.editor.state.lock().unwrap().local_writer = Default::default();
    assert!(matches!(
        f.editor
            .consume_reviewed_schedule(lease, visit, raw, 500)
            .await,
        Err(Fault::Stale)
    ));
    assert!(
        f.editor
            .owner
            .load()
            .await
            .unwrap()
            .stored
            .unwrap()
            .state
            .consumption()
            .is_none()
    );
}

#[tokio::test]
async fn newer_input_during_owner_read_fails_admission_and_is_preserved() {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    let gate = f.editor.owner.gate.lock().await;
    let editor = f.editor.clone();
    let task = tokio::spawn(async move {
        editor
            .consume_reviewed_schedule(lease, visit, raw, 500)
            .await
    });
    editor_gate_held(&f.editor).await;
    f.editor.submit(local("Newer before admission"));
    drop(gate);
    assert!(matches!(task.await.unwrap(), Err(Fault::Stale)));
    f.editor.flush().await.unwrap();
    let disk = f.editor.owner.load().await.unwrap();
    assert_eq!(disk.local.composer.text, "Newer before admission");
    assert!(disk.stored.unwrap().state.consumption().is_none());
}

#[tokio::test]
async fn typing_during_paired_commit_and_canceled_waiter_keep_same_editor_base() {
    for cancel in [false, true] {
        let f = Fixture::new(false).await;
        let (lease, visit, raw) = f.lease().await;
        let expected = raw.clone();
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let (finish, proceed) = tokio::sync::oneshot::channel();
        let editor = f.editor.clone();
        let task = tokio::spawn(async move {
            editor
                .consume_reviewed_schedule_using(
                    lease,
                    visit,
                    raw,
                    500,
                    |backend, review, raw, time| async move {
                        entered.send(()).unwrap();
                        proceed.await.unwrap();
                        backend.commit_reviewed_schedule(review, raw, time).await
                    },
                )
                .await
        });
        waiting.await.unwrap();
        assert!(f.editor.state.lock().unwrap().settling);
        if cancel {
            task.abort();
        }
        f.editor.submit(newer());
        finish.send(()).unwrap();
        let result = task.await;
        if cancel {
            assert!(result.unwrap_err().is_cancelled());
        } else {
            result.unwrap().unwrap();
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), f.editor.flush())
            .await
            .unwrap()
            .unwrap();
        let disk = f.editor.owner.load().await.unwrap();
        assert!(same_local(&disk.local, &newer()));
        assert!(disk.blocked.is_none());
        let stored = disk.stored.unwrap();
        assert_eq!(stored.state.consumption().unwrap().snapshot, expected);
        assert_eq!(f.editor.snapshot().view.stored.unwrap().state, stored.state);
        assert!(f.editor.snapshot().error.is_none());
    }
}

#[tokio::test]
async fn commit_fault_parks_editor_and_prevents_reusing_claimed_dialog() {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    let saved_lease = lease.clone();
    let saved_visit = visit.clone();
    let saved_raw = raw.clone();
    let writer = f.editor.clone();
    let result = f
        .editor
        .consume_reviewed_schedule_using(lease, visit, raw, 500, move |_, _, _, _| async move {
            writer.submit(newer());
            Err(Fault::Storage)
        })
        .await;
    assert!(matches!(result, Err(Fault::Storage)));
    assert!(matches!(f.editor.flush().await, Err(Fault::Storage)));
    assert!(
        !f.editor
            .reviewed_schedule_current(&saved_lease, &saved_visit)
    );
    assert!(matches!(
        f.editor
            .consume_reviewed_schedule(saved_lease, saved_visit, saved_raw, 500)
            .await,
        Err(Fault::Storage)
    ));
    let disk = f.editor.owner.load().await.unwrap();
    assert_eq!(disk.local.composer.text, "Original");
    assert!(same_local(&disk.blocked.unwrap().draft, &newer()));
    assert!(disk.stored.unwrap().state.consumption().is_none());
}

#[tokio::test]
async fn actual_sqlite_failure_before_admission_cannot_clear_or_create_intent() {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    let backup = f.path.with_extension("backup");
    std::fs::rename(&f.path, &backup).unwrap();
    std::fs::create_dir(&f.path).unwrap();
    let result = f
        .editor
        .consume_reviewed_schedule(lease, visit, raw, 500)
        .await;
    std::fs::remove_dir(&f.path).unwrap();
    std::fs::rename(&backup, &f.path).unwrap();
    assert!(matches!(result, Err(Fault::Storage)));
    let disk = f.editor.owner.load().await.unwrap();
    assert_eq!(disk.local.composer.text, "Original");
    assert!(disk.stored.unwrap().state.consumption().is_none());
}

#[tokio::test]
async fn lost_commit_receipt_retains_exact_intent_and_newer_alternate_without_replay() {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    let expected = raw.clone();
    let writer = f.editor.clone();
    let result = f
        .editor
        .consume_reviewed_schedule_using(
            lease,
            visit,
            raw,
            500,
            move |backend, review, raw, time| async move {
                assert!(matches!(
                    backend
                        .commit_reviewed_schedule(review, raw, time)
                        .await
                        .unwrap(),
                    ReviewedScheduleCommit::Applied(_)
                ));
                writer.submit(newer());
                Err(Fault::Storage)
            },
        )
        .await;
    assert!(matches!(result, Err(Fault::Storage)));
    assert!(matches!(f.editor.flush().await, Err(Fault::Storage)));
    let original = f.editor.owner.load().await.unwrap();
    assert!(same_local(&original.local, &LocalDraft::default()));
    assert!(same_local(
        &original.blocked.as_ref().unwrap().draft,
        &newer()
    ));
    assert_eq!(
        original
            .stored
            .as_ref()
            .unwrap()
            .state
            .consumption()
            .unwrap()
            .snapshot,
        expected
    );
    assert!(original.stored.as_ref().unwrap().state.pending().is_none());
    let restarted = Account::new(f.editor.owner.account.backend.clone())
        .owner(f.editor.owner.scope.key.clone());
    let reloaded = restarted.load().await.unwrap();
    let recovered = Editor::new(restarted, reloaded).unwrap();
    assert!(matches!(
        recovered.flush().await,
        Err(Fault::AlternateEdits)
    ));
    let visit = ScheduleVisit::new();
    assert!(matches!(
        recovered
            .prepare_reviewed_schedule(LocalDraft::default(), visit)
            .await,
        Err(Fault::AlternateEdits)
    ));
    assert_eq!(
        recovered.snapshot().view.stored.unwrap().state,
        original.stored.unwrap().state
    );
}

#[tokio::test]
async fn full_inventory_transaction_rejects_blocked_or_alternate_added_after_owner_read() {
    for owned in [false, true] {
        let f = Fixture::new(false).await;
        let (lease, visit, raw) = f.lease().await;
        let path = f.path.clone();
        let result = f
            .editor
            .consume_reviewed_schedule_using(
                lease,
                visit,
                raw,
                500,
                move |backend, review, raw, time| async move {
                    // Models a different writer after owner read but before SQLite CAS.
                    if owned {
                        backend
                            .preserve_alternate(
                                review.scope.key.clone(),
                                newer(),
                                LocalWriter::default(),
                                1,
                            )
                            .await
                            .unwrap();
                    } else {
                        // A stale conditional local handoff preserves a plain
                        // blocked row when the destination is already managed.
                        assert!(
                            !DraftStore::open(&path)
                                .unwrap()
                                .save_local_if_unchanged(&review.scope, &review.local, &newer())
                                .unwrap()
                        );
                    }
                    let injected = DraftStore::open(&path)
                        .unwrap()
                        .read_local_review(&review.scope)
                        .unwrap();
                    assert!(same_local(&injected.local, &review.local));
                    let injected_stored = injected.stored.as_ref().unwrap();
                    let reviewed_stored = review.stored.as_ref().unwrap();
                    assert_eq!(injected_stored.revision, reviewed_stored.revision);
                    assert_eq!(injected_stored.state, reviewed_stored.state);
                    assert!(same_local(&injected.blocked.unwrap().draft, &newer()));
                    assert_eq!(injected.alternates.len(), usize::from(owned));
                    backend.commit_reviewed_schedule(review, raw, time).await
                },
            )
            .await;
        assert!(matches!(result, Err(Fault::Stale)));
        let disk = f.editor.owner.load().await.unwrap();
        assert_eq!(disk.local.composer.text, "Original");
        assert!(same_local(&disk.blocked.unwrap().draft, &newer()));
        assert!(disk.stored.unwrap().state.consumption().is_none());
    }
}

#[tokio::test]
async fn another_owner_state_change_without_local_change_rejects_whole_view() {
    let f = Fixture::new(false).await;
    let (lease, visit, raw) = f.lease().await;
    let row = f.editor.owner.load().await.unwrap().stored.unwrap();
    let mut remote = row.state.acknowledged().unwrap().clone();
    remote.last_updated_ts = "101.000001".into();
    f.editor.owner.observe(remote).await.unwrap();
    assert!(matches!(
        f.editor
            .consume_reviewed_schedule(lease, visit, raw, 500)
            .await,
        Err(Fault::Stale)
    ));
    let disk = f.editor.owner.load().await.unwrap();
    assert_eq!(disk.local.composer.text, "Original");
    assert!(disk.stored.unwrap().state.consumption().is_none());
}

#[tokio::test]
async fn released_editor_and_reenrolled_identical_row_cannot_reuse_old_dialog() {
    let f = Fixture::new(false).await;
    let (old, old_visit, raw) = f.lease().await;
    let base = old
        .inventory
        .stored
        .as_ref()
        .unwrap()
        .state
        .acknowledged()
        .unwrap()
        .clone();
    let replacement =
        Editor::new(f.editor.owner.clone(), f.editor.owner.load().await.unwrap()).unwrap();
    assert!(matches!(
        replacement
            .consume_reviewed_schedule(old.clone(), old_visit.clone(), raw.clone(), 500)
            .await,
        Err(Fault::Stale)
    ));
    let (lease, visit, _) = f.lease().await;
    let frozen = f
        .editor
        .consume_reviewed_schedule(lease, visit, raw.clone(), 500)
        .await
        .unwrap();
    f.editor
        .owner
        .dispatch(frozen.revision, |op| async move { positive(&op) })
        .await
        .unwrap();
    f.editor.release().await.unwrap();
    DraftStore::open(&f.path)
        .unwrap()
        .save([(base.scope.key.clone(), local("Original"))])
        .unwrap();
    let row = f
        .editor
        .owner
        .enroll(local("Original"), Some(base))
        .await
        .unwrap();
    assert_eq!(row.state, old.inventory.stored.as_ref().unwrap().state);
    assert_eq!(
        row.revision,
        old.inventory.stored.as_ref().unwrap().revision
    );
    assert!(matches!(
        f.editor
            .consume_reviewed_schedule(old, old_visit, raw, 500)
            .await,
        Err(Fault::Stale)
    ));
    assert!(
        f.editor
            .owner
            .load()
            .await
            .unwrap()
            .stored
            .unwrap()
            .state
            .consumption()
            .is_none()
    );
}

#[path = "schedule_guard_tests.rs"]
mod guard_tests;
#[path = "schedule_pipeline_tests.rs"]
mod pipeline_tests;

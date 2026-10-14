use super::*;
use super_platinum_core::{ComposerState, drafts::store::DraftStore};

struct Fixture {
    path: std::path::PathBuf,
    account: Arc<Account>,
    owner: Owner,
}
impl Fixture {
    async fn new(account: &str) -> (Self, Arc<Editor>) {
        let id = super_platinum_core::slack::api::ForwardMessage::new(
            "C1".into(),
            "100.000001".into(),
            "C2".into(),
            vec![],
        )
        .client_msg_id;
        let path = std::env::temp_dir().join(format!("draft-editor-{id}.sqlite"));
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
            account: account.into(),
            path: path.clone(),
        });
        let owner = account.owner(key);
        owner.enroll(local("Original"), None).await.unwrap();
        let editor = Editor::new(owner.clone(), owner.load().await.unwrap()).unwrap();
        (
            Self {
                path,
                account,
                owner,
            },
            editor,
        )
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
const ACCOUNT: &str = "00000000-0000-4000-8000-000000000091";

#[tokio::test]
async fn receipt_refresh_preserves_pending_serial_writer_and_subsequent_typing() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let revision = editor.snapshot().view.stored.unwrap().revision;
    f.owner
        .dispatch(revision, |op| async move {
            super::super::dispatch::Outcome::Draft(RemoteDraft {
                scope: op.scope,
                client_msg_id: op.client_msg_id,
                id: "Dr1".into(),
                last_updated_ts: "100.000001".into(),
                snapshot: op.snapshot.unwrap(),
            })
        })
        .await
        .unwrap();
    // Hold the editor gate, admit refresh first, then submit while it awaits.
    let gate = editor.gate.lock().await;
    let refreshing = editor.clone();
    let refresh = tokio::spawn(async move { refreshing.refresh_receipt().await });
    tokio::task::yield_now().await;
    editor.submit(local("New pending input"));
    let serial = editor.snapshot().serial;
    drop(gate);
    refresh.await.unwrap().unwrap();
    editor.flush().await.unwrap();
    assert_eq!(editor.snapshot().serial, serial);
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "New pending input"
    );
    assert!(!editor.state.lock().unwrap().released);
    editor.submit(local("Still the same managed writer"));
    editor.flush().await.unwrap();
    let saved = f.owner.load().await.unwrap();
    assert!(saved.stored.is_some());
    assert_eq!(saved.local.composer.text, "Still the same managed writer");
}

#[tokio::test]
async fn receipt_refresh_refuses_other_local_edits_without_replacing_editor_input() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let revision = editor.snapshot().view.stored.unwrap().revision;
    f.owner
        .checkpoint(revision, local("Another writer's content"))
        .await
        .unwrap();
    assert!(matches!(editor.refresh_receipt().await, Err(Fault::Stale)));
    assert_eq!(editor.snapshot().view.local.composer.text, "Original");
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Another writer's content"
    );
    assert!(!editor.state.lock().unwrap().released);
}

#[tokio::test]
async fn canceled_consume_waiter_does_not_leave_new_typing_on_an_obsolete_base() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let owner_gate = f.owner.gate.lock().await;
    let sending = editor.clone();
    let waiter =
        tokio::spawn(async move { sending.consume(local("Original"), ConsumeKind::Send).await });
    while editor.gate.try_lock().is_ok() {
        tokio::task::yield_now().await;
    }
    waiter.abort();
    let _ = waiter.await;
    editor.submit(local("Typing after the view was canceled"));
    drop(owner_gate);
    editor.flush().await.unwrap();
    let saved = f.owner.load().await.unwrap();
    assert_eq!(
        saved.local.composer.text,
        "Typing after the view was canceled"
    );
    assert!(saved.stored.unwrap().state.consumption().is_some());
    assert!(editor.snapshot().error.is_none());
}

#[tokio::test]
async fn disk_failure_keeps_input_then_persists_alternate_without_resuming_writer() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let backup = f.path.with_extension("backup");
    std::fs::rename(&f.path, &backup).unwrap();
    std::fs::create_dir(&f.path).unwrap();
    editor.submit(local("Input while disk is unavailable"));
    assert!(matches!(editor.flush().await, Err(Fault::Storage)));
    assert_eq!(
        editor.snapshot().pending.unwrap().composer.text,
        "Input while disk is unavailable"
    );
    std::fs::remove_dir(&f.path).unwrap();
    std::fs::rename(&backup, &f.path).unwrap();
    editor.submit(local("Latest retained input"));
    assert!(matches!(editor.flush().await, Err(Fault::Storage)));
    let saved = f.owner.load().await.unwrap();
    assert_eq!(saved.local.composer.text, "Original");
    assert_eq!(
        saved.blocked.unwrap().draft.composer.text,
        "Latest retained input"
    );
}

#[tokio::test]
async fn coalesced_submit_starts_without_tick_and_survives_dropped_view() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let gate = editor.gate.lock().await;
    editor.submit(local("first"));
    editor.submit(local("latest 日本語 🚀"));
    let observer = editor.clone();
    drop(gate);
    drop(editor);
    observer.flush().await.unwrap();
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "latest 日本語 🚀"
    );
    assert!(!observer.snapshot().unsaved);
}

#[tokio::test]
async fn receipt_only_storage_revision_can_rebase_but_other_local_input_is_preserved_as_alternate()
{
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let revision = editor.snapshot().view.stored.unwrap().revision;
    f.owner
        .dispatch(revision, |op| async move {
            super::super::dispatch::Outcome::Draft(RemoteDraft {
                scope: op.scope.clone(),
                id: "Dr1".into(),
                client_msg_id: op.client_msg_id.clone(),
                last_updated_ts: "1719800001000".into(),
                snapshot: op.snapshot.unwrap(),
            })
        })
        .await
        .unwrap();
    editor.submit(local("my next edit"));
    editor.flush().await.unwrap();
    let current = f.owner.load().await.unwrap();
    f.owner
        .checkpoint(current.stored.unwrap().revision, local("another editor"))
        .await
        .unwrap();
    editor.submit(local("do not lose my input"));
    assert!(matches!(editor.flush().await, Err(Fault::Stale)));
    let saved = f.owner.load().await.unwrap();
    assert_eq!(saved.local.composer.text, "another editor");
    assert_eq!(
        saved.blocked.unwrap().draft.composer.text,
        "do not lose my input"
    );
    assert!(editor.snapshot().unsaved);
    assert_eq!(
        editor.snapshot().view.blocked.unwrap().draft.composer.text,
        "do not lose my input"
    );
    // New typing remains recoverable, but cannot silently reopen this writer.
    editor.submit(local("latest blocked input"));
    assert!(editor.flush().await.is_err());
    let saved = f.owner.load().await.unwrap();
    assert_eq!(saved.local.composer.text, "another editor");
    assert_eq!(
        saved.blocked.unwrap().draft.composer.text,
        "do not lose my input"
    );
    let loaded = f.account.backend.load(LoadMode::Current).await.unwrap();
    assert_eq!(
        loaded.local_alternates[&f.owner.scope.key][0]
            .draft
            .composer
            .text,
        "latest blocked input"
    );
}

#[tokio::test]
async fn latest_released_refused_inputs_survive_both_editors_and_queue_being_dropped() {
    let (f, _) = Fixture::new(ACCOUNT).await;
    let key = f.owner.scope.key.clone();
    let mut queue = Queue::default();
    let a = queue
        .editor(&f.account, 1, key.clone(), f.owner.load().await.unwrap())
        .unwrap();
    a.release().await.unwrap();
    f.owner.enroll(local("Original"), None).await.unwrap();
    let b = queue
        .editor(&f.account, 1, key.clone(), f.owner.load().await.unwrap())
        .unwrap();
    b.release().await.unwrap();
    DraftStore::open(&f.path)
        .unwrap()
        .save([(key.clone(), local("Foreign newer input"))])
        .unwrap();
    for (editor, text) in [
        (&a, "A first"),
        (&b, "B first"),
        (&a, "A latest"),
        (&b, "B latest"),
    ] {
        editor.submit(local(text));
        assert!(editor.flush().await.is_err());
    }
    drop(queue);
    drop(a);
    drop(b);
    let loaded = DraftStore::open(&f.path)
        .unwrap()
        .load_synced(ACCOUNT, LoadMode::AfterRestart)
        .unwrap();
    assert_eq!(
        loaded
            .local
            .iter()
            .find(|(k, _)| k == &key)
            .unwrap()
            .1
            .composer
            .text,
        "Foreign newer input"
    );
    let rows = &loaded.local_alternates[&key];
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|row| row.draft.composer.text == "A latest"));
    assert!(rows.iter().any(|row| row.draft.composer.text == "B latest"));
}

#[tokio::test]
async fn independent_managed_fallbacks_cannot_erase_each_others_latest_input() {
    let (f, a) = Fixture::new(ACCOUNT).await;
    let b = Editor::new(f.owner.clone(), f.owner.load().await.unwrap()).unwrap();
    let revision = f.owner.load().await.unwrap().stored.unwrap().revision;
    f.owner
        .checkpoint(revision, local("Foreign paired input"))
        .await
        .unwrap();
    for (editor, text) in [
        (&a, "A first"),
        (&b, "B first"),
        (&a, "A latest"),
        (&b, "B latest"),
    ] {
        editor.submit(local(text));
        assert!(editor.flush().await.is_err());
    }
    drop(a);
    drop(b);
    let loaded = DraftStore::open(&f.path)
        .unwrap()
        .load_synced(ACCOUNT, LoadMode::AfterRestart)
        .unwrap();
    let rows = &loaded.local_alternates[&f.owner.scope.key];
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|row| row.draft.composer.text == "A latest"));
    assert!(rows.iter().any(|row| row.draft.composer.text == "B latest"));
    assert_eq!(
        loaded.blocked_local[&f.owner.scope.key].draft.composer.text,
        "A first"
    );
}

#[tokio::test]
async fn consume_freezes_expected_local_and_typing_during_commit_survives() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let owner_gate = f.owner.gate.lock().await;
    let sending = editor.clone();
    let consume =
        tokio::spawn(async move { sending.consume(local("Original"), ConsumeKind::Send).await });
    // consume owns its serial gate and is waiting for the owner/disk boundary.
    while editor.gate.try_lock().is_ok() {
        tokio::task::yield_now().await;
    }
    editor.submit(local("New typing while the send freezes"));
    drop(owner_gate);
    let frozen = consume.await.unwrap().unwrap();
    assert_eq!(
        frozen.state.consumption().unwrap().snapshot,
        codec::encode(&f.owner.scope.key, "Original").unwrap()
    );
    editor.flush().await.unwrap();
    let saved = f.owner.load().await.unwrap();
    assert_eq!(
        saved.local.composer.text,
        "New typing while the send freezes"
    );
    assert!(saved.stored.unwrap().state.consumption().is_some());
}

#[tokio::test]
async fn consume_rejects_stale_expected_local_after_flushing_latest_input() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    editor.submit(local("newer"));
    assert!(matches!(
        editor.consume(local("Original"), ConsumeKind::Send).await,
        Err(Fault::Stale)
    ));
    let saved = f.owner.load().await.unwrap();
    assert_eq!(saved.local.composer.text, "newer");
    assert!(saved.stored.unwrap().state.consumption().is_none());
}

#[tokio::test]
async fn queue_account_switch_does_not_cancel_original_detached_writer() {
    let (first, _) = Fixture::new(ACCOUNT).await;
    let (second, _) = Fixture::new("00000000-0000-4000-8000-000000000092").await;
    let mut queue = Queue::default();
    let editor = queue
        .editor(
            &first.account,
            1,
            first.owner.scope.key.clone(),
            first.owner.load().await.unwrap(),
        )
        .unwrap();
    let gate = first.owner.gate.lock().await;
    editor.submit(local("first account private edit"));
    let other = queue
        .editor(
            &second.account,
            2,
            second.owner.scope.key.clone(),
            second.owner.load().await.unwrap(),
        )
        .unwrap();
    other.submit(local("second account input"));
    assert!(queue.snapshots(ACCOUNT, 1).is_empty());
    assert_eq!(
        queue.snapshots(second.account.backend.account(), 2).len(),
        1
    );
    drop(queue);
    drop(gate);
    editor.flush().await.unwrap();
    other.flush().await.unwrap();
    assert_eq!(
        first.owner.load().await.unwrap().local.composer.text,
        "first account private edit"
    );
    assert_eq!(
        second.owner.load().await.unwrap().local.composer.text,
        "second account input"
    );
}

#[tokio::test]
async fn unsupported_input_is_durable_alternate_and_wrong_account_cannot_write_it() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    editor.submit(local("<@unsupported-mention>"));
    assert!(matches!(editor.flush().await, Err(Fault::Unsupported)));
    let saved = f.owner.load().await.unwrap();
    assert_eq!(saved.local.composer.text, "Original");
    assert_eq!(
        saved.blocked.unwrap().draft.composer.text,
        "<@unsupported-mention>"
    );
    let mut wrong = f.owner.scope.clone();
    wrong.account_id = "00000000-0000-4000-8000-000000000092".into();
    assert!(
        DraftStore::open(&f.path)
            .unwrap()
            .save_local_alternate(&wrong, local("wrong account"))
            .is_err()
    );
    assert_eq!(
        f.owner
            .load()
            .await
            .unwrap()
            .blocked
            .unwrap()
            .draft
            .composer
            .text,
        "<@unsupported-mention>"
    );
}

#[tokio::test]
async fn release_flushes_input_and_continues_ordinary_edits_without_managed_encoder() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    editor.submit(local("Before release"));
    editor.release().await.unwrap();
    assert!(editor.snapshot().view.stored.is_none());
    editor.submit(local("<@U123> after release"));
    let view = editor.flush().await.unwrap();
    assert!(view.stored.is_none());
    assert_eq!(view.local.composer.text, "<@U123> after release");
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "<@U123> after release"
    );
}

#[tokio::test]
async fn canceled_release_waiter_transfers_input_submitted_during_handoff() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let owner_gate = f.owner.gate.lock().await;
    let releasing = editor.clone();
    let waiter = tokio::spawn(async move { releasing.release().await });
    while editor.gate.try_lock().is_ok() {
        tokio::task::yield_now().await;
    }
    editor.submit(local("New input during release"));
    waiter.abort();
    let _ = waiter.await;
    drop(owner_gate);
    let view = editor.flush().await.unwrap();
    assert!(view.stored.is_none());
    assert_eq!(view.local.composer.text, "New input during release");
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "New input during release"
    );
}

#[tokio::test]
async fn released_input_is_ordered_even_after_queue_removal_and_new_visit() {
    let (f, _) = Fixture::new(ACCOUNT).await;
    let mut queue = Queue::default();
    let key = f.owner.scope.key.clone();
    let editor = queue
        .editor(&f.account, 1, key.clone(), f.owner.load().await.unwrap())
        .unwrap();
    editor.release().await.unwrap();
    let gate = editor.gate.lock().await;
    editor.submit(local("Old visit input"));
    queue.remove(ACCOUNT, 1, &key);
    // Both visits enqueue in event order. Disk work is asynchronous even in
    // isolated fixtures; the detached observer must never submit it again.
    f.account
        .backend
        .queue_local(
            key.clone(),
            local("Old visit input"),
            local("New visit input"),
            Default::default(),
            1,
        )
        .finish()
        .await
        .unwrap();
    drop(gate);
    editor.flush().await.unwrap();
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "New visit input"
    );
    editor.submit(local("Late old visit input"));
    assert!(matches!(editor.flush().await, Err(Fault::Stale)));
    assert_eq!(
        editor.snapshot().pending.unwrap().composer.text,
        "Late old visit input"
    );
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "New visit input"
    );
}

#[tokio::test]
async fn release_failure_preserves_queued_input_and_managed_ownership() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    editor
        .consume(local("Original"), ConsumeKind::Send)
        .await
        .unwrap();
    assert!(matches!(editor.release().await, Err(Fault::Pending)));
    editor.submit(local("New input after refused release"));
    let view = editor.flush().await.unwrap();
    assert!(view.stored.unwrap().state.consumption().is_some());
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "New input after refused release"
    );
}

#[tokio::test]
async fn reenrollment_replaces_passthrough_and_rejects_detached_old_writer() {
    let (f, _) = Fixture::new(ACCOUNT).await;
    let mut queue = Queue::default();
    let key = f.owner.scope.key.clone();
    let old = queue
        .editor(&f.account, 1, key.clone(), f.owner.load().await.unwrap())
        .unwrap();
    old.release().await.unwrap();
    f.owner.enroll(local("Original"), None).await.unwrap();
    let new = queue
        .editor(&f.account, 1, key, f.owner.load().await.unwrap())
        .unwrap();
    assert!(!Arc::ptr_eq(&old, &new));
    old.submit(local("Obsolete passthrough input"));
    assert!(matches!(old.flush().await, Err(Fault::Stale)));
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Original"
    );
    assert_eq!(
        f.owner
            .load()
            .await
            .unwrap()
            .blocked
            .unwrap()
            .draft
            .composer
            .text,
        "Obsolete passthrough input"
    );
}

#[tokio::test]
async fn another_visit_edit_during_release_is_never_overwritten_by_handoff() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let owner_gate = f.owner.gate.lock().await;
    let releasing = editor.clone();
    let waiter = tokio::spawn(async move { releasing.release().await });
    while editor.gate.try_lock().is_ok() {
        tokio::task::yield_now().await;
    }
    editor.submit(local("Old visit queued input"));
    // A separate account visit has its own owner gate, as in a restarted UI.
    let account = Account::new(Backend::Isolated {
        account: ACCOUNT.into(),
        path: f.path.clone(),
    });
    let other = account.owner(f.owner.scope.key.clone());
    let revision = other.load().await.unwrap().stored.unwrap().revision;
    other
        .checkpoint(revision, local("New visit authoritative input"))
        .await
        .unwrap();
    drop(owner_gate);
    assert!(matches!(waiter.await.unwrap(), Err(Fault::Stale)));
    assert!(matches!(editor.flush().await, Err(Fault::Stale)));
    let saved = f.owner.load().await.unwrap();
    assert!(saved.stored.is_some());
    assert_eq!(saved.local.composer.text, "New visit authoritative input");
    assert_eq!(
        saved.blocked.unwrap().draft.composer.text,
        "Old visit queued input"
    );
    assert_eq!(
        editor.snapshot().pending.unwrap().composer.text,
        "Old visit queued input"
    );
}

fn unsupported_local(with_file: bool) -> LocalDraft {
    let mut draft = local("<@U123> newer mention 日本語");
    if with_file {
        draft
            .attachments
            .push(super_platinum_core::domain::ComposerAttachment {
                id: 91,
                path: "/tmp/local-only-attachment.txt".into(),
                name: "local-only-attachment.txt".into(),
                bytes: 42,
                uploading: false,
                upload_started: None,
                upload_cancel: None,
                upload_progress: None,
                preview_path: None,
            });
    }
    draft
}

#[tokio::test]
async fn direct_consume_preserves_unsupported_typing_during_commit_and_rejects_restore_over_it() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let owner_gate = f.owner.gate.lock().await;
    let sending = editor.clone();
    let consume =
        tokio::spawn(async move { sending.consume(local("Original"), ConsumeKind::Send).await });
    while editor.gate.try_lock().is_ok() {
        tokio::task::yield_now().await;
    }
    let newer = unsupported_local(true);
    editor.submit(newer.clone());
    drop(owner_gate);
    let frozen = consume.await.unwrap().unwrap();
    let intent = frozen.state.consumption().unwrap();
    let saved = editor.flush().await.unwrap();
    assert!(same_local(&saved.local, &newer));
    assert!(saved.blocked.is_none());
    let mut state = saved.stored.unwrap().state;
    assert_eq!(state.consumption(), Some(intent));
    assert!(state.desired().is_none());
    assert!(state.local_revision() > intent.local_revision + 1);
    assert!(matches!(
        state.cancel_consumption(&f.owner.scope, &intent.id),
        Err(super_platinum_core::drafts::sync::ConsumeError::NewerEdits)
    ));
}

#[tokio::test]
async fn direct_send_preserves_files_and_mentions_during_request_and_after_receipt_then_releases() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let frozen = editor
        .consume(local("Original"), ConsumeKind::Send)
        .await
        .unwrap();
    // The general owner API remains strict even beside a direct send.
    assert!(matches!(
        f.owner
            .checkpoint(frozen.revision, unsupported_local(true))
            .await,
        Err(Fault::Unsupported)
    ));
    let writer = editor.clone();
    let owner = f.owner.clone();
    let intent = frozen.state.consumption().unwrap().clone();
    let result = f
        .owner
        .dispatch(frozen.revision, move |operation| async move {
            let expected = operation.clone();
            writer.submit(unsupported_local(true));
            let view = writer.flush().await.unwrap();
            assert!(same_local(&view.local, &unsupported_local(true)));
            let stored = view.stored.unwrap();
            assert_eq!(stored.state.pending(), Some(&expected));
            assert_eq!(stored.state.consumption(), Some(&intent));
            assert_eq!(
                owner.load().await.unwrap().stored.unwrap().state.pending(),
                Some(&expected)
            );
            super::super::dispatch::Outcome::Sent {
                client_msg_id: operation.client_msg_id,
                draft_id: None,
                ts: "1800000000.000001".into(),
            }
        })
        .await
        .unwrap();
    assert!(result.applied);
    let after_receipt = unsupported_local(false);
    editor.submit(after_receipt.clone());
    editor.flush().await.unwrap();
    editor.release().await.unwrap();
    let saved = f.owner.load().await.unwrap();
    assert!(saved.stored.is_none());
    assert!(same_local(&saved.local, &after_receipt));
    assert!(saved.blocked.is_none());
    // Arbitrary next content now uses the released local writer, not codec.
    editor.submit(unsupported_local(true));
    assert!(same_local(
        &editor.flush().await.unwrap().local,
        &unsupported_local(true)
    ));
}

#[tokio::test]
async fn restart_derives_local_only_capability_from_uncertain_direct_intent() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let frozen = editor
        .consume(local("Original"), ConsumeKind::Send)
        .await
        .unwrap();
    f.owner
        .dispatch(frozen.revision, |_| async {
            super::super::dispatch::Outcome::Uncertain
        })
        .await
        .unwrap();
    let restarted = Account::new(f.account.backend.clone()).owner(f.owner.scope.key.clone());
    let restored = Editor::new(restarted.clone(), restarted.load().await.unwrap()).unwrap();
    let before = restored.snapshot().view.stored.unwrap();
    restored.submit(unsupported_local(true));
    let saved = restored.flush().await.unwrap();
    assert!(same_local(&saved.local, &unsupported_local(true)));
    assert!(saved.blocked.is_none());
    let after = saved.stored.unwrap();
    assert_eq!(after.state.pending(), before.state.pending());
    assert_eq!(after.state.consumption(), before.state.consumption());
    assert!(matches!(
        after.state.workflow(),
        super_platinum_core::drafts::sync::Workflow::Uncertain(_)
    ));
}

#[tokio::test]
async fn scheduled_consume_does_not_authorize_local_only_checkpoint() {
    let (f, editor) = Fixture::new(ACCOUNT).await;
    let frozen = editor
        .consume(
            local("Original"),
            ConsumeKind::Schedule {
                post_at: 1800000000,
            },
        )
        .await
        .unwrap();
    assert!(local::direct_intent(&frozen).is_none());
    editor.submit(unsupported_local(true));
    assert!(matches!(editor.flush().await, Err(Fault::Unsupported)));
    assert!(f.owner.load().await.unwrap().local.empty());
}

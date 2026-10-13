//! Real SQLite/editor/dispatch orchestration; the only fake is the HTTP result.
use super::*;
use crate::drafts::{checkpoint::Queue, network::Attempt, storage::Backend};
use std::sync::atomic::{AtomicUsize, Ordering};
use super_platinum_core::{
    ComposerState,
    drafts::{
        store::DraftStore,
        sync::{Operation, OperationKind},
    },
};

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000098";
struct Fixture {
    path: std::path::PathBuf,
    account: Arc<Account>,
    owner: Owner,
}
impl Fixture {
    async fn new(account_id: &str) -> Self {
        let id = super_platinum_core::slack::api::ForwardMessage::new(
            "C1".into(),
            "100.000001".into(),
            "C2".into(),
            vec![],
        )
        .client_msg_id;
        let path = std::env::temp_dir().join(format!("draft-send-flow-{id}.sqlite"));
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
            account: account_id.into(),
            path: path.clone(),
        });
        let owner = account.owner(key);
        owner.enroll(local("Original"), None).await.unwrap();
        Self {
            path,
            account,
            owner,
        }
    }
    async fn editor(&self, queue: &mut Queue) -> Arc<Editor> {
        queue
            .editor(
                &self.account,
                1,
                self.owner.scope.key.clone(),
                self.owner.load().await.unwrap(),
            )
            .unwrap()
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
fn sent(op: &Operation) -> Attempt {
    Attempt {
        outcome: Outcome::Sent {
            client_msg_id: op.client_msg_id.clone(),
            draft_id: op.base.as_ref().map(|base| base.id.clone()),
            ts: "1800000001.000001".into(),
        },
        cooldown_until_ms: None,
        issue: None,
        dispatched: true,
        raw: None,
    }
}

#[tokio::test]
async fn direct_send_prepares_once_preserves_typing_and_next_send_reenrolls_released_writer() {
    let f = Fixture::new(ACCOUNT).await;
    let mut queue = Queue::default();
    let editor = f.editor(&mut queue).await;
    let frozen = editor
        .consume(local("Original"), ConsumeKind::Send)
        .await
        .unwrap();
    assert!(f.owner.load().await.unwrap().local.empty());
    let intent = frozen.state.consumption().unwrap().clone();
    let calls = AtomicUsize::new(0);
    let owner = &f.owner;
    let writer = editor.clone();
    let calls_ref = &calls;
    let (receipt, _) = dispatch_once_with(owner, &intent, |operation| async move {
        calls_ref.fetch_add(1, Ordering::SeqCst);
        assert_eq!(operation.kind, OperationKind::Send);
        assert!(operation.base.is_none());
        assert_eq!(
            owner.load().await.unwrap().stored.unwrap().state.pending(),
            Some(&operation)
        );
        writer.submit(local("Typing during the request"));
        writer.flush().await.unwrap();
        sent(&operation)
    })
    .await
    .unwrap();
    assert!(receipt.applied);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Typing during the request"
    );
    editor.release().await.unwrap();
    assert!(f.owner.load().await.unwrap().stored.is_none());
    editor.submit(local("Second send content"));
    editor.flush().await.unwrap();
    f.owner
        .enroll(local("Second send content"), None)
        .await
        .unwrap();
    let next = f.editor(&mut queue).await;
    assert!(!Arc::ptr_eq(&editor, &next));
    let frozen = next
        .consume(local("Second send content"), ConsumeKind::Send)
        .await
        .unwrap();
    let next_intent = frozen.state.consumption().unwrap();
    assert_ne!(next_intent.client_msg_id, intent.client_msg_id);
    let (receipt, _) = dispatch_once_with(&f.owner, next_intent, |operation| async move {
        calls_ref.fetch_add(1, Ordering::SeqCst);
        assert_eq!(operation.kind, OperationKind::Send);
        sent(&operation)
    })
    .await
    .unwrap();
    assert!(receipt.applied);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn delayed_captured_account_outcome_cannot_touch_another_accounts_same_destination() {
    let first = Fixture::new(ACCOUNT).await;
    let second = Fixture::new("00000000-0000-4000-8000-000000000099").await;
    let mut queue = Queue::default();
    let original = first.editor(&mut queue).await;
    let frozen = original
        .consume(local("Original"), ConsumeKind::Send)
        .await
        .unwrap();
    // Switch the same UI writer queue to the other account before the receipt.
    let other = second.editor(&mut queue).await;
    other.submit(local("Other account private input"));
    other.flush().await.unwrap();
    let before = second.owner.load().await.unwrap().stored.unwrap();
    let intent = frozen.state.consumption().unwrap();
    let (receipt, _) = dispatch_once_with(&first.owner, intent, |operation| async move {
        assert_eq!(operation.scope.account_id, ACCOUNT);
        sent(&operation)
    })
    .await
    .unwrap();
    assert!(receipt.applied);
    let after = second.owner.load().await.unwrap();
    assert_eq!(after.local.composer.text, "Other account private input");
    assert_eq!(after.stored.unwrap().revision, before.revision);
    assert!(
        first
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
async fn uncertain_send_parks_across_restart_without_calling_dispatch_again() {
    let f = Fixture::new(ACCOUNT).await;
    let editor = f.editor(&mut Queue::default()).await;
    let frozen = editor
        .consume(local("Original"), ConsumeKind::Send)
        .await
        .unwrap();
    let intent = frozen.state.consumption().unwrap();
    let (receipt, _) = dispatch_once_with(&f.owner, intent, |operation| async move {
        let mut attempt = sent(&operation);
        attempt.outcome = Outcome::Uncertain;
        attempt
    })
    .await
    .unwrap();
    assert!(receipt.applied);
    let restarted = f.restart();
    let calls = AtomicUsize::new(0);
    let calls_ref = &calls;
    assert!(matches!(
        dispatch_once_with(&restarted, intent, |operation| async move {
            calls_ref.fetch_add(1, Ordering::SeqCst);
            sent(&operation)
        })
        .await,
        Err(Fault::Pending)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        restarted
            .load()
            .await
            .unwrap()
            .stored
            .unwrap()
            .state
            .workflow(),
        Workflow::Uncertain(_)
    ));
}

#[tokio::test]
async fn receipt_disk_failure_retains_exact_operation_and_never_replays_after_restart() {
    let f = Fixture::new(ACCOUNT).await;
    let editor = f.editor(&mut Queue::default()).await;
    let frozen = editor
        .consume(local("Original"), ConsumeKind::Send)
        .await
        .unwrap();
    let intent = frozen.state.consumption().unwrap();
    let path = f.path.clone();
    let backup = path.with_extension("backup");
    let mut dispatched = None;
    let result = dispatch_once_with(&f.owner, intent, |operation| {
        dispatched = Some(operation.clone());
        let path = &path;
        let backup = &backup;
        async move {
            std::fs::rename(path, backup).unwrap();
            std::fs::create_dir(path).unwrap();
            sent(&operation)
        }
    })
    .await;
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(&backup, &path).unwrap();
    assert!(matches!(result, Err(Fault::Storage)));
    let restarted = f.restart();
    let saved = restarted.load().await.unwrap();
    assert_eq!(
        saved.stored.as_ref().unwrap().state.pending(),
        dispatched.as_ref()
    );
    assert!(matches!(
        saved.stored.unwrap().state.workflow(),
        Workflow::Uncertain(_)
    ));
    let calls = AtomicUsize::new(0);
    let calls_ref = &calls;
    assert!(
        dispatch_once_with(&restarted, intent, |operation| async move {
            calls_ref.fetch_add(1, Ordering::SeqCst);
            sent(&operation)
        })
        .await
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn aborted_fresh_enrollment_releases_only_reviewed_clean_owner_and_keeps_typing() {
    let f = Fixture::new(ACCOUNT).await;
    let enrolled = f.owner.load().await.unwrap().stored.unwrap().revision;
    assert_eq!(
        abort_enrollment(&f.owner, enrolled, None).await.unwrap(),
        enrolled
    );
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Original"
    );
    assert!(f.owner.load().await.unwrap().stored.is_none());
    let enrolled = f
        .owner
        .enroll(local("Original"), None)
        .await
        .unwrap()
        .revision;
    let editor = f.editor(&mut Queue::default()).await;
    editor.submit(local("Newer typing after navigation"));
    editor.flush().await.unwrap();
    assert!(
        abort_enrollment(&f.owner, enrolled, Some(&editor))
            .await
            .is_ok()
    );
    let restored = f.restart().load().await.unwrap();
    assert!(restored.stored.is_none());
    assert_eq!(
        restored.local.composer.text,
        "Newer typing after navigation"
    );
}

#[tokio::test]
async fn stale_abort_cannot_release_changed_or_consumed_ownership() {
    let f = Fixture::new(ACCOUNT).await;
    let enrolled = f.owner.load().await.unwrap().stored.unwrap().revision;
    let edited = f
        .owner
        .checkpoint(enrolled, local("Newer saved text"))
        .await
        .unwrap();
    assert_eq!(
        abort_enrollment(&f.owner, enrolled, None).await,
        Err(Fault::Stale)
    );
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Newer saved text"
    );
    let editor = f.editor(&mut Queue::default()).await;
    editor
        .consume(local("Newer saved text"), ConsumeKind::Send)
        .await
        .unwrap();
    assert!(
        abort_enrollment(&f.owner, edited.revision, Some(&editor))
            .await
            .is_err()
    );
    assert!(
        f.owner
            .load()
            .await
            .unwrap()
            .stored
            .unwrap()
            .state
            .consumption()
            .is_some()
    );
}

use super::*;
use crate::drafts::{dispatch::Outcome, storage::Backend};
use super_platinum_core::{
    ComposerState,
    drafts::store::{DraftStore, sync::LoadMode},
};
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
struct Fixture {
    path: std::path::PathBuf,
    owner: Owner,
    editor: Arc<Editor>,
    revision: u64,
    intent: ConsumeIntent,
}
impl Fixture {
    async fn new() -> Self {
        let id = super_platinum_core::slack::api::ForwardMessage::new(
            "C1".into(),
            "1.000001".into(),
            "C2".into(),
            vec![],
        )
        .client_msg_id;
        let path = std::env::temp_dir().join(format!("draft-return-{id}.sqlite"));
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        };
        DraftStore::open(&path)
            .unwrap()
            .save([(key.clone(), local("Unsent 日本語 🚀"))])
            .unwrap();
        let account = Account::new(Backend::Isolated {
            account: "00000000-0000-4000-8000-000000000099".into(),
            path: path.clone(),
        });
        let owner = account.owner(key);
        let row = owner.enroll(local("Unsent 日本語 🚀"), None).await.unwrap();
        let row = owner
            .consume(row.revision, ConsumeKind::Send)
            .await
            .unwrap();
        let intent = row.state.consumption().unwrap().clone();
        let rejected = owner
            .dispatch(row.revision, |_| async { Outcome::Rejected })
            .await
            .unwrap();
        let revision = rejected.stored.revision;
        let editor = Editor::new(owner.clone(), owner.load().await.unwrap()).unwrap();
        Self {
            path,
            owner,
            editor,
            revision,
            intent,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
#[tokio::test]
async fn exact_rejected_send_returns_without_dispatch_and_releases_to_local() {
    let f = Fixture::new().await;
    assert!(
        f.editor
            .return_rejected(f.revision - 1, f.intent.clone(), 0)
            .await
            .is_err()
    );
    let view = f
        .editor
        .return_rejected(f.revision, f.intent.clone(), 0)
        .await
        .unwrap();
    assert_eq!(view.local.composer.text, "Unsent 日本語 🚀");
    assert!(view.stored.as_ref().unwrap().state.consumption().is_none());
    assert!(f.editor.finish_return(0));
    f.editor.release().await.unwrap();
    let restored = f.owner.load().await.unwrap();
    assert!(restored.stored.is_none());
    assert_eq!(restored.local.composer.text, "Unsent 日本語 🚀");
    assert!(
        f.owner
            .return_rejected(f.revision, &f.intent)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn typing_during_restore_is_saved_separately_without_overwriting_unsent_message() {
    let f = Fixture::new().await;
    let gate = f.owner.gate.lock().await;
    let editor = f.editor.clone();
    let intent = f.intent.clone();
    let revision = f.revision;
    let task = tokio::spawn(async move { editor.return_rejected(revision, intent, 0).await });
    while !f.editor.snapshot().running {
        tokio::task::yield_now().await;
    }
    f.editor.submit(local("Newer input during disk save"));
    drop(gate);
    task.await.unwrap().unwrap();
    assert_eq!(f.editor.flush().await.unwrap_err(), Fault::AlternateEdits);
    let loaded = f
        .owner
        .account
        .backend
        .load(LoadMode::Current)
        .await
        .unwrap();
    assert_eq!(
        loaded
            .local
            .iter()
            .find(|(key, _)| key == &f.owner.scope.key)
            .unwrap()
            .1
            .composer
            .text,
        "Unsent 日本語 🚀"
    );
    assert_eq!(
        loaded.local_alternates[&f.owner.scope.key][0]
            .draft
            .composer
            .text,
        "Newer input during disk save"
    );
    assert!(
        loaded.synced[&f.owner.scope.key]
            .state
            .consumption()
            .is_none()
    );
}
#[tokio::test]
async fn uncertain_newer_or_preserved_versions_cannot_be_returned() {
    let f = Fixture::new().await;
    let mut wrong = f.intent.clone();
    wrong.id = "wrong".into();
    assert!(f.owner.return_rejected(f.revision, &wrong).await.is_err());
    f.owner
        .account
        .backend
        .preserve_alternate(
            f.owner.scope.key.clone(),
            local("Independent writer"),
            Default::default(),
            1,
        )
        .await
        .unwrap();
    assert_eq!(
        f.owner
            .return_rejected(f.revision, &f.intent)
            .await
            .unwrap_err(),
        Fault::AlternateEdits
    );
    let current = f.owner.load().await.unwrap();
    assert_eq!(current.stored.unwrap().state.consumption(), Some(&f.intent));
}

#[tokio::test]
async fn alternate_arriving_after_restore_review_blocks_atomic_commit() {
    let f = Fixture::new().await;
    let mut stored = f.owner.load().await.unwrap().stored.unwrap();
    stored
        .state
        .cancel_consumption(&f.owner.scope, &f.intent.id)
        .unwrap();
    f.owner
        .account
        .backend
        .preserve_alternate(
            f.owner.scope.key.clone(),
            local("Arrived after review"),
            Default::default(),
            1,
        )
        .await
        .unwrap();
    let result = DraftStore::open(&f.path)
        .unwrap()
        .commit_synced(
            &f.owner.scope,
            Some(f.revision),
            &local("Unsent 日本語 🚀"),
            &stored.state,
            None,
        )
        .unwrap();
    assert!(matches!(
        result,
        super_platinum_core::drafts::store::sync::Commit::Stale(_)
    ));
    let view = f.owner.load().await.unwrap();
    assert!(view.local.empty());
    assert_eq!(view.stored.unwrap().state.consumption(), Some(&f.intent));
}

#[tokio::test]
async fn typing_after_disk_receipt_waits_for_explicit_projection_handoff() {
    for projected in [false, true] {
        let f = Fixture::new().await;
        f.editor
            .return_rejected(f.revision, f.intent.clone(), 0)
            .await
            .unwrap();
        if projected {
            assert!(f.editor.finish_return(0));
        }
        f.editor.submit(local("Input after disk receipt"));
        let result = f.editor.flush().await;
        let loaded = f
            .owner
            .account
            .backend
            .load(LoadMode::Current)
            .await
            .unwrap();
        let authoritative = &loaded
            .local
            .iter()
            .find(|(key, _)| key == &f.owner.scope.key)
            .unwrap()
            .1
            .composer
            .text;
        if projected {
            assert!(result.is_ok());
            assert_eq!(authoritative, "Input after disk receipt");
        } else {
            assert_eq!(result.unwrap_err(), Fault::AlternateEdits);
            assert!(!f.editor.finish_return(0));
            assert_eq!(authoritative, "Unsent 日本語 🚀");
            assert_eq!(
                loaded.local_alternates[&f.owner.scope.key][0]
                    .draft
                    .composer
                    .text,
                "Input after disk receipt"
            );
        }
    }
}

#[tokio::test]
async fn interrupted_return_can_be_reviewed_and_rearmed_without_dispatch() {
    let f = Fixture::new().await;
    assert!(!f.editor.waiting_for_return());
    let restored = f
        .editor
        .return_rejected(f.revision, f.intent.clone(), 0)
        .await
        .unwrap();
    let revision = restored.stored.as_ref().unwrap().revision;
    assert!(f.editor.waiting_for_return());
    assert_eq!(f.editor.release().await.unwrap_err(), Fault::AlternateEdits);
    assert!(f.editor.review_return(revision - 1).await.is_err());
    let (serial, view) = f.editor.review_return(revision).await.unwrap();
    assert_eq!(view.local.composer.text, "Unsent 日本語 🚀");
    assert!(f.editor.finish_return(serial));
    assert!(!f.editor.waiting_for_return());
    f.editor.release().await.unwrap();
    assert!(f.owner.load().await.unwrap().stored.is_none());
}

#[tokio::test]
async fn interrupted_return_does_not_rearm_over_newer_durable_versions() {
    let f = Fixture::new().await;
    let restored = f
        .editor
        .return_rejected(f.revision, f.intent.clone(), 0)
        .await
        .unwrap();
    let revision = restored.stored.as_ref().unwrap().revision;
    f.owner
        .account
        .backend
        .preserve_alternate(
            f.owner.scope.key.clone(),
            local("A different editor"),
            Default::default(),
            1,
        )
        .await
        .unwrap();
    assert!(f.editor.review_return(revision).await.is_err());
    assert_eq!(f.editor.snapshot().error, Some(Fault::AlternateEdits));
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Unsent 日本語 🚀"
    );
}

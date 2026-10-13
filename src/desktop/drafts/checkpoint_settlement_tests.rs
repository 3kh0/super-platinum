use super::*;
use super_platinum_core::{ComposerState, drafts::store::DraftStore};

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000092";

struct Fixture {
    path: std::path::PathBuf,
    owner: Owner,
    editor: Arc<Editor>,
}
impl Fixture {
    async fn new() -> Self {
        let writer = super_platinum_core::drafts::store::LocalWriter::default();
        let path = std::env::temp_dir().join(format!("draft-settlement-{}.sqlite", writer.id()));
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
            account: ACCOUNT.into(),
            path: path.clone(),
        });
        let owner = account.owner(key);
        owner.enroll(local("Original"), None).await.unwrap();
        let editor = Editor::new(owner.clone(), owner.load().await.unwrap()).unwrap();
        Self {
            path,
            owner,
            editor,
        }
    }

    async fn conflict(&self) -> LocalReview {
        let revision = self.owner.load().await.unwrap().stored.unwrap().revision;
        self.owner
            .checkpoint(revision, local("Another editor's local draft"))
            .await
            .unwrap();
        self.editor.submit(local("My retained version"));
        assert!(self.editor.flush().await.is_err());
        self.owner.review_local().await.unwrap()
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

#[tokio::test]
async fn reviewed_editor_resumes_on_selected_local_instead_of_resurrecting_parked_input() {
    let f = Fixture::new().await;
    let review = f.conflict().await;
    let serial = f.editor.snapshot().serial;
    let saved = f
        .editor
        .settle(review, LocalChoice::Current, serial)
        .await
        .unwrap();
    assert_eq!(saved.local.composer.text, "Another editor's local draft");
    assert!(f.editor.snapshot().pending.is_none());
    assert!(f.editor.snapshot().error.is_none());
    f.editor.submit(local("Continued after review"));
    f.editor.flush().await.unwrap();
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Continued after review"
    );
    assert!(f.owner.review_local().await.unwrap().alternates.is_empty());
}

#[tokio::test]
async fn input_after_review_capture_refuses_stale_choice() {
    let f = Fixture::new().await;
    let review = f.conflict().await;
    let serial = f.editor.snapshot().serial;
    f.editor.submit(local("Later input must be reviewed too"));
    assert!(f.editor.flush().await.is_err());
    assert!(matches!(
        f.editor.settle(review, LocalChoice::Current, serial).await,
        Err(Fault::Stale)
    ));
    assert_eq!(
        f.editor.snapshot().pending.unwrap().composer.text,
        "Later input must be reviewed too"
    );
    assert!(!f.owner.review_local().await.unwrap().alternates.is_empty());
}

#[tokio::test]
async fn canceled_review_waiter_still_hands_newer_typing_to_committed_managed_base() {
    let f = Fixture::new().await;
    let review = f.conflict().await;
    let serial = f.editor.snapshot().serial;
    let owner_gate = f.owner.gate.lock().await;
    let editor = f.editor.clone();
    let waiter =
        tokio::spawn(async move { editor.settle(review, LocalChoice::Current, serial).await });
    while !f.editor.state.lock().unwrap().settling {
        tokio::task::yield_now().await;
    }
    f.editor.submit(local("Typing while choice is saved"));
    waiter.abort();
    let _ = waiter.await;
    drop(owner_gate);
    f.editor.flush().await.unwrap();
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Typing while choice is saved"
    );
    assert!(f.owner.review_local().await.unwrap().alternates.is_empty());
}

#[tokio::test]
async fn released_input_during_review_uses_the_selected_predecessor() {
    let f = Fixture::new().await;
    f.editor.release().await.unwrap();
    DraftStore::open(&f.path)
        .unwrap()
        .save([(f.owner.scope.key.clone(), local("Foreign local"))])
        .unwrap();
    f.editor.submit(local("Released retained input"));
    assert!(f.editor.flush().await.is_err());
    let review = f.owner.review_local().await.unwrap();
    let serial = f.editor.snapshot().serial;
    let owner_gate = f.owner.gate.lock().await;
    let editor = f.editor.clone();
    let waiter =
        tokio::spawn(async move { editor.settle(review, LocalChoice::Current, serial).await });
    while !f.editor.state.lock().unwrap().settling {
        tokio::task::yield_now().await;
    }
    f.editor.submit(local("Released new typing during choice"));
    assert!(f.editor.state.lock().unwrap().local_writes.is_empty());
    drop(owner_gate);
    waiter.await.unwrap().unwrap();
    f.editor.flush().await.unwrap();
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Released new typing during choice"
    );
    assert!(f.owner.review_local().await.unwrap().alternates.is_empty());
}

#[tokio::test]
async fn unrepresented_memory_only_input_cannot_be_discarded_by_review() {
    let f = Fixture::new().await;
    let backup = f.path.with_extension("backup");
    std::fs::rename(&f.path, &backup).unwrap();
    std::fs::create_dir(&f.path).unwrap();
    f.editor.submit(local("Only surviving in memory"));
    assert!(f.editor.flush().await.is_err());
    std::fs::remove_dir(&f.path).unwrap();
    std::fs::rename(&backup, &f.path).unwrap();
    let review = f.owner.review_local().await.unwrap();
    let serial = f.editor.snapshot().serial;
    assert!(matches!(
        f.editor.settle(review, LocalChoice::Current, serial).await,
        Err(Fault::Stale)
    ));
    assert_eq!(
        f.editor.snapshot().pending.unwrap().composer.text,
        "Only surviving in memory"
    );
}

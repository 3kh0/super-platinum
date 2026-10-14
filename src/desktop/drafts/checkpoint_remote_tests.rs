use super::*;
use serde_json::{Value, json};
use super_platinum_core::{
    ComposerState,
    drafts::{
        store::{DraftStore, LocalWriter},
        sync::RemoteDraft,
    },
};
pub(crate) struct Fixture {
    path: std::path::PathBuf,
    pub(crate) editor: Arc<Editor>,
    pub(crate) raw: Value,
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
impl Fixture {
    pub(crate) async fn new() -> Self {
        let account = LocalWriter::default().id().to_owned();
        let path = std::env::temp_dir().join(format!("remote-review-{}.sqlite", account));
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        };
        let scope = Scope {
            account_id: account.clone(),
            key: key.clone(),
        };
        let original = local("local");
        let base = RemoteDraft {
            scope: scope.clone(),
            id: "D1".into(),
            client_msg_id: LocalWriter::default().id().into(),
            last_updated_ts: "10.000001".into(),
            snapshot: codec::encode_composer(&key, &original.composer).unwrap(),
        };
        let state = DraftSync::adopt_remote(&scope, base.clone()).unwrap();
        let mut store = DraftStore::open(&path).unwrap();
        store.save([(key.clone(), original.clone())]).unwrap();
        assert!(matches!(
            store
                .commit_synced(&scope, None, &original, &state, None)
                .unwrap(),
            Commit::Applied(_)
        ));
        drop(store);
        let account = Account::new(Backend::Isolated {
            account,
            path: path.clone(),
        });
        let owner = account.owner(key.clone());
        let editor = Editor::new(owner.clone(), owner.load().await.unwrap()).unwrap();
        let snap = codec::encode_composer(&key, &local("remote").composer).unwrap();
        let raw = json!({"ok":true,"draft":{"id":"D1","client_msg_id":base.client_msg_id,"last_updated_ts":"11.000001","is_sent":false,"is_deleted":false,"date_scheduled":0,"blocks":snap.blocks,"file_ids":snap.file_ids,"destinations":snap.destinations}});
        Self { path, editor, raw }
    }
    pub(crate) async fn prepare(&self, live: LocalDraft) -> (RemoteLease, RemoteReview) {
        let lease = self.editor.prepare_remote_review(live).await.unwrap();
        let review =
            RemoteReview::prepare(lease.inventory().clone(), self.raw.clone(), "U1").unwrap();
        (lease, review)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
#[tokio::test]
async fn explicit_choice_saves_but_never_resumes_before_native_ack() {
    let f = Fixture::new().await;
    assert!(!f.editor.remote_review_available());
    let (lease, review) = f.prepare(local("local")).await;
    let replay = (lease.clone(), review.clone());
    let barrier = f
        .editor
        .reconcile_remote(lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    assert_eq!(barrier.before.local.composer.text, "local");
    assert_eq!(
        barrier.after.as_ref().unwrap().local.composer.text,
        "remote"
    );
    let snapshot = f.editor.snapshot();
    assert_eq!(snapshot.error, Some(Fault::Stale));
    assert!(f.editor.remote_review_available());
    assert!(Arc::ptr_eq(snapshot.projection.as_ref().unwrap(), &barrier));
    assert!(f.editor.prepare_projection(&barrier.id).await.is_ok());
    assert!(matches!(
        f.editor
            .reconcile_remote(replay.0, replay.1, RemoteChoice::Remote)
            .await,
        Err(Fault::Stale)
    ));
    f.editor.submit(local("typing on old visible field"));
    assert!(f.editor.flush().await.is_err());
    let inventory = f.editor.owner.review_local().await.unwrap();
    assert_eq!(inventory.local.composer.text, "remote");
    assert!(
        inventory
            .alternates
            .iter()
            .any(|row| row.draft.composer.text == "typing on old visible field")
    );
}
#[tokio::test]
async fn input_after_read_capture_rejects_choice_before_storage() {
    let f = Fixture::new().await;
    let (lease, review) = f.prepare(local("local")).await;
    f.editor.submit(local("newer input"));
    f.editor.flush().await.unwrap();
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = called.clone();
    let result = f
        .editor
        .reconcile_remote_using(
            lease,
            review,
            RemoteChoice::Remote,
            move |_, _, _| async move {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                panic!("stale lease")
            },
        )
        .await;
    assert!(matches!(result, Err(Fault::Stale)));
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(
        f.editor.owner.load().await.unwrap().local.composer.text,
        "newer input"
    );
}
#[tokio::test]
async fn second_explicit_review_uses_new_durably_reviewed_live_predecessor() {
    let f = Fixture::new().await;
    let (lease, review) = f.prepare(local("local")).await;
    let first = f
        .editor
        .reconcile_remote(lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    f.editor.submit(local("new visible text"));
    assert!(f.editor.flush().await.is_err());
    let (lease, review) = f.prepare(local("new visible text")).await;
    assert_eq!(lease.before_local().composer.text, "new visible text");
    let second = f
        .editor
        .reconcile_remote(lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(second.before.local.composer.text, "new visible text");
    assert!(f.editor.snapshot().pending.is_none());
    assert!(f.editor.prepare_projection(&second.id).await.is_ok());
    assert!(
        f.editor
            .owner
            .review_local()
            .await
            .unwrap()
            .alternates
            .is_empty()
    );
}
#[tokio::test]
async fn failed_commit_parks_unknown_after_but_explicit_fresh_review_can_recover() {
    let f = Fixture::new().await;
    let (lease, review) = f.prepare(local("local")).await;
    let result = f
        .editor
        .reconcile_remote_using(lease, review, RemoteChoice::Remote, |_, _, _| async {
            Err(Fault::Storage)
        })
        .await;
    assert!(matches!(result, Err(Fault::Storage)));
    let first = f.editor.snapshot().projection.unwrap();
    assert!(first.after.is_none());
    assert!(
        f.editor
            .preserve_remote_predecessor(first.id.clone(), first.serial, local("local"))
            .await
            .is_err()
    );
    assert_eq!(f.editor.snapshot().serial, first.serial);
    assert!(f.editor.prepare_projection(&first.id).await.is_err());
    let (lease, review) = f.prepare(local("local")).await;
    let next = f
        .editor
        .reconcile_remote(lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    assert_ne!(first.id, next.id);
    assert!(f.editor.prepare_projection(&next.id).await.is_ok());
}
#[tokio::test]
async fn canceled_waiter_retains_commit_and_parks_typing_during_disk_await() {
    let f = Fixture::new().await;
    let (lease, review) = f.prepare(local("local")).await;
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    let editor = f.editor.clone();
    let waiter = tokio::spawn(async move {
        editor
            .reconcile_remote_using(
                lease,
                review,
                RemoteChoice::Remote,
                move |owner, review, choice| async move {
                    started_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    let result = owner.reconcile_remote_review(review, choice).await;
                    let _ = done_tx.send(());
                    result
                },
            )
            .await
    });
    started_rx.await.unwrap();
    f.editor.submit(local("during commit"));
    waiter.abort();
    release_tx.send(()).unwrap();
    done_rx.await.unwrap();
    assert!(f.editor.flush().await.is_err());
    let snapshot = f.editor.snapshot();
    assert_eq!(snapshot.error, Some(Fault::Stale));
    assert!(snapshot.projection.is_some());
    let inventory = f.editor.owner.review_local().await.unwrap();
    assert_eq!(inventory.local.composer.text, "remote");
    assert!(
        inventory
            .alternates
            .iter()
            .any(|row| row.draft.composer.text == "during commit")
    );
}

#[tokio::test]
async fn inventory_refresh_before_choice_cannot_authorize_native_projection() {
    let f = Fixture::new().await;
    let owner = f.editor.owner();
    let previous = owner.load().await.unwrap().stored.unwrap();
    owner
        .checkpoint(previous.revision, local("external canonical"))
        .await
        .unwrap();
    let writer = LocalWriter::default();
    DraftStore::open(&f.path)
        .unwrap()
        .save_owned_local_alternate(&owner.scope, &local("local"), &writer, 1)
        .unwrap();
    let (lease, review) = f.prepare(local("local")).await;
    assert_eq!(lease.inventory().local.composer.text, "external canonical");
    let parked = f.editor.snapshot().projection.unwrap();
    assert!(parked.after.is_none());
    assert!(f.editor.prepare_projection(&parked.id).await.is_err());
    let chosen = f
        .editor
        .reconcile_remote(lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    assert_ne!(chosen.id, parked.id);
    assert_eq!(chosen.after.as_ref().unwrap().local.composer.text, "remote");
    assert!(f.editor.prepare_projection(&chosen.id).await.is_ok());
}

#[tokio::test]
async fn wrong_editor_or_different_review_inventory_never_reaches_commit() {
    for wrong_editor in [true, false] {
        let f = Fixture::new().await;
        let (lease, mut review) = f.prepare(local("local")).await;
        let editor = if wrong_editor {
            Editor::new(f.editor.owner(), f.editor.owner.load().await.unwrap()).unwrap()
        } else {
            let writer = LocalWriter::default();
            DraftStore::open(&f.path)
                .unwrap()
                .save_owned_local_alternate(
                    &f.editor.owner.scope,
                    &local("unseen alternate"),
                    &writer,
                    1,
                )
                .unwrap();
            review = RemoteReview::prepare(
                f.editor.owner.review_local().await.unwrap(),
                f.raw.clone(),
                "U1",
            )
            .unwrap();
            f.editor.clone()
        };
        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = called.clone();
        let result = editor
            .reconcile_remote_using(
                lease,
                review,
                RemoteChoice::Remote,
                move |_, _, _| async move {
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    panic!("unmatched authority reached transaction")
                },
            )
            .await;
        assert!(matches!(result, Err(Fault::Stale)));
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            f.editor.owner.load().await.unwrap().local.composer.text,
            "local"
        );
    }
}

#[tokio::test]
async fn finish_reuses_same_committed_barrier_without_new_read_or_cas() {
    let f = Fixture::new().await;
    assert!(!f.editor.projection_finish_available());
    let (lease, review) = f.prepare(local("local")).await;
    let barrier = f
        .editor
        .reconcile_remote(lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    let revision = f
        .editor
        .owner
        .load()
        .await
        .unwrap()
        .stored
        .unwrap()
        .revision;
    assert!(f.editor.projection_finish_available());
    // A dropped/deferred native attempt conveys no ack. A fresh mount may
    // request another ticket for this exact immutable choice without re-reading
    // Slack, choosing again, changing SQLite or refreshing the predecessor.
    let first = f.editor.prepare_projection(&barrier.id).await.unwrap();
    assert_eq!(first.before_local().composer.text, "local");
    drop(first);
    let next = f.editor.prepare_projection(&barrier.id).await.unwrap();
    assert_eq!(next.barrier_id(), barrier.id);
    assert_eq!(next.after_local().composer.text, "remote");
    assert_eq!(
        f.editor
            .owner
            .load()
            .await
            .unwrap()
            .stored
            .unwrap()
            .revision,
        revision
    );
    assert!(Arc::ptr_eq(
        f.editor.snapshot().projection.as_ref().unwrap(),
        &barrier
    ));
    f.editor.submit(local("changed after deferral"));
    assert!(f.editor.flush().await.is_err());
    assert!(!f.editor.projection_finish_available());
    assert!(f.editor.prepare_projection(&barrier.id).await.is_err());
    let current = f.editor.owner.review_local().await.unwrap();
    assert_eq!(current.local.composer.text, "remote");
    assert!(
        current
            .alternates
            .iter()
            .any(|row| row.draft.composer.text == "changed after deferral")
    );
}

#[tokio::test]
async fn explicit_review_preserves_retired_visible_predecessor_without_refreshing_choice() {
    let f = Fixture::new().await;
    let (lease, review) = f.prepare(local("local")).await;
    let barrier = f
        .editor
        .reconcile_remote(lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    let chosen_revision = f
        .editor
        .owner
        .load()
        .await
        .unwrap()
        .stored
        .unwrap()
        .revision;
    let foreign = LocalWriter::default();
    DraftStore::open(&f.path)
        .unwrap()
        .save_owned_local_alternate(&f.editor.owner.scope, &local("foreign writer"), &foreign, 1)
        .unwrap();
    assert!(f.editor.prepare_projection(&barrier.id).await.is_err());
    assert!(
        f.editor
            .prepare_remote_review(local("local"))
            .await
            .is_err()
    );
    let serial = f.editor.snapshot().serial;
    assert!(
        f.editor
            .preserve_remote_predecessor("foreign-barrier".into(), serial, local("local"))
            .await
            .is_err()
    );
    assert!(
        f.editor
            .preserve_remote_predecessor(barrier.id.clone(), serial + 1, local("local"))
            .await
            .is_err()
    );
    assert_eq!(f.editor.snapshot().serial, serial);
    assert!(
        f.editor
            .preserve_remote_predecessor(barrier.id.clone(), serial, local("unproven text"))
            .await
            .is_err()
    );
    let saved = f
        .editor
        .preserve_remote_predecessor(barrier.id.clone(), serial, local("local"))
        .await
        .unwrap();
    assert_eq!(saved.local().composer.text, "local");
    assert!(Arc::ptr_eq(
        f.editor.snapshot().projection.as_ref().unwrap(),
        &barrier
    ));
    let inventory = f.editor.owner.review_local().await.unwrap();
    assert_eq!(inventory.stored.as_ref().unwrap().revision, chosen_revision);
    assert_eq!(inventory.local.composer.text, "remote");
    assert_eq!(inventory.alternates.len(), 2);
    assert!(
        inventory
            .alternates
            .iter()
            .any(|row| row.draft.composer.text == "foreign writer")
    );
    assert!(
        inventory
            .alternates
            .iter()
            .any(|row| row.draft.composer.text == "local")
    );
    assert!(f.editor.prepare_projection(&barrier.id).await.is_err());
    let (lease, review) = f.prepare(local("local")).await;
    assert_eq!(lease.inventory().alternates.len(), 2);
    let next = f
        .editor
        .reconcile_remote(lease, review, RemoteChoice::Remote)
        .await
        .unwrap();
    assert_ne!(next.id, barrier.id);
    assert!(f.editor.prepare_projection(&next.id).await.is_ok());
}

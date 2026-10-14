use super::*;
use super_platinum_core::{
    ComposerState,
    drafts::store::{
        DraftStore, LocalWriter,
        sync::{LocalChoice, LocalSettlement},
    },
};
use tokio::sync::oneshot;

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000083";
const CLIENT: &str = "00000000-0000-4000-8000-000000000084";
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
}
impl Fixture {
    async fn new() -> Self {
        Self::account(ACCOUNT).await
    }
    async fn account(account_id: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "draft-projection-{}.sqlite",
            LocalWriter::default().id()
        ));
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
        let remote = RemoteDraft {
            scope: owner.scope.clone(),
            id: "Dr1".into(),
            client_msg_id: CLIENT.into(),
            last_updated_ts: "1000".into(),
            snapshot: codec::encode(&owner.scope.key, "Original").unwrap(),
        };
        owner.enroll(local("Original"), Some(remote)).await.unwrap();
        let editor = Editor::new(owner.clone(), owner.load().await.unwrap()).unwrap();
        let fixture = Self {
            path,
            owner,
            editor,
        };
        fixture.observed("Remote", "2000").await;
        fixture
    }
    fn remote(&self, text: &str, stamp: &str) -> RemoteDraft {
        RemoteDraft {
            scope: self.owner.scope.clone(),
            id: "Dr1".into(),
            client_msg_id: CLIENT.into(),
            last_updated_ts: stamp.into(),
            snapshot: codec::encode(&self.owner.scope.key, text).unwrap(),
        }
    }
    async fn observed(&self, text: &str, stamp: &str) {
        let before = self.editor.snapshot().view;
        self.owner.observe(self.remote(text, stamp)).await.unwrap();
        let after = self.owner.load().await.unwrap();
        // Establish the postcondition exercised through the real ordered event
        // API in checkpoint_observation_tests; no transport/session is needed
        // for this independent authority/receipt suite.
        let mut state = self.editor.state.lock().unwrap();
        state.base = after.clone();
        state.error = Some(Fault::Stale);
        state.projection = Some(Arc::new(ProjectionBarrier {
            id: LocalWriter::default().id().into(),
            scope: self.owner.scope.clone(),
            serial: state.serial,
            before,
            after: Some(after),
        }));
    }
    fn id(&self) -> String {
        self.editor.snapshot().projection.unwrap().id.clone()
    }
    async fn ticket(&self) -> ProjectionTicket {
        self.editor.prepare_projection(&self.id()).await.unwrap()
    }
    fn submission(&self) -> Submission {
        let state = self.editor.state.lock().unwrap();
        let (serial, draft) = state.pending.as_ref().unwrap();
        Submission {
            scope: self.owner.scope.clone(),
            writer_id: state.local_writer.id().into(),
            serial: *serial,
            local: draft.clone(),
        }
    }
    fn restart(&self) -> Owner {
        Account::new(self.owner.account.backend.clone()).owner(self.owner.scope.key.clone())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

type PausedAck = (
    tokio::task::JoinHandle<Result<(), Fault>>,
    oneshot::Receiver<()>,
    oneshot::Sender<()>,
);
fn paused_ack(editor: Arc<Editor>, ticket: ProjectionTicket) -> PausedAck {
    let (started, entered) = oneshot::channel();
    let (resume, resumed) = oneshot::channel();
    let task = tokio::spawn(async move {
        editor
            .acknowledge_using(ticket, move |owner| async move {
                let review = read_review(owner).await?;
                started.send(()).unwrap();
                resumed.await.unwrap();
                Ok(review)
            })
            .await
    });
    (task, entered, resume)
}

#[tokio::test]
async fn exact_clean_ticket_guard_and_ack_resume_the_projected_pair() {
    let f = Fixture::new().await;
    let ticket = f.ticket().await;
    assert_eq!(ticket.scope(), &f.owner.scope);
    assert_eq!(ticket.barrier_id(), f.id());
    assert_eq!(ticket.serial(), 0);
    assert_eq!(ticket.before_local().composer.text, "Original");
    assert_eq!(ticket.after_local().composer.text, "Remote");
    let ticket = f
        .editor
        .with_current_projection(ticket, |ticket| {
            assert!(
                f.editor.state.try_lock().is_err(),
                "guard spans the synchronous install"
            );
            Ok(ticket)
        })
        .unwrap();
    // Domain tests enter below the view's opaque RendererApplied wrapper. View
    // tests/native fixtures separately exercise receipt construction and the
    // public typed acknowledge_projection entry.
    f.editor
        .acknowledge_using(ticket, read_review)
        .await
        .unwrap();
    assert!(f.editor.snapshot().projection.is_none());
    assert!(f.editor.snapshot().error.is_none());
    f.editor
        .submit(local("Typing on the installed remote base"));
    f.editor.flush().await.unwrap();
    assert_eq!(
        f.restart().load().await.unwrap().local.composer.text,
        "Typing on the installed remote base"
    );
}

#[tokio::test]
async fn clean_install_error_keeps_barrier_and_does_not_acknowledge() {
    let f = Fixture::new().await;
    let id = f.id();
    assert!(matches!(
        f.editor
            .with_current_projection(f.ticket().await, |_| Err::<(), _>(Fault::InvalidReceipt)),
        Err(Fault::InvalidReceipt)
    ));
    assert_eq!(f.id(), id);
    assert!(matches!(f.editor.flush().await, Err(Fault::Stale)));
}

#[tokio::test]
async fn same_revision_alternate_inventory_blocks_ticket_and_ack() {
    let f = Fixture::new().await;
    let ticket = f.ticket().await;
    let revision = ticket.review.stored.as_ref().unwrap().revision;
    f.owner
        .account
        .backend
        .preserve_alternate(
            f.owner.scope.key.clone(),
            local("Other writer"),
            LocalWriter::default(),
            1,
        )
        .await
        .unwrap();
    let current = f.owner.review_local().await.unwrap();
    assert_eq!(
        current.stored.unwrap().revision,
        revision,
        "alternate writes do not advance the managed revision"
    );
    assert!(matches!(
        f.editor.prepare_projection(&f.id()).await,
        Err(Fault::Stale)
    ));
    assert!(matches!(
        f.editor.acknowledge_using(ticket, read_review).await,
        Err(Fault::Stale)
    ));
    assert!(f.editor.snapshot().projection.is_some());
}

#[tokio::test]
async fn unknown_result_and_foreign_ticket_cannot_authorize_install_or_ack() {
    let f = Fixture::new().await;
    let ticket = f.ticket().await;
    let other = Fixture::account("00000000-0000-4000-8000-000000000085").await;
    assert!(matches!(
        other
            .editor
            .with_current_projection::<()>(ticket, |_| panic!("foreign install called")),
        Err(Fault::Stale)
    ));
    let ticket = f.ticket().await;
    assert!(matches!(
        other.editor.acknowledge_using(ticket, read_review).await,
        Err(Fault::Stale)
    ));
    {
        let mut state = f.editor.state.lock().unwrap();
        Arc::get_mut(state.projection.as_mut().unwrap())
            .unwrap()
            .after = None;
    }
    assert!(matches!(
        f.editor.prepare_projection(&f.id()).await,
        Err(Fault::Stale)
    ));
    assert!(other.editor.snapshot().projection.is_some());
}

#[tokio::test]
async fn a_serial_aba_cannot_install_even_when_text_returns_to_the_old_value() {
    let f = Fixture::new().await;
    let ticket = f.ticket().await;
    f.editor.submit(local("Different"));
    assert!(f.editor.flush().await.is_err());
    f.editor.submit(local("Remote"));
    assert!(f.editor.flush().await.is_err());
    assert!(matches!(
        f.editor
            .with_current_projection::<()>(ticket, |_| panic!("stale install called")),
        Err(Fault::Stale)
    ));
    assert_eq!(f.editor.snapshot().serial, 2);
}

#[tokio::test]
async fn metadata_or_content_revision_advance_never_refreshes_the_barrier() {
    for text in ["Remote", "Newer remote"] {
        let f = Fixture::new().await;
        let ticket = f.ticket().await;
        let frozen_revision = ticket.review.stored.as_ref().unwrap().revision;
        f.owner.observe(f.remote(text, "3000")).await.unwrap();
        assert!(matches!(
            f.editor.acknowledge_using(ticket, read_review).await,
            Err(Fault::Stale)
        ));
        assert!(matches!(
            f.editor.prepare_projection(&f.id()).await,
            Err(Fault::Stale)
        ));
        assert_eq!(
            f.editor
                .snapshot()
                .projection
                .unwrap()
                .after
                .as_ref()
                .unwrap()
                .stored
                .as_ref()
                .unwrap()
                .revision,
            frozen_revision
        );
    }
}

#[tokio::test]
async fn input_during_ack_read_back_is_preserved_and_prevents_clear() {
    let f = Fixture::new().await;
    let (task, entered, resume) = paused_ack(f.editor.clone(), f.ticket().await);
    entered.await.unwrap();
    f.editor.submit(local("After provisional install"));
    resume.send(()).unwrap();
    assert!(matches!(task.await.unwrap(), Err(Fault::Stale)));
    assert!(matches!(f.editor.flush().await, Err(Fault::Stale)));
    let review = f.restart().review_local().await.unwrap();
    assert_eq!(review.local.composer.text, "Remote");
    assert_eq!(
        review.alternates[0].draft.composer.text,
        "After provisional install"
    );
    assert!(f.editor.snapshot().projection.is_some());
}

#[tokio::test]
async fn canceled_ack_waiter_still_completes_its_exact_handoff() {
    let f = Fixture::new().await;
    let (task, entered, resume) = paused_ack(f.editor.clone(), f.ticket().await);
    entered.await.unwrap();
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    resume.send(()).unwrap();
    let gate = f.editor.gate.lock().await; // retained task owns this gate until clear
    assert!(f.editor.snapshot().projection.is_none());
    assert!(f.editor.snapshot().error.is_none());
    drop(gate);
    assert!(f.editor.flush().await.is_ok());
}

#[tokio::test]
async fn duplicate_ticket_cannot_clear_a_later_projection_barrier() {
    let f = Fixture::new().await;
    let first = f.ticket().await;
    let duplicate = f.ticket().await;
    f.editor
        .acknowledge_using(first, read_review)
        .await
        .unwrap();
    f.observed("Next remote", "3000").await;
    let next_id = f.id();
    assert!(matches!(
        f.editor.acknowledge_using(duplicate, read_review).await,
        Err(Fault::Stale)
    ));
    assert_eq!(f.id(), next_id);
}

#[tokio::test]
async fn ack_storage_failure_keeps_the_same_barrier_for_explicit_retry() {
    let f = Fixture::new().await;
    let ticket = f.ticket().await;
    let id = f.id();
    let backup = f.path.with_extension("backup");
    std::fs::rename(&f.path, &backup).unwrap();
    std::fs::create_dir(&f.path).unwrap();
    let result = f.editor.acknowledge_using(ticket, read_review).await;
    std::fs::remove_dir(&f.path).unwrap();
    std::fs::rename(&backup, &f.path).unwrap();
    assert!(matches!(result, Err(Fault::Storage)));
    assert_eq!(f.id(), id);
    f.editor
        .acknowledge_using(f.ticket().await, read_review)
        .await
        .unwrap();
    assert!(f.editor.snapshot().projection.is_none());
}

#[tokio::test]
async fn preservation_receipt_proves_exact_tuple_despite_parked_stale_and_restart() {
    let f = Fixture::new().await;
    let id = f.id();
    let preserved = f
        .editor
        .preserve_projection_input(id.clone(), 0, local("Native ahead"))
        .await
        .unwrap();
    assert_eq!(preserved.scope(), &f.owner.scope);
    assert_eq!(preserved.serial(), 1);
    assert_eq!(preserved.local().composer.text, "Native ahead");
    assert!(matches!(f.editor.flush().await, Err(Fault::Stale)));
    let review = f.restart().review_local().await.unwrap();
    assert_eq!(review.local.composer.text, "Remote");
    assert_eq!(
        review.stored.as_ref().unwrap().revision,
        preserved.storage_revision()
    );
    assert!(
        review
            .alternates
            .iter()
            .any(|row| row.writer_id == preserved.writer_id()
                && row.revision == preserved.serial()
                && same_local(&row.draft, preserved.local()))
    );
    assert_eq!(f.id(), id);
}

#[tokio::test]
async fn preservation_requires_exact_barrier_and_serial_before_enqueuing() {
    let f = Fixture::new().await;
    for (id, serial) in [("foreign".into(), 0), (f.id(), 1)] {
        assert!(matches!(
            f.editor
                .preserve_projection_input(id, serial, local("Do not enqueue"))
                .await,
            Err(Fault::Stale)
        ));
    }
    assert_eq!(f.editor.snapshot().serial, 0);
    assert!(f.owner.review_local().await.unwrap().alternates.is_empty());
}

#[tokio::test]
async fn equal_or_older_no_op_save_cannot_prove_different_content() {
    for saved_serial in [1, 2] {
        let f = Fixture::new().await;
        let writer = f.editor.state.lock().unwrap().local_writer.clone();
        f.owner
            .account
            .backend
            .preserve_alternate(
                f.owner.scope.key.clone(),
                local("Already retained"),
                writer,
                saved_serial,
            )
            .await
            .unwrap();
        assert!(matches!(
            f.editor
                .preserve_projection_input(f.id(), 0, local("Different capture"))
                .await,
            Err(Fault::Stale)
        ));
        let review = f.owner.review_local().await.unwrap();
        assert_eq!(review.alternates[0].revision, saved_serial);
        assert_eq!(review.alternates[0].draft.composer.text, "Already retained");
    }
}

#[tokio::test]
async fn retired_writer_no_op_and_other_writer_same_text_are_not_proof() {
    for other_writer in [false, true] {
        let f = Fixture::new().await;
        f.editor
            .preserve_projection_input(f.id(), 0, local("Captured"))
            .await
            .unwrap();
        let submission = f.submission();
        let writer = f.editor.state.lock().unwrap().local_writer.clone();
        let review = f.owner.review_local().await.unwrap();
        assert!(matches!(
            f.owner
                .account
                .backend
                .settle_local_versions(review, LocalChoice::Current)
                .await
                .unwrap(),
            LocalSettlement::Applied { .. }
        ));
        // Retired revision succeeds as a no-op. A different writer retaining
        // identical bytes must not fill the retired writer's proof obligation.
        f.owner
            .account
            .backend
            .preserve_alternate(
                f.owner.scope.key.clone(),
                local("Captured"),
                writer,
                submission.serial,
            )
            .await
            .unwrap();
        if other_writer {
            f.owner
                .account
                .backend
                .preserve_alternate(
                    f.owner.scope.key.clone(),
                    local("Captured"),
                    LocalWriter::default(),
                    1,
                )
                .await
                .unwrap();
        }
        assert!(matches!(
            f.editor
                .prove_submission(&f.id(), submission, read_review)
                .await,
            Err(Fault::Stale)
        ));
        assert!(f.editor.snapshot().projection.is_some());
    }
}

#[tokio::test]
async fn read_failure_after_committed_alternate_is_not_a_receipt_but_exact_retry_can_prove_it() {
    let f = Fixture::new().await;
    assert!(matches!(
        f.editor
            .preserve_using(f.id(), 0, local("Durable despite failed read"), |_| async {
                Err(Fault::Storage)
            })
            .await,
        Err(Fault::Storage)
    ));
    let submission = f.submission();
    let proof = f
        .editor
        .prove_submission(&f.id(), submission, read_review)
        .await
        .unwrap();
    assert_eq!(proof.serial(), 1);
    assert_eq!(proof.local().composer.text, "Durable despite failed read");
    assert_eq!(
        f.editor.snapshot().serial,
        1,
        "proof retry does not manufacture another edit"
    );
}

#[tokio::test]
async fn newer_input_during_preservation_readback_supersedes_the_captured_proof() {
    let f = Fixture::new().await;
    let (started, entered) = oneshot::channel();
    let (resume, resumed) = oneshot::channel();
    let editor = f.editor.clone();
    let id = f.id();
    let task = tokio::spawn(async move {
        editor
            .preserve_using(id, 0, local("Native captured"), move |owner| async move {
                let review = read_review(owner).await?;
                started.send(()).unwrap();
                resumed.await.unwrap();
                Ok(review)
            })
            .await
    });
    entered.await.unwrap();
    f.editor.submit(local("Later input"));
    resume.send(()).unwrap();
    assert!(matches!(task.await.unwrap(), Err(Fault::Stale)));
    assert!(matches!(f.editor.flush().await, Err(Fault::Stale)));
    let review = f.restart().review_local().await.unwrap();
    assert_eq!(review.local.composer.text, "Remote");
    assert_eq!(review.alternates[0].draft.composer.text, "Later input");
    assert_eq!(review.alternates[0].revision, 2);
}

#[tokio::test]
async fn canceled_preservation_waiter_leaves_the_durable_alternate_and_barrier_intact() {
    let f = Fixture::new().await;
    let (started, entered) = oneshot::channel();
    let (resume, resumed) = oneshot::channel();
    let editor = f.editor.clone();
    let id = f.id();
    let task = tokio::spawn(async move {
        editor
            .preserve_using(
                id,
                0,
                local("Retained after cancellation"),
                move |owner| async move {
                    started.send(()).unwrap();
                    resumed.await.unwrap();
                    read_review(owner).await
                },
            )
            .await
    });
    entered.await.unwrap();
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    resume.send(()).unwrap();
    let gate = f.editor.gate.lock().await;
    assert!(f.editor.snapshot().projection.is_some());
    drop(gate);
    let review = f.restart().review_local().await.unwrap();
    assert_eq!(
        review.alternates[0].draft.composer.text,
        "Retained after cancellation"
    );
    assert_eq!(review.local.composer.text, "Remote");
}

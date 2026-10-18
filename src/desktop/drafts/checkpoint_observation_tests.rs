use super::super::super::observation::Source;
use super::*;
use serde_json::json;
use super_platinum_core::{
    ComposerState,
    config::{Session, WorkspaceSession},
    drafts::{store::DraftStore, store::sync::LocalChoice, sync::Workflow},
    slack::{Transport, events::RawEvent},
};
use tokio::sync::oneshot;

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000081";
const CLIENT: &str = "00000000-0000-4000-8000-000000000082";

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
    shell: crate::state::ShellState,
    source: Source,
    _worker: crate::drafts::observation::WorkerSource,
}
impl Fixture {
    async fn new() -> Self {
        // Synthetic admission credentials only; constructing Transport does not
        // send a request. No production account/session backend is opened.
        let mut shell = crate::state::ShellState::fixture(crate::media::MediaRegistry::default());
        let team = shell.core.active_team.clone().unwrap();
        let user = shell.core.workspaces[&team].self_user_id.clone();
        shell.core.workspaces.get_mut(&team).unwrap().rt_generation = 0;
        shell.core.active_account = Some(ACCOUNT.into());
        shell.core.account_epoch = 7;
        shell.core.transport = Some(Arc::new(Transport::new("fixture-only-cookie").unwrap()));
        shell.core.session = Some(Session {
            unified_user_client_enabled: Default::default(),
            d_cookie: "fixture-only-cookie".into(),
            workspaces: [(
                team.clone(),
                WorkspaceSession {
                    team_id: team.clone(),
                    enterprise_id: None,
                    user_id: user,
                    name: "Fixture".into(),
                    url: "https://fixture.slack.com".into(),
                    token: "fixture-only-token".into(),
                },
            )]
            .into(),
        });
        let params = super_platinum_core::slack::realtime::ConnectParams {
            team: team.clone(),
            ws_url: super_platinum_core::slack::realtime::flannel_url("fixture-only-token", &team),
            d_cookie: "fixture-only-cookie".into(),
            user_agent: "fixture-only-agent".into(),
        };
        let mut worker =
            crate::drafts::observation::WorkerSource::capture(&mut shell, params).unwrap();
        worker.take_connect_params(&shell).unwrap();
        assert!(worker.can_accept(&shell, &team, 9));
        shell.core.workspaces.get_mut(&team).unwrap().rt_generation = 9;
        let source = worker.accept(&mut shell, &team, 9).unwrap();
        let key = DraftKey {
            team,
            channel: "C1".into(),
            thread: None,
        };
        let writer = super_platinum_core::drafts::store::LocalWriter::default();
        let path =
            std::env::temp_dir().join(format!("draft-editor-observation-{}.sqlite", writer.id()));
        DraftStore::open(&path)
            .unwrap()
            .save([(key.clone(), local("Original"))])
            .unwrap();
        let account = Account::new(Backend::Isolated {
            account: ACCOUNT.into(),
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
        Self {
            path,
            owner,
            editor,
            shell,
            source,
            _worker: worker,
        }
    }

    fn prepared(&self, text: &str, stamp: &str) -> Prepared {
        let snapshot = codec::encode(&self.owner.scope.key, text).unwrap();
        let raw: RawEvent = serde_json::from_value(json!({
            "type":"draft_update", "draft_id":"Dr1", "event_ts":stamp,
            "client_msg_id":CLIENT, "draft": {
                "id":"Dr1", "client_msg_id":CLIENT,
                "last_updated_ts":"2000", "is_deleted":false, "is_sent":false,
                "date_scheduled":0, "blocks":snapshot.blocks,
                "destinations":snapshot.destinations, "file_ids":[]
            }
        }))
        .unwrap();
        self.source
            .prepare(&self.shell, 9, &self.owner.scope.key, &raw)
            .unwrap()
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

type Paused = (
    tokio::task::JoinHandle<Result<Observed, Fault>>,
    oneshot::Receiver<()>,
    oneshot::Sender<()>,
);
fn paused(editor: Arc<Editor>, prepared: Prepared) -> Paused {
    let (started, entered) = oneshot::channel();
    let (resume, resumed) = oneshot::channel();
    let task = tokio::spawn(async move {
        editor
            .observe_using(move |owner| async move {
                started.send(()).unwrap();
                resumed.await.unwrap();
                owner.observe_event(prepared).await
            })
            .await
    });
    (task, entered, resume)
}

#[tokio::test]
async fn prior_input_commits_before_observation_and_remote_conflict_keeps_typing_local() {
    let f = Fixture::new().await;
    let gate = f.editor.gate.lock().await;
    f.editor.submit(local("Prior typing"));
    let (task, entered, resume) = paused(f.editor.clone(), f.prepared("Remote", "10.000001"));
    drop(gate);
    entered.await.unwrap();
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Prior typing"
    );
    resume.send(()).unwrap();
    let observed = task.await.unwrap().unwrap();
    assert_eq!(observed.before.local.composer.text, "Prior typing");
    assert_eq!(observed.after.local.composer.text, "Prior typing");
    assert!(matches!(
        observed.after.stored.unwrap().state.workflow(),
        Workflow::Conflict {
            remote: Some(_),
            ..
        }
    ));
    assert!(f.editor.snapshot().projection.is_none());
    f.editor.submit(local("Continued local typing"));
    f.editor.flush().await.unwrap();
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Continued local typing"
    );
    assert!(matches!(
        f.editor
            .consume(local("Continued local typing"), ConsumeKind::Send)
            .await,
        Err(Fault::Pending)
    ));
}

#[tokio::test]
async fn input_during_await_becomes_a_durable_alternate_and_survives_restart() {
    let f = Fixture::new().await;
    let (task, entered, resume) = paused(f.editor.clone(), f.prepared("Remote", "10.000001"));
    entered.await.unwrap();
    assert!(f.editor.snapshot().observing);
    f.editor.submit(local("First in-flight edit"));
    f.editor.submit(local("Latest in-flight edit"));
    resume.send(()).unwrap();
    let observed = task.await.unwrap().unwrap();
    assert_eq!(observed.after.local.composer.text, "Remote");
    assert!(matches!(f.editor.flush().await, Err(Fault::Stale)));
    let snapshot = f.editor.snapshot();
    assert_eq!(
        snapshot.pending.unwrap().composer.text,
        "Latest in-flight edit"
    );
    let barrier = snapshot.projection.unwrap();
    assert_eq!(barrier.scope, f.owner.scope);
    assert_eq!(barrier.serial, 0);
    assert_eq!(barrier.before.local.composer.text, "Original");
    assert_eq!(
        barrier.after.as_ref().unwrap().local.composer.text,
        "Remote"
    );
    let restarted = f.restart();
    let review = restarted.review_local().await.unwrap();
    assert_eq!(review.local.composer.text, "Remote");
    assert_eq!(review.alternates.len(), 1);
    assert_eq!(
        review.alternates[0].draft.composer.text,
        "Latest in-flight edit"
    );
    assert_eq!(review.alternates[0].revision, 2);
    let reopened = Editor::new(restarted.clone(), restarted.load().await.unwrap()).unwrap();
    assert!(
        reopened.snapshot().projection.is_none(),
        "the previous DOM does not survive restart"
    );
    assert!(matches!(reopened.flush().await, Err(Fault::AlternateEdits)));
}

#[tokio::test]
async fn input_after_return_stays_parked_and_updates_only_its_own_alternate() {
    let f = Fixture::new().await;
    f.editor
        .observe_event(f.prepared("Remote", "10.000001"))
        .await
        .unwrap();
    let barrier = f.editor.snapshot().projection.unwrap();
    assert!(f.editor.snapshot().pending.is_none());
    let other = super_platinum_core::drafts::store::LocalWriter::default();
    f.owner
        .account
        .backend
        .preserve_alternate(
            f.owner.scope.key.clone(),
            local("Another writer"),
            other.clone(),
            1,
        )
        .await
        .unwrap();
    for text in ["First after return", "Newest after return"] {
        f.editor.submit(local(text));
        assert!(matches!(f.editor.flush().await, Err(Fault::Stale)));
    }
    let review = f.restart().review_local().await.unwrap();
    assert_eq!(review.local.composer.text, "Remote");
    assert_eq!(review.alternates.len(), 2);
    assert_eq!(
        review
            .alternates
            .iter()
            .find(|row| row.writer_id == other.id())
            .unwrap()
            .draft
            .composer
            .text,
        "Another writer"
    );
    assert_eq!(
        review
            .alternates
            .iter()
            .find(|row| row.writer_id != other.id())
            .unwrap()
            .draft
            .composer
            .text,
        "Newest after return"
    );
    assert!(Arc::ptr_eq(
        &barrier,
        f.editor.snapshot().projection.as_ref().unwrap()
    ));
}

#[tokio::test]
async fn metadata_only_observation_rebases_pending_input_without_a_projection() {
    let f = Fixture::new().await;
    let (task, entered, resume) = paused(f.editor.clone(), f.prepared("Original", "10.000001"));
    entered.await.unwrap();
    f.editor.submit(local("Input beside metadata"));
    resume.send(()).unwrap();
    let observed = task.await.unwrap().unwrap();
    assert!(observed.changed);
    assert!(same_local(&observed.before.local, &observed.after.local));
    f.editor.flush().await.unwrap();
    assert!(f.editor.snapshot().projection.is_none());
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Input beside metadata"
    );
    assert!(f.owner.review_local().await.unwrap().alternates.is_empty());
    f.editor.submit(local("Later normal edit"));
    f.editor.flush().await.unwrap();
    assert_eq!(
        f.owner.load().await.unwrap().local.composer.text,
        "Later normal edit"
    );
}

#[tokio::test]
async fn changed_predecessor_or_only_its_semantic_revision_prevents_silent_rebase() {
    for restore_text in [false, true] {
        let f = Fixture::new().await;
        let (task, entered, resume) = paused(f.editor.clone(), f.prepared("Original", "10.000001"));
        entered.await.unwrap();
        let revision = f.owner.load().await.unwrap().stored.unwrap().revision;
        let stored = f
            .owner
            .checkpoint(revision, local("Another owner"))
            .await
            .unwrap();
        if restore_text {
            f.owner
                .checkpoint(stored.revision, local("Original"))
                .await
                .unwrap();
        }
        resume.send(()).unwrap();
        let observed = task.await.unwrap().unwrap();
        assert!(same_predecessor(&observed.before, &observed.after));
        assert!(f.editor.snapshot().projection.is_some());
        f.editor.submit(local("Still from the earlier editor"));
        assert!(matches!(f.editor.flush().await, Err(Fault::Stale)));
        let review = f.owner.review_local().await.unwrap();
        assert_eq!(
            review.local.composer.text,
            if restore_text {
                "Original"
            } else {
                "Another owner"
            }
        );
        assert_eq!(
            review.alternates[0].draft.composer.text,
            "Still from the earlier editor"
        );
    }
}

#[tokio::test]
async fn canceled_waiter_and_account_navigation_do_not_cancel_or_redirect_handoff() {
    let mut f = Fixture::new().await;
    let prepared = f.prepared("Remote", "10.000001");
    let (task, entered, resume) = paused(f.editor.clone(), prepared);
    entered.await.unwrap();
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    f.shell.core.active_account = Some("00000000-0000-4000-8000-000000000099".into());
    f.shell.core.account_epoch += 1;
    f.editor.submit(local("After canceled foreground"));
    resume.send(()).unwrap();
    assert!(matches!(f.editor.flush().await, Err(Fault::Stale)));
    let review = f.restart().review_local().await.unwrap();
    assert_eq!(review.scope.account_id, ACCOUNT);
    assert_eq!(review.local.composer.text, "Remote");
    assert_eq!(
        review.alternates[0].draft.composer.text,
        "After canceled foreground"
    );
    assert!(f.editor.snapshot().projection.is_some());
}

#[tokio::test]
async fn uncertain_receipt_parks_typing_even_when_disk_commit_succeeded() {
    let f = Fixture::new().await;
    let prepared = f.prepared("Remote", "10.000001");
    let (started, entered) = oneshot::channel();
    let (resume, resumed) = oneshot::channel();
    let editor = f.editor.clone();
    let task = tokio::spawn(async move {
        editor
            .observe_using(move |owner| async move {
                owner.observe_event(prepared).await.unwrap();
                started.send(()).unwrap();
                resumed.await.unwrap();
                Err(Fault::Storage)
            })
            .await
    });
    entered.await.unwrap();
    f.editor.submit(local("Input without a reliable receipt"));
    resume.send(()).unwrap();
    assert!(matches!(task.await.unwrap(), Err(Fault::Storage)));
    assert!(matches!(f.editor.flush().await, Err(Fault::Storage)));
    assert!(f.editor.snapshot().projection.unwrap().after.is_none());
    let review = f.restart().review_local().await.unwrap();
    assert_eq!(review.local.composer.text, "Remote");
    assert_eq!(
        review.alternates[0].draft.composer.text,
        "Input without a reliable receipt"
    );
}

#[tokio::test]
async fn unresolved_projection_blocks_control_paths_without_requiring_pending_input() {
    let f = Fixture::new().await;
    f.editor
        .observe_event(f.prepared("Remote", "10.000001"))
        .await
        .unwrap();
    assert!(matches!(
        f.editor.consume(local("Remote"), ConsumeKind::Send).await,
        Err(Fault::Stale)
    ));
    assert!(matches!(f.editor.release().await, Err(Fault::Stale)));
    let review = f.owner.review_local().await.unwrap();
    assert!(matches!(
        f.editor.settle(review, LocalChoice::Current, 0).await,
        Err(Fault::Stale)
    ));
    assert!(matches!(
        f.editor
            .observe_event(f.prepared("Remote", "10.000002"))
            .await,
        Err(Fault::Stale)
    ));
    assert!(!f.editor.finish_return(0));
    // A waiter that passed flush BEFORE the observation still must refuse after
    // acquiring the gate. consume/release/checkpoint all share this check.
    assert!(matches!(f.editor.rebased().await, Err(Fault::Stale)));
    assert_eq!(f.owner.load().await.unwrap().local.composer.text, "Remote");
    assert!(f.editor.snapshot().projection.is_some());
}

#[tokio::test]
async fn failed_storage_read_keeps_old_pair_and_parks_later_input_after_recovery() {
    let f = Fixture::new().await;
    let (task, entered, resume) = paused(f.editor.clone(), f.prepared("Remote", "10.000001"));
    entered.await.unwrap();
    let backup = f.path.with_extension("backup");
    std::fs::rename(&f.path, &backup).unwrap();
    std::fs::create_dir(&f.path).unwrap();
    resume.send(()).unwrap();
    assert!(matches!(task.await.unwrap(), Err(Fault::Storage)));
    assert!(f.editor.snapshot().projection.unwrap().after.is_none());
    std::fs::remove_dir(&f.path).unwrap();
    std::fs::rename(&backup, &f.path).unwrap();
    f.editor.submit(local("Typing after storage recovers"));
    assert!(matches!(f.editor.flush().await, Err(Fault::Storage)));
    let review = f.restart().review_local().await.unwrap();
    assert_eq!(review.local.composer.text, "Original");
    assert_eq!(
        review.alternates[0].draft.composer.text,
        "Typing after storage recovers"
    );
}

#[tokio::test]
async fn projection_cannot_be_cleared_by_a_return_receipt_or_return_review() {
    let f = Fixture::new().await;
    f.editor
        .observe_event(f.prepared("Remote", "10.000001"))
        .await
        .unwrap();
    let row = f.editor.snapshot().view.stored.unwrap();
    let mut sync = row.state.clone();
    let intent = sync
        .request_consume(&f.owner.scope, sync.local_revision(), ConsumeKind::Send)
        .unwrap();
    assert!(matches!(
        f.editor.return_rejected(row.revision, intent, 0).await,
        Err(Fault::Stale)
    ));
    // Make all the older return-handshake guards eligible. The independent
    // projection guard must still prevent either receipt API from unblocking it.
    {
        let mut state = f.editor.state.lock().unwrap();
        state.return_handoff = Some((0, row.revision));
        state.error = Some(Fault::AlternateEdits);
    }
    assert!(matches!(
        f.editor.review_return(row.revision).await,
        Err(Fault::Stale)
    ));
    assert!(!f.editor.finish_return(0));
    assert!(f.editor.snapshot().projection.is_some());
}

#[tokio::test]
async fn polls_emit_activity_and_barrier_once_then_skip_unchanged_payloads() {
    let f = Fixture::new().await;
    let key = f.owner.scope.key.clone();
    let queue = Queue {
        scope: Some((ACCOUNT.into(), 7)),
        editors: [(key.clone(), f.editor.clone())].into(),
    };
    let mut previous = [(key.clone(), Signature::from(&f.editor.snapshot()))].into();
    let (task, entered, resume) = paused(f.editor.clone(), f.prepared("Remote", "10.000001"));
    entered.await.unwrap();
    let active = queue.changed_snapshots(ACCOUNT, 7, &previous);
    assert_eq!(active.len(), 1);
    assert!(active[0].1.observing && active[0].1.running);
    previous.insert(key.clone(), Signature::from(&active[0].1));
    for _ in 0..32 {
        assert!(queue.changed_snapshots(ACCOUNT, 7, &previous).is_empty());
    }
    resume.send(()).unwrap();
    task.await.unwrap().unwrap();
    let ready = queue.changed_snapshots(ACCOUNT, 7, &previous);
    assert_eq!(ready.len(), 1);
    assert!(!ready[0].1.observing && !ready[0].1.running);
    let barrier = ready[0].1.projection.as_ref().unwrap();
    assert!(!barrier.id.is_empty());
    previous.insert(key, Signature::from(&ready[0].1));
    let references = Arc::strong_count(barrier);
    for _ in 0..32 {
        assert!(queue.changed_snapshots(ACCOUNT, 7, &previous).is_empty());
    }
    assert_eq!(Arc::strong_count(barrier), references);
}

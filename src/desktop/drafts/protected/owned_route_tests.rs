//! Actual temporary Owned Worker, Account, Editor and retained C3 operation.
//! No Dioxus/native paint or HTTP claim: only final HTTP closure is synthetic.
use super::owned_route::{Held, o};
use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use super_platinum_core::{ComposerState, drafts::sync::ConsumeKind};

const ORIGINAL: &str = "Frozen \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}\nsecond";
const NEWER: &str = "Newer \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}";
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        ..Default::default()
    }
}
fn done<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: o::StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), o::JobState::Ready);
    job.result().unwrap().unwrap()
}
struct Fixture {
    account: Arc<Account>,
    owner: Owner,
    editor: Arc<editor::Editor>,
    stream: durable::Stream,
}
impl Fixture {
    async fn new(thread: bool) -> Self {
        let (initialized, scope, stream) = tokio::task::spawn_blocking(move || {
            let route = o::OwnedRoute::fresh_fixture().unwrap();
            let before = route.inspect().unwrap().wait().unwrap();
            route.commit(&before).unwrap().wait().unwrap();
            let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
            let initialized = opened
                .initialize_cold(&o::FactoryIdentity::default())
                .unwrap()
                .wait()
                .unwrap();
            let ops = initialized.operations();
            let scope = Scope {
                account_id: ops.account().into(),
                key: DraftKey {
                    team: "T1".into(),
                    channel: "C1".into(),
                    thread: thread.then(|| "100.000001".into()),
                },
            };
            let inventory = done(ops.local_inventory(scope.clone()).unwrap());
            let review = done(ops.enroll(inventory).unwrap());
            let expected = done(
                ops.stream_state(o::TeamInput {
                    scope: scope.clone(),
                })
                .unwrap(),
            );
            let stream = done(
                ops.open_stream(o::OpenInput {
                    scope: scope.clone(),
                    expected,
                })
                .unwrap(),
            );
            done(
                ops.verify_local(o::MutationInput {
                    stream: stream.clone(),
                    expected: review,
                })
                .unwrap(),
            );
            (initialized, scope, stream)
        })
        .await
        .unwrap();
        let account = Account::from_retained(initialized);
        let owner = account.owner(scope.key).unwrap();
        let editor = editor::Editor::open(owner.clone()).await.unwrap();
        editor.submit(&local(ORIGINAL)).unwrap();
        editor.flush().await.unwrap();
        Self {
            account,
            owner,
            editor,
            stream,
        }
    }
    fn ops(&self) -> o::Operations {
        self.account.owned.as_ref().unwrap().operations.clone()
    }
    async fn consume(&self) -> (operation::Operation, Box<Applied>, Arc<owned_send::Send>) {
        let snapshot = self.editor.snapshot();
        let parent = operation::Operation::reserve(
            &self.editor,
            operation::Seed {
                request: operation::Request::Send,
                local: Arc::new(local(ORIGINAL)),
                book: None,
                serial: snapshot.serial,
                epoch: 7,
            },
        )
        .unwrap_or_else(|_| panic!("actual C3 reservation"));
        let lease = self
            .editor
            .prepare_review(Visit::new(), &local(ORIGINAL))
            .await
            .unwrap();
        let stream = self.stream.clone();
        let applied = self
            .editor
            .apply_review_owned(
                lease,
                Action::Consume(ConsumeKind::Send),
                parent.clone(),
                move || Ok(Some(stream)),
            )
            .await
            .unwrap();
        assert_eq!(applied.native_clear, NativeClear::NotAttempted);
        let held = parent
            .record()
            .evidence
            .lock()
            .unwrap()
            .owned_send
            .clone()
            .unwrap();
        assert!(
            held.consume
                .job()
                .unwrap()
                .result()
                .unwrap()
                .unwrap()
                .same(&applied.after)
        );
        (parent, applied, held)
    }
}

#[tokio::test]
async fn exact_owned_account_opens_editor_without_old_client_and_retains_lost_open_waiter() {
    let f = Box::pin(Fixture::new(false)).await;
    let opened = editor_open::Request {
        owner: f.owner.clone(),
        expected: Some(f.owner.review().await.unwrap()),
    }
    .begin()
    .unwrap_or_else(|_| panic!("opening reservation"));
    let id = opened.id().to_owned();
    drop(opened);
    let retained = f
        .account
        .retained_openings()
        .unwrap()
        .into_iter()
        .find(|v| v.id() == id)
        .unwrap();
    retained.ready().await;
    let editor = retained.result().unwrap();
    assert_eq!(editor.snapshot().view.local().composer.text, ORIGINAL);
    assert!(f.stream.is_live());
    f.account.initialize().await.unwrap();
    assert!(f.stream.is_live(), "no second AfterRestart");
    assert!(
        !f.account
            .owned
            .as_ref()
            .unwrap()
            .unsupported
            .load(Ordering::Acquire)
    );
    assert!(matches!(
        f.account.client(),
        Err(durable::Error::Unsupported)
    ));
    assert!(
        f.account
            .owned
            .as_ref()
            .unwrap()
            .unsupported
            .load(Ordering::Acquire)
    );
}

#[tokio::test]
async fn actual_named_inventory_installs_session_after_enrollment_without_reinitializing() {
    let f = Box::pin(Fixture::new(false)).await;
    let loaded = f.account.owned.as_ref().unwrap().inventory().await.unwrap();
    let before = f.owner.review().await.unwrap();
    assert!(
        loaded
            .managed()
            .get(&f.owner.scope().key)
            .unwrap()
            .same(&before)
    );
    let mut projection = session::Session::default();
    projection.select(f.account.id().into(), 7);
    let ticket = projection.begin_load().unwrap();
    projection.accept_load(ticket, &loaded).unwrap();
    let projected = projection
        .entry(f.account.id(), 7, &f.owner.scope().key)
        .unwrap();
    assert!(projected.same(&f.editor.snapshot().view));
    assert!(f.stream.is_live());
    assert!(
        !f.account
            .owned
            .as_ref()
            .unwrap()
            .unsupported
            .load(Ordering::Acquire)
    );
}

#[tokio::test]
async fn capacity_refusal_keeps_exact_command_and_newer_input_without_preserve_or_reconstruction() {
    let f = Box::pin(Fixture::new(false)).await;
    let ops = f.ops();
    loop {
        match ops.review(f.owner.scope().clone()) {
            Ok(_) => (),
            Err(e) => {
                assert_eq!(e.reason, o::AdmissionError::Capacity);
                break;
            }
        }
    }
    let before = ops.retained().len();
    f.editor.submit(&local(NEWER)).unwrap();
    assert!(matches!(
        f.editor.flush().await,
        Err(durable::Error::Unsupported)
    ));
    let held = f.editor.owned_write.lock().unwrap().clone().unwrap();
    assert_eq!(held.refusal(), Some(o::AdmissionError::Capacity));
    assert!(same_local(&held.original.local, &local(NEWER)));
    f.editor.submit(&local("Even newer")).unwrap();
    assert_eq!(
        f.editor.snapshot().pending.unwrap().composer.text,
        "Even newer"
    );
    f.editor.retry_storage().unwrap();
    assert!(matches!(
        f.editor.flush().await,
        Err(durable::Error::Unsupported)
    ));
    assert!(Arc::ptr_eq(
        &held,
        f.editor.owned_write.lock().unwrap().as_ref().unwrap()
    ));
    assert_eq!(ops.retained().len(), before);
    assert!(
        !ops.retained()
            .iter()
            .any(|v| v.family == "protected-preserve-version")
    );
}

#[tokio::test]
async fn actual_editor_newer_before_prepare_retains_frozen_send_and_exact_handoff_both_destinations()
 {
    for thread in [false, true] {
        Box::pin(send_and_handoff(thread)).await;
    }
}
async fn send_and_handoff(thread: bool) {
    let f = Box::pin(Fixture::new(thread)).await;
    let (parent, applied, held) = Box::pin(f.consume()).await;
    let intent = applied.after.row().state().consumption().unwrap().clone();
    f.editor.submit(&local(NEWER)).unwrap();
    f.editor.flush().await.unwrap();
    let before = f.owner.review().await.unwrap();
    assert!(
        before.row().state().pending().is_none(),
        "newer input precedes Prepare"
    );
    assert!(same_local(before.local(), &local(NEWER)));
    assert_eq!(before.row().state().consumption(), Some(&intent));
    let calls = AtomicUsize::new(0);
    let binding = crate::drafts::network::Binding::retained_route_test(f.account.id());
    let copy = parent.clone();
    let dispatched=held.dispatch_with(&parent,&binding,|request| {
        calls.fetch_add(1,Ordering::SeqCst);
        assert_eq!(request.method,"POST");
        assert_eq!(url::Url::parse(&request.url).unwrap().path(),"/api/chat.postMessage");
        let receipt=copy.record().evidence.lock().unwrap().receipt.clone().unwrap();
        let operation=receipt.operation();
        assert_eq!(operation.snapshot.as_ref(),Some(&intent.snapshot));
        let super_platinum_core::slack::client::RequestBody::Form(fields)=request.body else {panic!("real form builder")};
        assert_eq!(fields.iter().filter(|(k,_)|k=="client_msg_id").map(|(_,v)|v.as_str()).collect::<Vec<_>>(),vec![operation.client_msg_id.as_str()]);
        let mut message=json!({"ts":"200.000001","channel":"C1","user":"U1","client_msg_id":operation.client_msg_id});
        if thread {message["thread_ts"]=json!("100.000001");}
        std::future::ready(Ok(json!({"ok":true,"channel":"C1","ts":"200.000001","message":message})))
    }).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(dispatched.attempt.dispatched);
    assert!(same_local(dispatched.review.local(), &local(NEWER)));
    assert!(dispatched.review.row().state().pending().is_none());
    assert!(same_local(&f.editor.snapshot().view.local(), &local(NEWER)));
    f.editor.refresh_receipt().await.unwrap();
    f.editor
        .release_completion_owned(dispatched.review, intent, parent.clone())
        .await
        .unwrap();
    let inventory = f.owner.local_inventory().await.unwrap();
    assert!(inventory.row().is_none());
    assert!(same_local(inventory.local(), &local(NEWER)));
    assert!(
        parent
            .record()
            .evidence
            .lock()
            .unwrap()
            .local_lease
            .is_some()
    );
    f.editor.submit(&local("After handoff")).unwrap();
    f.editor.flush().await.unwrap();
    assert!(f.editor.snapshot().pending.is_none());
    assert!(same_local(
        f.owner.local_inventory().await.unwrap().local(),
        &local("After handoff")
    ));
    assert!(
        held.dispatch_with(&parent, &binding, |_| async { panic!("no second HTTP") })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn root_retirement_before_dispatch_preserves_consumption_and_sends_nothing() {
    let f = Box::pin(Fixture::new(false)).await;
    let (parent, applied, held) = Box::pin(f.consume()).await;
    parent.unknown();
    let binding = crate::drafts::network::Binding::retained_route_test(f.account.id());
    assert!(matches!(
        held.dispatch_with(&parent, &binding, |_| async { panic!("retired HTTP") })
            .await,
        Err(durable::Error::Stale)
    ));
    assert!(f.owner.review().await.unwrap().same(&applied.after));
    assert!(parent.record().evidence.lock().unwrap().receipt.is_none());
}

#[tokio::test]
async fn response_after_root_retirement_keeps_exact_raw_and_storage_without_reviving_publication() {
    let f = Box::pin(Fixture::new(false)).await;
    let (parent, _, held) = Box::pin(f.consume()).await;
    let binding = crate::drafts::network::Binding::retained_route_test(f.account.id());
    let (entered, at_http) = tokio::sync::oneshot::channel();
    let (release, response) = tokio::sync::oneshot::channel();
    let task_parent = parent.clone();
    let task_held = held.clone();
    let task = async move {
        task_held
            .dispatch_with(&task_parent, &binding, |_| {
                let receipt = task_parent
                    .record()
                    .evidence
                    .lock()
                    .unwrap()
                    .receipt
                    .clone()
                    .unwrap();
                let client = receipt.operation().client_msg_id.clone();
                async move {
                    entered.send(()).unwrap();
                    response.await.unwrap();
                    Ok(json!({"ok":true,"channel":"C1","ts":"200.000001",
                    "message":{"ts":"200.000001","channel":"C1","user":"U1","client_msg_id":client},
                    "unmodeled_fixture_evidence":{"retained":true}}))
                }
            })
            .await
    };
    // The real renderer execution evidence is deliberately !Send. Poll the
    // held HTTP future and its controller on this same test task; retirement
    // still follows actual HTTP entry and the durable newer-input checkpoint.
    let (dispatched, ()) = tokio::join!(Box::pin(task), async {
        at_http.await.unwrap();
        f.editor.submit(&local(NEWER)).unwrap();
        f.editor.flush().await.unwrap();
        parent.unknown();
        release.send(()).unwrap();
    });
    let dispatched = dispatched.unwrap();
    assert!(same_local(dispatched.review.local(), &local(NEWER)));
    let evidence = parent.record().evidence.lock().unwrap();
    assert_eq!(evidence.status, operation::Status::Unknown);
    assert!(evidence.recorded.as_ref().unwrap().same(&dispatched.review));
    assert_eq!(
        evidence.attempt.as_ref().unwrap().raw.as_ref().unwrap()["unmodeled_fixture_evidence"],
        json!({"retained":true})
    );
    assert_eq!(evidence.publication, operation::Publications::default());
    assert!(evidence.release.is_none());
    assert!(!parent.current());
}

#[tokio::test]
async fn unsupported_gap_teardown_revokes_real_stream_and_retains_gap_without_old_client_submission()
 {
    let f = Box::pin(Fixture::new(false)).await;
    let before = f.ops().retained().len();
    f.account
        .retire_stream(f.stream.clone(), durable::GapReason::SourceReplaced, None);
    assert!(!f.stream.is_live());
    assert!(f.account.team_blocked("T1"));
    assert_eq!(f.ops().retained().len(), before);
    // Existing repair can only reach the explicit Unsupported legacy boundary.
    f.account.retry_gap("T1").unwrap();
    tokio::task::yield_now().await;
    assert!(f.account.team_blocked("T1"));
    assert_eq!(f.ops().retained().len(), before);
}

async fn fault(f: &Fixture, point: durable::test_support::FaultPoint) {
    Held::new(f.ops().fixture_fault(f.owner.scope().clone(), point))
        .value()
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn owned_checkpoint_storage_retry_keeps_exact_old_job_and_then_newer_input() {
    let f = Box::pin(Fixture::new(false)).await;
    let before = f.owner.review().await.unwrap();
    fault(&f, durable::test_support::FaultPoint::WriterCommit).await;
    f.editor.submit(&local(NEWER)).unwrap();
    assert!(matches!(
        f.editor.flush().await,
        Err(durable::Error::Storage)
    ));
    let old = f.editor.owned_write.lock().unwrap().clone().unwrap();
    assert_eq!(
        old.checkpoint_state(),
        Some(o::JobState::Failed(durable::Error::Storage))
    );
    assert!(f.owner.review().await.unwrap().same(&before));
    f.editor
        .submit(&local("Typed after failed checkpoint"))
        .unwrap();
    assert!(same_local(&old.original.local, &local(NEWER)));
    f.editor.retry_storage().unwrap();
    f.editor.flush().await.unwrap();
    assert_eq!(old.checkpoint_state(), Some(o::JobState::Ready));
    assert_eq!(
        f.owner.review().await.unwrap().local().composer.text,
        "Typed after failed checkpoint"
    );
    assert!(
        !f.ops()
            .retained()
            .iter()
            .any(|x| x.family == "protected-preserve-version")
    );
}
async fn sent() -> (
    Fixture,
    operation::Operation,
    Dispatched,
    super_platinum_core::drafts::sync::ConsumeIntent,
) {
    let f = Box::pin(Fixture::new(false)).await;
    let (parent, applied, held) = Box::pin(f.consume()).await;
    let intent = applied.after.row().state().consumption().unwrap().clone();
    f.editor.submit(&local(NEWER)).unwrap();
    f.editor.flush().await.unwrap();
    let binding = crate::drafts::network::Binding::retained_route_test(f.account.id());
    let captured = parent.clone();
    let result=held.dispatch_with(&parent,&binding,move|_|{
        let receipt=captured.record().evidence.lock().unwrap().receipt.clone().unwrap();
        std::future::ready(Ok(json!({"ok":true,"channel":"C1","ts":"200.000001","message":{
            "channel":"C1","ts":"200.000001","user":"U1","client_msg_id":receipt.operation().client_msg_id}})))
    }).await.unwrap();
    f.editor.refresh_receipt().await.unwrap();
    (f, parent, result, intent)
}
#[tokio::test]
async fn start_local_rollback_retries_same_child_and_preserves_typing_during_handoff() {
    let (f, parent, result, intent) = Box::pin(sent()).await;
    fault(&f, durable::test_support::FaultPoint::StartLocalCommit).await;
    assert!(matches!(
        f.editor
            .release_completion_owned(result.review, intent, parent.clone())
            .await,
        Err(durable::Error::Storage)
    ));
    let held = parent
        .record()
        .evidence
        .lock()
        .unwrap()
        .owned_release
        .clone()
        .unwrap();
    assert!(parent.record().evidence.lock().unwrap().release.is_some());
    // The real transfer consumed the parent Release result. Retrying must
    // reattach to its retained child rather than expect this result again.
    assert!(held.held.job().unwrap().result().is_none());
    let before = f.owner.local_inventory().await.unwrap();
    assert!(before.row().is_none());
    assert!(same_local(before.local(), &local(NEWER)));
    f.editor
        .submit(&local("Typed during failed handoff"))
        .unwrap();
    f.editor.retry_owned_handoff(parent.clone()).await.unwrap();
    f.editor.flush().await.unwrap();
    assert!(Arc::ptr_eq(
        &held,
        parent
            .record()
            .evidence
            .lock()
            .unwrap()
            .owned_release
            .as_ref()
            .unwrap()
    ));
    assert!(parent.record().evidence.lock().unwrap().release.is_some());
    assert_eq!(
        f.ops()
            .retained()
            .iter()
            .filter(|x| x.family == "protected-release")
            .count(),
        1
    );
    assert_eq!(
        f.ops()
            .retained()
            .iter()
            .filter(|x| x.family == "protected-start-local")
            .count(),
        1
    );
    let after = f.owner.local_inventory().await.unwrap();
    assert!(after.row().is_none());
    assert_eq!(after.local().composer.text, "Typed during failed handoff");
    assert!(f.editor.snapshot().pending.is_none());
    let original_lease = parent
        .record()
        .evidence
        .lock()
        .unwrap()
        .local_lease
        .clone()
        .unwrap();
    let jobs_before = f.ops().retained().len();
    // A second successful waiter retrieves the same historical lease without
    // another Release, StartLocal, or disk write, even after newer local work.
    let reattached = held.finish(&parent).await.unwrap();
    assert_eq!(reattached.scope(), original_lease.scope());
    assert_eq!(reattached.writer_id(), original_lease.writer_id());
    assert_eq!(reattached.generation(), original_lease.generation());
    assert_eq!(reattached.initial_serial(), original_lease.initial_serial());
    assert_eq!(f.ops().retained().len(), jobs_before);
    assert!(f.owner.local_inventory().await.unwrap().same(&after));
}
#[tokio::test]
async fn local_fifo_storage_retry_keeps_failed_input_and_coalesced_newer_input() {
    let (f, parent, result, intent) = Box::pin(sent()).await;
    f.editor
        .release_completion_owned(result.review, intent, parent)
        .await
        .unwrap();
    let writer = f.editor.state.lock().unwrap().released.clone().unwrap();
    let initial_serial = writer.snapshot().serial;
    fault(&f, durable::test_support::FaultPoint::LocalCommit).await;
    f.editor.submit(&local("First local write")).unwrap();
    assert!(matches!(
        f.editor.flush().await,
        Err(durable::Error::Storage)
    ));
    // Retained means outstanding custody, not a permanent successful-write log.
    // Before repair, keep the exact failed job/input and refuse every receipt.
    let retained: Vec<_> = f
        .ops()
        .retained()
        .into_iter()
        .filter(|job| job.family == "protected-write-local")
        .collect();
    assert_eq!(retained.len(), 1);
    let key = retained[0].key.clone();
    let first = f.ops().resume_write_local(&key).unwrap();
    let input = first.input();
    assert_eq!(first.state(), o::JobState::Failed(durable::Error::Storage));
    assert!(matches!(
        first.local_result(),
        Some(Err(durable::Error::Storage))
    ));
    assert!(first.observe_local_write().is_err());
    assert!(first.observe_local_write_settlement().is_err());
    assert_eq!(input.predecessor, initial_serial);
    assert_eq!(input.serial, initial_serial + 1);
    assert!(same_local(&input.expected, &local(NEWER)));
    assert!(same_local(&input.local, &local("First local write")));
    let failed = writer.snapshot();
    assert_eq!(failed.serial, input.serial);
    assert_eq!(failed.failed_inputs.len(), 1);
    assert_eq!(failed.failed_inputs[0].0, input.serial);
    assert!(same_local(&failed.failed_inputs[0].1, &input.local));
    assert!(matches!(
        f.editor.submit(&local("Newest local write")),
        Err(durable::Error::Pending)
    ));
    assert_eq!(
        f.owner
            .local_inventory()
            .await
            .unwrap()
            .local()
            .composer
            .text,
        NEWER
    );
    let coalesced = writer.snapshot();
    assert_eq!(coalesced.serial, input.serial);
    assert_eq!(coalesced.failed_inputs.len(), 1);
    assert!(same_local(&coalesced.failed_inputs[0].1, &input.local));
    assert!(same_local(
        coalesced.pending.as_ref().unwrap(),
        &local("Newest local write")
    ));
    assert!(Arc::ptr_eq(
        &input,
        &f.ops().resume_write_local(&key).unwrap().input()
    ));
    assert_eq!(
        f.ops()
            .retained()
            .iter()
            .filter(|job| job.family == "protected-write-local")
            .count(),
        1
    );
    // Pause the real freshness read after the retry applies, before the lane
    // acknowledges its settlement. Observing/dropping a receipt is not cleanup.
    let (at_read, continue_read) = f.account.owned.as_ref().unwrap().hold_next_local_read();
    f.editor.retry_local_storage().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), at_read)
        .await
        .unwrap()
        .unwrap();
    assert!(first.observe_local_write().is_err());
    let (applied, history, dropped) = first.observe_local_write_settlement().unwrap();
    assert!(same_local(applied.local(), &input.local));
    assert_eq!(history.failed_submissions().len(), 1);
    assert!(history.failed_submissions()[0] < history.successful_submission());
    assert!(!history.operation_pending());
    drop(dropped);
    assert_eq!(
        f.ops()
            .retained()
            .iter()
            .filter(|job| job.family == "protected-write-local")
            .count(),
        1
    );
    assert!(Arc::ptr_eq(&input, &first.input()));
    continue_read.send(()).unwrap();
    f.editor.flush().await.unwrap();
    assert_eq!(
        f.owner
            .local_inventory()
            .await
            .unwrap()
            .local()
            .composer
            .text,
        "Newest local write"
    );
    assert!(f.editor.snapshot().pending.is_none());
    // The historical result proves the first immutable write really applied;
    // the distinct next serial and fresh inventory prove the newer FIFO write.
    assert!(Arc::ptr_eq(&input, &first.input()));
    let Some(Ok(o::LocalOutcome::Applied(first_applied))) = first.local_result() else {
        panic!("exact repaired first write must retain its Applied result")
    };
    assert!(same_local(first_applied.local(), &input.local));
    let settled = writer.snapshot();
    assert_eq!(settled.serial, input.serial + 1);
    assert!(!settled.running);
    assert!(settled.error.is_none());
    assert!(settled.failed_inputs.is_empty());
    assert!(settled.pending.is_none());
    let current = f.owner.local_inventory().await.unwrap();
    assert!(same_local(current.local(), &local("Newest local write")));
    assert!(settled.inventory.as_ref().unwrap().same(&current));
    // Exact settlement acknowledgment retires the repaired slot/history; the
    // clean receipt retires the second write. Neither can be replayed.
    assert!(matches!(
        f.ops().resume_write_local(&key),
        Err(o::AdmissionError::Stale)
    ));
    assert_eq!(first.submit(), Err(o::AdmissionError::Stale));
    assert_eq!(first.retry_storage(), Err(o::AdmissionError::Stale));
    assert!(first.observe_local_write_settlement().is_err());
    assert_eq!(
        f.ops()
            .retained()
            .iter()
            .filter(|x| x.family == "protected-write-local")
            .count(),
        0
    );
}

async fn retire_during_handoff_read(retry: bool) {
    let (f, parent, result, intent) = Box::pin(sent()).await;
    let first = if retry {
        fault(&f, durable::test_support::FaultPoint::StartLocalCommit).await;
        assert!(matches!(
            f.editor
                .release_completion_owned(result.review, intent, parent.clone())
                .await,
            Err(durable::Error::Storage)
        ));
        None
    } else {
        Some((result.review, intent))
    };
    let (at_read, release) = f.account.owned.as_ref().unwrap().hold_next_local_read();
    let editor = f.editor.clone();
    let task_parent = parent.clone();
    let task = tokio::spawn(async move {
        match first {
            Some((review, intent)) => {
                editor
                    .release_completion_owned(review, intent, task_parent)
                    .await
            }
            None => editor.retry_owned_handoff(task_parent).await,
        }
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), at_read)
        .await
        .unwrap()
        .unwrap();
    assert!(
        parent
            .record()
            .evidence
            .lock()
            .unwrap()
            .local_lease
            .is_some()
    );
    f.editor
        .submit(&local("Typing while inventory read held"))
        .unwrap();
    parent.unknown();
    release.send(()).unwrap();
    assert!(matches!(task.await.unwrap(), Err(durable::Error::Stale)));
    assert!(f.editor.state.lock().unwrap().released.is_none());
    assert!(f.editor.state.lock().unwrap().handoff_lost);
    assert_eq!(
        f.editor.snapshot().pending.unwrap().composer.text,
        "Typing while inventory read held"
    );
    assert_eq!(
        f.owner
            .local_inventory()
            .await
            .unwrap()
            .local()
            .composer
            .text,
        NEWER
    );
    let evidence = parent.record().evidence.lock().unwrap();
    assert_eq!(evidence.status, operation::Status::Unknown);
    assert!(evidence.local_lease.is_some());
    assert!(evidence.attempt.is_some());
    assert_eq!(evidence.publication, operation::Publications::default());
    assert_eq!(
        f.ops()
            .retained()
            .iter()
            .filter(|x| x.family == "protected-release")
            .count(),
        1
    );
    assert_eq!(
        f.ops()
            .retained()
            .iter()
            .filter(|x| x.family == "protected-start-local")
            .count(),
        1
    );
}
#[tokio::test]
async fn parent_retired_during_actual_handoff_inventory_read_cannot_install_current_local_writer() {
    Box::pin(retire_during_handoff_read(false)).await;
}
#[tokio::test]
async fn parent_retired_during_retry_inventory_read_keeps_same_completed_child_without_install() {
    Box::pin(retire_during_handoff_read(true)).await;
}

mod inventory;

#[path = "owned_route_tests/retirement.rs"]
mod retirement;

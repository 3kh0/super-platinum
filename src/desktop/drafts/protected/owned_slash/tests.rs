//! Actual core owner/worker and Editor. No test below claims a native DOM clear.
use super::*;
use crate::drafts::protected::owned_route::o;
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: super_platinum_core::ComposerState::with_text(text),
        attachments: vec![],
    }
}
fn done<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: o::StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), o::JobState::Ready);
    job.result().unwrap().unwrap()
}
struct Fixture {
    account: Arc<Account>,
    editor: Arc<editor::Editor>,
    context: Arc<o::SlashFixture>,
    stream: durable::Stream,
}
impl Fixture {
    async fn new(thread: bool) -> Self {
        Self::with_outcome(thread, o::SlashFixtureOutcome::HeldClear).await
    }
    async fn with_outcome(thread: bool, outcome: o::SlashFixtureOutcome) -> Self {
        let (initialized, context, stream) = tokio::task::spawn_blocking(move || {
            let target = if thread {
                o::SlashFixtureTarget::Thread
            } else {
                o::SlashFixtureTarget::Channel
            };
            let (route, context) = o::OwnedRoute::fresh_slash_fixture(target, outcome).unwrap();
            let inspected = route.inspect().unwrap().wait().unwrap();
            assert_eq!(inspected.metadata().kind, o::StoreKind::Protected7);
            let initialized = inspected
                .open()
                .unwrap()
                .initialize_cold(&o::FactoryIdentity::default())
                .unwrap()
                .wait()
                .unwrap();
            let ops = initialized.operations();
            let scope = context.scope().clone();
            let review = done(
                ops.enroll(done(ops.local_inventory(scope.clone()).unwrap()))
                    .unwrap(),
            );
            let expected = done(
                ops.stream_state(o::TeamInput {
                    scope: scope.clone(),
                })
                .unwrap(),
            );
            let stream = done(ops.open_stream(o::OpenInput { scope, expected }).unwrap());
            done(
                ops.verify_local(o::MutationInput {
                    stream: stream.clone(),
                    expected: review,
                })
                .unwrap(),
            );
            (initialized, Arc::new(context), stream)
        })
        .await
        .unwrap();
        let account = Account::from_retained(initialized);
        let editor = editor::Editor::open(account.owner(context.scope().key.clone()).unwrap())
            .await
            .unwrap();
        editor.submit(&local("/example original")).unwrap();
        editor.flush().await.unwrap();
        Self {
            account,
            editor,
            context,
            stream,
        }
    }
    fn parent(&self) -> operation::Operation {
        operation::Operation::reserve(
            &self.editor,
            operation::Seed {
                request: operation::Request::Slash {
                    context: Context::fixture(self.context.clone()),
                    choice: self.context.catalog().choice(0).unwrap(),
                    client_token: "fixture-parent".into(),
                },
                local: Arc::new(local("/example original")),
                book: None,
                serial: self.editor.snapshot().serial,
                epoch: 1,
            },
        )
        .unwrap_or_else(|_| panic!("parent refused"))
    }
    async fn child(&self, parent: &operation::Operation) -> Arc<Slash> {
        let lease = self
            .editor
            .prepare_review(Visit::new(), &local("/example original"))
            .await
            .unwrap();
        let held = self
            .editor
            .prepare_slash(lease, parent, || Ok(self.stream.clone()))
            .await
            .unwrap();
        assert!(
            parent
                .record()
                .evidence
                .lock()
                .unwrap()
                .owned_slash
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(v, &held))
        );
        held.prepare_dispatch().await.unwrap();
        held
    }
}
#[tokio::test]
async fn actual_editor_slash_channel_thread_wait_and_once_reconcile() {
    for thread in [false, true] {
        let f = Fixture::new(thread).await;
        let parent = f.parent();
        let held = f.child(&parent).await;
        let execute = held.execute();
        tokio::pin!(execute);
        tokio::select! { v=&mut execute=>panic!("held unexpectedly {v:?}"),v=f.context.wait_dispatched()=>v.unwrap() }
        assert!(!held.has_fresh_clear().unwrap());
        f.context.release_result().unwrap();
        execute.await.unwrap();
        let after = f.editor.slash_canonical(&held).await.unwrap();
        assert!(after.local().empty());
        assert!(held.has_fresh_clear().unwrap());
        held.wait_completion().await.unwrap();
        held.wait_completion().await.unwrap();
        assert!(held.has_fresh_clear().unwrap());
        assert!(f.editor.reconcile_slash(&held, &after, || true).is_err());
        held.consume_fresh_clear().unwrap();
        assert!(held.consume_fresh_clear().is_err());
        held.wait_completion().await.unwrap();
        assert!(!held.has_fresh_clear().unwrap());
        assert!(held.consume_fresh_clear().is_err());
        assert!(f.editor.reconcile_slash(&held, &after, || false).is_err());
        f.editor.reconcile_slash(&held, &after, || true).unwrap();
        assert!(f.editor.snapshot().view.local().empty());
        assert_eq!(f.context.requests().len(), 1);
        assert!(held.execute().await.is_err());
        // Continued real input retains registered writer lineage after canonical clear.
        f.editor.submit(&local("new after clear")).unwrap();
        f.editor.flush().await.unwrap();
        assert_eq!(
            f.editor.snapshot().view.local().composer.text,
            "new after clear"
        );
        assert_eq!(f.account.retained_operations().unwrap().len(), 1);
    }
}
#[tokio::test]
async fn actual_editor_input_during_held_dispatch_preserves_newer_and_no_clear() {
    let f = Fixture::new(false).await;
    let parent = f.parent();
    let held = f.child(&parent).await;
    let execution = held.execute();
    tokio::pin!(execution);
    tokio::select! { v=&mut execution=>panic!("held unexpectedly {v:?}"),v=f.context.wait_dispatched()=>v.unwrap() }
    f.editor.submit(&local("/example newer")).unwrap();
    f.editor.flush().await.unwrap();
    assert!(!f.context.is_live());
    assert!(!f.editor.slash_current(&held));
    f.context.release_result().unwrap();
    execution.await.unwrap();
    let after = f.editor.slash_canonical(&held).await.unwrap();
    assert_eq!(after.local().composer.text, "/example newer");
    assert!(!held.has_fresh_clear().unwrap());
    assert!(held.consume_fresh_clear().is_err());
    assert!(f.editor.reconcile_slash(&held, &after, || true).is_err());
    assert_eq!(
        f.editor.snapshot().view.local().composer.text,
        "/example newer"
    );
    assert_eq!(f.context.requests().len(), 1);
}
#[tokio::test]
async fn retirement_after_handoff_retains_real_result_without_reviving_parent() {
    let f = Fixture::new(true).await;
    let parent = f.parent();
    let held = f.child(&parent).await;
    let execution = held.execute();
    tokio::pin!(execution);
    tokio::select! { v=&mut execution=>panic!("held unexpectedly {v:?}"),v=f.context.wait_dispatched()=>v.unwrap() }
    parent.unknown();
    assert!(!f.context.is_live());
    f.context.release_result().unwrap();
    execution.await.unwrap();
    assert!(held.completion().unwrap().report().is_some());
    assert!(held.completion().unwrap().terminal().is_some());
    parent.finish(Ok(operation::Status::Finished));
    assert_eq!(parent.ready().await.unwrap(), operation::Status::Unknown);
    assert_eq!(f.context.requests().len(), 1);
}
#[tokio::test]
async fn stale_review_and_foreign_parent_refuse_before_actual_reserve() {
    let f = Fixture::new(false).await;
    let parent = f.parent();
    let lease = f
        .editor
        .prepare_review(Visit::new(), &local("/example original"))
        .await
        .unwrap();
    f.editor.submit(&local("/example changed")).unwrap();
    f.editor.flush().await.unwrap();
    assert!(
        f.editor
            .prepare_slash(lease, &parent, || Ok(f.stream.clone()))
            .await
            .is_err()
    );
    assert!(
        parent
            .record()
            .evidence
            .lock()
            .unwrap()
            .owned_slash
            .is_none()
    );
    assert!(f.context.requests().is_empty());
    let g = Fixture::new(false).await;
    let lease = g
        .editor
        .prepare_review(Visit::new(), &local("/example original"))
        .await
        .unwrap();
    assert!(
        g.editor
            .prepare_slash(lease, &parent, || Ok(g.stream.clone()))
            .await
            .is_err()
    );
    assert!(g.context.requests().is_empty());
}

#[path = "tests/archive.rs"]
mod archive;

//! Actual Editor + private owned queue. Closed oracle consumes the normal arm;
//! no loopback transport exception, arbitrary result setter or native proof.
use super::*;
struct Closed {
    f: Fixture,
    oracle: o::HostedFixture,
}
impl Closed {
    async fn new() -> Self {
        let (initialized, oracle, stream) = tokio::task::spawn_blocking(|| {
            let (route, oracle) =
                o::OwnedRoute::fresh_hosted_fixture(o::HostedFixtureCase::Caption).unwrap();
            let before = route.inspect().unwrap().wait().unwrap();
            route.commit(&before).unwrap().wait().unwrap();
            let review = route.inspect().unwrap().wait().unwrap();
            route.initialize_slash(&review).unwrap().wait().unwrap();
            let review = route.inspect().unwrap().wait().unwrap();
            route.initialize_hosted(&review).unwrap().wait().unwrap();
            let initialized = route
                .inspect()
                .unwrap()
                .wait()
                .unwrap()
                .open()
                .unwrap()
                .initialize_cold(&o::FactoryIdentity::default())
                .unwrap()
                .wait()
                .unwrap();
            let ops = initialized.operations();
            let scope = oracle.scope().clone();
            let local = done(ops.local_inventory(scope.clone()).unwrap());
            let review = done(ops.enroll(local).unwrap());
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
            (initialized, oracle, stream)
        })
        .await
        .unwrap();
        let account = Account::from_retained(initialized);
        let editor = editor::Editor::open(account.owner(oracle.scope().key.clone()).unwrap())
            .await
            .unwrap();
        editor
            .submit(&LocalDraft {
                composer: super_platinum_core::ComposerState::with_text(oracle.caption()),
                ..Default::default()
            })
            .unwrap();
        editor.flush().await.unwrap();
        Self {
            f: Fixture {
                account,
                editor,
                stream,
            },
            oracle,
        }
    }
    async fn armed(&self) -> (o::HostedCompletion, o::HostedAttempt) {
        let f = &self.f;
        let c = f.editor.hosted();
        let s = c.load(&f.editor).await.unwrap();
        let v = c
            .insert(s.version, o::HostedFixture::file("F00000002").unwrap())
            .unwrap();
        c.insert(v, o::HostedFixture::file("F00000001").unwrap())
            .unwrap();
        let saved = c.persist(&f.editor, f.stream.clone()).await.unwrap();
        let input = c
            .begin_send(&f.editor, f.stream.clone(), saved.version)
            .unwrap();
        let completion = c.claim_send().await.unwrap();
        let destination = o::HostedFixture::channel();
        let mut facts =
            super_platinum_core::slack::file_share_policy::PostingFacts::new("C00000001".into());
        assert!(facts.observe(&destination, None));
        let fresh =
            o::HostedFresh::new(input.clone(), input.input(), &destination, &facts, None).unwrap();
        let attempt = completion.arm(fresh, &self.oracle.session()).unwrap();
        (completion, attempt)
    }
    async fn accept(&self) -> o::HostedCompletion {
        let (completion, attempt) = Box::pin(self.armed()).await;
        self.oracle.complete(attempt).unwrap();
        self.f.editor.hosted().settle_send().await.unwrap();
        self.f.editor.hosted().acknowledge_send().unwrap();
        completion
    }
}
#[tokio::test]
async fn accepted_without_native_clear_keeps_actor_text_until_explicit_local_review() {
    let t = Box::pin(Closed::new()).await;
    let c = t.f.editor.hosted();
    let completion = Box::pin(t.accept()).await;
    let before = c.snapshot().unwrap();
    assert!(before.sending && before.can_retire);
    assert_eq!(before.completed.len(), 1);
    assert!(
        c.begin_send(&t.f.editor, t.f.stream.clone(), before.version)
            .is_err()
    );
    let id = before.completed[0].record().id().to_owned();
    c.retire_reviewed(&t.f.editor, &before, &id).await.unwrap();
    let after = c.snapshot().unwrap();
    assert!(after.completed.is_empty() && after.selection.ids().is_empty() && !after.sending);
    assert_eq!(
        t.f.editor.snapshot().view.local().composer.text,
        t.oracle.caption()
    );
    assert!(completion.returned().is_some());
    assert!(c.clear_basis(&t.f.editor).is_err());
    assert!(c.retire_reviewed(&t.f.editor, &before, &id).await.is_err());
    assert_eq!(t.oracle.request().unwrap()["path"], "/api/files.share");
}
#[tokio::test]
async fn returned_storage_failure_retries_original_claim_value_without_a_second_arm() {
    let t = Box::pin(Closed::new()).await;
    let c = t.f.editor.hosted();
    let (completion, attempt) = Box::pin(t.armed()).await;
    done(
        t.f.ops()
            .fixture_fault(
                t.f.editor.scope().clone(),
                durable::test_support::FaultPoint::MutationReceipt,
            )
            .unwrap(),
    );
    t.oracle.complete(attempt).unwrap();
    assert!(matches!(
        c.settle_send().await,
        Err(durable::Error::Storage)
    ));
    let raw = completion.returned().unwrap();
    let request = t.oracle.request().unwrap();
    assert!(c.snapshot().unwrap().retry_result);
    c.retry_returned_storage().await.unwrap();
    assert!(Arc::ptr_eq(&raw, &completion.returned().unwrap()));
    assert_eq!(t.oracle.request().unwrap(), request);
    assert_eq!(c.snapshot().unwrap().completed.len(), 1);
    c.acknowledge_send().unwrap();
    assert!(c.retry_returned_storage().await.is_err());
    assert!(
        c.begin_send(
            &t.f.editor,
            t.f.stream.clone(),
            c.snapshot().unwrap().version
        )
        .is_err()
    );
}
#[tokio::test]
async fn retirement_storage_failure_retries_same_owned_job_and_keeps_actual_returned_value() {
    let t = Box::pin(Closed::new()).await;
    let c = t.f.editor.hosted();
    let completion = Box::pin(t.accept()).await;
    let raw = completion.returned().unwrap();
    let before = c.snapshot().unwrap();
    let id = before.completed[0].record().id().to_owned();
    done(
        t.f.ops()
            .fixture_fault(
                t.f.editor.scope().clone(),
                durable::test_support::FaultPoint::MutationReceipt,
            )
            .unwrap(),
    );
    assert!(matches!(
        c.retire_reviewed(&t.f.editor, &before, &id).await,
        Err(durable::Error::Storage)
    ));
    assert!(c.snapshot().unwrap().retry_retirement);
    let held = c.state.lock().unwrap().retirement.clone().unwrap();
    let input = held.job().unwrap().input();
    c.retry_retirement().await.unwrap();
    assert!(Arc::ptr_eq(&input, &held.job().unwrap().input()));
    assert!(Arc::ptr_eq(&raw, &completion.returned().unwrap()));
    assert!(!c.snapshot().unwrap().sending);
    assert!(c.retry_retirement().await.is_err());
    assert!(
        t.f.ops()
            .resume_hosted_retirement(&held.job().unwrap().key())
            .is_err()
    );
}
#[tokio::test]
async fn fresh_editor_restores_completed_review_without_dispatch_or_native_clear_authority() {
    let t = Box::pin(Closed::new()).await;
    Box::pin(t.accept()).await;
    let restored = editor::Editor::open(t.f.account.owner(t.f.editor.scope().key.clone()).unwrap())
        .await
        .unwrap();
    let c = restored.hosted();
    let snapshot = c.load(&restored).await.unwrap();
    assert_eq!(snapshot.completed.len(), 1);
    assert!(snapshot.sending);
    assert!(
        c.begin_send(&restored, t.f.stream.clone(), snapshot.version)
            .is_err()
    );
    assert!(c.clear_basis(&restored).is_err());
    c.retire_reviewed(&restored, &snapshot, snapshot.completed[0].record().id())
        .await
        .unwrap();
    assert!(!c.snapshot().unwrap().sending);
    assert!(restored.snapshot().view.local().empty());
}
#[tokio::test]
async fn foreign_closed_queue_cannot_complete_another_original_attempt() {
    let t = Box::pin(Closed::new()).await;
    let other = Box::pin(Closed::new()).await;
    let (completion, attempt) = Box::pin(t.armed()).await;
    assert!(other.oracle.complete(attempt).is_err());
    assert!(completion.returned().is_none());
    assert!(t.oracle.request().is_none());
    assert!(other.oracle.request().is_none());
    assert!(t.f.editor.hosted().snapshot().unwrap().sending);
}

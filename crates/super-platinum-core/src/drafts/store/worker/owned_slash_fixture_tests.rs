//! Real fresh owner, schemas, cold load, storage queue and completion algorithm.
use super::*;
use p::owned_store::{FactoryIdentity, OwnedRoute, StoreKind};

fn done<I: Send + Sync + 'static, T: Send + Clone + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    job.result().unwrap().unwrap()
}
struct Fixture {
    context: SlashFixture,
    ops: Operations,
    stream: p::Stream,
    writer: p::Writer,
    review: p::Review,
}
impl Fixture {
    fn new(target: SlashFixtureTarget, outcome: SlashFixtureOutcome) -> Self {
        let (route, context) = OwnedRoute::fresh_slash_fixture(target, outcome).unwrap();
        let inspected = route.inspect().unwrap().wait().unwrap();
        assert_eq!(inspected.metadata().kind, StoreKind::Protected7);
        // Existing Protected7 is inspection/open, not activation Commit again.
        let opened = inspected.open().unwrap();
        let initialized = opened
            .initialize_cold(&FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap();
        let ops = initialized.operations();
        let scope = context.scope().clone();
        let review = done(
            ops.enroll(done(ops.local_inventory(scope.clone()).unwrap()))
                .unwrap(),
        );
        let state = done(
            ops.stream_state(TeamInput {
                scope: scope.clone(),
            })
            .unwrap(),
        );
        let stream = done(
            ops.open_stream(OpenInput {
                scope,
                expected: state,
            })
            .unwrap(),
        );
        let review = done(
            ops.verify_local(MutationInput {
                stream: stream.clone(),
                expected: review,
            })
            .unwrap(),
        );
        let writer = done(ops.register(review.clone()).unwrap());
        let job = ops
            .checkpoint(CheckpointInput {
                writer: writer.clone(),
                serial: 1,
                expected: review,
                local: sync::tests::local("/example argument"),
                direct: None,
            })
            .unwrap();
        job.submit().unwrap();
        assert_eq!(job.wait().unwrap(), JobState::Ready);
        let WriteOutcome::Applied(review) = job.write_result().unwrap().unwrap() else {
            panic!("checkpoint");
        };
        Self {
            context,
            ops,
            stream,
            writer,
            review,
        }
    }
    fn input(&self) -> SlashInput {
        self.context
            .prepare(
                &self.ops,
                self.stream.clone(),
                self.review.clone(),
                self.writer.clone(),
                1,
                &self.context.catalog().choice(0).unwrap(),
                "fixture-client-token",
            )
            .unwrap()
    }
    fn completion(&self, input: SlashInput) -> SlashCompletion {
        let reserve = self.ops.slash(input).unwrap();
        reserve.submit().unwrap();
        assert_eq!(reserve.wait().unwrap(), JobState::Ready);
        let claim = reserve.claim().unwrap();
        claim.submit().unwrap();
        assert_eq!(claim.wait().unwrap(), JobState::Ready);
        claim.completion().unwrap()
    }
}

#[tokio::test]
async fn fresh_context_channel_thread_exact_once_and_public_clear_wrapper() {
    for target in [SlashFixtureTarget::Channel, SlashFixtureTarget::Thread] {
        let f = Fixture::new(target, SlashFixtureOutcome::Clear);
        let foreign = Fixture::new(target, SlashFixtureOutcome::Clear);
        let input = f.input();
        let c = f.completion(input.clone());
        let other = foreign.completion(foreign.input());
        assert!(matches!(
            foreign.context.execute(&c).await,
            Err(SlashExecuteError::BeforeDispatch(AdmissionError::Stale))
        ));
        assert!(foreign.context.requests().is_empty());
        f.context.execute(&c).await.unwrap();
        assert_eq!(c.wait().unwrap(), JobState::Ready);
        let terminal = c.terminal().unwrap();
        let wrapper = c.take_fresh_clear().unwrap();
        assert!(wrapper.matches(input.prior(), &terminal));
        assert!(wrapper.belongs_to(&c));
        assert!(!wrapper.belongs_to(&other));
        let wrapper = match wrapper.consume(&other, input.prior(), &terminal) {
            Err(original) => original,
            Ok(()) => panic!("foreign holder"),
        };
        let resumed = f.ops.resume_slash_completion(&c.key()).unwrap();
        assert!(wrapper.belongs_to(&resumed));
        assert!(resumed.take_fresh_clear().is_none());
        assert!(wrapper.consume(&resumed, input.prior(), &terminal).is_ok());
        assert!(f.context.execute(&resumed).await.is_err());
        let requests = f.context.requests();
        assert_eq!(requests.len(), 1);
        let field = |name: &str| {
            requests[0]
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(field("command"), Some("/example"));
        assert_eq!(field("client_token"), Some("fixture-client-token"));
        assert_eq!(field("thread_ts"), f.context.scope().key.thread.as_deref());
        assert_eq!(field("token"), None);
        let observed = done(f.ops.review(f.context.scope().clone()).unwrap());
        assert!(observed.local().empty());
    }
}

#[tokio::test]
async fn closed_nonclearing_outcomes_retain_exact_input_and_no_clear() {
    for outcome in [
        SlashFixtureOutcome::KeepInput,
        SlashFixtureOutcome::Rejected,
        SlashFixtureOutcome::Unknown,
        SlashFixtureOutcome::NotSent,
    ] {
        let f = Fixture::new(SlashFixtureTarget::Channel, outcome);
        let c = f.completion(f.input());
        f.context.execute(&c).await.unwrap();
        assert_eq!(c.wait().unwrap(), JobState::Ready);
        assert!(c.report().is_some());
        assert!(c.terminal().is_some());
        assert!(c.take_fresh_clear().is_none());
        let observed = done(f.ops.review(f.context.scope().clone()).unwrap());
        assert_eq!(
            serde_json::to_value(observed.local()).unwrap(),
            serde_json::to_value(f.review.local()).unwrap()
        );
        assert_eq!(f.context.requests().len(), 1);
        assert!(f.context.execute(&c).await.is_err());
    }
}

#[test]
fn context_rejects_foreign_choice_owner_writer_and_repeated_preparation() {
    let f = Fixture::new(SlashFixtureTarget::Channel, SlashFixtureOutcome::Clear);
    let other = Fixture::new(SlashFixtureTarget::Channel, SlashFixtureOutcome::Clear);
    let choice = f.context.catalog().choice(0).unwrap();
    assert!(
        f.context
            .prepare(
                &other.ops,
                f.stream.clone(),
                f.review.clone(),
                f.writer.clone(),
                1,
                &choice,
                "token"
            )
            .is_err()
    );
    assert!(
        f.context
            .prepare(
                &f.ops,
                f.stream.clone(),
                other.review.clone(),
                f.writer.clone(),
                1,
                &choice,
                "token"
            )
            .is_err()
    );
    assert!(
        f.context
            .prepare(
                &f.ops,
                f.stream.clone(),
                f.review.clone(),
                other.writer.clone(),
                1,
                &choice,
                "token"
            )
            .is_err()
    );
    assert!(
        f.context
            .prepare(
                &f.ops,
                f.stream.clone(),
                f.review.clone(),
                f.writer.clone(),
                1,
                &other.context.catalog().choice(0).unwrap(),
                "token"
            )
            .is_err()
    );
    f.input();
    assert!(
        f.context
            .prepare(
                &f.ops,
                f.stream.clone(),
                f.review.clone(),
                f.writer.clone(),
                1,
                &choice,
                "token"
            )
            .is_err()
    );
}

#[tokio::test]
async fn permanent_revocation_before_arm_and_drop_cannot_dispatch() {
    let f = Fixture::new(SlashFixtureTarget::Channel, SlashFixtureOutcome::Clear);
    let input = f.input();
    let c = f.completion(input);
    f.context.revoke();
    assert!(matches!(
        f.context.execute(&c).await,
        Err(SlashExecuteError::BeforeDispatch(_))
    ));
    assert!(f.context.requests().is_empty());
    assert!(c.report().is_none());
    let f = Fixture::new(SlashFixtureTarget::Channel, SlashFixtureOutcome::Clear);
    let c = f.completion(f.input());
    drop(f.context);
    assert!(matches!(
        c.execute().await,
        Err(SlashExecuteError::BeforeDispatch(_))
    ));
    assert!(c.report().is_none());
}

#[tokio::test]
async fn ordinary_execute_on_fixture_carrier_is_not_sent_without_network() {
    let f = Fixture::new(SlashFixtureTarget::Channel, SlashFixtureOutcome::Clear);
    let c = f.completion(f.input());
    c.execute().await.unwrap();
    assert_eq!(c.wait().unwrap(), JobState::Ready);
    assert!(matches!(
        c.terminal().unwrap().phase(),
        crate::slash_commands::controller::Phase::NotSent
    ));
    assert!(c.take_fresh_clear().is_none());
    assert!(f.context.requests().is_empty());
}

#[test]
fn thread_broadcast_is_refused_by_actual_pairing() {
    let f = Fixture::new(SlashFixtureTarget::Thread, SlashFixtureOutcome::Clear);
    let mut local = f.review.local().clone();
    local.composer.reply_broadcast = true;
    let job = f
        .ops
        .checkpoint(CheckpointInput {
            writer: f.writer.clone(),
            serial: 2,
            expected: f.review.clone(),
            local,
            direct: None,
        })
        .unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let WriteOutcome::Applied(review) = job.write_result().unwrap().unwrap() else {
        panic!("checkpoint")
    };
    assert!(matches!(
        f.context.prepare(
            &f.ops,
            f.stream.clone(),
            review,
            f.writer.clone(),
            2,
            &f.context.catalog().choice(0).unwrap(),
            "token"
        ),
        Err(AdmissionError::Unsupported)
    ));
}

#[test]
fn storage_still_refuses_stale_review_and_wrong_writer_serial() {
    let f = Fixture::new(SlashFixtureTarget::Channel, SlashFixtureOutcome::Clear);
    let stale = f.input();
    let job = f
        .ops
        .checkpoint(CheckpointInput {
            writer: f.writer.clone(),
            serial: 2,
            expected: f.review.clone(),
            local: sync::tests::local("/example newer input"),
            direct: None,
        })
        .unwrap();
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let reserve = f.ops.slash(stale).unwrap();
    reserve.submit().unwrap();
    assert_eq!(reserve.wait().unwrap(), JobState::Failed(p::Error::Stale));
    assert!(f.context.requests().is_empty());

    let f = Fixture::new(SlashFixtureTarget::Channel, SlashFixtureOutcome::Clear);
    let input = f
        .context
        .prepare(
            &f.ops,
            f.stream.clone(),
            f.review.clone(),
            f.writer.clone(),
            2,
            &f.context.catalog().choice(0).unwrap(),
            "token",
        )
        .unwrap();
    let reserve = f.ops.slash(input).unwrap();
    reserve.submit().unwrap();
    assert_eq!(reserve.wait().unwrap(), JobState::Ready);
    let claim = reserve.claim().unwrap();
    claim.submit().unwrap();
    assert_eq!(claim.wait().unwrap(), JobState::Failed(p::Error::Stale));
    assert!(f.context.requests().is_empty());
}

#[test]
fn seeded_schema_reopens_through_actual_inspection_and_cold_load() {
    let ticket = {
        let (route, context) = OwnedRoute::fresh_slash_fixture(
            SlashFixtureTarget::Channel,
            SlashFixtureOutcome::Clear,
        )
        .unwrap();
        let inspected = route.inspect().unwrap().wait().unwrap();
        assert_eq!(inspected.metadata().kind, StoreKind::Protected7);
        let opened = inspected.open().unwrap();
        let ticket = opened.fixture_ticket().unwrap();
        let initialized = opened
            .initialize_cold(&FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap();
        assert_eq!(initialized.account(), context.scope().account_id);
        ticket
    };
    let route = OwnedRoute::reopen_fixture(&ticket).unwrap();
    let inspected = route.inspect().unwrap().wait().unwrap();
    assert_eq!(inspected.metadata().kind, StoreKind::Protected7);
    let initialized = inspected
        .open()
        .unwrap()
        .initialize_cold(&FactoryIdentity::default())
        .unwrap()
        .wait()
        .unwrap();
    assert!(initialized.inventory().locals().is_empty());
}

#[tokio::test]
async fn held_result_is_after_dispatch_and_changed_writer_prevents_fresh_clear() {
    // Both a current edit and a stale original writer retain the newer input.
    // The latter is an alternate, never a silent rebase to the claim successor.
    for change in 0..3 {
        let changed = change != 0;
        let stale = change == 2;
        let f = Fixture::new(SlashFixtureTarget::Channel, SlashFixtureOutcome::HeldClear);
        assert!(f.context.release_result().is_err());
        let c = f.completion(f.input());
        let execute = f.context.execute(&c);
        tokio::pin!(execute);
        tokio::select! {
            value = &mut execute => panic!("held response completed: {value:?}"),
            _ = f.context.wait_dispatched() => {}
        }
        assert_eq!(f.context.requests().len(), 1);
        assert!(c.report().is_none());
        assert!(c.terminal().is_none());
        assert!(c.take_fresh_clear().is_none());
        if changed {
            let post_claim = done(f.ops.review(f.context.scope().clone()).unwrap());
            assert!(!post_claim.same(&f.review));
            assert_eq!(
                serde_json::to_value(post_claim.local()).unwrap(),
                serde_json::to_value(f.review.local()).unwrap()
            );
            let job = f
                .ops
                .checkpoint(CheckpointInput {
                    writer: f.writer.clone(),
                    serial: 2,
                    expected: if stale { f.review.clone() } else { post_claim },
                    local: sync::tests::local("/example newer input"),
                    direct: None,
                })
                .unwrap();
            job.submit().unwrap();
            assert_eq!(job.wait().unwrap(), JobState::Ready);
            match job.write_result().unwrap().unwrap() {
                WriteOutcome::Applied(review) if !stale => {
                    assert_eq!(review.local().composer.text, "/example newer input");
                }
                WriteOutcome::Preserved(inventory) if stale => {
                    assert_eq!(
                        serde_json::to_value(inventory.local()).unwrap(),
                        serde_json::to_value(f.review.local()).unwrap()
                    );
                    assert!(inventory.alternates().iter().any(|a| {
                        a.writer_id == f.writer.id()
                            && a.revision == 2
                            && a.draft.composer.text == "/example newer input"
                    }));
                }
                _ => panic!("unexpected actual changed-writer result"),
            }
            f.context.revoke();
        }
        f.context.release_result().unwrap();
        assert!(f.context.release_result().is_err());
        execute.await.unwrap();
        assert_eq!(c.wait().unwrap(), JobState::Ready);
        assert!(c.report().is_some());
        assert!(c.terminal().is_some());
        assert_eq!(c.take_fresh_clear().is_some(), !changed);
        let current = done(f.ops.review(f.context.scope().clone()).unwrap());
        if stale {
            assert_eq!(
                serde_json::to_value(current.local()).unwrap(),
                serde_json::to_value(f.review.local()).unwrap()
            );
            assert!(current.alternates().iter().any(|a| {
                a.writer_id == f.writer.id()
                    && a.revision == 2
                    && a.draft.composer.text == "/example newer input"
            }));
        } else if changed {
            assert_eq!(current.local().composer.text, "/example newer input");
        } else {
            assert!(current.local().empty());
        }
        assert!(f.context.execute(&c).await.is_err());
        assert_eq!(f.context.requests().len(), 1);
    }
}

#[tokio::test]
async fn dropped_held_request_retains_dispatch_possible_and_cannot_replay() {
    let f = Fixture::new(SlashFixtureTarget::Thread, SlashFixtureOutcome::HeldClear);
    let completion = f.completion(f.input());
    {
        let execute = f.context.execute(&completion);
        tokio::pin!(execute);
        tokio::select! { v=&mut execute=>panic!("held unexpectedly {v:?}"),v=f.context.wait_dispatched()=>v.unwrap() }
    }
    f.context.revoke();
    f.context.release_result().unwrap();
    assert!(completion.report().is_none());
    assert!(completion.terminal().is_none());
    assert!(completion.take_fresh_clear().is_none());
    assert!(f.context.execute(&completion).await.is_err());
    assert_eq!(f.context.requests().len(), 1);
}

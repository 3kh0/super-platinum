//! Real public inspect/migrate/cold/operations; no SQL or constructed review.
use super::*;
use crate::drafts::store::sync::{LocalChoice, RemoteChoice};
use p::owned_store::{FactoryIdentity, OwnedRoute, StoreKind};

fn done<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    let (value, ack) = job.observe_success().unwrap();
    assert!(ack.acknowledge().is_ok());
    value
}
struct Open {
    fixture: BaselineFixture,
    ops: Operations,
    review: p::Review,
    stream: p::Stream,
}
fn open(target: BaselineFixtureTarget, outcome: BaselineFixtureOutcome) -> Open {
    let (route, fixture) = OwnedRoute::fresh_baseline_fixture(target, outcome).unwrap();
    assert!(route.opened().is_none());
    let inspected = route.inspect().unwrap().wait().unwrap();
    assert_eq!(inspected.metadata().kind, StoreKind::Legacy5);
    route.commit(&inspected).unwrap().wait().unwrap();
    let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
    let initialized = opened
        .initialize_cold(&FactoryIdentity::default())
        .unwrap()
        .wait()
        .unwrap();
    let review = initialized.inventory().managed()[&fixture.scope().key].clone();
    let ops = initialized.operations();
    let state = done(
        ops.stream_state(TeamInput {
            scope: fixture.scope().clone(),
        })
        .unwrap(),
    );
    let stream = done(
        ops.open_stream(OpenInput {
            scope: fixture.scope().clone(),
            expected: state,
        })
        .unwrap(),
    );
    Open {
        fixture,
        ops,
        review,
        stream,
    }
}
fn reserve(f: &Open, now: u64) -> BaselineReserved {
    let reserved = f
        .ops
        .baseline(BaselineInput::new(f.stream.clone(), f.review.clone(), now))
        .unwrap();
    reserved.submit().unwrap();
    assert_eq!(reserved.wait().unwrap(), JobState::Ready);
    reserved
}
fn claim(reserved: &BaselineReserved) -> BaselineCompletion {
    let claimed = reserved.claim().unwrap();
    claimed.submit().unwrap();
    assert_eq!(claimed.wait().unwrap(), JobState::Ready);
    claimed.completion().unwrap()
}
fn retain(completion: &BaselineCompletion, raw: BaselineRead) -> BaselineOutcome {
    completion.bind(raw).unwrap();
    completion.submit().unwrap();
    completion.wait().unwrap().unwrap()
}

#[test]
fn actual_seed_migrates_both_destinations_and_preserves_canonical_metadata() {
    for target in [
        BaselineFixtureTarget::Channel,
        BaselineFixtureTarget::Thread,
    ] {
        for outcome in [
            BaselineFixtureOutcome::Unchanged,
            BaselineFixtureOutcome::ChangedWithAttachment,
        ] {
            let f = open(target, outcome);
            assert_eq!(f.review.scope(), f.fixture.scope());
            assert_eq!(f.review.row().lineage().basis(), p::Basis::Unverified);
            assert_eq!(f.review.row().lineage().verified_epoch(), 0);
            assert!(f.review.row().state().pending().is_none());
            assert!(f.review.row().state().consumption().is_none());
            assert_eq!(
                f.review.row().state().acknowledged(),
                Some(&f.fixture.remote)
            );
            assert_eq!(
                f.review.local().composer.text,
                "Local *retained* 日本語 🚀 e\u{301}\nsecond line"
            );
            assert_eq!(
                f.review.local().composer.selection,
                crate::TextRange::new(6, 14)
            );
            let attachments = &f.review.local().attachments;
            assert_eq!(
                attachments.len(),
                usize::from(outcome == BaselineFixtureOutcome::ChangedWithAttachment)
            );
            if let Some(file) = attachments.first() {
                assert_eq!(file.id, 17);
                assert_eq!(file.name, "retained 日本語.txt");
                assert_eq!(
                    file.path,
                    std::path::PathBuf::from("baseline-fixture-local.txt")
                );
                assert_eq!(file.bytes, 7);
                assert!(!file.uploading);
                assert!(file.upload_started.is_none());
            }
            assert!(f.fixture.reads().is_empty());
            let verification = f
                .ops
                .verify_local(MutationInput {
                    stream: f.stream.clone(),
                    expected: f.review.clone(),
                })
                .unwrap();
            verification.submit().unwrap();
            assert!(matches!(verification.wait().unwrap(), JobState::Failed(_)));
        }
    }
}

#[test]
fn exact_owner_request_once_raw_retention_and_explicit_choices() {
    for choice in [
        RemoteChoice::Remote,
        RemoteChoice::Local(LocalChoice::Current),
    ] {
        let f = open(
            BaselineFixtureTarget::Thread,
            BaselineFixtureOutcome::Changed,
        );
        let foreign = open(
            BaselineFixtureTarget::Thread,
            BaselineFixtureOutcome::Changed,
        );
        let c = claim(&reserve(&f, 1000));
        assert_eq!(
            foreign.fixture.read(&c, 1000).err(),
            Some(AdmissionError::Stale)
        );
        assert_eq!(c.state().unwrap(), BaselineState::Ready);
        assert!(foreign.fixture.reads().is_empty());
        assert_eq!(
            f.fixture.read(&c, u64::MAX).err(),
            Some(AdmissionError::Counter)
        );
        assert_eq!(c.state().unwrap(), BaselineState::Ready);
        let raw = f.fixture.read(&c, 1000).unwrap();
        assert!(
            matches!(&raw.response, p::BaselineResponse::Positive { raw, .. } if raw["fixture_unknown_root"]["keep"][0] == "日本語")
        );
        assert_eq!(f.fixture.read(&c, 1000).err(), Some(AdmissionError::Stale));
        assert!(c.take_request().is_err());
        assert_eq!(f.fixture.reads().len(), 1);
        assert!(f.fixture.reads()[0].read.as_ref().unwrap().response == raw.response);
        let outcome = retain(&c, raw.clone());
        assert!(c.retained_read().unwrap().response == raw.response);
        let evidence = outcome.evidence().unwrap();
        assert_eq!(evidence.remote().snapshot, f.fixture.changed);
        assert!(done(f.ops.review(f.fixture.scope().clone()).unwrap()).same(&f.review));
        assert!(c.observe_success().unwrap().1.acknowledge().is_ok());
        let settled = done(
            f.ops
                .finish_baseline(BaselineFinishInput::new(evidence, choice.clone()))
                .unwrap(),
        );
        assert_eq!(settled.row().lineage().basis(), p::Basis::Remote);
        assert_eq!(settled.row().lineage().verified_epoch(), f.stream.epoch());
        match choice {
            RemoteChoice::Remote => assert_eq!(
                settled.local().composer.text,
                "Remote *changed* 日本語 🚀\nsecond line"
            ),
            RemoteChoice::Local(_) => {
                assert_eq!(settled.local().composer, f.review.local().composer)
            }
        }
    }
}

#[test]
fn retained_rate_limit_causes_real_reserve_cooldown_and_explicit_fresh_retry() {
    let f = open(
        BaselineFixtureTarget::Channel,
        BaselineFixtureOutcome::RateLimitedThenChanged,
    );
    let c = claim(&reserve(&f, 1000));
    let raw = f.fixture.read(&c, 1000).unwrap();
    assert_eq!(raw.known_until, 61000);
    assert!(matches!(
        raw.response,
        p::BaselineResponse::Nonpositive(p::KnownReadFailure::RateLimited)
    ));
    assert!(retain(&c, raw).evidence().is_none());
    assert!(c.observe_success().unwrap().1.acknowledge().is_ok());
    let failed = f
        .ops
        .baseline(BaselineInput::new(
            f.stream.clone(),
            f.review.clone(),
            60999,
        ))
        .unwrap();
    failed.submit().unwrap();
    assert_eq!(
        failed.wait().unwrap(),
        JobState::Failed(p::Error::Cooldown(61000))
    );
    assert!(failed.claim().is_err());
    assert_eq!(f.fixture.reads().len(), 1);
    let c = claim(&reserve(&f, 61000));
    assert_eq!(c.state().unwrap(), BaselineState::Ready);
    let raw = f.fixture.read(&c, 61000).unwrap();
    assert!(retain(&c, raw).evidence().is_some());
    assert_eq!(f.fixture.reads().len(), 2);
    assert_eq!(failed.state(), JobState::Failed(p::Error::Cooldown(61000)));
    assert!(f.ops.resume_baseline_reserved(&failed.key()).is_ok());
    assert!(c.observe_success().unwrap().1.acknowledge().is_ok());
}

#[test]
fn attachment_local_choice_refuses_without_changing_actual_full_inventory() {
    let f = open(
        BaselineFixtureTarget::Channel,
        BaselineFixtureOutcome::ChangedWithAttachment,
    );
    let c = claim(&reserve(&f, 1000));
    let evidence = retain(&c, f.fixture.read(&c, 1000).unwrap())
        .evidence()
        .unwrap();
    let finish = f
        .ops
        .finish_baseline(BaselineFinishInput::new(
            evidence,
            RemoteChoice::Local(LocalChoice::Current),
        ))
        .unwrap();
    finish.submit().unwrap();
    assert_eq!(
        finish.wait().unwrap(),
        JobState::Failed(p::Error::Unsupported)
    );
    assert!(done(f.ops.review(f.fixture.scope().clone()).unwrap()).same(&f.review));
    assert_eq!(f.fixture.reads().len(), 1);
}

#[test]
fn actual_late_result_keeps_raw_but_revoked_stream_cannot_finish_choice() {
    let f = open(
        BaselineFixtureTarget::Channel,
        BaselineFixtureOutcome::Unchanged,
    );
    let c = claim(&reserve(&f, 1000));
    let raw = f.fixture.read(&c, 1000).unwrap();
    done(
        f.ops
            .mark_gap(GapInput {
                stream: f.stream.clone(),
                reason: p::GapReason::Disconnect,
            })
            .unwrap(),
    );
    let evidence = retain(&c, raw.clone()).evidence().unwrap();
    assert!(c.retained_read().unwrap().response == raw.response);
    let finish = f
        .ops
        .finish_baseline(BaselineFinishInput::new(evidence, RemoteChoice::Remote))
        .unwrap();
    finish.submit().unwrap();
    assert_eq!(finish.wait().unwrap(), JobState::Failed(p::Error::Gap));
    assert!(done(f.ops.review(f.fixture.scope().clone()).unwrap()).same(&f.review));
    assert_eq!(f.fixture.reads().len(), 1);
}

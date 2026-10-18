//! Actual owner/worker/SQLite/claim/completion. Only discovery response and the
//! final transport report are synthetic; never a network or desktop test.
use super::*;
use crate::slash_commands::controller::Delivery;
use p::owned_store::{FactoryIdentity, OwnedRoute};

#[test]
fn discovery_preserves_transport_classification_and_retry_after_without_debug_payload() {
    use crate::slack::Error;
    let offline = SlashDiscoveryError::from(DiscoveryFailure::Transport(Error::Offline(
        "synthetic private detail".into(),
    )));
    assert_eq!(format!("{offline:?}"), "SlashDiscoveryError::Transport(..)");
    assert!(
        matches!(offline, SlashDiscoveryError::Transport(Error::Offline(detail)) if detail=="synthetic private detail")
    );
    for seconds in [None, Some(37)] {
        let limited = SlashDiscoveryError::from(DiscoveryFailure::Transport(Error::RateLimited {
            retry_after_secs: seconds,
        }));
        assert!(
            matches!(limited, SlashDiscoveryError::Transport(Error::RateLimited { retry_after_secs }) if retry_after_secs==seconds)
        );
    }
    assert!(matches!(
        SlashDiscoveryError::from(DiscoveryFailure::Admission(
            crate::slash_commands::Failure::Invalid
        )),
        SlashDiscoveryError::Stale
    ));
}

fn done<I: Send + Sync + 'static, T: Send + Clone + 'static>(job: StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), JobState::Ready);
    job.result().unwrap().unwrap()
}
struct Fixture {
    ops: Operations,
    context: SlashContext,
    review: p::Review,
    writer: p::Writer,
}
impl Fixture {
    fn new() -> Self {
        let (route, seed) = OwnedRoute::fresh_slash_fixture(
            SlashFixtureTarget::Channel,
            SlashFixtureOutcome::Clear,
        )
        .unwrap();
        let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
        let initialized = opened
            .initialize_cold(&FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap();
        let ops = initialized.operations();
        let scope = seed.scope().clone();
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
                scope: scope.clone(),
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
            panic!("actual checkpoint")
        };
        let owner = seed.catalog().owner().clone();
        let context = ProductionContext::synthetic_response(
            owner,
            session(),
            seed.transport(),
            seed.catalog().raw().clone(),
        )
        .unwrap();
        Self {
            context: SlashContext {
                queue: ops.queue.clone(),
                stream,
                context,
                prepared: Mutex::new(None),
            },
            ops,
            review,
            writer,
        }
    }
    fn input(&self) -> Result<SlashInput, AdmissionError> {
        self.context.prepare(
            &self.ops,
            self.review.clone(),
            self.writer.clone(),
            1,
            &self.context.catalog().choice(0).unwrap(),
            "owned-production-test",
        )
    }
    fn completion(&self, input: SlashInput) -> SlashCompletion {
        let r = self.ops.slash(input).unwrap();
        r.submit().unwrap();
        assert_eq!(r.wait().unwrap(), JobState::Ready);
        let c = r.claim().unwrap();
        c.submit().unwrap();
        assert_eq!(c.wait().unwrap(), JobState::Ready);
        c.completion().unwrap()
    }
}
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "synthetic".into(),
        url: "https://slash-owned-fixture.slack.com".into(),
        token: "synthetic-only".into(),
    }
}

#[test]
fn production_response_decoder_preserves_raw_and_refuses_malformed_or_wrong_session() {
    let f = Fixture::new();
    let owner = f.context.catalog().owner().clone();
    let raw = f.context.catalog().raw().clone();
    let transport = Arc::new(Transport::disconnected_fixture("synthetic"));
    let decoded = ProductionContext::synthetic_response(
        owner.clone(),
        session(),
        transport.clone(),
        raw.clone(),
    )
    .unwrap();
    assert_eq!(decoded.catalog().raw(), &raw);
    for invalid in [
        serde_json::json!(null),
        serde_json::json!({"ok":true,"commands":[],"app_actions":null}),
        serde_json::json!({"ok":false,"commands":[],"app_actions":[]}),
    ] {
        assert!(
            ProductionContext::synthetic_response(
                owner.clone(),
                session(),
                transport.clone(),
                invalid
            )
            .is_err()
        );
    }
    let mut wrong = session();
    wrong.user_id = "UOTHER".into();
    assert!(ProductionContext::synthetic_response(owner, wrong, transport, raw).is_err());
}

#[tokio::test]
async fn discovered_input_uses_actual_claim_once_and_retains_canonical_completion() {
    let f = Fixture::new();
    let input = f.input().unwrap();
    assert!(f.input().is_err());
    let completion = f.completion(input.clone());
    completion
        .execute_fixture(|permit| async move {
            let crate::slack::client::RequestBody::Form(fields) = &permit.request().body else {
                panic!("form")
            };
            assert!(
                fields
                    .iter()
                    .any(|(k, v)| k == "command" && v == "/example")
            );
            assert!(
                fields
                    .iter()
                    .any(|(k, v)| k == "client_token" && v == "owned-production-test")
            );
            permit.report(Delivery::Reply(serde_json::json!({"ok":true})), None)
        })
        .await
        .unwrap();
    assert_eq!(completion.wait().unwrap(), JobState::Ready);
    let terminal = completion.terminal().unwrap();
    let fresh = completion.take_fresh_clear().unwrap();
    assert!(fresh.consume(&completion, input.prior(), &terminal).is_ok());
    assert!(completion.take_fresh_clear().is_none());
    assert!(
        completion
            .execute_fixture(|_| async { panic!("replay") })
            .await
            .is_err()
    );
}
#[test]
fn foreign_owner_catalog_writer_and_zero_serial_cannot_prepare() {
    let f = Fixture::new();
    let other = Fixture::new();
    let choice = f.context.catalog().choice(0).unwrap();
    assert!(
        f.context
            .prepare(
                &other.ops,
                f.review.clone(),
                f.writer.clone(),
                1,
                &choice,
                "one"
            )
            .is_err()
    );
    assert!(
        f.context
            .prepare(
                &f.ops,
                f.review.clone(),
                other.writer.clone(),
                1,
                &choice,
                "one"
            )
            .is_err()
    );
    assert!(
        f.context
            .prepare(
                &f.ops,
                f.review.clone(),
                f.writer.clone(),
                0,
                &choice,
                "one"
            )
            .is_err()
    );
    assert!(
        f.context
            .prepare(
                &f.ops,
                f.review.clone(),
                f.writer.clone(),
                1,
                &other.context.catalog().choice(0).unwrap(),
                "one"
            )
            .is_err()
    );
    assert!(f.input().is_ok());
}
#[tokio::test]
async fn permanent_revocation_after_claim_prevents_actual_transport_seam() {
    let f = Fixture::new();
    let completion = f.completion(f.input().unwrap());
    let retained = f.ops.retained().len();
    f.context.revoke();
    assert!(!f.context.is_live());
    assert!(
        completion
            .execute_fixture(|_| async { panic!("revoked dispatch") })
            .await
            .is_err()
    );
    assert!(f.input().is_err());
    assert_eq!(f.ops.retained().len(), retained);
}
#[tokio::test]
async fn mismatched_session_refuses_before_discovery_and_dropping_context_revokes_plan() {
    let f = Fixture::new();
    let mut wrong = session();
    wrong.team_id = "TOTHER".into();
    assert!(matches!(
        f.ops
            .discover_slash(
                f.context.stream.clone(),
                f.review.scope().clone(),
                wrong,
                SlackClient::default(),
                Arc::new(Transport::disconnected_fixture("synthetic")),
                Arc::new(serde_json::json!({}))
            )
            .await,
        Err(SlashDiscoveryError::Stale)
    ));
    let input = f.input().unwrap();
    let completion = f.completion(input);
    drop(f.context);
    assert!(
        completion
            .execute_fixture(|_| async { panic!("dropped context dispatch") })
            .await
            .is_err()
    );
}

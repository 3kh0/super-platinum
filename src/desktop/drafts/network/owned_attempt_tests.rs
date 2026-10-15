//! Actual public Owned migration/cold/admission and existing HTTP builder/decoder.
//! Only the final HTTP closure is synthetic. No Session/keyring/network/native UI.
use super::*;
use durable::owned_store::*;
use serde_json::json;
use std::{
    cell::Cell,
    time::{Duration, Instant},
};
use super_platinum_core::{
    ComposerState,
    drafts::{DraftKey, LocalDraft, sync::Scope},
};
fn until<T>(mut f: impl FnMut() -> Result<T, AdmissionError>) -> T {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match f() {
            Ok(v) => return v,
            Err(AdmissionError::Incomplete) => {
                assert!(Instant::now() < end);
                std::thread::yield_now()
            }
            Err(e) => panic!("actual admission {e:?}"),
        }
    }
}
fn done<I: Send + Sync + 'static, T: Send + Clone + 'static>(j: StorageJob<I, T>) -> T {
    j.submit().unwrap();
    until(|| match j.state() {
        JobState::Ready => Ok(j.result().unwrap().unwrap()),
        JobState::Waiting => Err(AdmissionError::Incomplete),
        v => panic!("actual storage {v:?}"),
    })
}
fn ready<I, T>(j: &StorageJob<I, T>) {
    until(|| match j.state() {
        JobState::Ready => Ok(()),
        JobState::Waiting => Err(AdmissionError::Incomplete),
        v => panic!("actual ticket {v:?}"),
    })
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        ..Default::default()
    }
}
struct Fixture {
    initialized: Initialized,
    ops: Operations,
    scope: Scope,
    writer: durable::Writer,
    completion: MutationCompletion,
    binding: Binding,
    operation: Operation,
}
impl Fixture {
    fn new(thread: bool) -> Self {
        let route = OwnedRoute::fresh_fixture().unwrap();
        let before = route.inspect().unwrap().wait().unwrap();
        route.commit(&before).unwrap().wait().unwrap();
        let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
        let initialized = opened
            .initialize_cold(&FactoryIdentity::default())
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
        let j = ops
            .checkpoint(CheckpointInput {
                writer: writer.clone(),
                serial: 1,
                expected: review,
                local: local("Frozen \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301}\nsecond"),
                direct: None,
            })
            .unwrap();
        j.submit().unwrap();
        let WriteOutcome::Applied(review) =
            until(|| j.write_result().ok_or(AdmissionError::Incomplete)).unwrap()
        else {
            panic!("paired input")
        };
        let consumed = ops
            .consume_send(SendInput {
                stream,
                expected: review,
            })
            .unwrap();
        consumed.submit().unwrap();
        ready(&consumed);
        assert_eq!(
            consumed.result().unwrap().unwrap().local().composer.text,
            ""
        );
        let prepare = until(|| consumed.prepare());
        prepare.submit().unwrap();
        ready(&prepare);
        let claim = until(|| prepare.claim());
        claim.submit().unwrap();
        ready(&claim);
        let completion = until(|| claim.completion());
        // Read actual stored operation; this clone is observation only. Only
        // RegisteredMutation takes and consumes the real issued claim.
        let review = done(ops.review(scope.clone()).unwrap());
        let operation = review.row().state().pending().unwrap().clone();
        let binding = Binding {
            account: scope.account_id.clone(),
            epoch: 7,
            cookie: "fixture-cookie".into(),
            transport: Arc::new(Transport::new("fixture-cookie").unwrap()),
            client: SlackClient::default(),
            workspace: WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "fixture-token".into(),
            },
            cooldown_until_ms: Arc::new(Mutex::new(0)),
        };
        Self {
            initialized,
            ops,
            scope,
            writer,
            completion,
            binding,
            operation,
        }
    }
    fn response(&self) -> Value {
        let mut message = json!({"ts":"200.000001","channel":"C1","user":"U1","client_msg_id":self.operation.client_msg_id});
        if let Some(root) = &self.scope.key.thread {
            message["thread_ts"] = json!(root);
        }
        json!({"ok":true,"channel":"C1","ts":"200.000001","message":message})
    }
    fn newer(&self) -> LocalDraft {
        let review = done(self.ops.review(self.scope.clone()).unwrap());
        let newer = local("newer \u{65e5}\u{1f680}");
        let j = self
            .ops
            .checkpoint(CheckpointInput {
                writer: self.writer.clone(),
                serial: 2,
                expected: review,
                local: newer.clone(),
                direct: None,
            })
            .unwrap();
        j.submit().unwrap();
        assert!(matches!(
            until(|| j.write_result().ok_or(AdmissionError::Incomplete)),
            Ok(WriteOutcome::Applied(_))
        ));
        newer
    }
}
#[tokio::test]
async fn actual_claim_prepares_once_decodes_then_preserves_newer_and_hands_off() {
    for thread in [false, true] {
        let f = Fixture::new(thread);
        let owner = RegisteredMutation::hold(f.completion.clone());
        let waiter = owner.clone();
        let expected = f.binding.prepare(&f.operation, 1000).unwrap();
        let calls = Cell::new(0);
        let newer = f.newer();
        let raw = f.response();
        Box::pin(waiter.run_with(&f.binding, 1000, |request| {
            calls.set(calls.get() + 1);
            assert!(!request.retry_safe());
            // Each real prepare generates three request-context UUIDs. Validate
            // their exact shape, then compare all remaining wire fields.
            assert_eq!(
                stable_wire(&request).unwrap(),
                stable_wire(&expected.request).unwrap()
            );
            assert!(request.url.contains("chat.postMessage"));
            std::future::ready(Ok(raw.clone()))
        }))
        .await
        .unwrap();
        drop(waiter);
        assert_eq!(calls.get(), 1);
        assert_eq!(owner.attempt().unwrap().raw.as_ref(), Some(&raw));
        assert!(matches!(
            Box::pin(owner.run_with(&f.binding, 1000, |_| {
                calls.set(calls.get() + 1);
                std::future::ready(Ok(raw.clone()))
            }))
            .await,
            Err(AdmissionError::Stale)
        ));
        owner.commit().unwrap();
        let review = until(|| {
            owner
                .completion()
                .result()?
                .ok_or(AdmissionError::Incomplete)
        })
        .unwrap();
        let r = f
            .ops
            .release(ReleaseInput {
                writer: f.writer.clone(),
                expected: review,
            })
            .unwrap();
        r.submit().unwrap();
        ready(&r);
        let handoff = until(|| r.start_local());
        handoff.submit().unwrap();
        let lease = until(|| handoff.result()?.ok_or(AdmissionError::Incomplete)).unwrap();
        assert_eq!(lease.writer_id(), f.writer.id());
        let inventory = done(f.ops.local_inventory(f.scope.clone()).unwrap());
        assert!(inventory.row().is_none());
        assert_eq!(
            serde_json::to_value(inventory.local()).unwrap(),
            serde_json::to_value(newer).unwrap()
        );
        assert_eq!(calls.get(), 1);
    }
}
#[tokio::test]
async fn full_raw_survives_actual_unadapted_admission_refusal_without_replay() {
    let f = Fixture::new(false);
    let owner = RegisteredMutation::hold(f.completion.clone());
    let raw = f.response();
    // Real old Client route latches Unsupported. It is not a test-only bool.
    Box::pin(owner.run_with(&f.binding, 1000, |_| {
        let _ = f
            .initialized
            .client()
            .review(f.scope.clone())
            .wait()
            .unwrap();
        std::future::ready(Ok(raw.clone()))
    }))
    .await
    .unwrap();
    assert_eq!(owner.commit(), Err(AdmissionError::Unsupported));
    assert_eq!(owner.attempt().unwrap().raw.as_ref(), Some(&raw));
    assert!(owner.completion().retained_outcome().is_some());
    assert!(matches!(
        Box::pin(owner.run_with(&f.binding, 1000, |_| async { panic!("no replay") })).await,
        Err(AdmissionError::Stale)
    ));
}
#[tokio::test]
async fn lost_response_records_uncertain_and_retains_newer_input() {
    let f = Fixture::new(true);
    let owner = RegisteredMutation::hold(f.completion.clone());
    let newer = f.newer();
    Box::pin(owner.run_with(&f.binding, 1000, |_| {
        std::future::ready(Err(ProtectedDraftFailure::Response(Error::Transport(
            "lost".into(),
        ))))
    }))
    .await
    .unwrap();
    let attempt = owner.attempt().unwrap();
    assert!(attempt.dispatched);
    assert!(attempt.raw.is_none());
    assert!(matches!(attempt.outcome, Outcome::Uncertain));
    owner.commit().unwrap();
    let review = until(|| {
        owner
            .completion()
            .result()?
            .ok_or(AdmissionError::Incomplete)
    })
    .unwrap();
    assert!(matches!(
        review.row().state().workflow(),
        super_platinum_core::drafts::sync::Workflow::Uncertain(_)
    ));
    assert_eq!(
        serde_json::to_value(review.local()).unwrap(),
        serde_json::to_value(newer).unwrap()
    );
}
#[tokio::test]
async fn native_not_requested_provenance_and_retry_after_survive_adapter() {
    let f = Fixture::new(false);
    let owner = RegisteredMutation::hold(f.completion.clone());
    Box::pin(owner.run_with(&f.binding, 1000, |_| {
        std::future::ready(Err(ProtectedDraftFailure::NotRequested(
            Error::RateLimited {
                retry_after_secs: Some(7),
            },
        )))
    }))
    .await
    .unwrap();
    let attempt = owner.attempt().unwrap();
    assert!(!attempt.dispatched);
    assert!(attempt.cooldown_until_ms.unwrap() >= 8000);
    assert!(matches!(attempt.outcome, Outcome::Uncertain));
    assert_eq!(attempt.issue, Some(Issue::Blocked(Block::Session)));
    assert_eq!(
        owner.completion().retained_outcome().unwrap().known_until,
        attempt.cooldown_until_ms.unwrap()
    );
}
#[tokio::test]
async fn contradictory_raw_is_kept_and_cannot_become_positive() {
    let f = Fixture::new(false);
    let owner = RegisteredMutation::hold(f.completion.clone());
    let raw = json!({"ok":false,"error":"invalid_blocks","unexpected_authority":{"sent":true}});
    Box::pin(owner.run_with(&f.binding, 1000, |_| std::future::ready(Ok(raw.clone()))))
        .await
        .unwrap();
    let attempt = owner.attempt().unwrap();
    assert_eq!(attempt.raw.as_ref(), Some(&raw));
    assert!(matches!(attempt.outcome, Outcome::Uncertain));
    assert_eq!(attempt.issue, Some(Issue::MalformedResponse));
}
#[test]
fn interrupted_http_future_cannot_retake_claim_or_fabricate_completion() {
    use std::task::{Context, Poll, Wake, Waker};
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let f = Fixture::new(false);
    let owner = RegisteredMutation::hold(f.completion.clone());
    let calls = Cell::new(0);
    let mut future = Box::pin(owner.run_with(&f.binding, 1000, |_| {
        calls.set(calls.get() + 1);
        std::future::pending()
    }));
    let waker = Waker::from(Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
    drop(future);
    assert_eq!(calls.get(), 1);
    assert!(owner.attempt().is_none());
    assert_eq!(owner.commit(), Err(AdmissionError::Incomplete));
    assert!(matches!(owner.take(), Err(AdmissionError::Stale)));
    let review = done(f.ops.review(f.scope.clone()).unwrap());
    assert_eq!(review.row().state().pending(), Some(&f.operation));
}

// XParams::rest_pairs creates these three UUIDv4 values on EVERY prepare, even
// on the same Binding. There is no captured-context injection in that builder.
// Do not normalize any stable query field, method, header, body or retry policy.
fn stable_wire(
    request: &super_platinum_core::slack::PreparedRequest,
) -> Result<super_platinum_core::slack::PreparedRequest, &'static str> {
    const NAMES: [&str; 3] = ["_x_id", "_x_csid", "fp"];
    let (base, query) = request.url.split_once('?').ok_or("missing query")?;
    let mut pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    if url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(&pairs)
        .finish()
        != query
    {
        return Err("noncanonical query encoding");
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut identities = std::collections::BTreeSet::new();
    for (name, value) in &mut pairs {
        if NAMES.contains(&name.as_str()) {
            if !seen.insert(name.clone()) {
                return Err("duplicate identity key");
            }
            if value.len() != 32
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("noncanonical identity");
            }
            // Canonical simple UUIDv4: version nibble 4, RFC4122 variant 10xx.
            if value.as_bytes()[12] != b'4'
                || !matches!(value.as_bytes()[16], b'8' | b'9' | b'a' | b'b')
            {
                return Err("not UUIDv4");
            }
            if !identities.insert(value.clone()) {
                return Err("repeated identity");
            }
            *value = "<validated-v4>".into();
        }
    }
    if seen.len() != NAMES.len() {
        return Err("missing identity");
    }
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(&pairs)
        .finish();
    let mut result = request.clone();
    result.url = format!("{base}?{query}");
    Ok(result)
}

#[test]
fn request_oracle_preserves_stable_query_method_headers_and_body() {
    let f = Fixture::new(false);
    let request = f.binding.prepare(&f.operation, 1000).unwrap().request;
    let expected = stable_wire(&request).unwrap();
    for (old, new) in [
        ("slack_route=default", "slack_route=other"),
        ("_x_num_retries=0", "_x_num_retries=1"),
        ("chat.postMessage", "chat.update"),
    ] {
        assert!(request.url.contains(old));
        let mut altered = request.clone();
        altered.url = altered.url.replace(old, new);
        assert_ne!(stable_wire(&altered).unwrap(), expected);
    }
    let mut altered = request.clone();
    altered.method = "GET";
    assert_ne!(stable_wire(&altered).unwrap(), expected);
    let mut altered = request.clone();
    altered
        .headers
        .push(("x-unexpected".into(), "value".into()));
    assert_ne!(stable_wire(&altered).unwrap(), expected);
    let mut altered = request.clone();
    altered.body = super_platinum_core::slack::client::RequestBody::Json(Value::Null);
    assert_ne!(stable_wire(&altered).unwrap(), expected);
}

#[test]
fn request_oracle_rejects_missing_duplicate_and_malformed_context_identities() {
    let f = Fixture::new(false);
    let request = f.binding.prepare(&f.operation, 1000).unwrap().request;
    let (base, query) = request.url.split_once('?').unwrap();
    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    for name in ["_x_id", "_x_csid", "fp"] {
        for bad in [
            None,
            Some("00000000000000000000000000000000"),
            Some("ABCD"),
            Some("00000000000040000000000000000000"),
        ] {
            let mut changed = pairs.clone();
            if let Some(bad) = bad {
                changed.iter_mut().find(|(key, _)| key == name).unwrap().1 = bad.into();
            } else {
                changed.retain(|(key, _)| key != name);
            }
            let mut altered = request.clone();
            altered.url = format!(
                "{base}?{}",
                url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(&changed)
                    .finish()
            );
            assert!(stable_wire(&altered).is_err());
        }
        let mut altered = request.clone();
        let value = &pairs.iter().find(|(key, _)| key == name).unwrap().1;
        altered.url.push_str(&format!("&{name}={value}"));
        assert!(stable_wire(&altered).is_err());
    }
}

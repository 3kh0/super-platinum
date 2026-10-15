//! Regression tests through the real FIFO and opaque API. No HTTP or keyring.
use super::*;
use crate::drafts::store::sync::tests::local;
use crate::drafts::sync::{ConsumeKind, OperationKind, RemoteDraft};
use serde_json::json;

struct Fixture {
    _owned: test_support::Isolated,
    client: Client,
    scope: Scope,
    path: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let account = uuid::Uuid::new_v4().to_string();
        let path =
            std::env::temp_dir().join(format!("protected-adapter-test-{}", uuid::Uuid::new_v4()));
        let owned = test_support::Isolated::create(account.clone(), path.clone()).unwrap();
        Self {
            client: owned.client(),
            _owned: owned,
            scope: Scope {
                account_id: account,
                key: DraftKey {
                    team: "T1".into(),
                    channel: "C1".into(),
                    thread: None,
                },
            },
            path,
        }
    }
    fn seed(&self, text: &str) -> Review {
        let empty = self.client.review_local(self.scope.clone()).wait().unwrap();
        let lease = self.client.renew_local(empty).wait().unwrap();
        let LocalWriteResult::Applied(written) = self
            .client
            .write_local(lease, 0, 1, LocalDraft::default(), local(text))
            .wait()
            .unwrap()
        else {
            panic!("seed was not canonical")
        };
        self.client
            .enroll_local(self.scope.clone(), written)
            .wait()
            .unwrap()
    }
    fn open(&self) -> Stream {
        let state = self
            .client
            .stream_state(self.scope.key.team.clone())
            .wait()
            .unwrap();
        self.client
            .open_stream(self.scope.key.team.clone(), state)
            .wait()
            .unwrap()
    }
    fn verified(&self, text: &str) -> (Stream, Review) {
        let review = self.seed(text);
        let stream = self.open();
        let review = self
            .client
            .verify_local(stream.clone(), review)
            .wait()
            .unwrap();
        (stream, review)
    }
    fn current(&self) -> Review {
        self.client.review(self.scope.clone()).wait().unwrap()
    }
    fn sql(&self, sql: &str) {
        let conn = Connection::open(self.path.join("drafts.sqlite")).unwrap();
        conn.execute_batch(sql).unwrap();
    }
    fn create_remote(&self) -> (Stream, Review, RemoteDraft) {
        let (stream, review) = self.verified("original");
        let ticket = self.client.prepare(stream.clone(), review).wait().unwrap();
        let claimed = self.client.claim_dispatch(ticket).wait().unwrap();
        let (op, receipt) = claimed.into_parts();
        assert_eq!(op.kind, OperationKind::Create);
        let remote = RemoteDraft {
            scope: self.scope.clone(),
            id: "D1".into(),
            client_msg_id: op.client_msg_id.clone(),
            last_updated_ts: "10.000001".into(),
            snapshot: op.snapshot.clone().unwrap(),
        };
        let review = self
            .client
            .record(receipt, Outcome::Active(remote.clone()), 0)
            .wait()
            .unwrap();
        (stream, review, remote)
    }
    fn baseline(&self) -> (Stream, BaselineCompletion, RemoteDraft) {
        let (old, _, remote) = self.create_remote();
        self.client
            .mark_gap(old, GapReason::Disconnect)
            .wait()
            .unwrap();
        let stream = self.open();
        let review = self.current();
        let ticket = self
            .client
            .reserve_baseline(stream.clone(), review, 1000)
            .wait()
            .unwrap();
        let BaselineClaim::Ready { completion, .. } =
            self.client.claim_baseline(ticket, 1000).wait().unwrap()
        else {
            panic!("new baseline deferred")
        };
        (stream, completion, remote)
    }
}
fn raw(remote: &RemoteDraft) -> serde_json::Value {
    json!({"ok":true,"draft":{"id":remote.id,"client_msg_id":remote.client_msg_id,"last_updated_ts":"11.000001","team_id":"T1","user_id":"U1","is_deleted":false,"is_sent":false,"date_scheduled":0,"blocks":remote.snapshot.blocks,"file_ids":remote.snapshot.file_ids,"destinations":remote.snapshot.destinations}})
}

#[test]
fn local_inventory_clock_rejects_same_empty_legacy_write() {
    let f = Fixture::new();
    let before = f.client.review_local(f.scope.clone()).wait().unwrap();
    let mut store = DraftStore::open(f.path.join("drafts.sqlite")).unwrap();
    store
        .save([(f.scope.key.clone(), LocalDraft::default())])
        .unwrap();
    drop(store);
    let after = f.client.review_local(f.scope.clone()).wait().unwrap();
    assert!(!before.same(&after));
    assert!(after.clock() > before.clock());
    assert!(matches!(
        f.client.renew_local(before).wait(),
        Err(Error::Stale)
    ));
}
#[test]
fn same_serial_requires_exact_input_and_returns_original_result() {
    let f = Fixture::new();
    let before = f.seed("A");
    let writer = f.client.register_writer(before.clone()).wait().unwrap();
    assert_eq!(writer.initial_serial(), 0);
    let WriteResult::Applied(first) = f
        .client
        .checkpoint(writer.clone(), 1, before.clone(), local("B"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    let other = f.client.register_writer(first.clone()).wait().unwrap();
    let WriteResult::Applied(second) = f
        .client
        .checkpoint(other, 1, first.clone(), local("C"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        f.client
            .checkpoint(writer.clone(), 1, before.clone(), local("different"), None)
            .wait(),
        Err(Error::Stale)
    ));
    let WriteResult::Applied(retry) = f
        .client
        .checkpoint(writer, 1, before, local("B"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert!(retry.same(&first));
    assert!(!retry.same(&second));
    assert!(f.current().same(&second));
}
#[test]
fn failed_writer_receipt_rolls_back_canonical_reducer_and_clock() {
    let f = Fixture::new();
    let before = f.seed("before");
    let writer = f.client.register_writer(before.clone()).wait().unwrap();
    f.sql("CREATE TRIGGER fail_writer BEFORE UPDATE ON composer_draft_writer_lineage BEGIN SELECT RAISE(ABORT,'fixture failure'); END;");
    assert!(matches!(
        f.client
            .checkpoint(writer.clone(), 1, before.clone(), local("after"), None)
            .wait(),
        Err(Error::Storage)
    ));
    assert!(f.current().same(&before));
    f.sql("DROP TRIGGER fail_writer;");
    let WriteResult::Applied(after) = f
        .client
        .checkpoint(writer, 1, before, local("after"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(after.local().composer.text, "after");
}
#[test]
fn archived_writer_preserves_input_under_its_old_enrollment() {
    let f = Fixture::new();
    let before = f.seed("same");
    let writer = f.client.register_writer(before.clone()).wait().unwrap();
    f.client.release(before.clone()).wait().unwrap();
    let inventory = f.client.review_local(f.scope.clone()).wait().unwrap();
    let newer = f
        .client
        .enroll_local(f.scope.clone(), inventory)
        .wait()
        .unwrap();
    assert!(!before.row().same_enrollment(newer.row()));
    let WriteResult::Preserved(preserved) = f
        .client
        .checkpoint(writer.clone(), 1, before, local("old visit input"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(preserved.local().composer.text, "same");
    assert_eq!(
        preserved.row().unwrap().lineage().enrollment(),
        newer.row().lineage().enrollment()
    );
    assert_eq!(
        preserved.origins()[0].origin(),
        &WriterOrigin::Enrollment(writer.enrollment().into())
    );
    assert_eq!(
        preserved.alternates()[0].draft.composer.text,
        "old visit input"
    );
}
#[test]
fn released_predecessor_failure_keeps_later_queued_input_as_version() {
    let f = Fixture::new();
    let review = f.seed("released");
    let writer = f.client.register_writer(review.clone()).wait().unwrap();
    let release = f.client.release(review).wait().unwrap();
    let lease = f.client.start_local(release, writer).wait().unwrap();
    let first = f.client.write_local(
        lease.clone(),
        lease.initial_serial() + 1,
        lease.initial_serial() + 2,
        local("invented predecessor"),
        local("first"),
    );
    let second = f.client.write_local(
        lease.clone(),
        lease.initial_serial() + 2,
        lease.initial_serial() + 3,
        local("first"),
        local("second"),
    );
    assert!(matches!(
        first.wait().unwrap(),
        LocalWriteResult::Preserved(_)
    ));
    let LocalWriteResult::Preserved(after) = second.wait().unwrap() else {
        panic!()
    };
    assert_eq!(after.local().composer.text, "released");
    assert_eq!(after.alternates()[0].draft.composer.text, "second");
}
#[test]
fn source_revocation_blocks_claim_but_keeps_already_admitted_receipt() {
    let f = Fixture::new();
    let (stream, review) = f.verified("send me");
    let consumed = f
        .client
        .consume(stream.clone(), review, ConsumeKind::Send)
        .wait()
        .unwrap();
    let ticket = f.client.prepare(stream.clone(), consumed).wait().unwrap();
    let current = f.current();
    let operation = current.row().state().pending().unwrap().clone();
    let receipt = f
        .client
        .receipt(current.row().clone(), operation.clone())
        .wait()
        .unwrap();
    stream.revoke();
    assert!(matches!(
        f.client.claim_dispatch(ticket).wait(),
        Err(Error::Gap)
    ));
    let result = f
        .client
        .record(receipt, Outcome::Uncertain, 0)
        .wait()
        .unwrap();
    assert!(matches!(result.row().state().workflow(),Workflow::Uncertain(op) if op==&operation));
}
#[test]
fn completed_baseline_cannot_be_rebound_after_typing_or_stream_replacement() {
    let f = Fixture::new();
    let (stream, completion, remote) = f.baseline();
    let before = f.current();
    let writer = f.client.register_writer(before.clone()).wait().unwrap();
    let WriteResult::Applied(typed) = f
        .client
        .checkpoint(writer, 1, before, local("newer input"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    let BaselineResult::Retained(evidence) = f
        .client
        .retain_baseline(
            completion,
            BaselineResponse::Positive {
                raw: raw(&remote),
                user: "U1".into(),
            },
            0,
        )
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        f.client
            .finish_baseline(evidence.clone(), RemoteChoice::Remote)
            .wait(),
        Err(Error::Stale)
    ));
    assert!(f.current().same(&typed));
    f.client
        .mark_gap(stream, GapReason::SourceReplaced)
        .wait()
        .unwrap();
    let _replacement = f.open();
    assert!(matches!(
        f.client
            .finish_baseline(evidence, RemoteChoice::Remote)
            .wait(),
        Err(Error::Gap)
    ));
    assert!(
        f.client
            .load(LoadMode::Current)
            .wait()
            .unwrap()
            .recovery()
            .iter()
            .all(|r| r.phase() != RecoveryPhase::Pending)
    );
}
#[test]
fn unsupported_baseline_finishes_read_and_retains_known_deadline() {
    let f = Fixture::new();
    let (stream, completion, _) = f.baseline();
    let result = f
        .client
        .retain_baseline(
            completion,
            BaselineResponse::Positive {
                raw: json!({"ok":true,"draft":{"id":"wrong"}}),
                user: "U1".into(),
            },
            90000,
        )
        .wait()
        .unwrap();
    assert!(matches!(result, BaselineResult::Nonpositive));
    assert_eq!(f.current().row().lineage().basis(), Basis::Unverified);
    assert!(matches!(
        f.client.reserve_baseline(stream, f.current(), 50000).wait(),
        Err(Error::Cooldown(90000))
    ));
    assert!(
        f.client
            .load(LoadMode::Current)
            .wait()
            .unwrap()
            .recovery()
            .iter()
            .all(|r| r.phase() != RecoveryPhase::Pending)
    );
}
#[test]
fn baseline_choice_is_one_use_and_receipt_retry_does_not_rearm_it() {
    let f = Fixture::new();
    let (_, completion, remote) = f.baseline();
    let response = BaselineResponse::Positive {
        raw: raw(&remote),
        user: "U1".into(),
    };
    let BaselineResult::Retained(evidence) = f
        .client
        .retain_baseline(completion.clone(), response.clone(), 0)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    let after = f
        .client
        .finish_baseline(evidence, RemoteChoice::Remote)
        .wait()
        .unwrap();
    assert_eq!(after.row().lineage().basis(), Basis::Remote);
    let BaselineResult::Retained(retry) = f
        .client
        .retain_baseline(completion, response, 0)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    let original = f
        .client
        .finish_baseline(retry.clone(), RemoteChoice::Remote)
        .wait()
        .unwrap();
    assert!(original.same(&after));
    assert!(matches!(
        f.client
            .finish_baseline(retry, RemoteChoice::Local(LocalChoice::Current))
            .wait(),
        Err(Error::Stale)
    ));
    assert!(f.current().same(&after));
}
#[path = "api_recovery_tests.rs"]
mod completion_tests;

#[path = "api_validation_tests.rs"]
mod validation_tests;

#[path = "api_lifecycle_tests.rs"]
mod lifecycle_tests;

//! Completion, release and revocation interleavings through the same worker.
use super::*;

#[test]
fn mutation_retry_returns_original_receipt_without_replacing_newer_typing() {
    let f = Fixture::new();
    let (stream, review) = f.verified("old");
    let ticket = f.client.prepare(stream, review).wait().unwrap();
    let (op, receipt) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    let remote = RemoteDraft {
        scope: f.scope.clone(),
        id: "D1".into(),
        client_msg_id: op.client_msg_id,
        last_updated_ts: "10.000001".into(),
        snapshot: op.snapshot.unwrap(),
    };
    let first = f
        .client
        .record(receipt.clone(), Outcome::Active(remote.clone()), 0)
        .wait()
        .unwrap();
    let writer = f.client.register_writer(first.clone()).wait().unwrap();
    let WriteResult::Applied(newer) = f
        .client
        .checkpoint(writer, 1, first.clone(), local("newer"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    let replay = f
        .client
        .record(receipt, Outcome::Active(remote), 0)
        .wait()
        .unwrap();
    assert!(replay.same(&first));
    assert!(f.current().same(&newer));
}

#[test]
fn failed_gap_disk_write_still_revokes_a_dispatchable_owner() {
    let f = Fixture::new();
    let (stream, review) = f.verified("still dispatchable");
    f.sql("CREATE TRIGGER fail_gap BEFORE UPDATE ON composer_draft_continuity BEGIN SELECT RAISE(ABORT,'fixture gap failure'); END;");
    assert!(matches!(
        f.client
            .mark_gap(stream.clone(), GapReason::StorageFailure)
            .wait(),
        Err(Error::Storage)
    ));
    assert!(f.current().same(&review));
    assert!(matches!(
        f.client.prepare(stream, review).wait(),
        Err(Error::Gap)
    ));
    f.sql("DROP TRIGGER fail_gap;");
    let replacement = f.open();
    let verified = f
        .client
        .verify_local(replacement.clone(), f.current())
        .wait()
        .unwrap();
    assert!(f.client.prepare(replacement, verified).wait().is_ok());
}

#[test]
fn dropped_prepare_reply_leaves_durable_inflight_without_replay_capability() {
    let f = Fixture::new();
    let (stream, review) = f.verified("frozen");
    drop(f.client.prepare(stream, review));
    let current = f.current();
    assert!(matches!(
        current.row().state().workflow(),
        Workflow::InFlight(_)
    ));
    let loaded = f.client.load(LoadMode::AfterRestart).wait().unwrap();
    let restored = loaded.managed().get(&f.scope.key).unwrap();
    assert!(matches!(
        restored.row().state().workflow(),
        Workflow::Uncertain(_)
    ));
}

#[test]
fn explicit_unavailable_status_finishes_only_read_and_retries_disk_only() {
    let f = Fixture::new();
    let (stream, review) = f.verified("send");
    let consumed = f
        .client
        .consume(stream.clone(), review, ConsumeKind::Send)
        .wait()
        .unwrap();
    let ticket = f.client.prepare(stream, consumed).wait().unwrap();
    let (op, receipt) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    assert_eq!(op.kind, OperationKind::Send);
    let uncertain = f
        .client
        .record(receipt.clone(), Outcome::Uncertain, 0)
        .wait()
        .unwrap();
    let expected = f.client.recovery_state("T1".into()).wait().unwrap();
    let recovery = f.client.take_over("T1".into(), expected).wait().unwrap();
    let ticket = f
        .client
        .reserve_check(recovery.clone(), uncertain, receipt, 1000)
        .wait()
        .unwrap();
    let CheckClaim::Ready { completion, .. } =
        f.client.claim_check(recovery, ticket, 1000).wait().unwrap()
    else {
        panic!()
    };
    let result = f
        .client
        .complete_check(
            completion.clone(),
            CheckResult::Nonpositive(KnownReadFailure::Unavailable),
            60000,
        )
        .wait()
        .unwrap();
    assert!(!result.positive_accepted());
    let current = f.current();
    assert!(matches!(
        current.row().state().workflow(),
        Workflow::Uncertain(_)
    ));
    assert!(current.row().state().pending_check(&op).is_none());
    let retry = f
        .client
        .complete_check(
            completion,
            CheckResult::Nonpositive(KnownReadFailure::Unavailable),
            60000,
        )
        .wait()
        .unwrap();
    assert!(!retry.positive_accepted());
    assert!(retry.review().unwrap().same(result.review().unwrap()));
    let loaded = f.client.load(LoadMode::AfterRestart).wait().unwrap();
    assert!(
        loaded
            .recovery()
            .iter()
            .all(|r| r.phase() != RecoveryPhase::Pending)
    );
    assert!(loaded.recovery().iter().any(|r| r.deadline() >= 60000));
}

#[test]
fn invalid_positive_mutation_retains_cooldown_without_confirming() {
    let f = Fixture::new();
    let (stream, review) = f.verified("content");
    let ticket = f.client.prepare(stream, review).wait().unwrap();
    let (op, receipt) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    let invalid = RemoteDraft {
        scope: f.scope.clone(),
        id: "D1".into(),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.000001".into(),
        snapshot: op.snapshot.unwrap(),
    };
    assert!(matches!(
        f.client
            .record(receipt, Outcome::Active(invalid), 120000)
            .wait(),
        Err(Error::Unsupported)
    ));
    assert!(matches!(
        f.current().row().state().workflow(),
        Workflow::InFlight(_)
    ));
    let state = f
        .client
        .recovery_state("T1".into())
        .wait()
        .unwrap()
        .unwrap();
    assert_eq!(state.inner.deadline, 120000);
}

#[test]
fn local_equal_serial_cannot_change_its_predecessor_or_expected_value() {
    let f = Fixture::new();
    let empty = f.client.review_local(f.scope.clone()).wait().unwrap();
    let lease = f.client.renew_local(empty).wait().unwrap();
    let LocalWriteResult::Applied(original) = f
        .client
        .write_local(
            lease.clone(),
            0,
            2,
            LocalDraft::default(),
            local("same output"),
        )
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        f.client
            .write_local(
                lease.clone(),
                1,
                2,
                LocalDraft::default(),
                local("same output")
            )
            .wait(),
        Err(Error::Stale)
    ));
    assert!(matches!(
        f.client
            .write_local(
                lease.clone(),
                0,
                2,
                local("different input"),
                local("same output")
            )
            .wait(),
        Err(Error::Stale)
    ));
    let LocalWriteResult::Applied(retry) = f
        .client
        .write_local(lease, 0, 2, LocalDraft::default(), local("same output"))
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert!(retry.same(&original));
    assert!(
        f.client
            .review_local(f.scope.clone())
            .wait()
            .unwrap()
            .same(&original)
    );
}

#[test]
fn isolated_route_rejects_preexisting_directory_and_identity_replacement() {
    let f = Fixture::new();
    let before = f.seed("retained");
    assert!(
        test_support::Isolated::create(uuid::Uuid::new_v4().to_string(), f.path.clone()).is_err()
    );
    let marker_path = f.path.join("fixture.json");
    let marker = std::fs::read(&marker_path).unwrap();
    std::fs::write(&marker_path, b"different identity").unwrap();
    assert!(matches!(
        f.client.review(f.scope.clone()).wait(),
        Err(Error::Storage)
    ));
    std::fs::write(&marker_path, marker).unwrap();
    assert!(f.current().same(&before));
}

#[test]
fn positive_check_receipt_acceptance_is_explicit_and_preserves_newer_input() {
    let f = Fixture::new();
    let (stream, review) = f.verified("send frozen");
    let consumed = f
        .client
        .consume(stream.clone(), review, ConsumeKind::Send)
        .wait()
        .unwrap();
    let ticket = f.client.prepare(stream, consumed).wait().unwrap();
    let (op, receipt) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    let uncertain = f
        .client
        .record(receipt.clone(), Outcome::Uncertain, 0)
        .wait()
        .unwrap();
    let state = f.client.recovery_state("T1".into()).wait().unwrap();
    let recovery = f.client.take_over("T1".into(), state).wait().unwrap();
    let ticket = f
        .client
        .reserve_check(recovery.clone(), uncertain, receipt, 1000)
        .wait()
        .unwrap();
    let CheckClaim::Ready { completion, .. } =
        f.client.claim_check(recovery, ticket, 1000).wait().unwrap()
    else {
        panic!()
    };
    let current = f.current();
    let writer = f.client.register_writer(current.clone()).wait().unwrap();
    let direct = current.row().state().local_preservation_intent().cloned();
    assert!(matches!(
        f.client
            .checkpoint(writer, 1, current, local("newer while checking"), direct)
            .wait()
            .unwrap(),
        WriteResult::Applied(_)
    ));
    let result = f
        .client
        .complete_check(
            completion.clone(),
            CheckResult::Positive(Outcome::Sent {
                client_msg_id: op.client_msg_id.clone(),
                draft_id: op.base.as_ref().map(|r| r.id.clone()),
                ts: "42.000001".into(),
            }),
            0,
        )
        .wait()
        .unwrap();
    assert!(result.positive_accepted());
    assert_eq!(
        result.review().unwrap().local().composer.text,
        "newer while checking"
    );
    let retry = f
        .client
        .complete_check(
            completion,
            CheckResult::Positive(Outcome::Sent {
                client_msg_id: op.client_msg_id,
                draft_id: op.base.map(|r| r.id),
                ts: "42.000001".into(),
            }),
            0,
        )
        .wait()
        .unwrap();
    assert!(retry.positive_accepted());
    assert!(retry.review().unwrap().same(result.review().unwrap()));
}

#[test]
fn revocation_is_immediate_with_admission_mutex_held() {
    let f = Fixture::new();
    let (stream, _) = f.verified("ready");
    let guard = stream.0.live.lock().unwrap();
    let (other, done) = std::sync::mpsc::channel();
    let clone = stream.clone();
    let task = std::thread::spawn(move || {
        clone.revoke();
        other.send(()).unwrap();
    });
    let result = done.recv_timeout(std::time::Duration::from_secs(1));
    drop(guard);
    task.join().unwrap();
    assert!(result.is_ok());
    assert!(!stream.is_live());
}
#[test]
fn revoke_during_admitted_transaction_withholds_capability_after_commit() {
    let f = Fixture::new();
    let (stream, review) = f.verified("frozen");
    let (entered, ready) = std::sync::mpsc::channel();
    let (resume, release) = std::sync::mpsc::channel();
    let (reply, armed) = std::sync::mpsc::channel();
    f.client.route.submit(commands::Command {
        account: f.scope.account_id.clone(),
        action: commands::Action::PausePrepared {
            entered,
            resume: release,
            reply,
        },
    });
    Reply(armed).wait().unwrap();
    let prepared = f.client.prepare(stream.clone(), review);
    ready
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    stream.revoke();
    assert!(!stream.is_live());
    resume.send(()).unwrap();
    assert!(matches!(prepared.wait(), Err(Error::Gap)));
    assert!(matches!(
        f.current().row().state().workflow(),
        Workflow::InFlight(_)
    ));
}
#[test]
fn isolated_completion_fault_rolls_back_then_original_outcome_retries_once() {
    let f = Fixture::new();
    let (stream, review) = f.verified("frozen");
    let ticket = f.client.prepare(stream, review).wait().unwrap();
    let (op, receipt) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    let outcome = Outcome::Active(RemoteDraft {
        scope: f.scope.clone(),
        id: "D1".into(),
        client_msg_id: op.client_msg_id,
        last_updated_ts: "10.000001".into(),
        snapshot: op.snapshot.unwrap(),
    });
    let before = f.current();
    f._owned
        .fail_next(test_support::FaultPoint::MutationReceipt)
        .wait()
        .unwrap();
    assert!(matches!(
        f.client.record(receipt.clone(), outcome.clone(), 0).wait(),
        Err(Error::Storage)
    ));
    assert!(f.current().same(&before));
    let accepted = f
        .client
        .record(receipt.clone(), outcome.clone(), 0)
        .wait()
        .unwrap();
    let retry = f.client.record(receipt, outcome, 0).wait().unwrap();
    assert!(accepted.same(&retry));
    assert!(f.current().same(&accepted));
}

#[test]
fn malformed_retained_baseline_blocks_load_without_mutating_owner() {
    let f = Fixture::new();
    let (_, completion, remote) = f.baseline();
    f.client
        .retain_baseline(
            completion,
            BaselineResponse::Positive {
                raw: raw(&remote),
                user: "U1".into(),
            },
            0,
        )
        .wait()
        .unwrap();
    let before = f.current();
    f.sql("UPDATE composer_draft_baseline_evidence SET content=json_remove(content,'$.applied')");
    assert!(matches!(
        f.client.load(LoadMode::Current).wait(),
        Err(Error::Storage)
    ));
    assert!(f.current().same(&before));
}

#[test]
fn reviewed_and_rejected_schedule_keep_frozen_intent_and_newer_input() {
    let f = Fixture::new();
    let (stream, completion, remote) = f.baseline();
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
    let reviewed = f
        .client
        .finish_baseline(evidence, RemoteChoice::Remote)
        .wait()
        .unwrap();
    let snapshot = reviewed
        .row()
        .state()
        .acknowledged()
        .unwrap()
        .snapshot
        .clone();
    let scheduled = f
        .client
        .reviewed_schedule(stream.clone(), reviewed, snapshot.clone(), 10_000)
        .wait()
        .unwrap();
    let ticket = f.client.prepare(stream.clone(), scheduled).wait().unwrap();
    let (operation, receipt) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    assert_eq!(operation.kind, OperationKind::Schedule { post_at: 10_000 });
    let rejected = f
        .client
        .record(receipt, Outcome::Rejected, 0)
        .wait()
        .unwrap();
    let writer = f.client.register_writer(rejected.clone()).wait().unwrap();
    let direct = rejected.row().state().local_preservation_intent().cloned();
    let WriteResult::Applied(newer) = f
        .client
        .checkpoint(
            writer,
            7,
            rejected.clone(),
            local("newer after rejection"),
            direct,
        )
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        f.client
            .rejected_schedule(
                stream.clone(),
                rejected,
                crate::drafts::sync::RejectedScheduleChoice::Retry,
                1000
            )
            .wait(),
        Err(Error::Stale)
    ));
    let ready = f
        .client
        .rejected_schedule(
            stream.clone(),
            newer,
            crate::drafts::sync::RejectedScheduleChoice::Retry,
            1000,
        )
        .wait()
        .unwrap();
    assert_eq!(ready.local().composer.text, "newer after rejection");
    let ticket = f.client.prepare(stream, ready).wait().unwrap();
    let (retry, _) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    assert_ne!(retry.id, operation.id);
    assert_eq!(retry.consume_id, operation.consume_id);
    assert_eq!(retry.snapshot, Some(snapshot));
}

#[test]
fn update_then_delete_receipts_use_exact_operation_without_erasing_newer_local() {
    let f = Fixture::new();
    let (stream, completion, remote) = f.baseline();
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
    let reviewed = f
        .client
        .finish_baseline(evidence, RemoteChoice::Remote)
        .wait()
        .unwrap();
    let writer = f.client.register_writer(reviewed.clone()).wait().unwrap();
    let WriteResult::Applied(edited) = f
        .client
        .checkpoint(writer, 1, reviewed, local("updated"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    let ticket = f.client.prepare(stream.clone(), edited).wait().unwrap();
    let (update, receipt) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    assert_eq!(update.kind, OperationKind::Update);
    let active = RemoteDraft {
        scope: f.scope.clone(),
        id: remote.id.clone(),
        client_msg_id: update.client_msg_id,
        last_updated_ts: "12.000001".into(),
        snapshot: update.snapshot.unwrap(),
    };
    let acknowledged = f
        .client
        .record(receipt, Outcome::Active(active), 0)
        .wait()
        .unwrap();
    let cleared = f.client.clear(acknowledged).wait().unwrap();
    let ticket = f.client.prepare(stream, cleared).wait().unwrap();
    let (delete, receipt) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    assert_eq!(delete.kind, OperationKind::Delete);
    let current = f.current();
    let writer = f.client.register_writer(current.clone()).wait().unwrap();
    let WriteResult::Applied(_) = f
        .client
        .checkpoint(writer, 1, current, local("after delete started"), None)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    let completed = f
        .client
        .record(
            receipt,
            Outcome::Deleted {
                draft_id: remote.id,
            },
            0,
        )
        .wait()
        .unwrap();
    assert_eq!(completed.local().composer.text, "after delete started");
    assert_eq!(completed.row().lineage().basis(), Basis::Unverified);
}

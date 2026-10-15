//! Actual isolated Owned Worker/SQLite and real command builder; no HTTP/native.
use super::super::admission::*;
use super::*;
use crate::drafts::store::protected as p;
use crate::slash_commands::{
    controller::{self, Delivery, Phase},
    journal,
};
#[path = "completion_test_support.rs"]
mod support;
use support::*;

fn setup() -> (Fixture, Arc<p::slash_runtime::Input>, Arc<AtomicBool>) {
    let f = Fixture::for_slash();
    let writer = call(&f.queue, f.queue.worker.hold_register(f.current()).unwrap());
    let step = f
        .queue
        .worker
        .hold_checkpoint(CheckpointInput {
            writer: writer.clone(),
            serial: 1,
            expected: f.current(),
            local: sync::tests::local("/example argument"),
            direct: None,
        })
        .unwrap();
    f.queue.worker.submit_step(&step).unwrap();
    let WriteOutcome::Applied(review) =
        until(|| step.write_result().ok_or(Reason::Incomplete)).unwrap()
    else {
        panic!("checkpoint")
    };
    let alive = Arc::new(AtomicBool::new(true));
    let plan = Arc::new(controller::fixture_plan(&review, alive.clone()));
    let input = Arc::new(p::slash_runtime::Input {
        stream: f.stream.clone(),
        expected: review,
        writer,
        serial: 1,
        plan,
    });
    (f, input, alive)
}
fn reserved(f: &Fixture, input: Arc<p::slash_runtime::Input>) -> SlashReserved {
    let s = f.queue.worker.hold_slash(input).unwrap();
    f.queue.worker.submit_step(&s).unwrap();
    wait(&s);
    s
}
fn claimed(f: &Fixture, input: Arc<p::slash_runtime::Input>) -> SlashClaimed {
    let s = reserved(f, input);
    let c = until(|| f.queue.worker.claim_slash(&s));
    f.queue.worker.submit_step(&c).unwrap();
    wait(&c);
    c
}
fn completion(f: &Fixture, input: Arc<p::slash_runtime::Input>) -> SlashCompletion {
    let s = claimed(f, input);
    until(|| f.queue.worker.begin_slash_completion(&s))
}
fn finished(c: &SlashCompletion) -> StepState {
    until(|| match c.state() {
        StepState::Waiting => Err(Reason::Incomplete),
        v => Ok(v),
    })
}
fn records(f: &Fixture) -> Vec<controller::Record> {
    journal::load(
        &mut DraftStore::open(f.queue.owner.path()).unwrap().0,
        &f.scope.account_id,
    )
    .unwrap()
}
#[test]
fn actual_paired_queue_claim_request_ack_and_first_commit_clear_are_one_chain() {
    let (f, input, _) = setup();
    let c = completion(&f, input.clone());
    assert_eq!(records(&f), vec![input.plan.next().clone()]);
    let permit = f.queue.worker.arm_slash(&c).unwrap();
    assert!(permit.current(input.plan.transport()));
    let crate::slack::client::RequestBody::Form(fields) = &permit.request().body else {
        panic!("actual form")
    };
    let url = url::Url::parse(&permit.request().url).unwrap();
    assert_eq!(permit.request().method, "POST");
    assert_eq!(
        url.origin().ascii_serialization(),
        "https://synthetic.slack.com"
    );
    assert_eq!(url.path(), "/api/chat.command");
    assert!(url.fragment().is_none());
    let query: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
    assert_eq!(query.get("_x_num_retries").map(|v| v.as_ref()), Some("0"));
    assert_eq!(
        query.get("slack_route").map(|v| v.as_ref()),
        Some("default")
    );
    for key in ["_x_id", "_x_csid", "fp"] {
        assert!(uuid::Uuid::parse_str(query.get(key).unwrap()).is_ok());
    }
    assert_eq!(
        fields.iter().find(|(k, _)| k == "command").unwrap().1,
        "/example"
    );
    assert_eq!(
        fields.iter().find(|(k, _)| k == "disp").unwrap().1,
        "/example"
    );
    let blocks: serde_json::Value =
        serde_json::from_str(&fields.iter().find(|(k, _)| k == "blocks").unwrap().1).unwrap();
    assert_eq!(
        blocks,
        serde_json::json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"argument"}]}]}])
    );
    assert!(matches!(f.queue.worker.arm_slash(&c), Err(Reason::Stale)));
    // Arm performs no second journal transition.
    assert_eq!(records(&f), vec![input.plan.next().clone()]);
    let report = permit.report(Delivery::Reply(serde_json::json!({"ok":true})), None);
    f.queue.worker.bind_slash_report(&c, report).unwrap();
    f.queue.worker.submit_slash_completion(&c).unwrap();
    assert_eq!(finished(&c), StepState::Ready);
    let terminal = c.terminal().unwrap();
    let witness = c.take_fresh_clear().expect("new exact attested commit");
    assert!(witness.matches(input.plan.prior(), &terminal));
    assert!(c.take_fresh_clear().is_none());
    assert!(f.current().local().empty());
    assert_eq!(records(&f), vec![terminal]);
}
#[test]
fn lost_handles_recover_original_inputs_and_actual_claim_without_reissuance() {
    let (f, input, _) = setup();
    let r = reserved(&f, input.clone());
    let key = r.key();
    drop(r);
    let r = f
        .queue
        .worker
        .reattach_step::<Arc<p::slash_runtime::Input>, ()>(&key)
        .unwrap();
    assert!(Arc::ptr_eq(r.input().as_ref(), &input));
    let c = until(|| f.queue.worker.claim_slash(&r));
    f.queue.worker.submit_step(&c).unwrap();
    wait(&c);
    let key = c.key();
    drop(c);
    let c = f
        .queue
        .worker
        .reattach_step::<Arc<p::slash_runtime::Input>, p::slash_runtime::QueuedClaim>(&key)
        .unwrap();
    let held = until(|| f.queue.worker.begin_slash_completion(&c));
    let again = f.queue.worker.begin_slash_completion(&c).unwrap();
    let key = held.key();
    drop(held);
    drop(again);
    drop(c);
    let recovered = f.queue.worker.resume_slash_completion(&key).unwrap();
    assert!(Arc::ptr_eq(&recovered.input(), &input));
    let permit = f.queue.worker.arm_slash(&recovered).unwrap();
    drop(permit);
    assert!(matches!(
        f.queue.worker.arm_slash(&recovered),
        Err(Reason::Stale)
    ));
    assert_eq!(records(&f), vec![input.plan.next().clone()]);
}
#[test]
fn original_open_generation_is_required_even_after_fence_resume() {
    let (f, input, _) = setup();
    let c = completion(&f, input.clone());
    let fence = f.queue.worker.begin_fence().unwrap();
    assert!(matches!(f.queue.worker.arm_slash(&c), Err(Reason::Fencing)));
    f.queue.worker.resume_admission(&fence).unwrap();
    assert!(matches!(f.queue.worker.arm_slash(&c), Err(Reason::Stale)));
    assert_eq!(records(&f), vec![input.plan.next().clone()]);
    assert!(c.report().is_none());
}
#[test]
fn admission_refusal_returns_exact_input_and_does_not_reserve_journal() {
    let (f, input, _) = setup();
    let _fence = f.queue.worker.begin_fence().unwrap();
    let refused = f.queue.worker.hold_slash(input.clone()).err().unwrap();
    assert_eq!(refused.reason, Reason::Fencing);
    assert!(Arc::ptr_eq(&refused.input, &input));
    assert!(records(&f).is_empty());
}
#[test]
fn revoked_native_admission_does_not_consume_actual_claim_or_issue_request() {
    let (f, input, alive) = setup();
    let c = completion(&f, input.clone());
    alive.store(false, Ordering::Release);
    assert!(matches!(f.queue.worker.arm_slash(&c), Err(Reason::Stale)));
    assert!(c.report().is_none());
    assert_eq!(records(&f), vec![input.plan.next().clone()]);
}
#[test]
fn sealed_report_survives_unsupported_queue_refusal_and_lost_waiter() {
    let (f, input, _) = setup();
    let c = completion(&f, input);
    let permit = f.queue.worker.arm_slash(&c).unwrap();
    let (reply, receive) = mpsc::channel();
    f.queue.submit_protected(ProtectedCommand {
        account: f.scope.account_id.clone(),
        action: p::commands::Action::Review {
            scope: f.scope.clone(),
            reply,
        },
    });
    receive.recv().unwrap().unwrap();
    f.queue
        .worker
        .bind_slash_report(&c, permit.report(Delivery::Unknown, None))
        .unwrap();
    let raw = c.report().unwrap();
    assert_eq!(
        f.queue.worker.submit_slash_completion(&c),
        Err(Reason::Unsupported)
    );
    let key = c.key();
    drop(c);
    let recovered = f.queue.worker.resume_slash_completion(&key).unwrap();
    assert!(Arc::ptr_eq(&raw, &recovered.report().unwrap()));
    assert!(recovered.take_fresh_clear().is_none());
}
#[test]
fn unknown_and_keep_input_retain_input_and_never_offer_clear() {
    for delivery in [
        Delivery::Unknown,
        Delivery::Reply(serde_json::json!({"ok":true,"keep_input":true})),
        Delivery::Reply(serde_json::json!({"ok":false,"error":"invalid_command"})),
    ] {
        let (f, input, _) = setup();
        let original = input.expected.local().clone();
        let c = completion(&f, input);
        let permit = f.queue.worker.arm_slash(&c).unwrap();
        f.queue
            .worker
            .bind_slash_report(&c, permit.report(delivery, None))
            .unwrap();
        f.queue.worker.submit_slash_completion(&c).unwrap();
        assert_eq!(finished(&c), StepState::Ready);
        assert!(c.take_fresh_clear().is_none());
        assert_eq!(
            serde_json::to_value(f.current().local()).unwrap(),
            serde_json::to_value(original).unwrap()
        );
    }
}
#[test]
fn failed_attested_commit_retains_report_then_flushes_without_rearm() {
    let (f, input, _) = setup();
    let c = completion(&f, input.clone());
    let claim_activity = f.queue.worker.pending.lock().unwrap().upgrade_generation;
    let permit = f.queue.worker.arm_slash(&c).unwrap();
    let db = DraftStore::open(f.queue.owner.path()).unwrap();
    db.0.execute_batch("CREATE TRIGGER slash_fail BEFORE INSERT ON composer_draft_slash_completions BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    f.queue
        .worker
        .bind_slash_report(
            &c,
            permit.report(Delivery::Reply(serde_json::json!({"ok":true})), None),
        )
        .unwrap();
    let raw = c.report().unwrap();
    f.queue.worker.submit_slash_completion(&c).unwrap();
    assert_eq!(finished(&c), StepState::Failed(p::Error::Storage));
    assert!(c.take_fresh_clear().is_none());
    assert_eq!(records(&f), vec![input.plan.next().clone()]);
    // Completion submission advances the independent queue fence before the
    // handed/report guard. Refusal must retain the same evidence and dispatch.
    assert!(f.queue.worker.pending.lock().unwrap().upgrade_generation > claim_activity);
    assert!(matches!(
        f.queue.worker.arm_slash(&c),
        Err(Reason::Incomplete)
    ));
    assert!(Arc::ptr_eq(&raw, &c.report().unwrap()));
    assert_eq!(records(&f), vec![input.plan.next().clone()]);
    assert!(!f.current().local().empty());
    db.0.execute_batch("DROP TRIGGER slash_fail").unwrap();
    until(|| f.queue.worker.retry_slash_completion(&c));
    assert_eq!(finished(&c), StepState::Ready);
    assert!(Arc::ptr_eq(&raw, &c.report().unwrap()));
    assert!(c.take_fresh_clear().is_some());
    let terminal = c.terminal().unwrap();
    assert_eq!(records(&f), vec![terminal.clone()]);
    let before_refusal = f.queue.worker.pending.lock().unwrap().upgrade_generation;
    assert!(matches!(
        f.queue.worker.arm_slash(&c),
        Err(Reason::Incomplete)
    ));
    assert_eq!(
        f.queue.worker.pending.lock().unwrap().upgrade_generation,
        before_refusal
    );
    assert!(Arc::ptr_eq(&raw, &c.report().unwrap()));
    assert_eq!(records(&f), vec![terminal]);
    assert!(c.take_fresh_clear().is_none());
}

#[test]
fn detached_checkpoint_and_completed_new_input_both_refuse_old_claim_handoff() {
    let (f, input, _) = setup();
    let c = completion(&f, input.clone());
    let db = DraftStore::open(f.queue.owner.path()).unwrap();
    let current = p::slash_runtime::fixture_review(&db, &f.scope);
    db.0.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let s = f
        .queue
        .worker
        .hold_checkpoint(CheckpointInput {
            writer: input.writer.clone(),
            serial: 2,
            expected: current,
            local: sync::tests::local("newer retained input"),
            direct: None,
        })
        .unwrap();
    f.queue.worker.submit_step(&s).unwrap();
    until(|| {
        if f.queue
            .diagnostics()
            .unwrap()
            .records
            .iter()
            .any(|r| r.stage == accounting::Stage::Detached)
        {
            Ok(())
        } else {
            Err(Reason::Incomplete)
        }
    });
    // Actual work has left Pending and is blocked by the real SQLite writer.
    assert!(f.queue.worker.pending.lock().unwrap().requests.is_empty());
    assert!(matches!(
        f.queue.worker.arm_slash(&c),
        Err(Reason::Incomplete)
    ));
    db.0.execute_batch("COMMIT").unwrap();
    let result = until(|| s.write_result().ok_or(Reason::Incomplete)).unwrap();
    assert!(matches!(result, WriteOutcome::Applied(_)));
    // Completed work cannot erase the activity stamp captured by this Claim.
    assert!(matches!(
        f.queue.worker.arm_slash(&c),
        Err(Reason::Incomplete)
    ));
    assert!(c.report().is_none());
    assert_eq!(records(&f), vec![input.plan.next().clone()]);
}

#[test]
fn pending_input_arriving_during_claim_database_open_refuses_admission() {
    let (f, input, _) = setup();
    let r = reserved(&f, input.clone());
    let claim = until(|| f.queue.worker.claim_slash(&r));
    let (entered, arrived) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    *f.queue.worker.slash_open_gate.lock().unwrap() = Some((entered, release));
    f.queue.worker.submit_step(&claim).unwrap();
    arrived
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let newer = f
        .queue
        .worker
        .hold_checkpoint(CheckpointInput {
            writer: input.writer.clone(),
            serial: 2,
            expected: input.expected.clone(),
            local: sync::tests::local("input while opening"),
            direct: None,
        })
        .unwrap();
    f.queue.worker.submit_step(&newer).unwrap();
    resume.send(()).unwrap();
    let state = until(|| match claim.state() {
        StepState::Waiting => Err(Reason::Incomplete),
        v => Ok(v),
    });
    assert_eq!(state, StepState::Failed(p::Error::Pending));
    assert!(f.queue.worker.begin_slash_completion(&claim).is_err());
    assert_eq!(records(&f), vec![input.plan.prior().clone()]);
    assert!(matches!(
        until(|| newer.write_result().ok_or(Reason::Incomplete)),
        Ok(WriteOutcome::Applied(_))
    ));
}

#[test]
fn foreign_report_is_returned_intact_and_cannot_replace_original_receipt() {
    let (f, input, _) = setup();
    let c = completion(&f, input);
    let permit = f.queue.worker.arm_slash(&c).unwrap();
    let (other, foreign, _) = setup();
    let other_c = completion(&other, foreign.clone());
    let other_permit = other.queue.worker.arm_slash(&other_c).unwrap();
    let refused = f
        .queue
        .worker
        .bind_slash_report(&c, other_permit.report(Delivery::Unknown, None))
        .err()
        .unwrap();
    assert_eq!(refused.reason, Reason::Stale);
    assert!(foreign.plan.accepts(&refused.input));
    assert!(c.report().is_none());
    f.queue
        .worker
        .bind_slash_report(&c, permit.report(Delivery::Unknown, None))
        .unwrap();
    assert!(c.report().is_some());
}

#[test]
fn lost_completion_waiter_preserves_original_new_commit_witness_in_owner() {
    let (f, input, _) = setup();
    let c = completion(&f, input.clone());
    let permit = f.queue.worker.arm_slash(&c).unwrap();
    f.queue
        .worker
        .bind_slash_report(
            &c,
            permit.report(Delivery::Reply(serde_json::json!({"ok":true})), None),
        )
        .unwrap();
    let key = c.key();
    f.queue.worker.submit_slash_completion(&c).unwrap();
    drop(c);
    let recovered = f.queue.worker.resume_slash_completion(&key).unwrap();
    assert_eq!(finished(&recovered), StepState::Ready);
    let terminal = recovered.terminal().unwrap();
    let first = recovered.take_fresh_clear().unwrap();
    assert!(first.matches(input.plan.prior(), &terminal));
    let again = f.queue.worker.resume_slash_completion(&key).unwrap();
    assert!(again.take_fresh_clear().is_none());
    f.queue.worker.submit_slash_completion(&again).unwrap();
    assert!(again.take_fresh_clear().is_none());
}

#[tokio::test]
async fn actual_once_attempt_adapter_retains_report_before_storage() {
    let (f, input, _) = setup();
    let c = completion(&f, input);
    f.queue
        .worker
        .fixture_execute_slash(&c, |permit| async move {
            permit.report(Delivery::NotSent, None)
        })
        .await
        .unwrap();
    assert!(c.report().is_some());
    assert_eq!(finished(&c), StepState::Ready);
    assert!(matches!(c.terminal().unwrap().phase(), Phase::NotSent));
    assert!(c.take_fresh_clear().is_none());
    assert!(
        f.queue
            .worker
            .fixture_execute_slash(&c, |_| async { panic!("must not reattempt") })
            .await
            .is_err()
    );
}

#[test]
fn canceled_http_future_leaves_original_dispatch_unresolved_without_replay() {
    use std::future::Future;
    let (f, input, _) = setup();
    let c = completion(&f, input.clone());
    let mut future = Box::pin(
        f.queue
            .worker
            .fixture_execute_slash(&c, |permit| async move {
                std::future::pending::<()>().await;
                permit.report(Delivery::Unknown, None)
            }),
    );
    assert!(
        future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
            .is_pending()
    );
    drop(future);
    assert!(c.report().is_none());
    assert_eq!(records(&f), vec![input.plan.next().clone()]);
    assert!(matches!(f.queue.worker.arm_slash(&c), Err(Reason::Stale)));
    assert!(c.take_fresh_clear().is_none());
}

#[test]
fn held_unsubmitted_checkpoint_is_an_input_fence_before_claim_extraction() {
    let (f, input, _) = setup();
    let c = completion(&f, input.clone());
    let before = f.queue.worker.pending.lock().unwrap().upgrade_generation;
    let db = DraftStore::open(f.queue.owner.path()).unwrap();
    let current = p::slash_runtime::fixture_review(&db, &f.scope);
    let local = sync::tests::local("held but not submitted");
    let held = f
        .queue
        .worker
        .hold_checkpoint(CheckpointInput {
            writer: input.writer.clone(),
            serial: 2,
            expected: current,
            local: local.clone(),
            direct: None,
        })
        .unwrap();
    assert_eq!(
        f.queue.worker.pending.lock().unwrap().upgrade_generation,
        before
    );
    assert!(matches!(
        f.queue.worker.arm_slash(&c),
        Err(Reason::Incomplete)
    ));
    assert_eq!(
        serde_json::to_value(&held.input().local).unwrap(),
        serde_json::to_value(local).unwrap()
    );
    let key = c.key();
    drop(c);
    let recovered = f.queue.worker.resume_slash_completion(&key).unwrap();
    assert!(matches!(
        f.queue.worker.arm_slash(&recovered),
        Err(Reason::Incomplete)
    ));
    assert!(recovered.report().is_none());
    assert_eq!(records(&f), vec![input.plan.next().clone()]);
}

//! Actual Owned Worker and temporary SQLite; no transport/session/native input.
use super::super::admission::*;
use super::*;
use crate::drafts::store::protected as p;
#[path = "completion_test_support.rs"]
mod support;
use support::*;

fn register(f: &Fixture) -> p::Writer {
    call(&f.queue, f.queue.worker.hold_register(f.current()).unwrap())
}
fn written(step: &Checkpointed) -> Result<WriteOutcome, p::Error> {
    until(|| step.write_result().ok_or(Reason::Incomplete))
}
fn checkpoint(
    f: &Fixture,
    writer: p::Writer,
    serial: u64,
    expected: p::Review,
    text: &str,
) -> Checkpointed {
    let step = f
        .queue
        .worker
        .hold_checkpoint(CheckpointInput {
            writer,
            serial,
            expected,
            local: sync::tests::local(text),
            direct: None,
        })
        .unwrap();
    f.queue.worker.submit_step(&step).unwrap();
    step
}
fn send(f: &Fixture) -> SendConsumed {
    let s = f
        .queue
        .worker
        .hold_send(SendInput {
            stream: f.stream.clone(),
            expected: f.current(),
        })
        .unwrap();
    f.queue.worker.submit_step(&s).unwrap();
    wait(&s);
    s
}
fn sent(f: &Fixture) -> (Completion, p::Review) {
    let s = send(f);
    let prepared = until(|| f.queue.worker.prepare_send(&s));
    f.queue.worker.submit_step(&prepared).unwrap();
    wait(&prepared);
    let claimed = until(|| f.queue.worker.claim_send(&prepared));
    f.queue.worker.submit_step(&claimed).unwrap();
    wait(&claimed);
    let c = until(|| f.queue.worker.begin_send_record(&claimed));
    let Dispatch::Mutation(op) = f.queue.worker.take_dispatch(&c).unwrap() else {
        panic!("issued Send")
    };
    let (op, _) = op.into_parts();
    assert_eq!(op.kind, crate::drafts::sync::OperationKind::Send);
    let result = finish(
        &f.queue,
        &c,
        Raw::Mutation {
            outcome: p::Outcome::Sent {
                client_msg_id: op.client_msg_id.clone(),
                draft_id: op.base.as_ref().map(|b| b.id.clone()),
                ts: "20.000001".into(),
            },
            known_until: 0,
        },
    );
    let Finished::Mutation(Ok(review)) = &*result else {
        panic!("record Send")
    };
    (c, review.clone())
}
fn released(f: &Fixture, writer: p::Writer, review: p::Review) -> Released {
    let r = f
        .queue
        .worker
        .hold_release(ReleaseInput {
            expected: review,
            writer,
        })
        .unwrap();
    f.queue.worker.submit_step(&r).unwrap();
    wait(&r);
    r
}

#[test]
fn registration_survives_last_waiter_and_preserves_actual_writer() {
    let f = Fixture::new();
    let r = f.queue.worker.hold_register(f.review.clone()).unwrap();
    f.queue.worker.submit_step(&r).unwrap();
    wait(&r);
    let writer = r.value().unwrap();
    let key = r.key();
    drop(r);
    let r = f
        .queue
        .worker
        .reattach_step::<p::Review, p::Writer>(&key)
        .unwrap();
    assert_eq!(r.value().unwrap().id(), writer.id());
    let count = f.queue.diagnostics().unwrap().records.len();
    f.queue.worker.submit_step(&r).unwrap();
    assert_eq!(count, f.queue.diagnostics().unwrap().records.len());
}
#[test]
fn checkpoint_storage_rollback_retries_same_payload_writer_and_serial() {
    let f = Fixture::new();
    let w = register(&f);
    let before = f.current();
    f.sql("CREATE TRIGGER fail_write BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'fixture'); END;");
    let c = checkpoint(&f, w.clone(), 1, before.clone(), "newer \u{65e5}\u{1f680}");
    assert!(matches!(written(&c), Err(p::Error::Storage)));
    let input = c.input();
    assert!(f.current().same(&before));
    f.sql("DROP TRIGGER fail_write;");
    until(|| f.queue.worker.retry_checkpoint(&c));
    let Ok(WriteOutcome::Applied(after)) = written(&c) else {
        panic!("checkpoint")
    };
    assert_eq!(
        serde_json::to_value(after.local()).unwrap(),
        serde_json::to_value(&input.local).unwrap()
    );
    assert!(Arc::ptr_eq(&input, &c.input()));
    assert_eq!(c.failed_attempts().len(), 1);
    assert_eq!(c.input().writer.id(), w.id());
    assert_eq!(c.input().serial, 1);
    assert_eq!(f.queue.worker.retry_checkpoint(&c), Err(Reason::Stale));
}
#[test]
fn checkpoint_fence_refuses_retry_without_reclassifying_or_losing_input() {
    let f = Fixture::new();
    let w = register(&f);
    f.sql("CREATE TRIGGER fail_write BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'fixture'); END;");
    let c = checkpoint(&f, w, 1, f.current(), "retained input");
    assert!(matches!(written(&c), Err(p::Error::Storage)));
    let input = c.input();
    let fence = f.queue.worker.begin_fence().unwrap();
    assert_eq!(f.queue.worker.retry_checkpoint(&c), Err(Reason::Fencing));
    f.queue.worker.resume_admission(&fence).unwrap();
    assert_eq!(f.queue.worker.retry_checkpoint(&c), Err(Reason::Stale));
    assert!(Arc::ptr_eq(&input, &c.input()));
    assert!(c.failed_attempts().is_empty());
    assert!(matches!(written(&c), Err(p::Error::Storage)));
}
#[test]
fn four_slots_are_reserved_before_consume_and_refusal_returns_exact_input() {
    let f = Fixture::new();
    while f.queue.worker.retained_keys().len() < 61 {
        f.queue.worker.hold_managed_review(f.scope.clone()).unwrap();
    }
    let input = SendInput {
        stream: f.stream.clone(),
        expected: f.review.clone(),
    };
    let ptr = input.expected.local().composer.text.as_ptr();
    let count = f.queue.diagnostics().unwrap().records.len();
    let e = match f.queue.worker.hold_send(input) {
        Err(e) => e,
        Ok(_) => panic!("full family"),
    };
    assert_eq!(e.reason, Reason::Capacity);
    assert_eq!(ptr, e.input.expected.local().composer.text.as_ptr());
    assert!(e.input.expected.row().state().consumption().is_none());
    assert_eq!(count, f.queue.diagnostics().unwrap().records.len());
}
#[test]
fn four_prebound_slots_finish_when_holder_capacity_is_full() {
    let f = Fixture::new();
    while f.queue.worker.retained_keys().len() < 60 {
        f.queue.worker.hold_managed_review(f.scope.clone()).unwrap();
    }
    // No further Review command/slot: issue against the already exact Review.
    let s = f
        .queue
        .worker
        .hold_send(SendInput {
            stream: f.stream.clone(),
            expected: f.review.clone(),
        })
        .unwrap();
    assert_eq!(f.queue.worker.retained_keys().len(), 64);
    f.queue.worker.submit_step(&s).unwrap();
    wait(&s);
    let p = until(|| f.queue.worker.prepare_send(&s));
    f.queue.worker.submit_step(&p).unwrap();
    wait(&p);
    let c = until(|| f.queue.worker.claim_send(&p));
    f.queue.worker.submit_step(&c).unwrap();
    wait(&c);
    let finish = until(|| f.queue.worker.begin_send_record(&c));
    f.queue.worker.cancel_before_dispatch(&finish).unwrap();
    f.queue.worker.submit_completion(&finish).unwrap();
    assert!(matches!(&*finished(&finish), Finished::Mutation(Ok(_))));
    assert_eq!(f.queue.worker.retained_keys().len(), 64);
}
#[test]
fn fence_after_consume_allows_registered_storage_but_never_http_handoff() {
    let f = Fixture::new();
    let s = send(&f);
    let frozen = s.value().unwrap();
    let fence = f.queue.worker.begin_fence().unwrap();
    f.queue.worker.resume_admission(&fence).unwrap();
    let p = until(|| f.queue.worker.prepare_send(&s));
    f.queue.worker.submit_step(&p).unwrap();
    wait(&p);
    let c = until(|| f.queue.worker.claim_send(&p));
    f.queue.worker.submit_step(&c).unwrap();
    wait(&c);
    let end = until(|| f.queue.worker.begin_send_record(&c));
    assert!(matches!(
        f.queue.worker.take_dispatch(&end),
        Err(Reason::Stale)
    ));
    f.queue.worker.cancel_before_dispatch(&end).unwrap();
    f.queue.worker.submit_completion(&end).unwrap();
    let result = finished(&end);
    let Finished::Mutation(Ok(review)) = &*result else {
        panic!("cancel")
    };
    assert_eq!(
        review.row().state().consumption(),
        frozen.row().state().consumption()
    );
}
#[test]
fn late_release_admission_is_not_granted_by_completed_send() {
    let f = Fixture::new();
    let w = register(&f);
    let (_, review) = sent(&f);
    let fence = f.queue.worker.begin_fence().unwrap();
    let e = match f.queue.worker.hold_release(ReleaseInput {
        expected: review.clone(),
        writer: w,
    }) {
        Err(e) => e,
        Ok(_) => panic!("new release"),
    };
    assert_eq!(e.reason, Reason::Fencing);
    assert!(e.input.expected.same(&review));
    f.queue.worker.resume_admission(&fence).unwrap();
    assert!(f.current().same(&review));
}
#[test]
fn successful_local_handoff_survives_waiter_loss_and_is_not_reexecuted() {
    let f = Fixture::new();
    let w = register(&f);
    let (_, review) = sent(&f);
    let c = checkpoint(&f, w.clone(), 1, review, "newer after Send");
    let Ok(WriteOutcome::Applied(review)) = written(&c) else {
        panic!("new local")
    };
    let expected = serde_json::to_value(review.local()).unwrap();
    let r = released(&f, w.clone(), review);
    let key = r.key();
    drop(r);
    let r = f
        .queue
        .worker
        .reattach_step::<ReleaseInput, p::Release>(&key)
        .unwrap();
    let c = until(|| f.queue.worker.begin_start_local(&r));
    f.queue.worker.submit_completion(&c).unwrap();
    let key = c.key();
    drop(c);
    let c = f.queue.worker.reattach_completion(&key).unwrap();
    let result = finished(&c);
    let Finished::Local(Ok(lease)) = &*result else {
        panic!("local lease")
    };
    assert_eq!(lease.writer_id(), w.id());
    let count = f.queue.diagnostics().unwrap().records.len();
    f.queue.worker.submit_completion(&c).unwrap();
    assert!(Arc::ptr_eq(&result, &finished(&c)));
    assert_eq!(count, f.queue.diagnostics().unwrap().records.len());
    let inventory = call(
        &f.queue,
        f.queue
            .worker
            .hold_local_inventory(f.scope.clone())
            .unwrap(),
    );
    assert_eq!(serde_json::to_value(inventory.local()).unwrap(), expected);
    assert!(inventory.row().is_none());
}
#[test]
fn start_local_rollback_keeps_exact_release_and_finishes_across_resume() {
    let f = Fixture::new();
    let w = register(&f);
    let (_, review) = sent(&f);
    let r = released(&f, w, review);
    let before = call(
        &f.queue,
        f.queue
            .worker
            .hold_local_inventory(f.scope.clone())
            .unwrap(),
    );
    call(
        &f.queue,
        f.queue
            .worker
            .hold_local_fault(
                f.scope.clone(),
                p::test_support::FaultPoint::StartLocalCommit,
            )
            .unwrap(),
    );
    let c = until(|| f.queue.worker.begin_start_local(&r));
    f.queue.worker.submit_completion(&c).unwrap();
    assert!(matches!(
        &*finished(&c),
        Finished::Local(Err(p::Error::Storage))
    ));
    let raw = c.raw().unwrap();
    let after = call(
        &f.queue,
        f.queue
            .worker
            .hold_local_inventory(f.scope.clone())
            .unwrap(),
    );
    assert!(before.same(&after));
    let fence = f.queue.worker.begin_fence().unwrap();
    f.queue.worker.resume_admission(&fence).unwrap();
    until(|| f.queue.worker.retry_completion(&c));
    assert!(matches!(&*finished(&c), Finished::Local(Ok(_))));
    assert!(Arc::ptr_eq(&raw, &c.raw().unwrap()));
    assert_eq!(f.queue.worker.retry_completion(&c), Err(Reason::Stale));
}
#[test]
fn release_child_binding_after_resume_uses_same_registered_result() {
    let f = Fixture::new();
    let w = register(&f);
    let (_, review) = sent(&f);
    let r = released(&f, w, review);
    let fence = f.queue.worker.begin_fence().unwrap();
    f.queue.worker.resume_admission(&fence).unwrap();
    let c = until(|| f.queue.worker.begin_start_local(&r));
    f.queue.worker.submit_completion(&c).unwrap();
    assert!(matches!(&*finished(&c), Finished::Local(Ok(_))));
    let again = until(|| f.queue.worker.begin_start_local(&r));
    assert!(Arc::ptr_eq(&finished(&c), &finished(&again)));
}
#[test]
fn checkpoint_capacity_refusal_preserves_exact_unmoved_local_input() {
    let f = Fixture::new();
    let writer = register(&f);
    let expected = f.current();
    while f.queue.worker.retained_keys().len() < 64 {
        f.queue.worker.hold_managed_review(f.scope.clone()).unwrap();
    }
    let input = CheckpointInput {
        writer,
        serial: 1,
        expected,
        local: sync::tests::local("full input \u{1f680}"),
        direct: None,
    };
    let ptr = input.local.composer.text.as_ptr();
    let value = serde_json::to_value(&input.local).unwrap();
    let count = f.queue.diagnostics().unwrap().records.len();
    let e = match f.queue.worker.hold_checkpoint(input) {
        Err(e) => e,
        Ok(_) => panic!("capacity"),
    };
    assert_eq!(e.reason, Reason::Capacity);
    assert_eq!(e.input.local.composer.text.as_ptr(), ptr);
    assert_eq!(serde_json::to_value(&e.input.local).unwrap(), value);
    assert_eq!(f.queue.diagnostics().unwrap().records.len(), count);
}
#[test]
fn explicit_preserve_keeps_alternate_without_replacing_canonical() {
    let f = Fixture::new();
    let writer = register(&f);
    let before = f.current();
    let local = sync::tests::local("explicit alternate");
    let p = f
        .queue
        .worker
        .hold_preserve(PreserveInput {
            writer: writer.clone(),
            serial: 1,
            local: local.clone(),
        })
        .unwrap();
    f.queue.worker.submit_step(&p).unwrap();
    let value = until(|| p.write_result().ok_or(Reason::Incomplete));
    let Ok(WriteOutcome::Preserved(inventory)) = value else {
        panic!("preserved")
    };
    assert_eq!(
        serde_json::to_value(inventory.local()).unwrap(),
        serde_json::to_value(before.local()).unwrap()
    );
    assert!(
        inventory
            .alternates()
            .iter()
            .any(|a| a.writer_id == writer.id()
                && serde_json::to_value(&a.draft).unwrap()
                    == serde_json::to_value(&local).unwrap())
    );
    assert_eq!(f.queue.worker.retry_preserve(&p), Err(Reason::Stale));
}
#[test]
fn foreign_writer_does_not_gain_release_admission() {
    let f = Fixture::new();
    let other = Fixture::new();
    let writer = register(&other);
    let review = f.current();
    let count = f.queue.worker.retained_keys().len();
    let e = match f.queue.worker.hold_release(ReleaseInput {
        writer: writer.clone(),
        expected: review.clone(),
    }) {
        Err(e) => e,
        Ok(_) => panic!("foreign writer"),
    };
    assert_eq!(e.reason, Reason::Stale);
    assert_eq!(e.input.writer.id(), writer.id());
    assert!(e.input.expected.same(&review));
    assert_eq!(f.queue.worker.retained_keys().len(), count);
}
#[test]
fn old_client_unsupported_latch_still_blocks_new_local_bridge() {
    let f = Fixture::new();
    let (reply, receive) = mpsc::channel();
    f.queue.submit_protected(ProtectedCommand {
        account: f.queue.account().into(),
        action: p::commands::Action::Review {
            scope: f.scope.clone(),
            reply,
        },
    });
    let review = receive.recv().unwrap().unwrap();
    let e = match f.queue.worker.hold_register(review.clone()) {
        Err(e) => e,
        Ok(_) => panic!("unsupported"),
    };
    assert_eq!(e.reason, Reason::Unsupported);
    assert!(e.input.same(&review));
}
#[test]
fn actual_retained_inventory_recovers_lost_consumption_and_issued_children() {
    let f = Fixture::new();
    let s = send(&f);
    let frozen = s.value().unwrap();
    drop(s);
    let key = f
        .queue
        .worker
        .retained_keys()
        .into_iter()
        .find(|r| r.family == "protected-consume-send")
        .unwrap()
        .key;
    let s = f
        .queue
        .worker
        .reattach_step::<SendInput, p::Review>(&key)
        .unwrap();
    assert!(s.value().unwrap().same(&frozen));
    let p = until(|| f.queue.worker.prepare_send(&s));
    f.queue.worker.submit_step(&p).unwrap();
    wait(&p);
    drop(p);
    let p = until(|| f.queue.worker.prepare_send(&s));
    let c = until(|| f.queue.worker.claim_send(&p));
    f.queue.worker.submit_step(&c).unwrap();
    wait(&c);
    drop(c);
    let c = until(|| f.queue.worker.claim_send(&p));
    let end = until(|| f.queue.worker.begin_send_record(&c));
    f.queue.worker.cancel_before_dispatch(&end).unwrap();
    f.queue.worker.submit_completion(&end).unwrap();
    assert!(matches!(&*finished(&end), Finished::Mutation(Ok(_))));
}
#[test]
fn old_enrollment_writer_cannot_release_new_managed_row_at_same_scope() {
    let f = Fixture::new();
    let old = register(&f);
    let r = released(&f, old.clone(), f.current());
    let c = until(|| f.queue.worker.begin_start_local(&r));
    f.queue.worker.submit_completion(&c).unwrap();
    assert!(matches!(&*finished(&c), Finished::Local(Ok(_))));
    let local = call(
        &f.queue,
        f.queue
            .worker
            .hold_local_inventory(f.scope.clone())
            .unwrap(),
    );
    let successor = call(&f.queue, f.queue.worker.hold_enroll(local).unwrap());
    assert_eq!(old.scope(), successor.scope());
    assert_ne!(old.enrollment(), successor.row().lineage().enrollment());
    let before = call(
        &f.queue,
        f.queue
            .worker
            .hold_local_inventory(f.scope.clone())
            .unwrap(),
    );
    let count = f.queue.diagnostics().unwrap().records.len();
    let slots = f.queue.worker.retained_keys().len();
    let e = match f.queue.worker.hold_release(ReleaseInput {
        expected: successor.clone(),
        writer: old.clone(),
    }) {
        Err(e) => e,
        Ok(_) => panic!("old enrollment"),
    };
    assert_eq!(e.reason, Reason::Stale);
    assert!(e.input.expected.same(&successor));
    assert_eq!(e.input.writer.id(), old.id());
    assert_eq!(f.queue.diagnostics().unwrap().records.len(), count);
    assert_eq!(f.queue.worker.retained_keys().len(), slots);
    let after = call(
        &f.queue,
        f.queue
            .worker
            .hold_local_inventory(f.scope.clone())
            .unwrap(),
    );
    assert!(before.same(&after));
}

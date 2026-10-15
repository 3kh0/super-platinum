//! Actual Owned worker/SQLite; fake raw evidence only, no HTTP or real stores.
use super::super::admission::*;
use super::*;
use crate::drafts::store::protected as p;
#[path = "completion_test_support.rs"]
mod support;
use support::*;

#[test]
fn issued_linear_ticket_survives_last_waiter_loss_and_transfers_once() {
    let f = Fixture::new();
    let prepared = f.prepared();
    let key = prepared.key();
    let input = prepared.input();
    drop(prepared);
    let p = f
        .queue
        .worker
        .reattach_step::<MutationInput, p::DispatchTicket>(&key)
        .unwrap();
    assert!(Arc::ptr_eq(&input, &p.input()));
    let c = until(|| f.queue.worker.claim_mutation(&p));
    assert_eq!(p.state(), StepState::Transferred);
    let again = f.queue.worker.claim_mutation(&p).unwrap();
    f.queue.worker.submit_step(&c).unwrap();
    f.queue.worker.submit_step(&again).unwrap();
    wait(&again);
    let before = f.queue.diagnostics().unwrap().records.len();
    f.queue.worker.submit_step(&c).unwrap();
    assert_eq!(before, f.queue.diagnostics().unwrap().records.len());
    let completion = until(|| f.queue.worker.begin_record(&again));
    assert!(matches!(
        f.queue.worker.take_dispatch(&completion),
        Ok(Dispatch::Mutation(_))
    ));
    assert!(matches!(
        f.queue.worker.take_dispatch(&completion),
        Err(Reason::Stale)
    ));
}
#[test]
fn fence_after_claim_keeps_reserved_completion_and_only_allows_local_cancellation() {
    let f = Fixture::new();
    let claimed = f.claimed();
    let slots = f.queue.worker.retained_keys().len();
    let fence = f.queue.worker.begin_fence().unwrap();
    let c = until(|| f.queue.worker.begin_record(&claimed));
    assert_eq!(slots, f.queue.worker.retained_keys().len());
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Fencing)
    ));
    f.queue.worker.cancel_before_dispatch(&c).unwrap();
    assert!(matches!(
        &*c.raw().unwrap(),
        Raw::Mutation {
            outcome: p::Outcome::Uncertain,
            known_until: 0
        }
    ));
    f.queue.worker.resume_admission(&fence).unwrap();
    f.queue.worker.submit_completion(&c).unwrap();
    assert!(matches!(&*finished(&c), Finished::Mutation(Ok(_))));
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Stale)
    ));
}
#[test]
fn prebound_completion_accepts_late_raw_across_fence_and_resume_without_new_dispatch() {
    let f = Fixture::new();
    let (c, op) = f.mutation();
    let fence = f.queue.worker.begin_fence().unwrap();
    f.queue.worker.resume_admission(&fence).unwrap();
    let result = finish(
        &f.queue,
        &c,
        Raw::Mutation {
            outcome: p::Outcome::Uncertain,
            known_until: 0,
        },
    );
    let Finished::Mutation(Ok(review)) = &*result else {
        panic!("record")
    };
    assert_eq!(review.row().state().pending(), Some(&op));
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Stale)
    ));
    let before = f.queue.diagnostics().unwrap().records.len();
    f.queue.worker.submit_completion(&c).unwrap();
    assert_eq!(before, f.queue.diagnostics().unwrap().records.len());
}
#[test]
fn cannot_handoff_network_after_fence_even_when_slot_was_registered() {
    let f = Fixture::new();
    let claimed = f.claimed();
    let c = until(|| f.queue.worker.begin_record(&claimed));
    let fence = f.queue.worker.begin_fence().unwrap();
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Fencing)
    ));
    let refused = f
        .queue
        .worker
        .bind_raw(
            &c,
            Raw::Mutation {
                outcome: p::Outcome::Uncertain,
                known_until: 0,
            },
        )
        .unwrap_err();
    assert_eq!(refused.reason, Reason::Stale);
    assert!(c.raw().is_none());
    f.queue.worker.resume_admission(&fence).unwrap();
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Stale)
    ));
}
#[test]
fn raw_outcome_is_retained_before_unsupported_storage_refusal() {
    let f = Fixture::new();
    let (c, _) = f.mutation();
    // Actual unsupported old Client path, not mutation of admission flags.
    let (reply, receive) = mpsc::channel();
    f.queue.submit_protected(ProtectedCommand {
        account: f.queue.account().into(),
        action: p::commands::Action::Review {
            scope: f.scope.clone(),
            reply,
        },
    });
    receive.recv().unwrap().unwrap();
    f.queue
        .worker
        .bind_raw(
            &c,
            Raw::Mutation {
                outcome: p::Outcome::Uncertain,
                known_until: 19,
            },
        )
        .unwrap();
    let raw = c.raw().unwrap();
    assert_eq!(
        f.queue.worker.submit_completion(&c),
        Err(Reason::Unsupported)
    );
    assert!(Arc::ptr_eq(&raw, &c.raw().unwrap()));
    assert!(c.result().unwrap().is_none());
}
#[test]
fn foreign_slot_and_second_raw_cannot_replace_first_evidence() {
    let f = Fixture::new();
    let other = Fixture::new();
    let (c, _) = f.mutation();
    let wrong = Raw::Baseline {
        response: p::BaselineResponse::Nonpositive(p::KnownReadFailure::Missing),
        known_until: 7,
    };
    assert!(matches!(
        other.queue.worker.bind_raw(&c, wrong),
        Err(Refused {
            reason: Reason::Stale,
            ..
        })
    ));
    f.queue
        .worker
        .bind_raw(
            &c,
            Raw::Mutation {
                outcome: p::Outcome::Uncertain,
                known_until: 9,
            },
        )
        .unwrap();
    let first = c.raw().unwrap();
    let refused = f
        .queue
        .worker
        .bind_raw(
            &c,
            Raw::Mutation {
                outcome: p::Outcome::Rejected,
                known_until: 999,
            },
        )
        .unwrap_err();
    assert!(matches!(
        refused.input,
        Raw::Mutation {
            outcome: p::Outcome::Rejected,
            known_until: 999
        }
    ));
    assert!(Arc::ptr_eq(&first, &c.raw().unwrap()));
}
#[test]
fn dropped_all_completion_handles_recover_exact_raw_and_terminal_result() {
    let f = Fixture::new();
    let (c, _) = f.mutation();
    f.queue
        .worker
        .bind_raw(
            &c,
            Raw::Mutation {
                outcome: p::Outcome::Uncertain,
                known_until: 0,
            },
        )
        .unwrap();
    let raw = c.raw().unwrap();
    let key = c.key();
    drop(c);
    let c = f.queue.worker.reattach_completion(&key).unwrap();
    assert!(Arc::ptr_eq(&raw, &c.raw().unwrap()));
    f.queue.worker.submit_completion(&c).unwrap();
    drop(c);
    let c = f.queue.worker.reattach_completion(&key).unwrap();
    let result = finished(&c);
    assert!(matches!(&*result, Finished::Mutation(Ok(_))));
    assert!(Arc::ptr_eq(&result, &finished(&c)));
}
#[test]
fn mutation_sql_rollback_retries_only_same_receipt_and_raw() {
    let f = Fixture::new();
    let (c, operation) = f.mutation();
    let before = f.current();
    f.sql("CREATE TRIGGER fail_completion BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'fixture'); END;");
    let result = finish(
        &f.queue,
        &c,
        Raw::Mutation {
            outcome: p::Outcome::Uncertain,
            known_until: 0,
        },
    );
    assert!(matches!(
        &*result,
        Finished::Mutation(Err(p::Error::Storage))
    ));
    let raw = c.raw().unwrap();
    assert!(f.current().same(&before));
    f.sql("DROP TRIGGER fail_completion;");
    let fence = f.queue.worker.begin_fence().unwrap();
    f.queue.worker.resume_admission(&fence).unwrap();
    until(|| f.queue.worker.retry_completion(&c));
    let result = finished(&c);
    let Finished::Mutation(Ok(after)) = &*result else {
        panic!("retry")
    };
    assert_eq!(after.row().state().pending(), Some(&operation));
    assert!(Arc::ptr_eq(&raw, &c.raw().unwrap()));
    assert_eq!(f.queue.worker.retry_completion(&c), Err(Reason::Stale));
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Stale)
    ));
}
#[test]
fn status_read_actual_ticket_completes_after_fence_without_mutation_replay() {
    let f = Fixture::for_status_check();
    let c = f.check();
    let before = f.current();
    let fence = f.queue.worker.begin_fence().unwrap();
    let result = finish(
        &f.queue,
        &c,
        Raw::Check {
            result: p::CheckResult::Nonpositive(p::KnownReadFailure::RateLimited),
            known_until: 50000,
        },
    );
    let Finished::Check(Ok(receipt)) = &*result else {
        panic!("check completion")
    };
    assert!(!receipt.positive_accepted());
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Fencing)
    ));
    f.queue.worker.resume_admission(&fence).unwrap();
    let after = f.current();
    assert_eq!(
        after.row().state().pending(),
        before.row().state().pending()
    );
    assert_eq!(
        serde_json::to_value(after.local()).unwrap(),
        serde_json::to_value(before.local()).unwrap()
    );
}
#[test]
fn baseline_raw_evidence_retained_after_resume_without_applying_editor() {
    let f = Fixture::new();
    let c = f.baseline();
    let before = f.current();
    let fence = f.queue.worker.begin_fence().unwrap();
    f.queue.worker.resume_admission(&fence).unwrap();
    let result = finish(
        &f.queue,
        &c,
        Raw::Baseline {
            response: p::BaselineResponse::Nonpositive(p::KnownReadFailure::Missing),
            known_until: 0,
        },
    );
    assert!(matches!(
        &*result,
        Finished::Baseline(Ok(p::BaselineResult::Nonpositive))
    ));
    assert!(f.current().same(&before));
}
#[test]
fn actual_observation_ticket_finishes_after_fence_without_new_observation() {
    let f = Fixture::new();
    let (c, event) = f.observation();
    let _fence = f.queue.worker.begin_fence().unwrap();
    let result = finish(&f.queue, &c, Raw::Observation);
    let Finished::Observation(Ok(receipt)) = &*result else {
        panic!("observation")
    };
    assert_eq!(
        receipt
            .after()
            .row()
            .state()
            .observed_event_ts(&event.draft_id),
        Some(event.event_ts.as_str())
    );
    let before = f.queue.diagnostics().unwrap().records.len();
    f.queue.worker.submit_completion(&c).unwrap();
    assert_eq!(before, f.queue.diagnostics().unwrap().records.len());
}
#[test]
fn observation_storage_failure_keeps_exact_token_for_retry() {
    let f = Fixture::new();
    let (c, event) = f.observation();
    f.sql("CREATE TRIGGER fail_completion BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'fixture'); END;");
    let result = finish(&f.queue, &c, Raw::Observation);
    assert!(matches!(
        &*result,
        Finished::Observation(Err(p::Error::Storage))
    ));
    f.sql("DROP TRIGGER fail_completion;");
    until(|| f.queue.worker.retry_completion(&c));
    let result = finished(&c);
    let Finished::Observation(Ok(after)) = &*result else {
        panic!("retry exact admission")
    };
    assert_eq!(
        after
            .after()
            .row()
            .state()
            .observed_event_ts(&event.draft_id),
        Some(event.event_ts.as_str())
    );
}

#[test]
fn family_capacity_refusal_precedes_prepare_and_returns_full_input() {
    let f = Fixture::new();
    while f.queue.worker.retained_keys().len() < 62 {
        f.queue.worker.hold_managed_review(f.scope.clone()).unwrap();
    }
    let count = f.queue.diagnostics().unwrap().records.len();
    let input = MutationInput {
        stream: f.stream.clone(),
        expected: f.review.clone(),
    };
    let ptr = input.expected.local().composer.text.as_ptr();
    let refused = match f.queue.worker.hold_prepare(input) {
        Err(e) => e,
        Ok(_) => panic!("three slots unavailable"),
    };
    assert_eq!(refused.reason, Reason::Capacity);
    assert_eq!(refused.input.expected.local().composer.text.as_ptr(), ptr);
    assert_eq!(f.queue.worker.retained_keys().len(), 62);
    assert_eq!(count, f.queue.diagnostics().unwrap().records.len());
    assert!(refused.input.expected.row().state().pending().is_none());
}
#[test]
fn full_holder_capacity_after_reservation_cannot_strand_issued_completion() {
    let f = Fixture::new();
    while f.queue.worker.retained_keys().len() < 61 {
        f.queue.worker.hold_managed_review(f.scope.clone()).unwrap();
    }
    let (c, _) = f.mutation();
    assert_eq!(f.queue.worker.retained_keys().len(), 64);
    let result = finish(
        &f.queue,
        &c,
        Raw::Mutation {
            outcome: p::Outcome::Uncertain,
            known_until: 0,
        },
    );
    assert!(matches!(&*result, Finished::Mutation(Ok(_))));
    assert_eq!(f.queue.worker.retained_keys().len(), 64);
}
#[test]
fn handed_dispatch_cannot_be_reclassified_as_not_requested() {
    let f = Fixture::new();
    let (c, _) = f.mutation();
    assert_eq!(
        f.queue.worker.cancel_before_dispatch(&c),
        Err(Reason::Stale)
    );
    assert!(c.raw().is_none());
}
#[test]
fn check_positive_binds_original_operation_and_settles_without_second_read() {
    let f = Fixture::for_status_check();
    let c = f.check();
    let before = f.current();
    let operation = before.row().state().pending().unwrap();
    let remote = crate::drafts::sync::RemoteDraft {
        scope: f.scope.clone(),
        id: "D1".into(),
        client_msg_id: operation.client_msg_id.clone(),
        last_updated_ts: "11.000001".into(),
        snapshot: operation.snapshot.clone().unwrap(),
    };
    let result = finish(
        &f.queue,
        &c,
        Raw::Check {
            result: p::CheckResult::Positive(p::Outcome::Active(remote.clone())),
            known_until: 0,
        },
    );
    let Finished::Check(Ok(receipt)) = &*result else {
        panic!("positive read")
    };
    assert!(receipt.positive_accepted());
    assert_eq!(
        receipt.review().unwrap().row().state().acknowledged(),
        Some(&remote)
    );
    let count = f.queue.diagnostics().unwrap().records.len();
    f.queue.worker.submit_completion(&c).unwrap();
    assert_eq!(count, f.queue.diagnostics().unwrap().records.len());
}
#[test]
fn check_storage_rollback_retries_exact_raw_after_resume() {
    let f = Fixture::for_status_check();
    let c = f.check();
    f.sql("CREATE TRIGGER fail_completion BEFORE UPDATE ON composer_draft_status_reads BEGIN SELECT RAISE(ABORT,'fixture'); END;");
    let result = finish(
        &f.queue,
        &c,
        Raw::Check {
            result: p::CheckResult::Nonpositive(p::KnownReadFailure::Missing),
            known_until: 0,
        },
    );
    assert!(matches!(&*result, Finished::Check(Err(p::Error::Storage))));
    let raw = c.raw().unwrap();
    f.sql("DROP TRIGGER fail_completion;");
    let fence = f.queue.worker.begin_fence().unwrap();
    f.queue.worker.resume_admission(&fence).unwrap();
    until(|| f.queue.worker.retry_completion(&c));
    assert!(matches!(&*finished(&c), Finished::Check(Ok(_))));
    assert!(Arc::ptr_eq(&raw, &c.raw().unwrap()));
}
#[test]
fn baseline_positive_retains_complete_raw_and_storage_failure_does_not_repeat_read() {
    let f = Fixture::new();
    let c = f.baseline();
    let before = f.current();
    let remote = before.row().state().acknowledged().unwrap();
    let raw = serde_json::json!({"ok":true,"unknown_root":{"keep":[1,2]},"draft":{"id":remote.id,"client_msg_id":remote.client_msg_id,"last_updated_ts":"11.000001","team_id":"T1","user_id":"U1","is_deleted":false,"is_sent":false,"date_scheduled":0,"blocks":remote.snapshot.blocks,"file_ids":remote.snapshot.file_ids,"destinations":remote.snapshot.destinations}});
    f.sql("CREATE TRIGGER fail_completion BEFORE INSERT ON composer_draft_baseline_evidence BEGIN SELECT RAISE(ABORT,'fixture'); END;");
    let result = finish(
        &f.queue,
        &c,
        Raw::Baseline {
            response: p::BaselineResponse::Positive {
                raw: raw.clone(),
                user: "U1".into(),
            },
            known_until: 0,
        },
    );
    assert!(matches!(
        &*result,
        Finished::Baseline(Err(p::Error::Storage))
    ));
    let held = c.raw().unwrap();
    f.sql("DROP TRIGGER fail_completion;");
    until(|| f.queue.worker.retry_completion(&c));
    assert!(matches!(
        &*finished(&c),
        Finished::Baseline(Ok(p::BaselineResult::Retained(_)))
    ));
    assert!(Arc::ptr_eq(&held, &c.raw().unwrap()));
    let Raw::Baseline {
        response: p::BaselineResponse::Positive { raw: actual, .. },
        ..
    } = &*held
    else {
        panic!("retained raw")
    };
    assert_eq!(actual, &raw);
    assert!(f.current().same(&before));
}
#[test]
fn deferred_baseline_keeps_actual_ticket_and_deadline_without_dispatch_or_fake_completion() {
    let f = Fixture::new();
    let (review, _) = f.remote();
    let step = f
        .queue
        .worker
        .hold_baseline(BaselineInput {
            stream: f.stream.clone(),
            expected: review,
            now_ms: 1000,
        })
        .unwrap();
    f.queue.worker.submit_step(&step).unwrap();
    wait(&step);
    f.sql("INSERT INTO composer_draft_known_read_limit(team,until_ms) VALUES('T1',50000) ON CONFLICT(team) DO UPDATE SET until_ms=50000;");
    let claim = until(|| f.queue.worker.claim_baseline(&step));
    f.queue.worker.submit_step(&claim).unwrap();
    wait(&claim);
    let c = until(|| f.queue.worker.begin_baseline_completion(&claim));
    assert_eq!(c.deferred_until(), Some(50000));
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Incomplete)
    ));
    assert_eq!(
        f.queue.worker.cancel_before_dispatch(&c),
        Err(Reason::Stale)
    );
    assert!(c.raw().is_none());
    assert!(c.result().unwrap().is_none());
    assert_eq!(f.queue.worker.retry_completion(&c), Err(Reason::Stale));
}

#[test]
fn preissued_mutation_claim_crosses_resume_only_to_complete_local_cancellation() {
    let f = Fixture::new();
    let prepared = f.prepared();
    let count = f.queue.worker.retained_keys().len();
    let fence = f.queue.worker.begin_fence().unwrap();
    f.queue.worker.resume_admission(&fence).unwrap();
    let claim = until(|| f.queue.worker.claim_mutation(&prepared));
    f.queue.worker.submit_step(&claim).unwrap();
    wait(&claim);
    let c = until(|| f.queue.worker.begin_record(&claim));
    assert_eq!(count, f.queue.worker.retained_keys().len());
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Stale)
    ));
    f.queue.worker.cancel_before_dispatch(&c).unwrap();
    f.queue.worker.submit_completion(&c).unwrap();
    assert!(matches!(&*finished(&c), Finished::Mutation(Ok(_))));
    assert!(matches!(
        &*c.raw().unwrap(),
        Raw::Mutation {
            outcome: p::Outcome::Uncertain,
            known_until: 0
        }
    ));
}
#[test]
fn preissued_baseline_claim_crosses_resume_without_a_read_or_new_time() {
    let f = Fixture::new();
    let (review, _) = f.remote();
    let reserved = f
        .queue
        .worker
        .hold_baseline(BaselineInput {
            stream: f.stream.clone(),
            expected: review,
            now_ms: 1000,
        })
        .unwrap();
    f.queue.worker.submit_step(&reserved).unwrap();
    wait(&reserved);
    let fence = f.queue.worker.begin_fence().unwrap();
    f.queue.worker.resume_admission(&fence).unwrap();
    let claim = until(|| f.queue.worker.claim_baseline(&reserved));
    assert_eq!(claim.input().now_ms, 1000);
    f.queue.worker.submit_step(&claim).unwrap();
    wait(&claim);
    let c = until(|| f.queue.worker.begin_baseline_completion(&claim));
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Stale)
    ));
    f.queue.worker.cancel_before_dispatch(&c).unwrap();
    f.queue.worker.submit_completion(&c).unwrap();
    assert!(matches!(
        &*finished(&c),
        Finished::Baseline(Ok(p::BaselineResult::Nonpositive))
    ));
    assert!(matches!(
        &*c.raw().unwrap(),
        Raw::Baseline {
            response: p::BaselineResponse::Nonpositive(p::KnownReadFailure::NotRequested),
            known_until: 0
        }
    ));
}
#[test]
fn preissued_status_claim_crosses_resume_without_a_read_or_new_ticket() {
    let f = Fixture::for_status_check();
    let reserved = f.check_reserved();
    let fence = f.queue.worker.begin_fence().unwrap();
    f.queue.worker.resume_admission(&fence).unwrap();
    let claim = until(|| f.queue.worker.claim_check(&reserved));
    assert_eq!(claim.input().now_ms, 1000);
    f.queue.worker.submit_step(&claim).unwrap();
    wait(&claim);
    let c = until(|| f.queue.worker.begin_check_completion(&claim));
    assert!(matches!(
        f.queue.worker.take_dispatch(&c),
        Err(Reason::Stale)
    ));
    f.queue.worker.cancel_before_dispatch(&c).unwrap();
    f.queue.worker.submit_completion(&c).unwrap();
    let result = finished(&c);
    let Finished::Check(Ok(receipt)) = &*result else {
        panic!("cancel read")
    };
    assert!(!receipt.positive_accepted());
    assert!(matches!(
        &*c.raw().unwrap(),
        Raw::Check {
            result: p::CheckResult::Nonpositive(p::KnownReadFailure::NotRequested),
            known_until: 0
        }
    ));
}

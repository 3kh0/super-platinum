//! Actual owned worker, cold load, upload journal and returned original input.
use super::super::admission::{self, Delivery, Operation, Reason};
use super::*;
use crate::drafts::store::uploads::{Draft, Reservation};
use std::time::{Duration, Instant};
fn inspect(queue: &Queue) -> Arc<upgrade::Review> {
    let value = queue
        .submit_upgrade(upgrade::Action::Inspect)
        .unwrap()
        .wait()
        .unwrap();
    let upgrade::Reply::Inspected(review) = &*value else {
        panic!("inspect")
    };
    review.clone()
}
fn migrated() -> Arc<Queue> {
    let queue = Queue::fresh().unwrap();
    queue
        .submit_upgrade(upgrade::Action::Commit(inspect(&queue)))
        .unwrap()
        .wait()
        .unwrap();
    queue.open(inspect(&queue)).unwrap();
    queue
}
fn fixture() -> Arc<Queue> {
    let queue = migrated();
    queue.cold(&Arc::new(())).unwrap().wait().unwrap();
    queue
}
fn draft(queue: &Queue) -> Draft {
    let mut draft = super::super::super::uploads::tests::draft();
    draft.account_id = queue.account().into();
    draft
}
fn scope(queue: &Queue) -> Scope {
    Scope {
        account_id: queue.account().into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    }
}
fn result<I, T>(operation: &Operation<I, T>) -> Arc<T> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match operation.poll() {
            Ok(result) => return result,
            Err(Delivery::Pending) => {
                assert!(Instant::now() < until, "worker result deadline");
                std::thread::yield_now();
            }
            Err(Delivery::Disconnected) => panic!("unexpected disconnect"),
        }
    }
}
fn completion(
    queue: &Queue,
    parent: &admission::Upload,
    files: Vec<String>,
) -> admission::UploadCompletion {
    let until = Instant::now() + Duration::from_secs(5);
    let mut files = files;
    loop {
        match queue.worker.hold_upload_completion(parent, files) {
            Ok(step) => return step,
            Err(refusal) if refusal.reason == Reason::Incomplete => {
                files = refusal.input;
                assert!(Instant::now() < until);
                std::thread::yield_now();
            }
            Err(e) => panic!("completion refused: {e:?}"),
        }
    }
}
fn identity(queue: &Queue) -> (Arc<admission::Identity>, u64) {
    let pending = queue.worker.pending.lock().unwrap();
    let a = pending.admission.as_ref().unwrap();
    (a.identity.clone(), a.generation)
}

#[test]
fn retained_admission_waits_for_actual_cold_result() {
    let queue = migrated();
    let input = draft(&queue);
    let ptr = input.caption.as_ptr();
    let refused = match queue.worker.hold_upload(input) {
        Err(value) => value,
        Ok(_) => panic!("must require cold"),
    };
    assert_eq!(refused.reason, Reason::ColdRequired);
    assert_eq!(refused.input.caption.as_ptr(), ptr);
    queue.cold(&Arc::new(())).unwrap().wait().unwrap();
    assert!(queue.worker.hold_upload(refused.input).is_ok());
}
#[test]
fn refused_full_map_preserves_selection_payload_and_original_allocations() {
    let queue = fixture();
    let (identity, generation) = identity(&queue);
    let fence = queue.worker.begin_fence().unwrap();
    let key = scope(&queue).key;
    let mut input = crate::drafts::store::sync::tests::local("retained Unicode — 日本語 🚀");
    input.composer.set_selection(0, 8);
    let pointer = input.composer.text.as_ptr();
    let serialized = serde_json::to_value(&input).unwrap();
    let map = [(key.clone(), input)].into();
    let error = queue
        .worker
        .try_enqueue(&identity, generation, queue.account().into(), map)
        .unwrap_err();
    assert_eq!(error.reason, Reason::Fencing);
    assert_eq!(error.input.1[&key].composer.text.as_ptr(), pointer);
    assert_eq!(
        serde_json::to_value(&error.input.1[&key]).unwrap(),
        serialized
    );
    assert!(queue.worker.pending.lock().unwrap().updates.is_empty());
    assert!(queue.worker.inspect_fence(&fence).is_ok());
}
#[test]
fn refused_retained_request_keeps_sender_and_input_and_rejects_stale_generation() {
    let queue = fixture();
    let held = queue.worker.hold_upload(draft(&queue)).unwrap();
    let retained = held.input();
    let fence = queue.worker.begin_fence().unwrap();
    assert_eq!(queue.worker.submit_held(&held), Err(Reason::Fencing));
    assert!(matches!(held.poll(), Err(Delivery::Pending)));
    assert!(Arc::ptr_eq(&held.input(), &retained));
    queue.worker.resume_admission(&fence).unwrap();
    assert_eq!(queue.worker.inspect_fence(&fence), Err(Reason::Stale));
    assert_eq!(queue.worker.submit_held(&held), Err(Reason::Stale));
    assert!(matches!(held.poll(), Err(Delivery::Pending)));
}
#[test]
fn actual_commit_retained_after_all_external_waiters_drop_and_no_duplicate_enqueue() {
    let queue = fixture();
    let held = queue.worker.hold_upload(draft(&queue)).unwrap();
    let key = held.key();
    queue.worker.submit_held(&held).unwrap();
    drop(held);
    let again = queue
        .worker
        .reattach::<Draft, Result<Reservation, AppError>>(&key)
        .unwrap();
    let actual = result(&again);
    assert!(actual.as_ref().as_ref().unwrap().created);
    let count = queue.diagnostics().unwrap().records.len();
    queue.worker.submit_held(&again).unwrap();
    assert_eq!(queue.diagnostics().unwrap().records.len(), count);
    assert!(Arc::ptr_eq(&actual, &result(&again)));
    assert_eq!(
        DraftStore::open(queue.owner.path())
            .unwrap()
            .load_uploads(queue.account())
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn foreign_route_and_wrong_typed_attachment_cannot_use_retained_slot() {
    let queue = fixture();
    let other = fixture();
    let held = queue.worker.hold_upload(draft(&queue)).unwrap();
    assert_eq!(other.worker.submit_held(&held), Err(Reason::Stale));
    assert!(matches!(
        other
            .worker
            .reattach::<Draft, Result<Reservation, AppError>>(&held.key()),
        Err(Reason::Stale)
    ));
    assert!(matches!(
        queue
            .worker
            .reattach::<Scope, Result<LocalReview, AppError>>(&held.key()),
        Err(Reason::Stale)
    ));
    assert!(matches!(held.poll(), Err(Delivery::Pending)));
}
#[test]
fn only_exact_prebound_completion_finishes_during_fence_once() {
    let queue = fixture();
    let parent = queue.worker.hold_upload(draft(&queue)).unwrap();
    queue.worker.submit_held(&parent).unwrap();
    result(&parent).as_ref().as_ref().unwrap();
    let step = completion(&queue, &parent, vec!["F1".into()]);
    let fence = queue.worker.begin_fence().unwrap();
    assert!(fence.slots.len() >= 2);
    assert!(!fence.records.is_empty());
    queue.worker.submit_held(&step).unwrap();
    let stored = result(&step);
    assert_eq!(
        stored.as_ref().as_ref().unwrap().as_ref().unwrap().file_ids,
        Some(vec!["F1".into()])
    );
    let count = queue.diagnostics().unwrap().records.len();
    queue.worker.submit_held(&step).unwrap();
    assert_eq!(queue.diagnostics().unwrap().records.len(), count);
    assert!(queue.worker.inspect_fence(&fence).is_ok()); // valid handle, NOT a drain result
    assert!(queue.worker.hold_upload(draft(&queue)).is_err());
}
#[test]
fn late_raw_completion_cannot_mint_new_continuation_after_fence() {
    let queue = fixture();
    let parent = queue.worker.hold_upload(draft(&queue)).unwrap();
    queue.worker.submit_held(&parent).unwrap();
    result(&parent);
    let fence = queue.worker.begin_fence().unwrap();
    let files = vec!["F1".to_string()];
    let pointer = files[0].as_ptr();
    let refused = match queue.worker.hold_upload_completion(&parent, files) {
        Err(value) => value,
        Ok(_) => panic!("late edge"),
    };
    assert_eq!(refused.reason, Reason::Fencing);
    assert_eq!(refused.input[0].as_ptr(), pointer);
    assert!(queue.worker.inspect_fence(&fence).is_ok());
}
#[test]
fn resume_preserves_already_bound_storage_completion_without_new_root_authority() {
    let queue = fixture();
    let parent = queue.worker.hold_upload(draft(&queue)).unwrap();
    queue.worker.submit_held(&parent).unwrap();
    result(&parent);
    let step = completion(&queue, &parent, vec!["F1".into()]);
    let fence = queue.worker.begin_fence().unwrap();
    queue.worker.resume_admission(&fence).unwrap();
    assert_eq!(queue.worker.inspect_fence(&fence), Err(Reason::Stale));
    queue.worker.submit_held(&step).unwrap();
    assert!(result(&step).as_ref().as_ref().unwrap().is_some());
}
#[test]
fn existing_reservation_does_not_authorize_finalization_or_replay() {
    let queue = fixture();
    let input = draft(&queue);
    let first = queue.worker.hold_upload(input.clone()).unwrap();
    queue.worker.submit_held(&first).unwrap();
    result(&first);
    let duplicate = queue.worker.hold_upload(input).unwrap();
    queue.worker.submit_held(&duplicate).unwrap();
    assert!(!result(&duplicate).as_ref().as_ref().unwrap().created);
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match queue
            .worker
            .hold_upload_completion(&duplicate, vec!["F1".into()])
        {
            Err(refused) if refused.reason == Reason::Incomplete => {
                assert!(Instant::now() < until);
                std::thread::yield_now();
            }
            Err(refused) => {
                assert_eq!(refused.reason, Reason::Stale);
                break;
            }
            Ok(_) => panic!("existing reservation minted completion"),
        }
    }
}
#[test]
fn unadapted_old_client_even_rejected_restart_invalidates_fence() {
    let queue = fixture();
    let fence = queue.worker.begin_fence().unwrap();
    let (reply, receiver) = mpsc::channel();
    queue.submit_protected(ProtectedCommand {
        account: queue.account().into(),
        action: super::super::super::protected::commands::Action::Load {
            mode: super::super::super::protected::LoadMode::AfterRestart,
            reply,
        },
    });
    assert!(matches!(receiver.recv().unwrap(), Err(Error::Stale)));
    assert_eq!(queue.worker.inspect_fence(&fence), Err(Reason::Unsupported));
}
#[test]
fn unadapted_enqueue_keeps_real_input_and_latches_unsupported() {
    let queue = fixture();
    let fence = queue.worker.begin_fence().unwrap();
    let key = scope(&queue).key;
    let input = crate::drafts::store::sync::tests::local("actual input");
    queue
        .worker
        .enqueue(queue.account().into(), [(key.clone(), input)].into());
    assert_eq!(queue.worker.inspect_fence(&fence), Err(Reason::Unsupported));
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let rows = DraftStore::open(queue.owner.path())
            .unwrap()
            .load()
            .unwrap();
        if rows
            .iter()
            .any(|(k, v)| k == &key && v.composer.text == "actual input")
        {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
}
#[test]
fn slot_capacity_refuses_exact_input_without_sender_or_payload_loss() {
    let queue = fixture();
    for _ in 0..64 {
        queue.worker.hold_upload(draft(&queue)).unwrap();
    }
    let input = draft(&queue);
    let pointer = input.caption.as_ptr();
    let refused = match queue.worker.hold_upload(input) {
        Err(value) => value,
        Ok(_) => panic!("capacity"),
    };
    assert_eq!(refused.reason, Reason::Capacity);
    assert_eq!(refused.input.caption.as_ptr(), pointer);
}
#[test]
fn explicit_ack_is_identity_bound_and_does_not_reclaim_history() {
    let queue = fixture();
    let other = fixture();
    let held = queue.worker.hold_upload(draft(&queue)).unwrap();
    assert_eq!(queue.worker.acknowledge(&held), Err(Reason::Incomplete));
    queue.worker.submit_held(&held).unwrap();
    result(&held);
    assert_eq!(other.worker.acknowledge(&held), Err(Reason::Stale));
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match queue.worker.acknowledge(&held) {
            Ok(()) => break,
            Err(Reason::Incomplete) => {
                assert!(Instant::now() < until);
                std::thread::yield_now();
            }
            Err(other) => panic!("unexpected acknowledgement failure: {other:?}"),
        }
    }
    assert!(
        queue
            .worker
            .reattach::<Draft, Result<Reservation, AppError>>(&held.key())
            .is_ok()
    );
    assert!(!queue.diagnostics().unwrap().records.is_empty());
}
#[test]
fn worker_route_drop_refuses_new_admission_and_retains_unknown_input() {
    let queue = fixture();
    let worker = queue.worker.clone();
    let owner = queue.owner.clone();
    let held = worker.hold_upload(draft(&queue)).unwrap();
    let input = held.input();
    drop(queue);
    assert!(owner.check().is_ok());
    assert_eq!(worker.submit_held(&held), Err(Reason::Dead));
    assert!(Arc::ptr_eq(&input, &held.input()));
    assert!(matches!(held.poll(), Err(Delivery::Pending)));
}
#[test]
fn resumed_generation_cannot_bind_new_step_from_old_parent() {
    let queue = fixture();
    let parent = queue.worker.hold_upload(draft(&queue)).unwrap();
    queue.worker.submit_held(&parent).unwrap();
    result(&parent);
    let fence = queue.worker.begin_fence().unwrap();
    queue.worker.resume_admission(&fence).unwrap();
    let files = vec!["F1".to_string()];
    let pointer = files[0].as_ptr();
    let refused = match queue.worker.hold_upload_completion(&parent, files) {
        Err(value) => value,
        Ok(_) => panic!("old parent minted edge"),
    };
    assert_eq!(refused.reason, Reason::Stale);
    assert_eq!(refused.input[0].as_ptr(), pointer);
}
#[test]
fn loss_of_last_handle_and_key_recovers_exact_unsent_payload_from_actual_route() {
    let queue = fixture();
    let operation = queue.worker.hold_upload(draft(&queue)).unwrap();
    let input = operation.input();
    let pointer = input.caption.as_ptr();
    drop(input);
    drop(operation);
    let keys = queue.worker.retained_keys();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].family, "upload-reserve");
    assert_eq!(keys[0].generation, 1);
    let recovered = queue
        .worker
        .reattach::<Draft, Result<Reservation, AppError>>(&keys[0].key)
        .unwrap();
    assert_eq!(recovered.input().caption.as_ptr(), pointer);
    queue.worker.submit_held(&recovered).unwrap();
    result(&recovered);
    drop(recovered);
    drop(keys);
    let key = queue.worker.retained_keys().pop().unwrap();
    let recovered = queue
        .worker
        .reattach::<Draft, Result<Reservation, AppError>>(&key.key)
        .unwrap();
    assert!(result(&recovered).as_ref().as_ref().unwrap().created);
}
#[test]
fn oversized_scope_refusal_is_bounded_and_retains_exact_input_generation() {
    let queue = fixture();
    let mut scope = scope(&queue);
    scope.key.channel = "X".repeat(10_000);
    let ptr = scope.key.channel.as_ptr();
    let refused = match queue.worker.hold_review(scope) {
        Err(value) => value,
        Ok(_) => panic!("oversized"),
    };
    assert_eq!(refused.reason, Reason::Metadata);
    assert_eq!(refused.generation, Some(1));
    assert_eq!(refused.input.key.channel.as_ptr(), ptr);
    assert!(queue.worker.retained_keys().is_empty());
    assert!(matches!(queue.worker.begin_fence(), Err(Reason::Metadata)));
}
#[test]
fn exhausted_real_queue_counter_keeps_original_retained_request() {
    let queue = fixture();
    let operation = queue.worker.hold_upload(draft(&queue)).unwrap();
    queue.worker.pending.lock().unwrap().upgrade_generation = u64::MAX;
    assert_eq!(queue.worker.submit_held(&operation), Err(Reason::Counter));
    assert!(matches!(operation.poll(), Err(Delivery::Pending)));
    assert!(matches!(queue.worker.begin_fence(), Err(Reason::Counter)));
    assert!(
        DraftStore::open(queue.owner.path())
            .unwrap()
            .load_uploads(queue.account())
            .unwrap()
            .is_empty()
    );
}
#[test]
fn fence_includes_actual_detached_command_while_sqlite_is_blocked() {
    let queue = fixture();
    let connection = rusqlite::Connection::open(queue.owner.path()).unwrap();
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    let operation = queue.worker.hold_upload(draft(&queue)).unwrap();
    queue.worker.submit_held(&operation).unwrap();
    let until = Instant::now() + Duration::from_secs(1);
    let detached =
        loop {
            let snapshot = queue.diagnostics().unwrap();
            if let Some(row) = snapshot.records.iter().find(|row| {
                row.stage == accounting::Stage::Detached && row.family == "upload-reserve"
            }) {
                break row.id;
            }
            assert!(Instant::now() < until);
            std::thread::yield_now();
        };
    let fence = queue.worker.begin_fence().unwrap();
    assert!(fence.records.contains(&detached));
    assert!(matches!(operation.poll(), Err(Delivery::Pending)));
    let (owner, generation) = identity(&queue);
    let refused = queue
        .worker
        .try_enqueue(
            &owner,
            generation,
            queue.account().into(),
            [(
                scope(&queue).key,
                crate::drafts::store::sync::tests::local("during SQL"),
            )]
            .into(),
        )
        .unwrap_err();
    assert_eq!(refused.reason, Reason::Fencing);
    connection.execute_batch("COMMIT").unwrap();
    assert!(result(&operation).as_ref().as_ref().unwrap().created);
}

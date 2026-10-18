use super::*;

const START: u64 = 10_000;
fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).into()).collect()
}
fn observation() -> Observation {
    Observation {
        connection_count: 1,
        slow_connection_count: 1,
        major_version: 1,
    }
}
fn scope() -> Scope {
    Scope::capture(
        "fixture-account",
        &WorkspaceSession {
            team_id: "T00000001".into(),
            user_id: "U00000999".into(),
            token: "fixture-token".into(),
            name: "Fixture".into(),
            url: "https://fixture.test".into(),
            enterprise_id: None,
        },
        7,
    )
    .unwrap()
}
fn setup(values: &[String]) -> (Scope, Manager, Cycle) {
    let s = scope();
    let mut m = Manager::new(s.clone());
    m.mark_fresh(&s, START, observation(), values).unwrap();
    let c = m.begin_due(&s, START, observation()).unwrap().unwrap();
    (s, m, c)
}
fn operation(m: &mut Manager, s: &Scope, c: &Cycle, now: u64) -> Operation {
    m.next_operation(s, c, now, 0.0).unwrap().unwrap()
}
fn complete(m: &mut Manager, s: &Scope, c: &Cycle, now: u64) {
    assert!(m.next_operation(s, c, now, 0.0).unwrap().is_none());
    assert!(m.cycle_done(s, c).unwrap());
    assert!(
        m.finish_parent(s, c, now, ParentOutcome::Completed)
            .unwrap()
    );
}
fn subscribe(values: &[String]) -> (Scope, Manager) {
    let (s, mut m, c) = setup(values);
    let op = operation(&mut m, &s, &c, START);
    m.started(&s, &op, START).unwrap();
    m.settle(&s, &op, START, Outcome::Success).unwrap();
    complete(&mut m, &s, &c, START);
    (s, m)
}

#[test]
fn duplicate_hits_pause_and_source_ingress_share_one_owned_parent() {
    let s = scope();
    let mut m = Manager::new(s.clone());
    m.hit(&s, START, observation(), "U00000001", true).unwrap();
    assert_eq!(m.next_wake_ms(), None);
    m.hit(&s, START, observation(), "U00000001", false).unwrap();
    m.hit(&s, START + 1, observation(), "U00000001", false)
        .unwrap();
    m.mark_fresh(
        &s,
        START + 2,
        observation(),
        &ids(&["U00000002", "U00000002"]),
    )
    .unwrap();
    assert_eq!(m.pending(), ids(&["U00000001", "U00000002"]));
    assert_eq!(m.next_parent_due_ms(), Some(START));
    let c = m.begin_due(&s, START + 2, observation()).unwrap().unwrap();
    assert_eq!(c.ids(), ids(&["U00000001", "U00000002"]));
    assert!(m.begin_due(&s, START + 3, observation()).unwrap().is_none());
    assert!(m.has_work_for(&ids(&["U00000001"])));
    assert_eq!(m.stats().cycles, 1);
}

#[test]
fn parent_reservation_is_two_hundred_ms_but_operations_are_one_second() {
    let (s, mut m, c) = setup(&ids(&["U00000001"]));
    let op = operation(&mut m, &s, &c, START);
    m.started(&s, &op, START).unwrap();
    m.hit(&s, START + 1, observation(), "U00000002", false)
        .unwrap();
    m.settle(&s, &op, START + 2, Outcome::Success).unwrap();
    complete(&mut m, &s, &c, START + 2);
    assert_eq!(m.next_parent_due_ms(), Some(START + 200));
    assert!(
        m.begin_due(&s, START + 199, observation())
            .unwrap()
            .is_none()
    );
    let next = m
        .begin_due(&s, START + 200, observation())
        .unwrap()
        .unwrap();
    assert!(
        m.next_operation(&s, &next, START + 200, 0.0)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        m.next_operation_due_ms(&s, &next).unwrap(),
        Some(START + 1000)
    );
    assert_eq!(m.next_wake_ms(), Some(START + 1000));
    let op = operation(&mut m, &s, &next, START + 1000);
    assert_eq!(op.ids(), ids(&["U00000002"]));
}

#[test]
fn two_hundred_one_hits_partition_into_ordered_hundred_id_chunks() {
    let values: Vec<_> = (0..201).map(|n| format!("U{n:08}")).collect();
    let (s, mut m, c) = setup(&values);
    for (index, expected) in values.chunks(100).enumerate() {
        let now = START + index as u64 * 1000;
        let op = operation(&mut m, &s, &c, now);
        assert_eq!(op.kind(), Kind::Subscribe);
        assert_eq!(op.ids(), expected);
        m.started(&s, &op, now).unwrap();
        m.settle(&s, &op, now, Outcome::Success).unwrap();
    }
    complete(&mut m, &s, &c, START + 2000);
    assert_eq!(m.subscribed().len(), 201);
    // Source set is new requested IDs first, then the previous subscription set.
    assert_eq!(m.subscribed()[0], values[200]);
    assert_eq!(m.next_wake_ms(), None);
}

#[test]
fn failed_issued_subscription_finally_installs_captured_set_without_ack_claim() {
    let (s, mut m, c) = setup(&ids(&["U00000001"]));
    let op = operation(&mut m, &s, &c, START);
    assert!(m.settle(&s, &op, START, Outcome::Failure).is_err());
    m.started(&s, &op, START).unwrap();
    m.settle(&s, &op, START, Outcome::Failure).unwrap();
    assert_eq!(m.subscribed(), ids(&["U00000001"]));
    assert!(m.settle(&s, &op, START, Outcome::Success).is_err());
    complete(&mut m, &s, &c, START);
    m.hit(&s, START + 1, observation(), "U00000001", false)
        .unwrap();
    assert!(m.pending().is_empty());
}

#[test]
fn local_refusal_keeps_ids_without_grant_or_automatic_retry() {
    let (s, mut m, c) = setup(&ids(&["U00000001"]));
    let op = operation(&mut m, &s, &c, START);
    m.return_unsent(&s, &c).unwrap();
    assert!(!m.operation_current(&s, &op));
    assert!(m.subscribed().is_empty());
    assert_eq!(m.pending(), ids(&["U00000001"]));
    assert_eq!(m.next_wake_ms(), None);
    m.resume(&s, START + 1, observation()).unwrap();
    assert_eq!(m.next_parent_due_ms(), Some(START + 200));
}

#[test]
fn invalidation_waits_for_hit_and_initial_failed_unsubscribe_is_retried_in_chunk() {
    let (s, mut m) = subscribe(&ids(&["U00000001"]));
    m.invalidate(&s, &ids(&["U00000001"])).unwrap();
    assert_eq!(m.stale(), ids(&["U00000001"]));
    assert_eq!(m.next_wake_ms(), None);
    m.hit(&s, START + 1000, observation(), "U00000001", false)
        .unwrap();
    let c = m
        .begin_due(&s, START + 1000, observation())
        .unwrap()
        .unwrap();
    let initial = operation(&mut m, &s, &c, START + 1000);
    assert_eq!(initial.kind(), Kind::Unsubscribe);
    assert_eq!(initial.deadline_ms(), None);
    m.started(&s, &initial, START + 1000).unwrap();
    m.settle(&s, &initial, START + 1000, Outcome::Failure)
        .unwrap();
    let retry = operation(&mut m, &s, &c, START + 2000);
    assert_eq!(retry.kind(), Kind::Unsubscribe);
    assert_eq!(retry.deadline_ms(), Some(START + 2000 + TIMEOUT_MS));
    m.started(&s, &retry, START + 2000).unwrap();
    m.settle(&s, &retry, START + 2000, Outcome::Success)
        .unwrap();
    let read = operation(&mut m, &s, &c, START + 3000);
    assert_eq!(read.kind(), Kind::Subscribe);
    m.started(&s, &read, START + 3000).unwrap();
    m.settle(&s, &read, START + 3000, Outcome::Success).unwrap();
    complete(&mut m, &s, &c, START + 3000);
    // Source successful unsubscribe does not erase the new stale set.
    assert_eq!(m.stale(), ids(&["U00000001"]));
}

#[test]
fn captured_finally_set_survives_inflight_invalidation_exactly_as_source() {
    let (s, mut m) = subscribe(&ids(&["U00000001"]));
    m.hit(&s, START + 1000, observation(), "U00000002", false)
        .unwrap();
    let c = m
        .begin_due(&s, START + 1000, observation())
        .unwrap()
        .unwrap();
    let op = operation(&mut m, &s, &c, START + 1000);
    m.started(&s, &op, START + 1000).unwrap();
    m.invalidate(&s, &ids(&["U00000001"])).unwrap();
    m.settle(&s, &op, START + 1001, Outcome::Failure).unwrap();
    assert_eq!(m.subscribed(), ids(&["U00000002", "U00000001"]));
    assert_eq!(m.stale(), ids(&["U00000001"]));
}

#[test]
fn slow_reset_major_reset_and_fast_reconnect_have_distinct_effects() {
    let (s, mut m) = subscribe(&ids(&["U00000001"]));
    let fast = Observation {
        connection_count: 2,
        ..observation()
    };
    m.observe(&s, START + 1000, fast).unwrap();
    assert_eq!(m.subscribed(), ids(&["U00000001"]));
    let reconnect = m.cycles().pop().unwrap();
    assert!(reconnect.is_reconnect());
    let op = operation(&mut m, &s, &reconnect, START + 1000);
    assert_eq!(op.deadline_ms(), None);
    m.started(&s, &op, START + 1000).unwrap();
    m.settle(&s, &op, START + 1000, Outcome::Success).unwrap();
    assert!(
        m.next_operation(&s, &reconnect, START + 1000, 0.0)
            .unwrap()
            .is_none()
    );
    assert!(
        !m.finish_parent(&s, &reconnect, START + 1000, ParentOutcome::Completed)
            .unwrap()
    );
    let major = Observation {
        major_version: 2,
        ..fast
    };
    m.observe(&s, START + 1001, major).unwrap();
    assert!(m.subscribed().is_empty());
    assert_eq!(m.stale(), ids(&["U00000001"]));
    m.observe(
        &s,
        START + 1002,
        Observation {
            slow_connection_count: 2,
            major_version: 3,
            ..major
        },
    )
    .unwrap();
    assert!(m.stale().is_empty());
    assert!(m.subscribed().is_empty());
}

#[test]
fn lifecycle_retirement_requeues_and_refuses_original_receipts() {
    let (s, mut m, c) = setup(&ids(&["U00000001"]));
    let op = operation(&mut m, &s, &c, START);
    m.started(&s, &op, START).unwrap();
    let next = Observation {
        connection_count: 2,
        slow_connection_count: 2,
        ..observation()
    };
    m.observe(&s, START + 1, next).unwrap();
    assert!(!m.cycle_current(&s, &c));
    assert!(m.settle(&s, &op, START + 2, Outcome::Success).is_err());
    assert_eq!(m.pending(), ids(&["U00000001"]));
    assert!(m.subscribed().is_empty());
    let mut foreign = s.clone();
    foreign.owner.token = "replacement-fixture".into();
    assert!(
        m.hit(&foreign, START + 2, next, "U00000002", false)
            .is_err()
    );
    foreign = s.clone();
    foreign.epoch += 1;
    assert!(!m.current(&foreign));
    foreign = s.clone();
    foreign.account = "other".into();
    assert!(!m.current(&foreign));
}

#[test]
fn parent_timeout_preserves_issued_chunk_and_requeues_only_future_unsent_ids() {
    let values: Vec<_> = (0..101).map(|n| format!("U{n:08}")).collect();
    let (s, mut m, c) = setup(&values);
    // Inner starts later than the parent, so these are truly distinct deadlines.
    let op = operation(&mut m, &s, &c, START + 500);
    m.started(&s, &op, START + 500).unwrap();
    assert_eq!(op.deadline_ms(), Some(c.parent_deadline_ms() + 500));
    assert!(m.parent_active(&s, &c));
    assert!(
        m.finish_parent(&s, &c, c.parent_deadline_ms(), ParentOutcome::Timeout)
            .unwrap()
    );
    assert!(!m.parent_active(&s, &c));
    assert!(m.operation_current(&s, &op));
    assert_eq!(m.pending(), &values[100..]);
    assert!(m.subscribed().is_empty());
    m.settle(&s, &op, c.parent_deadline_ms() + 1, Outcome::Failure)
        .unwrap();
    assert_eq!(m.subscribed().len(), 100);
    assert!(
        m.next_operation(&s, &c, c.parent_deadline_ms() + 1, 0.0)
            .unwrap()
            .is_none()
    );
    assert!(
        !m.finish_parent(&s, &c, c.parent_deadline_ms() + 1, ParentOutcome::Completed)
            .unwrap()
    );
    assert_eq!(m.pending(), &values[100..]);
}

#[test]
fn issued_chunk_timeout_has_finally_effect_but_refuses_late_completion() {
    let (s, mut m, c) = setup(&ids(&["U00000001"]));
    let op = operation(&mut m, &s, &c, START + 100);
    m.started(&s, &op, START + 100).unwrap();
    let deadline = op.deadline_ms().unwrap();
    assert!(m.timeout_chunk(&s, &c, deadline - 1).is_err());
    m.finish_parent(&s, &c, c.parent_deadline_ms(), ParentOutcome::Timeout)
        .unwrap();
    m.timeout_chunk(&s, &c, deadline).unwrap();
    assert_eq!(m.subscribed(), ids(&["U00000001"]));
    assert!(m.settle(&s, &op, deadline + 1, Outcome::Success).is_err());
}

#[test]
fn wholly_unsent_chunk_timeout_never_grants_source_subscription() {
    let (s, mut m, c) = setup(&ids(&["U00000001"]));
    let op = operation(&mut m, &s, &c, START);
    assert!(m.timeout_chunk(&s, &c, op.deadline_ms().unwrap()).is_err());
    m.finish_parent(&s, &c, c.parent_deadline_ms(), ParentOutcome::Timeout)
        .unwrap();
    assert!(!m.operation_current(&s, &op));
    assert!(m.subscribed().is_empty());
    assert_eq!(m.pending(), ids(&["U00000001"]));
}

#[test]
fn returning_detached_original_does_not_clear_successor_parent_or_due() {
    let values: Vec<_> = (0..101).map(|n| format!("U{n:08}")).collect();
    let (s, mut m, c) = setup(&values);
    let op = operation(&mut m, &s, &c, START + 100);
    m.started(&s, &op, START + 100).unwrap();
    m.finish_parent(&s, &c, c.parent_deadline_ms(), ParentOutcome::Timeout)
        .unwrap();
    let due = m.next_parent_due_ms();
    m.return_unsent(&s, &c).unwrap();
    assert_eq!(m.next_parent_due_ms(), due);
    let successor = m
        .begin_due(&s, c.parent_deadline_ms(), observation())
        .unwrap()
        .unwrap();
    assert!(m.cycle_current(&s, &successor));
    assert!(m.return_unsent(&s, &c).is_err());
    assert!(m.cycle_current(&s, &successor));
}

#[test]
fn invalid_entropy_does_not_consume_work_and_reserved_jitter_does_not_reroll() {
    let (s, mut m, c) = setup(&ids(&["U00000001"]));
    assert!(m.next_operation(&s, &c, START, f64::NAN).is_err());
    let op = operation(&mut m, &s, &c, START);
    m.started(&s, &op, START).unwrap();
    m.settle(&s, &op, START, Outcome::Success).unwrap();
    complete(&mut m, &s, &c, START);
    m.hit(&s, START + 200, observation(), "U00000002", false)
        .unwrap();
    let next = m
        .begin_due(&s, START + 200, observation())
        .unwrap()
        .unwrap();
    assert!(
        m.next_operation(&s, &next, START + 200, 0.5)
            .unwrap()
            .is_none()
    );
    // HyoU(0,.1,true) at entropy .5 uses epsilon .01 -> random .055.
    assert_eq!(
        m.next_operation_due_ms(&s, &next).unwrap(),
        Some(START + 1044)
    );
    assert!(
        m.next_operation(&s, &next, START + 1043, 0.0)
            .unwrap()
            .is_none()
    );
    let op = operation(&mut m, &s, &next, START + 1044);
    assert_eq!(op.ids(), ids(&["U00000002"]));
    assert!(m.started(&s, &op, START + 1043).is_err());
}

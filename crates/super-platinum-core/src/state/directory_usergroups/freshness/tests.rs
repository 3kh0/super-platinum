use super::*;

const A: &str = "S00000001";
const B: &str = "S00000002";
const C: &str = "S00000003";
fn clock(ms: u64) -> Clock {
    Clock::new(ms).unwrap()
}
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T_GROUP_TEST".into(),
        user_id: "U_SELF".into(),
        enterprise_id: None,
        name: "Test".into(),
        url: "https://fixture.test".into(),
        token: "synthetic-never-persisted".into(),
    }
}
fn obs(slow: Option<u64>, major: Option<u64>) -> Observation {
    Observation {
        slow_connection_count: slow,
        major_version: major,
        enabled: true,
    }
}
fn normal() -> Observation {
    obs(Some(1), Some(1))
}
fn setup() -> (Scope, Manager) {
    let scope = Scope::new("account", &owner(), 1).unwrap();
    let manager = Manager::new(scope.clone());
    (scope, manager)
}
fn hit(m: &mut Manager, s: &Scope, ms: u64, id: &str) {
    m.maybe_refetch(s, clock(ms), normal(), id, false).unwrap();
}
fn batch(m: &mut Manager, s: &Scope, ms: u64) -> Batch {
    m.begin_due(s, clock(ms), normal()).unwrap().unwrap()
}
fn stamp(m: &mut Manager, s: &Scope, observation: Observation, ids: &[&str]) {
    m.mark_fresh(
        s,
        clock(1_000),
        observation,
        &ids.iter().map(|v| (*v).into()).collect::<Vec<_>>(),
    )
    .unwrap();
}

#[test]
fn present_without_manager_stamp_is_stale_not_absent_or_automatically_fresh() {
    let (s, mut m) = setup();
    assert!(!m.is_fresh(&s, clock(1_000), normal(), A).unwrap());
    hit(&mut m, &s, 1_000, A);
    assert_eq!(m.pending(), [A]);
    assert_eq!(m.next_wake(), Some(1_000));
    assert!(m.fresh_ids(&s, clock(1_000)).unwrap().is_empty());
}

#[test]
fn group_stamps_are_infinite_and_equal_stamp_preserves_order() {
    let (s, mut m) = setup();
    stamp(&mut m, &s, normal(), &[B, A, B]);
    stamp(&mut m, &s, normal(), &[A]);
    let far = clock(8_000_000_000_000_000);
    assert!(m.is_fresh(&s, far, normal(), A).unwrap());
    assert_eq!(m.fresh_ids(&s, far).unwrap(), [B, A]);
    m.maybe_refetch(&s, far, normal(), A, false).unwrap();
    assert_eq!(m.next_wake(), None);
}

#[test]
fn normalized_zero_and_missing_slow_count_are_one() {
    let (s, mut m) = setup();
    stamp(&mut m, &s, obs(None, Some(1)), &[A]);
    assert!(
        m.is_fresh(&s, clock(2_000), obs(Some(0), Some(1)), A)
            .unwrap()
    );
    assert!(m.is_fresh(&s, clock(2_000), normal(), A).unwrap());
}

#[test]
fn slow_count_changes_in_both_directions_clear_stamps() {
    let (s, mut m) = setup();
    stamp(&mut m, &s, normal(), &[A]);
    assert!(
        !m.is_fresh(&s, clock(2_000), obs(Some(2), Some(1)), A)
            .unwrap()
    );
    stamp(&mut m, &s, obs(Some(2), Some(1)), &[A]);
    assert!(!m.is_fresh(&s, clock(2_000), normal(), A).unwrap());
}

#[test]
fn major_change_resets_but_initial_falsy_major_is_adopted() {
    let (s, mut m) = setup();
    stamp(&mut m, &s, obs(None, None), &[A]);
    assert!(m.is_fresh(&s, clock(2_000), normal(), A).unwrap());
    assert!(
        !m.is_fresh(&s, clock(2_000), obs(Some(1), Some(2)), A)
            .unwrap()
    );
    stamp(&mut m, &s, obs(Some(1), Some(0)), &[A]);
    assert!(
        m.is_fresh(&s, clock(2_000), obs(Some(1), Some(3)), A)
            .unwrap()
    );
}

#[test]
fn disabled_manager_does_not_reset_or_stamp_or_schedule() {
    let (s, mut m) = setup();
    stamp(&mut m, &s, normal(), &[A]);
    let disabled = Observation {
        enabled: false,
        ..obs(Some(8), Some(9))
    };
    assert!(m.is_fresh(&s, clock(2_000), disabled, B).unwrap());
    m.maybe_refetch(&s, clock(2_000), disabled, B, false)
        .unwrap();
    stamp(&mut m, &s, disabled, &[B]);
    assert_eq!(m.fresh_ids(&s, clock(2_000)).unwrap(), [A]);
    assert!(m.pending().is_empty());
}

#[test]
fn pause_blocks_new_demand_but_does_not_cancel_reserved_delay() {
    let (s, mut m) = setup();
    let mut pause = Pause::default();
    pause.indefinitely();
    m.maybe_refetch(&s, clock(1_000), normal(), A, pause.paused())
        .unwrap();
    assert_eq!(m.next_wake(), None);
    pause.unpause();
    hit(&mut m, &s, 1_000, A);
    pause.keypress(TimerId::new(7).unwrap());
    m.maybe_refetch(&s, clock(1_000), normal(), B, pause.paused())
        .unwrap();
    assert_eq!(batch(&mut m, &s, 1_000).ids(), [A]);
}

#[test]
fn delayed_batch_takes_current_ordered_set_with_one_flight() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 0, B);
    hit(&mut m, &s, 10, A);
    hit(&mut m, &s, 20, B);
    assert!(m.begin_due(&s, clock(199), normal()).unwrap().is_none());
    let first = batch(&mut m, &s, 200);
    assert_eq!(first.ids(), [B, A]);
    assert_eq!(first.deadline_ms(), 60_200);
    hit(&mut m, &s, 201, C);
    assert!(m.begin_due(&s, clock(999), normal()).unwrap().is_none());
    assert!(m.batch_current(&s, &first.clone()));
}

#[test]
fn cadence_uses_reservation_time_not_previous_completion_time() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let first = batch(&mut m, &s, 1_000);
    hit(&mut m, &s, 1_010, B);
    m.finish(&s, clock(1_100), normal(), &first).unwrap();
    assert_eq!(m.next_wake(), Some(1_200));
    let second = batch(&mut m, &s, 1_200);
    hit(&mut m, &s, 1_201, C);
    m.finish(&s, clock(2_000), normal(), &second).unwrap();
    assert_eq!(m.next_wake(), Some(2_000));
}

#[test]
fn repeated_inflight_id_is_removed_but_new_id_is_driven_after_finish() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let first = batch(&mut m, &s, 1_000);
    hit(&mut m, &s, 1_010, A);
    hit(&mut m, &s, 1_020, B);
    assert!(
        m.finish(&s, clock(1_030), normal(), &first)
            .unwrap()
            .notify_immediately
    );
    assert_eq!(m.pending(), [B]);
    assert_eq!(batch(&mut m, &s, 1_200).ids(), [B]);
}

#[test]
fn ordinary_failure_uses_same_finally_and_does_not_retry_on_next_hit() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let attempted = batch(&mut m, &s, 1_000);
    // No row/response is supplied: an attempted failed fetch grants only CFM
    // freshness, never canonical metadata. Source catches error before finally.
    m.finish(&s, clock(1_001), normal(), &attempted).unwrap();
    hit(&mut m, &s, 80_000, A);
    assert_eq!(m.next_wake(), None);
    assert!(m.is_fresh(&s, clock(80_000), normal(), A).unwrap());
}

#[test]
fn timeout_uses_original_deadline_then_retires_original_batch() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let first = batch(&mut m, &s, 1_000);
    assert_eq!(
        m.timeout(&s, clock(60_999), normal(), &first),
        Err(Error::Stale)
    );
    assert!(m.batch_current(&s, &first));
    m.timeout(&s, clock(61_000), normal(), &first).unwrap();
    assert_eq!(
        m.finish(&s, clock(61_001), normal(), &first),
        Err(Error::Stale)
    );
    assert_eq!(m.fresh_ids(&s, clock(61_001)).unwrap(), [A]);
}

#[test]
fn slow_change_greater_than_one_requeues_original_ids_behind_existing_pending() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let first = batch(&mut m, &s, 1_000);
    hit(&mut m, &s, 1_001, B);
    let changed = obs(Some(2), Some(1));
    m.finish(&s, clock(1_010), changed, &first).unwrap();
    assert_eq!(m.pending(), [B, A]);
    assert!(m.fresh_ids(&s, clock(1_010)).unwrap().is_empty());
    let next = m.begin_due(&s, clock(1_200), changed).unwrap().unwrap();
    m.finish(&s, clock(1_201), changed, &next).unwrap();
    assert_eq!(m.fresh_ids(&s, clock(1_202)).unwrap(), [B, A]);
}

#[test]
fn major_only_change_and_slow_return_to_one_do_not_requeue() {
    for end in [obs(Some(2), Some(2)), obs(Some(1), Some(1))] {
        let (s, mut m) = setup();
        let initial = obs(Some(2), Some(1));
        m.maybe_refetch(&s, clock(1_000), initial, A, false)
            .unwrap();
        let first = m.begin_due(&s, clock(1_000), initial).unwrap().unwrap();
        m.finish(&s, clock(1_001), end, &first).unwrap();
        assert_eq!(m.fresh_ids(&s, clock(1_002)).unwrap(), [A]);
        assert!(m.pending().is_empty());
    }
}

#[test]
fn dispatch_samples_raw_count_after_delay_not_at_hit() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 0, A);
    let changed = obs(Some(4), Some(1));
    let first = m.begin_due(&s, clock(200), changed).unwrap().unwrap();
    m.finish(&s, clock(201), changed, &first).unwrap();
    assert_eq!(m.fresh_ids(&s, clock(202)).unwrap(), [A]);
}

#[test]
fn original_scope_and_batch_identity_cannot_cross_managers_or_credentials() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let first = batch(&mut m, &s, 1_000);
    let mut other = Manager::new(s.clone());
    hit(&mut other, &s, 1_000, A);
    let second = batch(&mut other, &s, 1_000);
    assert!(!m.batch_current(&s, &second));
    assert_eq!(
        m.finish(&s, clock(1_001), normal(), &second),
        Err(Error::Stale)
    );
    for index in 0..8 {
        let mut user = owner();
        let mut account = "account";
        let mut epoch = 1;
        match index {
            0 => account = "other",
            1 => epoch = 2,
            2 => user.team_id = "T_OTHER".into(),
            3 => user.user_id = "U_OTHER".into(),
            4 => user.token = "different-fixture-secret".into(),
            5 => user.url = "https://other.test".into(),
            6 => user.enterprise_id = Some("E_OTHER".into()),
            _ => user.name = "Changed".into(),
        }
        let foreign = Scope::new(account, &user, epoch).unwrap();
        assert!(!m.current(&foreign));
        assert!(!m.batch_current(&foreign, &first));
        assert_eq!(
            m.is_fresh(&foreign, clock(2_000), normal(), A),
            Err(Error::Owner)
        );
    }
    assert!(m.batch_current(&s, &first));
}

#[test]
fn unsent_admission_defers_without_stamp_or_automatic_wake() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let first = batch(&mut m, &s, 1_000);
    m.return_unsent(&s, &first).unwrap();
    assert_eq!(m.pending(), [A]);
    assert_eq!(m.next_wake(), None);
    assert!(m.fresh_ids(&s, clock(2_000)).unwrap().is_empty());
    m.resume(&s, clock(2_000), normal()).unwrap();
    assert_eq!(batch(&mut m, &s, 2_000).ids(), [A]);
}

#[test]
fn partial_admission_grants_only_attempted_ids_and_never_auto_retries_unsent() {
    let (s, mut m) = setup();
    for id in [A, B, C] {
        hit(&mut m, &s, 1_000, id);
    }
    let first = batch(&mut m, &s, 1_000);
    assert_eq!(
        m.finish_admitted(&s, clock(1_001), normal(), &first, &[C.into(), A.into()]),
        Err(Error::Shape)
    );
    assert!(m.batch_current(&s, &first));
    m.finish_admitted(&s, clock(1_002), normal(), &first, &[A.into(), C.into()])
        .unwrap();
    assert_eq!(m.pending(), [B]);
    assert_eq!(m.fresh_ids(&s, clock(1_003)).unwrap(), [A, C]);
    assert_eq!(m.next_wake(), None);
}

#[test]
fn stamps_validate_atomically_and_never_accept_channel_or_malformed_ids() {
    let (s, mut m) = setup();
    for id in ["", "C00000001", "S1", "S00000_01", "S00000001\n"] {
        assert_eq!(
            m.mark_fresh(&s, clock(1_000), normal(), &[A.into(), id.into()]),
            Err(Error::Shape)
        );
        assert!(m.fresh_ids(&s, clock(1_000)).unwrap().is_empty());
        assert_eq!(
            m.maybe_refetch(&s, clock(1_000), normal(), id, false),
            Err(Error::Shape)
        );
        assert!(m.pending().is_empty());
    }
}

#[test]
fn no_group_scheduler_batch_limit_is_invented_from_http_chunk_size() {
    let (s, mut m) = setup();
    for index in 0..301 {
        hit(&mut m, &s, 1_000, &format!("S{index:08}"));
    }
    let first = batch(&mut m, &s, 1_000);
    assert_eq!(first.ids().len(), 301);
    assert_eq!(first.ids()[0], "S00000000");
    assert_eq!(first.ids()[300], "S00000300");
}

#[test]
fn timeout_after_reconnect_requeues_instead_of_certifying_old_attempt() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let first = batch(&mut m, &s, 1_000);
    m.timeout(&s, clock(61_000), obs(Some(2), Some(1)), &first)
        .unwrap();
    assert_eq!(m.pending(), [A]);
    assert!(m.fresh_ids(&s, clock(61_000)).unwrap().is_empty());
    assert_eq!(m.next_wake(), Some(61_000));
}

#[test]
fn disable_after_reservation_does_not_cancel_callback_but_finish_cannot_stamp() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let disabled = Observation {
        enabled: false,
        ..normal()
    };
    let first = m.begin_due(&s, clock(1_000), disabled).unwrap().unwrap();
    m.finish(&s, clock(1_001), disabled, &first).unwrap();
    assert!(m.fresh_ids(&s, clock(1_002)).unwrap().is_empty());
    assert!(m.pending().is_empty());
    assert_eq!(m.next_wake(), None);
}

#[test]
fn equal_version_publication_can_stamp_during_flight_without_changing_batch() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let first = batch(&mut m, &s, 1_000);
    // The publication owner calls this for both accepted upserts and unchanged
    // truthy date_update rows. It does not replace the original active token.
    stamp(&mut m, &s, normal(), &[A]);
    hit(&mut m, &s, 1_001, A);
    assert!(m.batch_current(&s, &first));
    assert!(m.pending().is_empty());
    m.finish(&s, clock(1_002), normal(), &first).unwrap();
    assert_eq!(m.fresh_ids(&s, clock(1_003)).unwrap(), [A]);
}

#[test]
fn partial_duplicate_refuses_without_consuming_original_and_empty_is_unsent() {
    let (s, mut m) = setup();
    hit(&mut m, &s, 1_000, A);
    let first = batch(&mut m, &s, 1_000);
    assert_eq!(
        m.finish_admitted(&s, clock(1_001), normal(), &first, &[A.into(), A.into()]),
        Err(Error::Shape)
    );
    assert!(m.batch_current(&s, &first));
    assert_eq!(
        m.finish_admitted(&s, clock(1_001), normal(), &first, &[])
            .unwrap(),
        Effects::default()
    );
    assert_eq!(m.pending(), [A]);
    assert_eq!(m.next_wake(), None);
    assert!(m.fresh_ids(&s, clock(1_001)).unwrap().is_empty());
}

#[test]
fn scope_rejects_missing_identity_without_exposing_credentials() {
    assert!(Scope::new("", &owner(), 1).is_err());
    for index in 0..4 {
        let mut user = owner();
        match index {
            0 => user.team_id.clear(),
            1 => user.user_id.clear(),
            2 => user.token.clear(),
            _ => user.url.clear(),
        }
        assert!(matches!(Scope::new("account", &user, 1), Err(Error::Owner)));
    }
}

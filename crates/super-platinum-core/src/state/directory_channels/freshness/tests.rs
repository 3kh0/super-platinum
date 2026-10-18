use super::*;
use serde_json::{Value, json};
fn scope() -> Scope {
    Scope::new(
        "A",
        &WorkspaceSession {
            team_id: "T".into(),
            user_id: "U".into(),
            enterprise_id: None,
            name: "Synthetic".into(),
            token: "synthetic".into(),
            url: "https://fixture.slack.com".into(),
        },
        1,
    )
    .unwrap()
}
fn now(ms: u64) -> Clock {
    Clock::new(ms).unwrap()
}
fn state() -> Observation {
    Observation {
        slow_connection_count: Some(1),
        major_version: Some(1),
        enabled: true,
    }
}
fn fresh(id: &str, kind: RowKind) -> Fresh {
    Fresh {
        id: id.into(),
        kind,
        entropy: 0.5,
    }
}
fn vectors() -> Value {
    serde_json::from_str(include_str!("cases.json")).unwrap()
}
fn golden(name: &str) -> Value {
    vectors()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == name)
        .unwrap()
        .clone()
}
const A: &str = "C00000001";
const B: &str = "C00000002";

#[test]
fn actual_jitter_rounding_and_strict_freshness_boundaries() {
    let s = scope();
    let mut manager = Manager::new(s.clone());
    for row in vectors()["jitter"].as_array().unwrap() {
        // Preserve the source Number exactly: serde_json's default approximate
        // float decoder may round the near-one oracle input up to 1.0.
        let entropy = row["js_text"].as_str().unwrap().parse::<f64>().unwrap();
        for (kind, key) in [
            (RowKind::NonExistent, "nonexistent"),
            (RowKind::Absent, "absent"),
        ] {
            let value = Fresh {
                id: A.into(),
                kind,
                entropy,
            };
            let age = row[key].as_u64().unwrap();
            assert_eq!(value.age_seconds().unwrap(), Some(age), "{row}");
            manager
                .mark_fresh(&s, now(1999), state(), &[value])
                .unwrap();
            assert!(
                manager
                    .is_fresh(&s, now((1 + age) * 1000 - 1), state(), A)
                    .unwrap()
            );
            assert!(
                !manager
                    .is_fresh(&s, now((1 + age) * 1000), state(), A)
                    .unwrap()
            );
        }
    }
    let existing = Fresh {
        id: A.into(),
        kind: RowKind::Existing,
        entropy: f64::NAN,
    };
    assert_eq!(existing.age_seconds().unwrap(), None); // No RNG for normal rows.
    for entropy in [f64::NAN, f64::INFINITY, -0.1, 1.0] {
        assert!(
            Fresh {
                id: A.into(),
                kind: RowKind::Absent,
                entropy
            }
            .age_seconds()
            .is_err()
        );
    }
}
#[test]
fn actual_failed_completion_coalesces_and_marks_current_rows() {
    let s = scope();
    let mut m = Manager::new(s.clone());
    for id in [A, A, B] {
        m.maybe_refetch(&s, now(1000), state(), id, false).unwrap();
    }
    let batch = m.begin_due(&s, now(1000), state()).unwrap().unwrap();
    assert_eq!(batch.ids(), [A, B]);
    assert_eq!(batch.deadline_ms(), 61000);
    // Source failure/timeout and success share the same finally path. No response
    // success bit or canonical mutation is fabricated by this helper.
    let effect = m
        .finish(
            &s,
            &batch,
            now(1000),
            state(),
            &[fresh(A, RowKind::Existing), fresh(B, RowKind::NonExistent)],
        )
        .unwrap();
    assert!(effect.notify_immediately);
    assert_eq!(m.fresh_ids(&s, now(1000)).unwrap(), [A, B]);
    let expected = golden("coalesced-failed-read-marks-current-rows");
    assert_eq!(json!(batch.ids()), expected["ids"]);
    assert_eq!(
        json!(m.fresh_ids(&s, now(1_810_000)).unwrap()),
        expected["remaining"]
    );
    assert!(!m.batch_current(&s, &batch));
    assert!(m.finish(&s, &batch, now(1000), state(), &[]).is_err());
}
#[test]
fn actual_pending_same_id_cleanup_and_two_hundred_ms_reservation() {
    let s = scope();
    let mut m = Manager::new(s.clone());
    m.maybe_refetch(&s, now(1000), state(), A, false).unwrap();
    let batch = m.begin_due(&s, now(1000), state()).unwrap().unwrap();
    for id in [A, B, B] {
        m.maybe_refetch(&s, now(1000), state(), id, false).unwrap();
    }
    m.finish(
        &s,
        &batch,
        now(1000),
        state(),
        &[fresh(A, RowKind::Existing)],
    )
    .unwrap();
    assert_eq!(m.pending(), [B]);
    assert_eq!(m.next_wake(), Some(1200));
    assert!(m.begin_due(&s, now(1199), state()).unwrap().is_none());
    let next = m.begin_due(&s, now(1200), state()).unwrap().unwrap();
    let expected = golden("queued-same-id-removed-new-id-delayed");
    assert_eq!(json!(next.ids()), expected["second_ids"]);
    assert_eq!(json!(1200), expected["second_at"]);
    // Cadence uses reservation time; a slow read is not delayed another200ms.
    m.maybe_refetch(&s, now(1500), state(), A, false).unwrap(); // Already fresh, ignored.
    m.maybe_refetch(&s, now(1500), state(), "C00000003", false)
        .unwrap();
    m.finish(
        &s,
        &next,
        now(1500),
        state(),
        &[fresh(B, RowKind::Existing)],
    )
    .unwrap();
    assert_eq!(m.next_wake(), Some(1500));
}
#[test]
fn actual_reconnect_requeues_and_version_only_completion_marks_fresh() {
    let s = scope();
    let mut m = Manager::new(s.clone());
    let mut obs = state();
    m.maybe_refetch(&s, now(1000), obs, A, false).unwrap();
    let batch = m.begin_due(&s, now(1000), obs).unwrap().unwrap();
    obs.slow_connection_count = Some(2);
    m.finish(&s, &batch, now(1000), obs, &[]).unwrap();
    assert!(m.fresh_ids(&s, now(1000)).unwrap().is_empty());
    assert_eq!(m.pending(), [A]);
    let next = m.begin_due(&s, now(1200), obs).unwrap().unwrap();
    m.finish(&s, &next, now(1200), obs, &[fresh(A, RowKind::Existing)])
        .unwrap();
    assert_eq!(
        json!(m.fresh_ids(&s, now(1200)).unwrap()),
        golden("connection-change-requeues")["fresh"]
    );
    obs.major_version = Some(2);
    assert_eq!(m.fresh_ids(&s, now(1200)).unwrap(), [A]); // Getter has no reset.
    assert!(!m.is_fresh(&s, now(1200), obs, A).unwrap());
    m.maybe_refetch(&s, now(1200), obs, A, false).unwrap();
    let batch = m.begin_due(&s, now(1400), obs).unwrap().unwrap();
    obs.major_version = Some(3);
    m.finish(&s, &batch, now(1400), obs, &[fresh(A, RowKind::Existing)])
        .unwrap();
    assert_eq!(
        json!(m.fresh_ids(&s, now(1400)).unwrap()),
        golden("get-fresh-does-not-reset-version-finally-does")["fresh"]
    );
}
#[test]
fn actual_keyboard_pause_keeps_reserved_turn_and_timeout_is_terminal() {
    let s = scope();
    let mut m = Manager::new(s.clone());
    let mut pause = TimedPause::default();
    m.maybe_refetch(&s, now(1000), state(), A, pause.paused(now(1000)))
        .unwrap();
    pause.keypress(now(1000));
    let batch = m.begin_due(&s, now(1000), state()).unwrap().unwrap();
    assert_eq!(batch.deadline_ms(), 61000);
    m.finish(
        &s,
        &batch,
        now(61000),
        state(),
        &[fresh(A, RowKind::Existing)],
    )
    .unwrap();
    assert_eq!(
        json!(m.fresh_ids(&s, now(61000)).unwrap()),
        golden("pause-does-not-cancel-reserved-turn-timeout-marks")["fresh"]
    );
    assert!(pause.paused(now(2999)));
    assert!(!pause.paused(now(3000)));
    let mut pause = TimedPause::default();
    let mut other = Manager::new(s.clone());
    pause.keypress(now(1000));
    other
        .maybe_refetch(&s, now(2999), state(), A, pause.paused(now(2999)))
        .unwrap();
    assert!(other.next_wake().is_none());
    other
        .maybe_refetch(&s, now(3000), state(), A, pause.paused(now(3000)))
        .unwrap();
    assert_eq!(
        json!(other.next_wake().unwrap()),
        golden("keyboard-pause-strict-deadline")["at"]
    );
    pause.indefinitely(now(3000));
    assert!(pause.paused(now(1_000_000)));
    pause.unpause(now(1_000_000));
    assert!(!pause.paused(now(1_000_000)));
}
#[test]
fn actual_invalidation_burst_strict_window_and_shared_capacity() {
    let s = scope();
    let mut m = Manager::new(s.clone());
    let mut bursts = InvalidationBursts::default();
    let mut notices = 0;
    for _ in 0..5 {
        m.mark_fresh(&s, now(1000), state(), &[fresh(A, RowKind::Existing)])
            .unwrap();
        let e = m
            .mark_stale(&s, now(1000), state(), &[A.into()], &mut bursts)
            .unwrap();
        assert_eq!(e.invalidated, [A]);
        notices += u64::from(e.notify_throttled);
    }
    m.mark_fresh(&s, now(1000), state(), &[fresh(A, RowKind::Existing)])
        .unwrap();
    let e = m
        .mark_stale(&s, now(1000), state(), &[A.into()], &mut bursts)
        .unwrap();
    assert_eq!(e.bursts, [(A.into(), Burst::Detected)]);
    let e = m
        .mark_stale(&s, now(11000), state(), &[A.into()], &mut bursts)
        .unwrap();
    assert_eq!(e.bursts, [(A.into(), Burst::Ongoing)]);
    let e = m
        .mark_stale(&s, now(11001), state(), &[A.into()], &mut bursts)
        .unwrap();
    assert_eq!(e.invalidated, [A]);
    notices += u64::from(e.notify_throttled);
    assert_eq!(
        json!(notices),
        golden("burst-strict-window")["notifications"]
    );
    // Actual WjOl removes the oldest key before replacing a present key at100.
    let mut bursts = InvalidationBursts::default();
    for i in 0..100 {
        assert_eq!(bursts.check(&format!("T:C{i:08}"), 1000), Burst::Allowed);
    }
    assert_eq!(bursts.check("T:C00000099", 1000), Burst::Allowed);
    assert_eq!(
        bursts.len_for_test(),
        golden("actual-lru-existing-key-write-at-capacity")["size"]
            .as_u64()
            .unwrap() as usize
    );
    assert!(!bursts.contains_for_test("T:C00000000"));
}
#[test]
fn owner_batch_and_unsent_admission_are_distinct_from_attempted_failure() {
    let s = scope();
    let mut m = Manager::new(s.clone());
    m.maybe_refetch(&s, now(1000), state(), A, false).unwrap();
    let b = m.begin_due(&s, now(1000), state()).unwrap().unwrap();
    let mut changed = s.clone();
    changed.session.token = "changed".into();
    assert!(!m.batch_current(&changed, &b));
    assert!(m.return_unsent(&changed, &b).is_err());
    let mut replacement = Manager::new(s.clone());
    replacement
        .maybe_refetch(&s, now(1000), state(), A, false)
        .unwrap();
    let other = replacement
        .begin_due(&s, now(1000), state())
        .unwrap()
        .unwrap();
    assert!(!m.batch_current(&s, &other));
    m.return_unsent(&s, &b).unwrap();
    assert_eq!(m.pending(), [A]);
    assert!(m.next_wake().is_none());
    assert!(m.fresh_ids(&s, now(1000)).unwrap().is_empty());
    m.resume(&s, now(1200), state()).unwrap();
    let retried = m.begin_due(&s, now(1200), state()).unwrap().unwrap();
    assert!(!m.batch_current(&s, &b));
    assert!(
        m.finish(
            &s,
            &retried,
            now(1200),
            state(),
            &[fresh(B, RowKind::Existing)]
        )
        .is_err()
    );
    assert!(m.batch_current(&s, &retried));
}
#[test]
fn native_partial_admission_marks_only_attempted_ids_and_preserves_unsent() {
    let s = scope();
    let mut m = Manager::new(s.clone());
    let c = "C00000003";
    for id in [A, B, c] {
        m.maybe_refetch(&s, now(1000), state(), id, false).unwrap();
    }
    let batch = m.begin_due(&s, now(1000), state()).unwrap().unwrap();
    for invalid in [
        vec![fresh(B, RowKind::Existing), fresh(A, RowKind::Existing)],
        vec![fresh(A, RowKind::Existing), fresh(A, RowKind::Existing)],
        vec![fresh("C00000004", RowKind::Existing)],
    ] {
        assert!(
            m.finish_admitted(&s, &batch, now(1000), state(), &invalid)
                .is_err()
        );
        assert!(m.batch_current(&s, &batch));
        assert!(m.fresh_ids(&s, now(1000)).unwrap().is_empty());
    }
    let effect = m
        .finish_admitted(
            &s,
            &batch,
            now(1000),
            state(),
            &[fresh(A, RowKind::Existing), fresh(c, RowKind::NonExistent)],
        )
        .unwrap();
    assert!(effect.notify_immediately);
    assert_eq!(m.fresh_ids(&s, now(1000)).unwrap(), [A, c]);
    assert_eq!(m.pending(), [B]);
    assert!(m.next_wake().is_none());
    m.resume(&s, now(1200), state()).unwrap();
    let unsent = m.begin_due(&s, now(1200), state()).unwrap().unwrap();
    assert_eq!(unsent.ids(), [B]);
    assert_eq!(
        m.finish_admitted(&s, &unsent, now(1200), state(), &[])
            .unwrap(),
        Effects::default()
    );
    assert_eq!(m.pending(), [B]);
}
#[test]
fn disabled_and_malformed_input_do_not_mutate_or_claim_source_absence() {
    let s = scope();
    let mut m = Manager::new(s.clone());
    let mut obs = state();
    m.mark_fresh(&s, now(1000), obs, &[fresh(A, RowKind::Existing)])
        .unwrap();
    obs.enabled = false;
    obs.major_version = Some(2);
    assert!(m.is_fresh(&s, now(1000), obs, B).unwrap());
    assert_eq!(m.fresh_ids(&s, now(1000)).unwrap(), [A]);
    m.maybe_refetch(&s, now(1000), obs, B, false).unwrap();
    assert!(m.pending().is_empty());
    let invalid = Fresh {
        id: B.into(),
        kind: RowKind::Absent,
        entropy: 1.0,
    };
    assert!(
        m.mark_fresh(
            &s,
            now(1000),
            state(),
            &[fresh(B, RowKind::Existing), invalid]
        )
        .is_err()
    );
    assert_eq!(m.fresh_ids(&s, now(1000)).unwrap(), [A]);
    assert!(
        m.maybe_refetch(&s, now(1000), state(), "__proto__", false)
            .is_err()
    );
}

#[test]
fn old_pause_timer_can_clear_a_newer_pause_and_indefinite_pause() {
    let mut pause = TimedPause::default();
    pause.keypress(now(1000));
    pause.unpause(now(1500));
    pause.keypress(now(2000));
    assert!(pause.paused(now(2999)));
    assert!(!pause.paused(now(3000)));
    // Timer at4000 was orphaned when the older timer cleared the handle.
    pause.indefinitely(now(3500));
    assert!(pause.paused(now(3999)));
    assert!(!pause.paused(now(4000)));
    pause.indefinitely(now(5000));
    assert!(pause.paused(now(6000)));
    pause.unpause(now(6000));
    assert!(!pause.paused(now(6000)));
    let mut other = TimedPause::default();
    other.keypress(now(1000));
    other.unpause(now(1500));
    other.indefinitely(now(2000));
    assert!(other.paused(now(2999)));
    assert!(!other.paused(now(3000)));
    // Equal-deadline timers remain distinct when only one handle is cancelled.
    let mut same_time = TimedPause::default();
    same_time.keypress(now(1000));
    same_time.unpause(now(1000));
    same_time.keypress(now(1000));
    same_time.keypress(now(2000));
    assert!(!same_time.paused(now(3000)));
    // An ordinary replacement cancels its still-owned predecessor timer.
    let mut replaced = TimedPause::default();
    replaced.keypress(now(1000));
    replaced.keypress(now(2000));
    assert!(replaced.paused(now(3000)));
    assert!(!replaced.paused(now(4000)));
}
#[test]
fn indexed_large_inventory_preserves_update_delete_reinsert_and_pending_order() {
    let s = scope();
    let mut m = Manager::new(s.clone());
    let rows: Vec<_> = (0..10_000)
        .map(|i| fresh(&format!("C{i:08}"), RowKind::Existing))
        .collect();
    m.mark_fresh(&s, now(1000), state(), &rows).unwrap();
    m.mark_fresh(
        &s,
        now(1000),
        state(),
        &rows.iter().rev().cloned().collect::<Vec<_>>(),
    )
    .unwrap();
    let ids: Vec<_> = rows.iter().map(|r| r.id.clone()).collect();
    assert_eq!(m.fresh_ids(&s, now(1000)).unwrap(), ids);
    let removed = vec![ids[3].clone(), ids[1].clone()];
    let mut bursts = InvalidationBursts::default();
    assert_eq!(
        m.mark_stale(&s, now(1000), state(), &removed, &mut bursts)
            .unwrap()
            .invalidated,
        removed
    );
    m.mark_fresh(&s, now(1000), state(), &[rows[3].clone(), rows[1].clone()])
        .unwrap();
    let expected: Vec<_> = ids
        .iter()
        .filter(|id| !removed.contains(id))
        .cloned()
        .chain(removed.clone())
        .collect();
    assert_eq!(m.fresh_ids(&s, now(1000)).unwrap(), expected);
    let mut queued = Manager::new(s.clone());
    for id in ids.iter().chain(ids.iter()) {
        queued
            .maybe_refetch(&s, now(1000), state(), id, false)
            .unwrap();
    }
    assert_eq!(queued.pending(), ids);
    let batch = queued.begin_due(&s, now(1000), state()).unwrap().unwrap();
    for id in &ids {
        queued
            .maybe_refetch(&s, now(1000), state(), id, false)
            .unwrap();
    }
    queued
        .finish(&s, &batch, now(1000), state(), &rows)
        .unwrap();
    assert!(queued.pending().is_empty());
    assert!(queued.pending_index.is_empty());
    assert_eq!(queued.fresh_ids(&s, now(1000)).unwrap(), ids);
}

// Test-only event-loop owner. Production Pause never allocates timer IDs or
// assumes an elapsed deadline proves that a callback was actually delivered.
struct TimedPause {
    pause: Pause,
    next: u64,
    timers: std::collections::BTreeMap<u64, u64>,
}
impl Default for TimedPause {
    fn default() -> Self {
        Self {
            pause: Pause::default(),
            next: 100,
            timers: Default::default(),
        }
    }
}
impl TimedPause {
    fn advance(&mut self, now: Clock) {
        let mut due: Vec<_> = self
            .timers
            .iter()
            .filter(|(_, at)| **at <= now.millis())
            .map(|(id, at)| (*at, *id))
            .collect();
        due.sort();
        for (_, id) in due {
            self.timers.remove(&id);
            self.pause.timer_fired();
        }
    }
    fn keypress(&mut self, now: Clock) {
        self.advance(now);
        let cancel = self.pause.cancellation_handle();
        if let Some(cancel) = cancel {
            self.timers.remove(&cancel.value());
        }
        self.next += 1;
        let id = TimerId::new(self.next).unwrap();
        assert_eq!(self.pause.keypress(id), cancel);
        self.timers
            .insert(id.value(), now.millis() + Pause::DELAY_MS);
    }
    fn indefinitely(&mut self, now: Clock) {
        self.advance(now);
        if let Some(cancel) = self.pause.indefinitely() {
            self.timers.remove(&cancel.value());
        }
    }
    fn unpause(&mut self, now: Clock) {
        self.advance(now);
        self.pause.unpause();
    }
    fn paused(&mut self, now: Clock) -> bool {
        self.advance(now);
        self.pause.paused()
    }
}
#[test]
fn actual_timer_identity_sentinel_collision_and_noncollision() {
    assert!(TimerId::new(0).is_err());
    for (initial_id, expected) in [(0, true), (100, false)] {
        let mut p = TimedPause {
            next: initial_id,
            ..Default::default()
        };
        p.keypress(now(1000));
        p.unpause(now(1500));
        p.indefinitely(now(2000));
        // Second indefinite call cancels raw sentinel1. This cancels the orphan
        // only when the actual timer owner allocated identity1 to that orphan.
        p.indefinitely(now(2500));
        assert_eq!(p.paused(now(3000)), expected);
    }
}

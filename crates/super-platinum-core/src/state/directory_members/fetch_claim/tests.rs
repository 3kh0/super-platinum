use super::*;
fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).into()).collect()
}
fn dispatch(m: &mut Manager<usize>, now: u64) -> Dispatch {
    let w = m.arm(now).unwrap().unwrap();
    m.dispatch(&w, now).unwrap()
}
#[test]
fn overlapping_consumers_merge_during_the_one_owned_wake() {
    let mut m = Manager::<usize>::default();
    let a = m.register(&ids(&["U00000001"])).unwrap();
    let wake = m.arm(1000).unwrap().unwrap();
    assert_eq!(wake.due_ms(), 1000);
    let b = m
        .register(&ids(&["U00000001", "U00000002", "U00000002"]))
        .unwrap();
    assert!(m.arm(1000).unwrap().is_none());
    let d = m.dispatch(&wake, 1000).unwrap();
    assert_eq!(d.ids(), ids(&["U00000001", "U00000002"]));
    assert!(m.dispatch(&wake, 1000).is_err());
    assert_eq!(m.status(&a), Status::Pending);
    assert_eq!(m.status(&b), Status::Pending);
}
#[test]
fn cancelled_wake_does_not_strand_ids_or_spend_dispatch_cadence() {
    let mut m = Manager::<usize>::default();
    m.register(&ids(&["U00000001"])).unwrap();
    let old = m.arm(1000).unwrap().unwrap();
    m.cancel_wake(&old).unwrap();
    let fresh = m.arm(1050).unwrap().unwrap();
    assert_eq!(fresh.due_ms(), 1050);
    assert!(m.dispatch(&old, 1050).is_err());
    assert_eq!(m.dispatch(&fresh, 1050).unwrap().ids(), ids(&["U00000001"]));
}
#[test]
fn inflight_duplicate_attaches_and_later_id_waits_for_next_dispatch() {
    let mut m = Manager::<usize>::default();
    m.register(&ids(&["U00000001", "U00000002"])).unwrap();
    let d = dispatch(&mut m, 1000);
    let sibling = m.register(&ids(&["U00000001", "U00000003"])).unwrap();
    assert!(m.arm(1050).unwrap().is_none());
    m.publish(&d, 7).unwrap();
    let (adoption, r) = m.claim_adoption(&sibling).unwrap().unwrap();
    assert_eq!(*r, 7);
    assert!(m.arm(1050).unwrap().is_none());
    // Private finish models the canonical installation checked by public settle.
    m.finish(&adoption);
    assert_eq!(m.status(&sibling), Status::Pending);
    let wake = m.arm(1050).unwrap().unwrap();
    assert_eq!(wake.due_ms(), 1200);
    assert!(m.dispatch(&wake, 1199).is_err());
    assert_eq!(m.dispatch(&wake, 1200).unwrap().ids(), ids(&["U00000003"]));
}
#[test]
fn sibling_retirement_transfers_adoption_without_another_fetch() {
    let mut m = Manager::<usize>::default();
    let first = m.register(&ids(&["U00000001"])).unwrap();
    let sibling = m.register(&ids(&["U00000001"])).unwrap();
    let d = dispatch(&mut m, 1000);
    m.retire(&first);
    assert!(m.dispatch_current(&d));
    m.publish(&d, 42).unwrap();
    let (lease, _) = m.claim_adoption(&sibling).unwrap().unwrap();
    let third = m.register(&ids(&["U00000001"])).unwrap();
    m.retire(&sibling);
    assert!(!m.adoption_current(&lease));
    let (next, r) = m.claim_adoption(&third).unwrap().unwrap();
    assert_eq!(*r, 42);
    assert!(m.release_adoption(&lease).is_err());
    assert!(m.arm(2000).unwrap().is_none());
    m.finish(&next);
    assert_eq!(m.status(&third), Status::Complete);
}
#[test]
fn receipt_survives_no_subscribers_until_a_current_consumer_can_adopt() {
    let mut m = Manager::<usize>::default();
    let a = m.register(&ids(&["U00000001"])).unwrap();
    let d = dispatch(&mut m, 1000);
    m.retire(&a);
    m.publish(&d, 3).unwrap();
    let b = m.register(&ids(&["U00000002"])).unwrap();
    let (lease, _) = m.claim_adoption(&b).unwrap().unwrap();
    assert_eq!(lease.ids(), ids(&["U00000001"]));
    m.finish(&lease);
    let wake = m.arm(1400).unwrap().unwrap();
    assert_eq!(wake.due_ms(), 1400);
    assert_eq!(m.dispatch(&wake, 1400).unwrap().ids(), ids(&["U00000002"]));
}
#[test]
fn separate_instances_and_replaced_context_cannot_reuse_tickets() {
    let mut one = Manager::<usize>::default();
    let mut two = Manager::<usize>::default();
    let a = one.register(&ids(&["U00000001"])).unwrap();
    two.register(&ids(&["U00000001"])).unwrap();
    let d = dispatch(&mut one, 1000);
    let other = dispatch(&mut two, 1000);
    assert!(two.publish(&d, 1).is_err());
    assert!(one.publish(&other, 1).is_err());
    assert_eq!(two.status(&a), Status::Retired);
    let mut replacement = Manager::<usize>::default();
    assert!(replacement.publish(&d, 1).is_err());
}
#[test]
fn timeout_retries_exactly_three_and_non_timeout_refusal_never_publishes_missing() {
    let mut m = Manager::<usize>::default();
    let a = m.register(&ids(&["U00000001"])).unwrap();
    let d = dispatch(&mut m, 0);
    assert_eq!(m.timeout(&d).unwrap(), Timeout::Retry);
    assert_eq!(m.timeout(&d).unwrap(), Timeout::Retry);
    assert_eq!(m.timeout(&d).unwrap(), Timeout::Exhausted);
    assert_eq!(ATTEMPT_TIMEOUT_MS, 65000);
    m.refuse(&d).unwrap();
    assert_eq!(m.status(&a), Status::Retired);
    assert!(m.publish(&d, 0).is_err());
    assert!(m.arm(300).unwrap().is_none());
}
#[test]
fn local_receipt_refusal_preserves_newer_queued_ids() {
    let mut m = Manager::<usize>::default();
    let old = m.register(&ids(&["U00000001"])).unwrap();
    let d = dispatch(&mut m, 1000);
    let newer = m.register(&ids(&["U00000002"])).unwrap();
    m.publish(&d, 9).unwrap();
    m.refuse_receipt(&d).unwrap();
    assert_eq!(m.status(&old), Status::Retired);
    assert_eq!(m.status(&newer), Status::Pending);
    let wake = m.arm(1500).unwrap().unwrap();
    assert_eq!(m.dispatch(&wake, 1500).unwrap().ids(), ids(&["U00000002"]));
}
#[test]
fn duplicate_publication_and_release_of_old_adoption_are_refused() {
    let mut m = Manager::<usize>::default();
    let a = m.register(&ids(&["U00000001"])).unwrap();
    let d = dispatch(&mut m, 0);
    m.publish(&d, 1).unwrap();
    assert!(m.publish(&d, 2).is_err());
    let (lease, _) = m.claim_adoption(&a).unwrap().unwrap();
    assert!(m.claim_adoption(&a).unwrap().is_none());
    m.release_adoption(&lease).unwrap();
    let (next, _) = m.claim_adoption(&a).unwrap().unwrap();
    assert!(!m.adoption_current(&lease));
    assert!(m.adoption_current(&next));
}
#[test]
fn settlement_requires_installed_source_and_exact_native_not_a_reported_bool() {
    use crate::state::directory_members::{Identity, Persisted, Provenance, Record};
    use serde_json::json;
    let owner = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U00000009".into(),
        name: "fixture".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    };
    let user: crate::slack::models::User =
        serde_json::from_value(json!({"id":"U00000001","name":"New"})).unwrap();
    let witness = serde_json::to_value(&user).unwrap();
    let mut store = Store::default();
    store.saved = Some(Persisted {
        version: 1,
        identity: Identity::capture("A", &owner).unwrap(),
        provenance: Provenance::EmptySessionIngress,
        records: vec![Record {
            native: json!({"id":"U00000001"}),
            source: json!({"id":"U00000001"}),
            interaction: None,
            wire: json!({"id":"U00000001"}),
            native_witness: witness,
        }],
        pending: vec![],
        invalidated: BTreeSet::new(),
        overflowed: false,
        disconnected: BTreeMap::new(),
    });
    let mut m = Manager::<usize>::default();
    let a = m.register(&ids(&["U00000001"])).unwrap();
    let d = dispatch(&mut m, 0);
    m.publish(&d, 1).unwrap();
    let (lease, _) = m.claim_adoption(&a).unwrap().unwrap();
    assert!(
        m.settle(&lease, &store, "A", &owner, &HashMap::new())
            .is_err()
    );
    assert!(m.adoption_current(&lease));
    let mut native = HashMap::from([("U00000001".into(), user)]);
    native.get_mut("U00000001").unwrap().name = Some("Old".into());
    assert!(m.settle(&lease, &store, "A", &owner, &native).is_err());
    native.get_mut("U00000001").unwrap().name = Some("New".into());
    // No account, stale-source or unknown placeholder can finish another
    // subscription merely because a matching typed native row exists.
    assert!(m.settle(&lease, &store, "OTHER", &owner, &native).is_err());
    let mut foreign = owner.clone();
    foreign.user_id = "U00000008".into();
    assert!(m.settle(&lease, &store, "A", &foreign, &native).is_err());
    for kind in 0..3 {
        let mut unavailable = store.clone();
        let saved = unavailable.saved.as_mut().unwrap();
        match kind {
            0 => {
                saved.invalidated.insert("U00000001".into());
            }
            1 => saved.overflowed = true,
            _ => saved.records[0].source["isUnknown"] = json!(true),
        }
        assert!(
            m.settle(&lease, &unavailable, "A", &owner, &native)
                .is_err()
        );
        assert!(m.adoption_current(&lease));
    }
    // Empty-row source effects still intersect the batch and must settle first.
    for kind in 0..3 {
        let mut pending = store.clone();
        if kind == 0 {
            pending
                .enqueue(
                    "A",
                    &owner,
                    crate::state::directory_members::Origin::Directory,
                    vec![],
                    json!({"U00000001":false}),
                )
                .unwrap();
        } else {
            pending
                .enqueue_member_info(
                    "A",
                    &owner,
                    vec![],
                    json!({}),
                    ids(&["U00000001"]),
                    if kind == 2 {
                        ids(&["U00000001"])
                    } else {
                        vec![]
                    },
                )
                .unwrap();
        }
        assert!(m.settle(&lease, &pending, "A", &owner, &native).is_err());
        assert!(m.adoption_current(&lease));
    }
    // An unrelated pending effect must not block this canonical batch.
    store
        .enqueue_member_info(
            "A",
            &owner,
            vec![],
            json!({"U00000002":true}),
            ids(&["U00000002"]),
            ids(&["U00000002"]),
        )
        .unwrap();
    m.settle(&lease, &store, "A", &owner, &native).unwrap();
    assert_eq!(m.status(&a), Status::Complete);
    assert!(m.settle(&lease, &store, "A", &owner, &native).is_err());
}

#[test]
fn completed_subset_keeps_deferred_ids_ahead_of_later_requests_and_not_complete() {
    let mut m = Manager::<usize>::default();
    let original = m
        .register(&ids(&["U00000001", "U00000002", "U00000003"]))
        .unwrap();
    let d = dispatch(&mut m, 1000);
    let later = m.register(&ids(&["U00000004", "U00000002"])).unwrap();
    assert!(
        m.publish_completed(&d, &ids(&["U00000003", "U00000001"]), |_| 1)
            .is_err()
    );
    assert!(
        m.publish_completed(&d, &ids(&["U00000009"]), |_| 1)
            .is_err()
    );
    assert!(m.dispatch_current(&d));
    assert!(
        m.publish_completed(&d, &ids(&["U00000001", "U00000001"]), |_| 1)
            .is_err()
    );
    assert!(m.dispatch_current(&d));
    m.publish_completed(&d, &ids(&["U00000001", "U00000003"]), |subset| {
        assert_eq!(subset.ids(), ids(&["U00000001", "U00000003"]));
        7
    })
    .unwrap();
    let (lease, received) = m.claim_adoption(&original).unwrap().unwrap();
    assert_eq!(*received, 7);
    m.finish(&lease);
    assert_eq!(m.status(&original), Status::Pending);
    assert_eq!(m.status(&later), Status::Pending);
    let wake = m.arm(1300).unwrap().unwrap();
    assert_eq!(
        m.dispatch(&wake, 1300).unwrap().ids(),
        ids(&["U00000002", "U00000004"])
    );
}
#[test]
fn empty_completed_scope_defers_without_missing_or_subscriber_retirement() {
    let mut m = Manager::<usize>::default();
    let sub = m.register(&ids(&["U00000001"])).unwrap();
    let d = dispatch(&mut m, 1000);
    m.register(&ids(&["U00000002"])).unwrap();
    m.defer(&d).unwrap();
    assert_eq!(m.status(&sub), Status::Pending);
    assert!(m.claim_adoption(&sub).unwrap().is_none());
    assert!(m.defer(&d).is_err());
    let wake = m.arm(1300).unwrap().unwrap();
    assert_eq!(
        m.dispatch(&wake, 1300).unwrap().ids(),
        ids(&["U00000001", "U00000002"])
    );
}

#[test]
fn cadence_uses_actual_dispatch_and_long_operations_have_no_extra_delay() {
    let mut m = Manager::<usize>::default();
    assert!(m.arm(0).unwrap().is_none());
    let first = m.register(&ids(&["U00000001"])).unwrap();
    let wake = m.arm(0).unwrap().unwrap();
    assert_eq!(wake.due_ms(), 0);
    // A delayed callback spends cadence at dispatch, not registration or arm.
    let active = m.dispatch(&wake, 1000).unwrap();
    m.publish(&active, 1).unwrap();
    let (adoption, _) = m.claim_adoption(&first).unwrap().unwrap();
    m.finish(&adoption);
    let second_sub = m.register(&ids(&["U00000002"])).unwrap();
    let next = m.arm(1100).unwrap().unwrap();
    assert_eq!(next.due_ms(), 1200);
    let second = m.dispatch(&next, 1200).unwrap();
    m.publish(&second, 2).unwrap();
    let (adoption, _) = m.claim_adoption(&second_sub).unwrap().unwrap();
    m.finish(&adoption);
    m.register(&ids(&["U00000003"])).unwrap();
    let late = m.arm(5000).unwrap().unwrap();
    assert_eq!(late.due_ms(), 5000);
    assert_eq!(m.dispatch(&late, 5000).unwrap().ids(), ids(&["U00000003"]));
}

#[test]
fn manager_snapshots_all_ids_lower_endpoint_chunk_limit_is_not_manager_limit() {
    let mut m = Manager::<usize>::default();
    let requested: Vec<_> = (1..=201).map(|n| format!("U{n:08}")).collect();
    let sub = m.register(&requested).unwrap();
    let active = dispatch(&mut m, 0);
    assert_eq!(active.ids(), requested);
    m.register(&requested).unwrap();
    assert!(m.arm(1000).unwrap().is_none());
    m.defer(&active).unwrap();
    assert_eq!(m.status(&sub), Status::Pending);
    let next = m.arm(1000).unwrap().unwrap();
    assert_eq!(m.dispatch(&next, 1000).unwrap().ids(), requested);
}

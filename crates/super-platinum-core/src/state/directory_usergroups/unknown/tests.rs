use super::*;
use serde_json::json;
const ID: &str = "SGROUP001";
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TGROUP001".into(),
        user_id: "U_SELF".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    }
}
fn scope() -> Scope {
    Scope::new("account", &owner(), 1, 7).unwrap()
}
fn ready() -> (Store, Native) {
    let store = Store::fresh();
    let native = Native::new();
    store.observe_boot(store.begin_boot(&native).unwrap(), "account", &owner(), 1, &native,
        &json!({"ok":true,"team":{"id":"TGROUP001"},"self":{"id":"U_SELF"},"subteams":{"self":[]}})).unwrap().into_parts()
}
fn finish(store: &Store, native: &Native, ticket: &Ticket) -> Commit {
    let pending = store
        .next_for(ticket, "account", &owner(), 1, native)
        .unwrap()
        .unwrap();
    let reply = RendererReply {
        generation: pending.generation().into(),
        fields: pending.fields().to_vec(),
        lowered: pending.fields().iter().map(|s| s.to_lowercase()).collect(),
        locale: (!pending.fields().is_empty()).then(|| "en-US".into()),
    };
    store
        .commit(pending, reply, "account", &owner(), 1, native)
        .unwrap()
}
fn apply(store: &mut Store, native: &mut Native, op: Operation) {
    let ticket = store
        .enqueue("account", &owner(), 1, "fixture", native, op)
        .unwrap();
    (*store, *native) = finish(store, native, &ticket).into_parts();
}
fn rows(store: &mut Store, native: &mut Native, values: Vec<Value>) {
    apply(
        store,
        native,
        Operation::Metadata {
            origin: Origin::Upsert,
            rows: values,
            native_members: NativeMembers::Preserve,
        },
    );
}
fn manager(store: &Store, native: &Native) -> (Manager, Lookup, Flight) {
    let mut m = Manager::new(scope());
    let lookup = m.lookup(&scope(), store, native, ID).unwrap();
    let flight = m.begin(&scope(), Duration::ZERO).unwrap().unwrap();
    (m, lookup, flight)
}
#[test]
fn original_eight_manager_vectors_pin_transient_and_exact_unresolved_ids() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 8);
    for case in oracle["cases"].as_array().unwrap() {
        let (mut s, mut n) = ready();
        let (mut m, first, f) = manager(&s, &n);
        assert_eq!(first.effective(), &case["before"]["placeholder"]);
        let second = m.lookup(&scope(), &s, &n, ID).unwrap();
        assert!(std::ptr::eq(first.effective(), second.effective()));
        assert_eq!(m.pending(), [ID]);
        // These are the original wrapper's returned IDs. Source processor
        // composition for a retained unknown is independently tested, not
        // mislabelled as the original raw replacement host's final snapshot.
        let terminal = matches!(
            case["name"].as_str().unwrap(),
            "absent-success-omission" | "absent-wrapper-error"
        );
        let returned = if terminal {
            vec![]
        } else {
            vec![ID.to_owned()]
        };
        let settlement = m.settlement(&scope(), &f, &s, &n, &returned).unwrap();
        if terminal {
            assert_eq!(settlement.missing().ids(), [ID]);
            let ticket = s
                .enqueue_missing(settlement.missing(), &n, "terminal")
                .unwrap();
            let pending_raw = match &s.queue[0].operation {
                Operation::Metadata { rows, .. } => rows,
                _ => panic!("metadata"),
            };
            assert_eq!(
                pending_raw,
                case["events"][0]["userGroups"].as_array().unwrap()
            );
            (s, n) = finish(&s, &n, &ticket).into_parts();
            assert_eq!(
                m.lookup(&scope(), &s, &n, ID).unwrap().kind(),
                LookupKind::NonExistent
            );
        } else {
            assert!(settlement.missing().ids().is_empty());
        }
        m.published(&scope(), settlement).unwrap();
        assert!(m.pending().is_empty());
        assert!(!first.current(&scope(), &s, &n));
    }
}
#[test]
fn observed_absence_is_not_cold_cache_or_membership_presence() {
    let mut m = Manager::new(scope());
    let n = Native::new();
    assert!(m.lookup(&scope(), &Store::default(), &n, ID).is_err());
    assert!(m.lookup(&scope(), &Store::fresh(), &n, ID).is_err());
    let (mut s, mut n) = ready();
    apply(
        &mut s,
        &mut n,
        Operation::Membership {
            ids: vec![ID.into()],
        },
    );
    assert!(n.contains_key(ID));
    assert!(n[ID].label_source.is_none());
    assert!(Lookup::observed(&scope(), &s, &n, ID).is_err());
    assert!(m.lookup_if(&scope(), &s, &n, ID, false).unwrap().is_none());
    assert!(m.pending().is_empty());
    let lookup = m.lookup(&scope(), &s, &n, ID).unwrap();
    assert!(lookup.is_transient());
    assert!(lookup.current(&scope(), &s, &n));
    let restored = Store::restore(Some(&s.save()), &n);
    assert!(m.lookup(&scope(), &restored, &n, ID).is_err());
}
#[test]
fn existing_effective_markers_and_partial_handles_are_source_hits() {
    let (mut s, mut n) = ready();
    rows(
        &mut s,
        &mut n,
        vec![json!({"id":ID,"name":"First","handle":"before"})],
    );
    rows(&mut s, &mut n, vec![json!({"id":ID,"name":"Renamed"})]);
    assert!(n[ID].profile_label().is_none());
    let lookup = Lookup::observed(&scope(), &s, &n, ID).unwrap();
    assert_eq!(lookup.effective()["handle"], "before");
    assert_eq!(lookup.kind(), LookupKind::Known);
    let oracle: Value = serde_json::from_str(include_str!("processor-oracle.json")).unwrap();
    rows(&mut s, &mut n, vec![oracle["prior"].clone()]);
    rows(&mut s, &mut n, vec![oracle["raw"].clone()]);
    let mut m = Manager::new(scope());
    let lookup = m.lookup(&scope(), &s, &n, ID).unwrap();
    assert!(lookup.is_source_hit());
    assert!(m.pending().is_empty());
    assert_eq!(lookup.kind(), LookupKind::NonExistent);
    for (key, value) in oracle["canonical"].as_object().unwrap() {
        assert_eq!(&lookup.effective()[key], value);
    }
    assert_eq!(n[ID].label_source.as_ref(), Some(&oracle["raw"]));
    assert!(n[ID].profile_label().is_none());
}
#[test]
fn raw_flags_and_wrong_scope_cannot_mint_current_lookup() {
    let (s, mut n) = ready();
    let mut m = Manager::new(scope());
    n.insert(
        ID.into(),
        UserGroup::from_observed(&json!({"id":ID,"isNonExistent":true})).unwrap(),
    );
    assert!(m.lookup(&scope(), &s, &n, ID).is_err());
    let n = Native::new();
    let lookup = m.lookup(&scope(), &s, &n, ID).unwrap();
    for wrong in [
        Scope::new("other", &owner(), 1, 7).unwrap(),
        Scope::new("account", &owner(), 2, 7).unwrap(),
        Scope::new("account", &owner(), 1, 8).unwrap(),
    ] {
        assert!(!lookup.current(&wrong, &s, &n));
        assert!(m.lookup(&wrong, &s, &n, ID).is_err());
    }
    for id in ["", "Sshort", "C12345678", "S1234567-"] {
        assert!(m.lookup(&scope(), &s, &n, id).is_err());
    }
    let (other, other_n) = ready();
    assert!(!m.source_current(&scope(), &other, &other_n));
    drop(m);
    assert!(!lookup.current(&scope(), &s, &n));
}
#[test]
fn exact_terminal_queue_commit_and_successor_order_are_required() {
    let (mut s, mut n) = ready();
    let (mut m, _, f) = manager(&s, &n);
    let settlement = m.settlement(&scope(), &f, &s, &n, &[]).unwrap();
    let ticket = s
        .enqueue_missing(settlement.missing(), &n, "terminal")
        .unwrap();
    assert!(
        s.enqueue_missing(settlement.missing(), &n, "duplicate")
            .is_err()
    );
    // An accepted later operation remains after the exact terminal head.
    let later = s
        .enqueue(
            "account",
            &owner(),
            1,
            "later",
            &n,
            Operation::Metadata {
                origin: Origin::Upsert,
                rows: vec![json!({"id":"SGROUP002","handle":"later"})],
                native_members: NativeMembers::Preserve,
            },
        )
        .unwrap();
    assert!(
        s.next_for(&later, "account", &owner(), 1, &n)
            .unwrap()
            .is_none()
    );
    let commit = finish(&s, &n, &ticket);
    assert!(
        !settlement
            .flight
            .completed
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .load(Ordering::Acquire)
    );
    (s, n) = commit.into_parts();
    m.published(&scope(), settlement).unwrap();
    (s, n) = finish(&s, &n, &later).into_parts();
    assert_eq!(
        Lookup::observed(&scope(), &s, &n, ID).unwrap().kind(),
        LookupKind::NonExistent
    );
    assert_eq!(n["SGROUP002"].handle, "later");
}
#[test]
fn snapshot_drift_and_cancelled_flight_cannot_publish_terminal() {
    let (mut s, mut n) = ready();
    let (mut m, _, f) = manager(&s, &n);
    let settlement = m.settlement(&scope(), &f, &s, &n, &[]).unwrap();
    rows(&mut s, &mut n, vec![json!({"id":ID,"handle":"new"})]);
    assert!(s.enqueue_missing(settlement.missing(), &n, "late").is_err());
    assert!(m.cancel(&scope(), &f));
    let f = m.begin(&scope(), CADENCE).unwrap().unwrap();
    let settlement = m.settlement(&scope(), &f, &s, &n, &[]).unwrap();
    let ticket = s
        .enqueue_missing(settlement.missing(), &n, "terminal")
        .unwrap();
    let pending = s
        .next_for(&ticket, "account", &owner(), 1, &n)
        .unwrap()
        .unwrap();
    assert!(m.cancel(&scope(), &f));
    assert!(!pending.current(&s, "account", &owner(), 1, &n));
    assert!(m.published(&scope(), settlement).is_err());
    drop(pending);
    drop(ticket);
    assert!(s.snapshot("account", &owner(), 1, &n).is_err());
}
#[test]
fn timeout_only_retries_are_bounded_and_pending_successor_survives() {
    let (s, n) = ready();
    let (mut m, _, a) = manager(&s, &n);
    let b = m.retry_timeout(&scope(), &a).unwrap().unwrap();
    assert_eq!(b.attempt(), 2);
    assert!(!m.flight_current(&scope(), &a));
    let c = m.retry_timeout(&scope(), &b).unwrap().unwrap();
    assert_eq!(c.attempt(), 3);
    assert!(m.retry_timeout(&scope(), &c).unwrap().is_none());
    m.lookup(&scope(), &s, &n, "SGROUP002").unwrap();
    let done = m.settlement(&scope(), &c, &s, &n, &[ID.into()]).unwrap();
    m.published(&scope(), done).unwrap();
    assert_eq!(m.pending(), ["SGROUP002"]);
    assert!(
        m.begin(&scope(), Duration::from_millis(199))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        m.begin(&scope(), CADENCE).unwrap().unwrap().ids(),
        ["SGROUP002"]
    );
}

#[test]
fn reservation_without_commit_and_dropped_manager_never_finish_or_publish() {
    let (mut s, n) = ready();
    let (mut m, _, flight) = manager(&s, &n);
    let settlement = m.settlement(&scope(), &flight, &s, &n, &[]).unwrap();
    assert!(m.published(&scope(), settlement).is_err());
    assert_eq!(m.pending(), [ID]);
    assert!(m.cancel(&scope(), &flight));
    let flight = m.begin(&scope(), CADENCE).unwrap().unwrap();
    let settlement = m.settlement(&scope(), &flight, &s, &n, &[]).unwrap();
    let ticket = s
        .enqueue_missing(settlement.missing(), &n, "queued")
        .unwrap();
    drop(m);
    assert!(s.next_for(&ticket, "account", &owner(), 1, &n).is_err());
    drop(ticket);
    assert!(s.snapshot("account", &owner(), 1, &n).is_err());
}
#[test]
fn accepted_queue_tail_and_credential_rebind_retire_premount_lookup() {
    let (mut s, n) = ready();
    let (mut m, lookup, _) = manager(&s, &n);
    let mut changed = owner();
    changed.token = "different-synthetic".into();
    let changed = Scope::new("account", &changed, 1, 7).unwrap();
    assert!(!m.current(&changed));
    assert!(!lookup.current(&changed, &s, &n));
    let ticket = s
        .enqueue(
            "account",
            &owner(),
            1,
            "later",
            &n,
            Operation::Membership { ids: vec![] },
        )
        .unwrap();
    assert!(!lookup.current(&scope(), &s, &n));
    assert!(m.lookup(&scope(), &s, &n, ID).is_err());
    drop(ticket);
    assert!(!m.source_current(&scope(), &s, &n));
}

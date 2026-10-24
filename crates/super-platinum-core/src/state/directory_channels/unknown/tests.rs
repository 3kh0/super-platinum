use super::*;
use serde_json::json;

const ID: &str = "C00000001";
const OTHER: &str = "C00000002";
fn owner() -> WorkspaceSession {
    serde_json::from_value(
        json!({"team_id":"TOWNER","user_id":"USELF","name":"Fixture",
        "token":"synthetic","url":"https://fixture.slack.com"}),
    )
    .unwrap()
}
fn setup() -> (Store, BTreeMap<String, Channel>, Scope, Manager) {
    let mut store = Store::default();
    let native = BTreeMap::new();
    let boot = store.begin_full_boot("A", &owner(), 1).unwrap();
    assert!(store.finish_full_boot(boot, "A", &owner(), 1));
    store.begin("A", &owner(), &native).unwrap();
    let scope = Scope::new("A", &owner(), 1, 7).unwrap();
    let manager = Manager::new(scope.clone());
    (store, native, scope, manager)
}
fn observe(store: &Store, native: &BTreeMap<String, Channel>) -> Arc<Observation> {
    Arc::new(store.observation("A", &owner(), 1, "q", native).unwrap())
}
fn lookup(
    manager: &mut Manager,
    scope: &Scope,
    store: &Store,
    native: &BTreeMap<String, Channel>,
    id: &str,
) -> Lookup {
    manager
        .lookup(scope, store, observe(store, native), "q", native, id, true)
        .unwrap()
}
fn facts() -> Facts {
    Facts {
        self_id: "USELF".into(),
        current_team: "TOWNER".into(),
        omit_team: "TOWNER".into(),
        retain_public_membership_on_archive: false,
        members: BTreeMap::new(),
    }
}
fn drain(store: &mut Store, native: &mut BTreeMap<String, Channel>) {
    while store.has_pending() {
        let observed = observe(store, native);
        if let Some(commit) = store
            .drain_source_action(&observed, "A", &owner(), 1, "q", native)
            .unwrap()
        {
            (*store, *native) = commit.into_parts();
            continue;
        }
        let facts = facts();
        let plan = store
            .prepare("A", &owner(), 1, "q", native, facts.clone())
            .unwrap();
        let reply = RendererReply {
            generation: "q".into(),
            fields: plan.fields().to_vec(),
            values: plan
                .fields()
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .collect(),
        };
        (*store, *native) = store
            .finish(plan, &facts, reply, "A", &owner(), 1, "q", native)
            .unwrap()
            .into_parts();
    }
}
fn raw(store: &mut Store, id: &str, name: &str) {
    store
        .enqueue(
            "A",
            &owner(),
            1,
            Origin::Direct,
            vec![json!({"id":id,"name":name,"is_channel":true,"is_member":false})],
        )
        .unwrap();
}

#[test]
fn immediate_unknown_is_owned_transient_deduplicated_and_not_canonical() {
    let (store, native, scope, mut manager) = setup();
    let first = lookup(&mut manager, &scope, &store, &native, ID);
    let duplicate = lookup(&mut manager, &scope, &store, &native, ID);
    assert_eq!(first.kind(), LookupKind::Unknown);
    assert!(first.is_transient());
    assert!(first.row().is_none());
    assert!(first.current(&manager, &scope, &store, "q", &native));
    assert!(duplicate.current(&manager, &scope, &store, "q", &native));
    assert_eq!(manager.pending(), &[ID.to_owned()]);
    assert!(
        store
            .ready("A", &owner(), 1, "q", &native)
            .unwrap()
            .rows()
            .is_empty()
    );
    assert!(!store.has_pending());
    let no_fetch = manager
        .lookup(
            &scope,
            &store,
            observe(&store, &native),
            "q",
            &native,
            OTHER,
            false,
        )
        .unwrap();
    assert_eq!(no_fetch.kind(), LookupKind::Unobserved);
    for id in ["", "bad", "Cshort", "C0000000!"] {
        assert_eq!(
            lookup(&mut manager, &scope, &store, &native, id).kind(),
            LookupKind::Unobserved
        );
    }
    assert!(
        manager
            .lookup(
                &scope,
                &store,
                observe(&store, &native),
                "q",
                &native,
                "Z00000001",
                true
            )
            .is_err()
    );
    assert_eq!(manager.pending().len(), 1);
}

#[test]
fn omission_and_wrapper_error_share_sealed_terminal_not_wire_absence() {
    // Original wrapper maps ordinary rejection to the same empty result as omission.
    for _wrapper_error in [false, true] {
        let (mut store, mut native, scope, mut manager) = setup();
        let transient = lookup(&mut manager, &scope, &store, &native, ID);
        let flight = manager.begin(&scope, Duration::ZERO).unwrap().unwrap();
        let settlement = manager.settlement(&scope, &flight, &[]).unwrap();
        assert!(manager.settlement(&scope, &flight, &[]).is_err());
        let observed = observe(&store, &native);
        store
            .enqueue_runtime_missing(&observed, &scope, "q", &native, settlement.missing())
            .unwrap();
        assert!(
            store
                .enqueue_runtime_missing(
                    &observe(&store, &native),
                    &scope,
                    "q",
                    &native,
                    settlement.missing()
                )
                .is_err()
        );
        manager.published(&scope, settlement).unwrap();
        assert!(!transient.current(&manager, &scope, &store, "q", &native));
        drain(&mut store, &mut native);
        let result = lookup(&mut manager, &scope, &store, &native, ID);
        assert_eq!(result.kind(), LookupKind::NonExistent);
        assert!(result.row().is_none());
        assert!(result.current(&manager, &scope, &store, "q", &native));
        let cached = store.cached().unwrap();
        assert_eq!(cached["records"][0]["runtime_missing"], true);
        assert!(cached["records"][0]["wire"].is_null());
        assert!(manager.pending().is_empty());
    }
}

#[test]
fn timeout_only_retries_retire_old_attempt_and_stop_at_three() {
    let (mut store, native, scope, mut manager) = setup();
    lookup(&mut manager, &scope, &store, &native, ID);
    let one = manager.begin(&scope, Duration::ZERO).unwrap().unwrap();
    let two = manager.retry_timeout(&scope, &one).unwrap().unwrap();
    assert_eq!(two.attempt(), 2);
    assert!(manager.settlement(&scope, &one, &[]).is_err());
    let three = manager.retry_timeout(&scope, &two).unwrap().unwrap();
    assert_eq!(three.attempt(), 3);
    assert!(manager.retry_timeout(&scope, &three).unwrap().is_none());
    let final_result = manager.settlement(&scope, &three, &[]).unwrap();
    store
        .enqueue_runtime_missing(
            &observe(&store, &native),
            &scope,
            "q",
            &native,
            final_result.missing(),
        )
        .unwrap();
    manager.published(&scope, final_result).unwrap();
    assert_eq!(
        manager.remaining(Duration::from_millis(50)),
        Duration::from_millis(150)
    );
}

#[test]
fn pending_new_id_survives_old_flight_and_cancel_cannot_publish() {
    let (mut store, native, scope, mut manager) = setup();
    let old = lookup(&mut manager, &scope, &store, &native, ID);
    let flight = manager.begin(&scope, Duration::ZERO).unwrap().unwrap();
    lookup(&mut manager, &scope, &store, &native, OTHER);
    let settlement = manager.settlement(&scope, &flight, &[]).unwrap();
    assert!(manager.cancel(&scope, &flight));
    assert!(!old.current(&manager, &scope, &store, "q", &native));
    assert!(
        store
            .enqueue_runtime_missing(
                &observe(&store, &native),
                &scope,
                "q",
                &native,
                settlement.missing()
            )
            .is_err()
    );
    assert!(!store.has_pending());
    // Cancellation keeps work registered but retires the old lookup capability.
    let replacement = lookup(&mut manager, &scope, &store, &native, ID);
    assert!(replacement.current(&manager, &scope, &store, "q", &native));
    let next = manager.begin(&scope, CADENCE).unwrap().unwrap();
    assert_eq!(next.ids(), &[ID.to_owned(), OTHER.to_owned()]);
}

#[test]
fn positive_return_and_later_registered_id_preserve_source() {
    let (mut store, mut native, scope, mut manager) = setup();
    lookup(&mut manager, &scope, &store, &native, ID);
    let flight = manager.begin(&scope, Duration::ZERO).unwrap().unwrap();
    lookup(&mut manager, &scope, &store, &native, OTHER);
    raw(&mut store, ID, "actual-return");
    let settlement = manager.settlement(&scope, &flight, &[ID.into()]).unwrap();
    assert!(settlement.missing().ids().is_empty());
    manager.published(&scope, settlement).unwrap();
    assert_eq!(manager.pending(), &[OTHER.to_owned()]);
    drain(&mut store, &mut native);
    let positive = lookup(&mut manager, &scope, &store, &native, ID);
    assert_eq!(positive.kind(), LookupKind::Known);
    assert_eq!(positive.row().unwrap()["name"], "actual-return");
    assert_eq!(
        store.cached().unwrap()["records"][0]["runtime_missing"],
        false
    );
}

#[test]
fn raw_flags_never_gain_runtime_lookup_provenance() {
    for flag in ["isNonExistent", "isUnknown"] {
        for origin in [Origin::Direct, Origin::Boot] {
            let (mut store, mut native, scope, mut manager) = setup();
            let mut row = json!({"id":ID});
            row[flag] = true.into();
            store.enqueue("A", &owner(), 1, origin, vec![row]).unwrap();
            drain(&mut store, &mut native);
            assert!(
                manager
                    .lookup(
                        &scope,
                        &store,
                        observe(&store, &native),
                        "q",
                        &native,
                        ID,
                        true
                    )
                    .is_err()
            );
            assert_eq!(
                store.cached().unwrap()["records"][0]["runtime_missing"],
                false
            );
        }
    }
    let (mut store, native, _, _) = setup();
    assert!(
        store
            .enqueue(
                "A",
                &owner(),
                1,
                Origin::RuntimeMissing,
                vec![json!({"id":ID,"isNonExistent":true})]
            )
            .is_err()
    );
    assert!(
        store
            .capture_ingress(
                &observe(&store, &native),
                "A",
                &owner(),
                1,
                "q",
                &native,
                Origin::RuntimeMissing,
                vec![json!({"id":ID,"isNonExistent":true})]
            )
            .is_err()
    );
}

#[test]
fn lookup_and_terminal_refuse_foreign_source_phase_and_manager() {
    let (mut store, native, scope, mut manager) = setup();
    let value = lookup(&mut manager, &scope, &store, &native, ID);
    let replacement = Manager::new(scope.clone());
    assert!(!value.current(&replacement, &scope, &store, "q", &native));
    let mut changed = owner();
    changed.token = "replacement".into();
    let mut foreign = vec![
        Scope::new("B", &owner(), 1, 7).unwrap(),
        Scope::new("A", &owner(), 2, 7).unwrap(),
        Scope::new("A", &owner(), 1, 8).unwrap(),
        Scope::new("A", &changed, 1, 7).unwrap(),
    ];
    changed = owner();
    changed.team_id = "TOTHER".into();
    foreign.push(Scope::new("A", &changed, 1, 7).unwrap());
    for other in foreign {
        assert!(!value.current(&manager, &other, &store, "q", &native));
    }
    assert!(!value.current(&manager, &scope, &store, "other-generation", &native));
    let flight = manager.begin(&scope, Duration::ZERO).unwrap().unwrap();
    let settlement = manager.settlement(&scope, &flight, &[]).unwrap();
    let replacement_boot = store.begin_full_boot("A", &owner(), 1).unwrap();
    assert!(!value.current(&manager, &scope, &store, "q", &native));
    assert!(
        store
            .enqueue_runtime_missing(
                &observe(&store, &native),
                &scope,
                "q",
                &native,
                settlement.missing()
            )
            .is_err()
    );
    drop(replacement_boot);
}

#[test]
fn drop_retires_terminal_and_fresh_observation_cannot_hide_native_drift() {
    let (mut store, mut native, scope, mut manager) = setup();
    lookup(&mut manager, &scope, &store, &native, ID);
    let flight = manager.begin(&scope, Duration::ZERO).unwrap().unwrap();
    let settlement = manager.settlement(&scope, &flight, &[]).unwrap();
    drop(manager);
    assert!(
        store
            .enqueue_runtime_missing(
                &observe(&store, &native),
                &scope,
                "q",
                &native,
                settlement.missing()
            )
            .is_err()
    );
    raw(&mut store, ID, "before");
    drain(&mut store, &mut native);
    native
        .get_mut(ID)
        .unwrap()
        .extra
        .insert("unobserved".into(), true.into());
    let mut manager = Manager::new(scope.clone());
    assert!(
        manager
            .lookup(
                &scope,
                &store,
                observe(&store, &native),
                "q",
                &native,
                ID,
                true
            )
            .is_err()
    );
}

#[test]
fn ordered_raw_membership_unchanged_and_marker_keep_exact_predecessor() {
    let (mut store, mut native, scope, mut manager) = setup();
    lookup(&mut manager, &scope, &store, &native, ID);
    lookup(&mut manager, &scope, &store, &native, OTHER);
    let flight = manager.begin(&scope, Duration::ZERO).unwrap().unwrap();
    raw(&mut store, ID, "first");
    (store, native) = manager
        .queue_joined_memberships(
            &scope,
            &flight,
            &store,
            &observe(&store, &native),
            "q",
            &native,
            &[ID.into()],
        )
        .unwrap()
        .into_parts();
    raw(&mut store, ID, "second");
    (store, native) = manager
        .queue_unchanged_channels(
            &scope,
            &flight,
            &store,
            &observe(&store, &native),
            "q",
            &native,
            &[ID.into()],
        )
        .unwrap()
        .into_parts();
    let settlement = manager.settlement(&scope, &flight, &[ID.into()]).unwrap();
    store
        .enqueue_runtime_missing(
            &observe(&store, &native),
            &scope,
            "q",
            &native,
            settlement.missing(),
        )
        .unwrap();
    manager.published(&scope, settlement).unwrap();
    assert!(native.is_empty()); // no eager membership mutation
    drain(&mut store, &mut native);
    assert_eq!(
        lookup(&mut manager, &scope, &store, &native, ID)
            .row()
            .unwrap()["name"],
        "second"
    );
    assert_eq!(
        lookup(&mut manager, &scope, &store, &native, OTHER).kind(),
        LookupKind::NonExistent
    );
    assert_eq!(
        store.cached().unwrap()["records"][0]["wire"]["name"],
        "second"
    );
}

#[test]
fn queued_action_conflict_and_foreign_action_ids_refuse() {
    let (mut store, mut native, scope, mut manager) = setup();
    lookup(&mut manager, &scope, &store, &native, ID);
    let flight = manager.begin(&scope, Duration::ZERO).unwrap().unwrap();
    assert!(
        manager
            .queue_joined_memberships(
                &scope,
                &flight,
                &store,
                &observe(&store, &native),
                "q",
                &native,
                &[OTHER.into()]
            )
            .is_err()
    );
    (store, native) = manager
        .queue_joined_memberships(
            &scope,
            &flight,
            &store,
            &observe(&store, &native),
            "q",
            &native,
            &[ID.into()],
        )
        .unwrap()
        .into_parts();
    assert!(
        manager
            .lookup(
                &scope,
                &store,
                observe(&store, &native),
                "q",
                &native,
                ID,
                true
            )
            .is_err()
    );
    drain(&mut store, &mut native);
    let joined = lookup(&mut manager, &scope, &store, &native, ID);
    assert_eq!(joined.kind(), LookupKind::Unknown);
    assert!(!joined.is_transient());
    assert_eq!(
        store.cached().unwrap()["records"][0]["runtime_unknown"],
        true
    );
    assert!(store.cached().unwrap()["records"][0]["wire"].is_null());
}

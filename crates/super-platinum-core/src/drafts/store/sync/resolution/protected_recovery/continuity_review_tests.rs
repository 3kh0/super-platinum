//! Regression cases raised during independent facade review.
use super::*;

fn second_owner(store: &mut DraftStore, first: &Scope) -> (Scope, Operation) {
    let scope = Scope {
        key: DraftKey {
            channel: "C2".into(),
            ..first.key.clone()
        },
        ..first.clone()
    };
    let input = local("second owner");
    let remote = RemoteDraft {
        scope: scope.clone(),
        id: "D2".into(),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.000001".into(),
        snapshot: codec::encode_composer(&scope.key, &input.composer).unwrap(),
    };
    let mut state = DraftSync::adopt_remote(&scope, remote).unwrap();
    state.edit(
        &scope,
        Some(codec::encode_composer(&scope.key, &local("second request").composer).unwrap()),
    );
    let op = state.begin_operation(&scope).unwrap();
    assert!(state.mark_uncertain(&scope, &op.id));
    store.save([(scope.key.clone(), input.clone())]).unwrap();
    store
        .commit_synced(&scope, None, &input, &state, None)
        .unwrap();
    (scope, op)
}

// Exact prior private envelope, with no access to the installed v6 module.
fn make_v6_fixture(store: &mut DraftStore) {
    let rows = {
        let mut q = store
            .0
            .prepare("SELECT destination,state FROM composer_draft_sync")
            .unwrap();
        q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    store.0.execute_batch("DROP TRIGGER composer_draft_require_v7_insert;
        DROP TRIGGER composer_draft_require_v7_update;
        DROP TABLE composer_draft_continuity_format;
        CREATE TABLE composer_draft_continuity_format(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL CHECK(version=6));
        INSERT INTO composer_draft_continuity_format VALUES(1,6);
        DROP TABLE composer_draft_recovery_owners; DROP TABLE composer_draft_status_reads;
        DROP TABLE composer_draft_read_abandonment; DROP TABLE composer_draft_read_late;
        DROP TABLE composer_draft_protected_archive; DROP TABLE composer_draft_recovery_editing;
        DROP TABLE composer_draft_release_head;").unwrap();
    for (key, raw) in rows {
        let mut value: Value = serde_json::from_str(&raw).unwrap();
        value.as_object_mut().unwrap().remove("basis");
        value.as_object_mut().unwrap().remove("origin");
        store
            .0
            .execute(
                "UPDATE composer_draft_sync SET format_version=6,state=?1 WHERE destination=?2",
                params![value.to_string(), key],
            )
            .unwrap();
    }
    store.0.execute_batch("CREATE TRIGGER composer_draft_require_v6_insert BEFORE INSERT ON composer_draft_sync WHEN NEW.format_version != 6 BEGIN SELECT RAISE(ABORT,'protected draft format required'); END;
        CREATE TRIGGER composer_draft_require_v6_update BEFORE UPDATE ON composer_draft_sync WHEN NEW.format_version != 6 BEGIN SELECT RAISE(ABORT,'protected draft format required'); END;").unwrap();
}

#[test]
fn resolved_send_lost_read_can_be_superseded_after_restart_without_touching_successor() {
    let db = Database::new();
    let mut store = db.open();
    let (scope, raw) = seed(&mut store);
    let (other, other_op) = second_owner(&mut store, &scope);
    storage::activate(&mut store, &scope.account_id).unwrap();
    let lease = open(&mut store, &scope);
    let expected = review(&store, &scope);
    let baseline = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
    readback::finish_positive(
        &mut store,
        &baseline,
        raw,
        "U1",
        &RemoteChoice::Local(LocalChoice::Current),
    )
    .unwrap();
    let target = sent(&mut store, &scope, &lease);
    let (_, _lost) = check(&mut store, &scope, &target);
    let ticket = review(&store, &scope)
        .row
        .stored
        .state
        .pending_check(&target.operation)
        .unwrap()
        .clone();
    facade::record(&mut store, &target, &positive(&target.operation)).unwrap();
    let resolved = review(&store, &scope);
    assert!(resolved.row.stored.state.pending().is_none());
    assert!(archive::release(&mut store, &resolved).is_err());
    drop(store);
    let mut store = db.open();
    migration::normalize(&mut store, &scope.account_id).unwrap();
    let permit = takeover(&mut store, &scope);
    let before = review(&store, &scope);
    let until =
        recovery::supersede_unknown(&mut store, &permit, &before, &ticket, 200_000).unwrap();
    assert!(before.matches(&review(&store, &scope)));
    assert!(!recovery::unresolved(&store.0, &scope.key.team).unwrap());
    assert_eq!(
        recovery::attempts(&store.0).unwrap()[ticket.id()].state,
        recovery::State::SupersededUnknown
    );
    let expected = review(&store, &other);
    assert!(recovery::reserve(&mut store, &permit, &expected, &other_op, until - 1).is_err());
    let issued = recovery::reserve(&mut store, &permit, &expected, &other_op, until).unwrap();
    assert!(matches!(
        recovery::claim(&mut store, &permit, issued, until).unwrap(),
        recovery::Claim::Ready { .. }
    ));
    archive::release(&mut store, &before).unwrap();
}

#[test]
fn deadline_only_import_fences_other_owner_for_both_v5_and_v6() {
    for upgrade_v6 in [false, true] {
        let mut store = DraftStore::open(":memory:").unwrap();
        let (scope, _) = seed(&mut store);
        let (other, other_op) = second_owner(&mut store, &scope);
        let mut state = store
            .read_local_review(&scope)
            .unwrap()
            .stored
            .unwrap()
            .state;
        state.edit(
            &scope,
            Some(codec::encode_composer(&scope.key, &local("first request").composer).unwrap()),
        );
        let op = state.begin_operation(&scope).unwrap();
        assert!(state.mark_uncertain(&scope, &op.id));
        let ticket = state.reserve_check(&op, 1000).unwrap();
        assert!(state.finish_check(&ticket, 900_000));
        assert!(state.protected_retained_check().is_none());
        store
            .0
            .execute(
                "UPDATE composer_draft_sync SET state=?1 WHERE destination=?2",
                params![
                    serde_json::to_string(&state).unwrap(),
                    serde_json::to_string(&scope.key).unwrap()
                ],
            )
            .unwrap();
        storage::activate(&mut store, &scope.account_id).unwrap();
        if upgrade_v6 {
            make_v6_fixture(&mut store);
            migration::upgrade_v6(&mut store, &scope.account_id).unwrap();
        }
        assert!(recovery::attempts(&store.0).unwrap().is_empty());
        assert_eq!(
            recovery::deadline(&store.0, &scope.key.team).unwrap(),
            900_000
        );
        let permit = takeover(&mut store, &scope);
        let expected = review(&store, &other);
        assert!(recovery::reserve(&mut store, &permit, &expected, &other_op, 899_999).is_err());
        let ticket = recovery::reserve(&mut store, &permit, &expected, &other_op, 900_000).unwrap();
        assert!(matches!(
            recovery::claim(&mut store, &permit, ticket, 900_000).unwrap(),
            recovery::Claim::Ready { .. }
        ));
    }
}

#[test]
fn late_baseline_429_defers_already_reserved_status_ticket() {
    for known_until in [145_000, 800_000] {
        let mut store = DraftStore::open(":memory:").unwrap();
        let (scope, _) = seed(&mut store);
        let (other, op) = second_owner(&mut store, &scope);
        storage::activate(&mut store, &scope.account_id).unwrap();
        let lease = open(&mut store, &scope);
        let expected = review(&store, &scope);
        let baseline = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
        let permit = takeover(&mut store, &scope);
        let expected = review(&store, &scope);
        let until = recovery::supersede_baseline(
            &mut store,
            &permit,
            &expected,
            &baseline.admission.pending.id,
            100_000,
        )
        .unwrap();
        let expected = review(&store, &other);
        let issued = recovery::reserve(&mut store, &permit, &expected, &op, until).unwrap();
        let pending = review(&store, &other)
            .row
            .stored
            .state
            .pending_check(&op)
            .cloned();
        readback::finish_nonpositive(
            &mut store,
            &baseline,
            KnownReadFailure::RateLimited,
            known_until,
        )
        .unwrap();
        let recovery::Claim::Deferred {
            ticket,
            until: limit,
        } = recovery::claim(&mut store, &permit, issued, until).unwrap()
        else {
            panic!("late baseline cooldown ignored")
        };
        assert_eq!(limit, known_until);
        assert_eq!(
            review(&store, &other).row.stored.state.pending_check(&op),
            pending.as_ref()
        );
        assert!(matches!(
            recovery::claim(&mut store, &permit, ticket, limit).unwrap(),
            recovery::Claim::Ready { .. }
        ));
    }
}

#[test]
fn late_status_limit_inside_replacement_floor_is_not_its_own_spacing() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    let (_, old) = check(&mut store, &scope, &target);
    let ticket = review(&store, &scope)
        .row
        .stored
        .state
        .pending_check(&target.operation)
        .unwrap()
        .clone();
    let permit = takeover(&mut store, &scope);
    let before = review(&store, &scope);
    let until =
        recovery::supersede_unknown(&mut store, &permit, &before, &ticket, 200_000).unwrap();
    let expected = review(&store, &scope);
    let issued =
        recovery::reserve(&mut store, &permit, &expected, &target.operation, until).unwrap();
    let before = review(&store, &scope);
    let known = until + 15_000;
    recovery::complete(
        &mut store,
        &old,
        recovery::ResultKind::Nonpositive(KnownReadFailure::RateLimited),
        known,
    )
    .unwrap();
    assert!(before.matches(&review(&store, &scope)));
    let recovery::Claim::Deferred {
        ticket,
        until: deadline,
    } = recovery::claim(&mut store, &permit, issued, until).unwrap()
    else {
        panic!("known status limit was mistaken for own spacing")
    };
    assert_eq!(deadline, known);
    assert!(matches!(
        recovery::claim(&mut store, &permit, ticket, known).unwrap(),
        recovery::Claim::Ready { .. }
    ));
}

#[test]
fn late_mutation_limit_inside_reserved_check_floor_defers_dispatch() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    facade::record(&mut store, &target, &facade::Outcome::Uncertain).unwrap();
    let permit = takeover(&mut store, &scope);
    let expected = review(&store, &scope);
    let issued =
        recovery::reserve(&mut store, &permit, &expected, &target.operation, 100_000).unwrap();
    facade::record_with_cooldown(&mut store, &target, &facade::Outcome::Uncertain, 115_000)
        .unwrap();
    let recovery::Claim::Deferred { ticket, until } =
        recovery::claim(&mut store, &permit, issued, 100_000).unwrap()
    else {
        panic!("known mutation limit was mistaken for own spacing")
    };
    assert_eq!(until, 115_000);
    assert!(matches!(
        recovery::claim(&mut store, &permit, ticket, until).unwrap(),
        recovery::Claim::Ready { .. }
    ));
}

fn released_local(store: &mut DraftStore) -> (Scope, archive::Release) {
    let (scope, _) = seed(store);
    storage::activate(store, &scope.account_id).unwrap();
    // Use a fresh destination with true local origin and no remote operation.
    let scope = Scope {
        key: DraftKey {
            channel: "C3".into(),
            ..scope.key
        },
        ..scope
    };
    let input = local("released");
    store.save([(scope.key.clone(), input.clone())]).unwrap();
    facade::enroll_local(store, &scope, &input).unwrap();
    let expected = review(store, &scope);
    let released = archive::release(store, &expected).unwrap();
    (scope, released)
}

#[test]
fn release_handoff_is_one_use_across_local_aba() {
    let mut store = DraftStore::open(":memory:").unwrap();
    let (scope, released) = released_local(&mut store);
    let writer = LocalWriter::default();
    assert!(archive::save_released(&mut store, &released, &local("newer"), &writer, 2).unwrap());
    store
        .save([(scope.key.clone(), released.local.clone())])
        .unwrap();
    assert!(!archive::save_released(&mut store, &released, &local("delayed"), &writer, 1).unwrap());
    let current = store.read_local_review(&scope).unwrap();
    assert!(same_local(&current.local, &released.local));
    assert!(
        current
            .alternates
            .iter()
            .any(|a| a.draft.composer.text == "delayed")
    );
}

#[test]
fn unchanged_release_handoff_consumes_authority_before_lower_serial() {
    let mut store = DraftStore::open(":memory:").unwrap();
    let (scope, released) = released_local(&mut store);
    let writer = LocalWriter::default();
    assert!(archive::save_released(&mut store, &released, &released.local, &writer, 5).unwrap());
    assert!(!archive::save_released(&mut store, &released, &local("older"), &writer, 4).unwrap());
    assert!(same_local(
        &store.read_local_review(&scope).unwrap().local,
        &released.local
    ));
}

#[test]
fn normalize_revokes_stream_after_last_owner_archived() {
    let (mut store, scope, lease) = setup();
    let target = sent(&mut store, &scope, &lease);
    facade::record(&mut store, &target, &positive(&target.operation)).unwrap();
    let expected = review(&store, &scope);
    archive::release(&mut store, &expected).unwrap();
    assert!(
        migration::normalize(&mut store, &scope.account_id)
            .unwrap()
            .is_empty()
    );
    let input = local("new local owner");
    store.save([(scope.key.clone(), input.clone())]).unwrap();
    facade::enroll_local(&mut store, &scope, &input).unwrap();
    let expected = review(&store, &scope);
    assert!(facade::verify_local(&mut store, &lease, &expected).is_err());
    assert_eq!(review(&store, &scope).row.ownership.verified_epoch, 0);
    let fresh = open(&mut store, &scope);
    facade::verify_local(&mut store, &fresh, &expected).unwrap();
}

#[test]
fn create_receipt_preserves_newer_local_and_rejects_wrong_identity() {
    let mut store = DraftStore::open(":memory:").unwrap();
    let (scope, released) = released_local(&mut store);
    facade::enroll_local(&mut store, &scope, &released.local).unwrap();
    let lease = open(&mut store, &scope);
    let expected = review(&store, &scope);
    facade::verify_local(&mut store, &lease, &expected).unwrap();
    let expected = review(&store, &scope);
    let (row, op) = storage::prepare(&mut store, &lease, &expected).unwrap();
    assert_eq!(op.kind, OperationKind::Create);
    let target = facade::ReceiptTarget::for_row(&row).unwrap();
    let expected = review(&store, &scope);
    facade::preserve(&mut store, &expected, &local("newer during create"), None).unwrap();
    let remote = RemoteDraft {
        scope: scope.clone(),
        id: "D3".into(),
        client_msg_id: op.client_msg_id.clone(),
        last_updated_ts: "20.000001".into(),
        snapshot: op.snapshot.clone().unwrap(),
    };
    let before = review(&store, &scope);
    let mut wrong = remote.clone();
    wrong.client_msg_id = uuid::Uuid::new_v4().to_string();
    assert!(facade::record(&mut store, &target, &facade::Outcome::Active(wrong)).is_err());
    assert!(before.matches(&review(&store, &scope)));
    facade::record(
        &mut store,
        &target,
        &facade::Outcome::Active(remote.clone()),
    )
    .unwrap();
    let after = review(&store, &scope);
    assert_eq!(after.row.stored.state.acknowledged(), Some(&remote));
    assert_eq!(after.review.local.composer.text, "newer during create");
    assert_eq!(after.row.ownership.basis, Basis::Unverified);
}

#[test]
fn delete_receipt_preserves_successor_and_requires_frozen_server_id() {
    let (mut store, scope, lease) = setup();
    let expected = review(&store, &scope);
    facade::control(&mut store, &expected, facade::Control::Clear).unwrap();
    let expected = review(&store, &scope);
    let (row, op) = storage::prepare(&mut store, &lease, &expected).unwrap();
    assert_eq!(op.kind, OperationKind::Delete);
    let target = facade::ReceiptTarget::for_row(&row).unwrap();
    let expected = review(&store, &scope);
    facade::preserve(&mut store, &expected, &local("newer during delete"), None).unwrap();
    let before = review(&store, &scope);
    assert!(
        facade::record(
            &mut store,
            &target,
            &facade::Outcome::Deleted {
                draft_id: "wrong".into()
            }
        )
        .is_err()
    );
    assert!(before.matches(&review(&store, &scope)));
    facade::record(
        &mut store,
        &target,
        &facade::Outcome::Deleted {
            draft_id: op.base.unwrap().id,
        },
    )
    .unwrap();
    let after = review(&store, &scope);
    assert_eq!(after.review.local.composer.text, "newer during delete");
    assert!(after.row.stored.state.acknowledged().is_none());
    assert!(after.row.stored.state.pending().is_none());
    assert_ne!(
        after.row.stored.state.client_msg_id(),
        before.row.stored.state.client_msg_id()
    );
    assert_eq!(after.row.ownership.basis, Basis::Unverified);
    assert_eq!(after.row.ownership.verified_epoch, 0);
}

#[test]
fn negative_reviewed_writer_floor_cannot_partially_release_owner() {
    let (mut store, scope, lease) = setup();
    let target = sent(&mut store, &scope, &lease);
    let during_send = review(&store, &scope);
    let intent = during_send
        .row
        .stored
        .state
        .local_preservation_intent()
        .unwrap()
        .clone();
    facade::preserve(
        &mut store,
        &during_send,
        &local("newer retained composer"),
        Some(&intent),
    )
    .unwrap();
    facade::record(&mut store, &target, &positive(&target.operation)).unwrap();
    let expected = review(&store, &scope);
    let key = serde_json::to_string(&scope.key).unwrap();
    let writer = LocalWriter::default();
    // Model damaged storage, not an ordinary writer: normal schema rejects it.
    store
        .0
        .execute_batch("PRAGMA ignore_check_constraints=ON;")
        .unwrap();
    store.0.execute(
        "INSERT INTO composer_draft_reviewed_writers(destination,writer_id,revision) VALUES(?1,?2,-1)",
        params![key, writer.id()],
    ).unwrap();
    store
        .0
        .execute_batch("PRAGMA ignore_check_constraints=OFF;")
        .unwrap();
    let local_before: Option<String> = store
        .0
        .query_row(
            "SELECT content FROM composer_drafts WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()
        .unwrap();
    assert!(local_before.is_some());
    let error = match archive::release(&mut store, &expected) {
        Ok(_) => panic!("negative writer floor released ownership"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("negative reviewed writer revision")
    );
    assert!(expected.matches(&review(&store, &scope)));
    let local_after: Option<String> = store
        .0
        .query_row(
            "SELECT content FROM composer_drafts WHERE destination=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()
        .unwrap();
    assert_eq!(local_before, local_after);
    for table in [
        "composer_draft_protected_archive",
        "composer_draft_release_head",
    ] {
        let count: i64 = store
            .0
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
    store.0.execute(
        "UPDATE composer_draft_reviewed_writers SET revision=7 WHERE destination=?1 AND writer_id=?2",
        params![key, writer.id()],
    ).unwrap();
    archive::release(&mut store, &expected).unwrap();
    let raw: String = store
        .0
        .query_row(
            "SELECT content FROM composer_draft_protected_archive",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let archived: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(archived["writer_floors"], json!([[writer.id(), 7]]));
}

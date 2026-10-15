//! Explicit retry, archival, migration and failure-injection regressions.
use super::*;

#[test]
fn explicit_unknown_read_supersession_preserves_mutation_and_enforces_backoff() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    let (permit, _) = check(&mut store, &scope, &target);
    let before = review(&store, &scope);
    let ticket = before
        .row
        .stored
        .state
        .pending_check(&target.operation)
        .unwrap()
        .clone();
    assert!(recovery::supersede_unknown(&mut store, &permit, &before, &ticket, 200_000).is_err());
    let replacement = takeover(&mut store, &scope);
    let before = review(&store, &scope);
    let until =
        recovery::supersede_unknown(&mut store, &replacement, &before, &ticket, 200_000).unwrap();
    assert_eq!(until, 230_000);
    let after = review(&store, &scope);
    assert_eq!(after.row.stored.state.pending(), Some(&target.operation));
    assert!(
        after
            .row
            .stored
            .state
            .pending_check(&target.operation)
            .is_none()
    );
    assert!(
        recovery::reserve(
            &mut store,
            &replacement,
            &after,
            &target.operation,
            until - 1
        )
        .is_err()
    );
    let issued =
        recovery::reserve(&mut store, &replacement, &after, &target.operation, until).unwrap();
    let recovery::Claim::Ready {
        completion: next, ..
    } = recovery::claim(&mut store, &replacement, issued, until).unwrap()
    else {
        panic!("unexpected cooldown")
    };
    assert_eq!(recovery::attempts(&store.0).unwrap().len(), 2);
    let _ = next;
    let next_owner = takeover(&mut store, &scope);
    let expected = review(&store, &scope);
    let second = expected
        .row
        .stored
        .state
        .pending_check(&target.operation)
        .unwrap()
        .clone();
    assert_eq!(
        recovery::supersede_unknown(&mut store, &next_owner, &expected, &second, 300_000).unwrap(),
        360_000
    );
}
#[test]
fn late_old_rate_limit_cannot_clear_replacement_ticket() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    let (_, old) = check(&mut store, &scope, &target);
    let permit = takeover(&mut store, &scope);
    let before = review(&store, &scope);
    let ticket = before
        .row
        .stored
        .state
        .pending_check(&target.operation)
        .unwrap()
        .clone();
    let until =
        recovery::supersede_unknown(&mut store, &permit, &before, &ticket, 200_000).unwrap();
    let expected = review(&store, &scope);
    let next = recovery::reserve(&mut store, &permit, &expected, &target.operation, until).unwrap();
    let pending = review(&store, &scope)
        .row
        .stored
        .state
        .pending_check(&target.operation)
        .cloned();
    recovery::complete(
        &mut store,
        &old,
        recovery::ResultKind::Nonpositive(KnownReadFailure::RateLimited),
        900_000,
    )
    .unwrap();
    assert_eq!(
        review(&store, &scope)
            .row
            .stored
            .state
            .pending_check(&target.operation),
        pending.as_ref()
    );
    let recovery::Claim::Deferred {
        ticket: still_issued,
        until: deadline,
    } = recovery::claim(&mut store, &permit, next, until).unwrap()
    else {
        panic!("late cooldown ignored")
    };
    assert_eq!(deadline, 900_000);
    assert!(matches!(
        recovery::claim(&mut store, &permit, still_issued, deadline).unwrap(),
        recovery::Claim::Ready { .. }
    ));
    assert_eq!(
        recovery::deadline(&store.0, &scope.key.team).unwrap(),
        900_000
    );
}
#[test]
fn malformed_missing_or_duplicate_attempt_guards_cannot_release_pending() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    let (_, completion) = check(&mut store, &scope, &target);
    let before = review(&store, &scope);
    let raw: String = store
        .0
        .query_row("SELECT content FROM composer_draft_status_reads", [], |r| {
            r.get(0)
        })
        .unwrap();
    for field in ["result", "late", "predecessor"] {
        let mut value: Value = serde_json::from_str(&raw).unwrap();
        value.as_object_mut().unwrap().remove(field);
        store
            .0
            .execute(
                "UPDATE composer_draft_status_reads SET content=?1",
                [value.to_string()],
            )
            .unwrap();
        assert!(recovery::attempts(&store.0).is_err());
        assert!(
            recovery::complete(
                &mut store,
                &completion,
                recovery::ResultKind::Positive(positive(&target.operation)),
                0
            )
            .is_err()
        );
        assert!(before.matches(&review(&store, &scope)));
    }
    let duplicate = raw.replace("\"result\":null", "\"result\":null,\"result\":null");
    assert_ne!(duplicate, raw);
    store
        .0
        .execute(
            "UPDATE composer_draft_status_reads SET content=?1",
            [duplicate],
        )
        .unwrap();
    assert!(recovery::attempts(&store.0).is_err());
}
#[test]
fn terminal_archive_keeps_history_and_invalidates_same_text_old_handoff() {
    let (mut store, scope, lease) = setup();
    let target = sent(&mut store, &scope, &lease);
    let expected = review(&store, &scope);
    facade::preserve(&mut store, &expected, &local("newer"), None).unwrap();
    facade::record(&mut store, &target, &positive(&target.operation)).unwrap();
    let expected = review(&store, &scope);
    let released = archive::release(&mut store, &expected).unwrap();
    assert!(
        archive::reject_claim(
            &store.0,
            &scope,
            target.operation.base.as_ref().map(|b| b.id.as_str()),
            Some(&target.operation.client_msg_id)
        )
        .is_err()
    );
    facade::enroll_local(&mut store, &scope, &released.local).unwrap();
    let next = review(&store, &scope);
    archive::release(&mut store, &next).unwrap();
    assert!(
        !archive::save_released(
            &mut store,
            &released,
            &local("stale writer"),
            &LocalWriter::default(),
            1
        )
        .unwrap()
    );
    assert_eq!(store.load().unwrap()[0].1.composer.text, "newer");
    assert!(read_alternates(&store.0).unwrap().contains_key(&scope.key));
}
#[test]
fn uncertain_editing_marker_never_removes_owner_or_allows_release() {
    let (mut store, scope, lease) = setup();
    let target = sent(&mut store, &scope, &lease);
    facade::record(&mut store, &target, &facade::Outcome::Uncertain).unwrap();
    let before = review(&store, &scope);
    archive::keep_editing(&mut store, &before).unwrap();
    assert!(before.matches(&review(&store, &scope)));
    assert!(archive::release(&mut store, &before).is_err());
    let intent = before
        .row
        .stored
        .state
        .local_preservation_intent()
        .cloned()
        .unwrap();
    facade::preserve(&mut store, &before, &local("editable copy"), Some(&intent)).unwrap();
    assert_eq!(
        review(&store, &scope).row.stored.state.pending(),
        Some(&target.operation)
    );
}
#[test]
fn baseline_unknown_supersession_retains_audit_and_late_429_deadline() {
    let mut store = DraftStore::open(":memory:").unwrap();
    let (scope, raw) = seed(&mut store);
    storage::activate(&mut store, &scope.account_id).unwrap();
    let lease = open(&mut store, &scope);
    let expected = review(&store, &scope);
    let ticket = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
    let id = ticket.admission.pending.id.clone();
    let permit = takeover(&mut store, &scope);
    let expected = review(&store, &scope);
    let until = recovery::supersede_baseline(&mut store, &permit, &expected, &id, 100_000).unwrap();
    assert_eq!(until, 130_000);
    assert_eq!(review(&store, &scope).row.ownership.verified_epoch, 0);
    readback::finish_nonpositive(&mut store, &ticket, KnownReadFailure::RateLimited, 600_000)
        .unwrap();
    let lease = open(&mut store, &scope);
    let expected = review(&store, &scope);
    assert!(readback::reserve(&mut store, &lease, &expected, until).is_err());
    let ticket = readback::reserve(&mut store, &lease, &expected, 600_000).unwrap();
    readback::finish_positive(
        &mut store,
        &ticket,
        raw,
        "U1",
        &RemoteChoice::Local(LocalChoice::Current),
    )
    .unwrap();
    assert_eq!(inventory::reads(&store.0).unwrap().len(), 2);
}
#[test]
fn restart_keeps_pending_and_completed_deadline_without_dispatch_capability() {
    let db = Database::new();
    let mut store = db.open();
    let (scope, raw) = seed(&mut store);
    storage::activate(&mut store, &scope.account_id).unwrap();
    let lease = open(&mut store, &scope);
    let expected = review(&store, &scope);
    let ticket = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
    readback::finish_positive(
        &mut store,
        &ticket,
        raw,
        "U1",
        &RemoteChoice::Local(LocalChoice::Current),
    )
    .unwrap();
    let target = update(&mut store, &scope, &lease);
    let (_, completion) = check(&mut store, &scope, &target);
    let pending = review(&store, &scope)
        .row
        .stored
        .state
        .pending_check(&target.operation)
        .cloned();
    drop(store);
    let mut store = db.open();
    migration::normalize(&mut store, &scope.account_id).unwrap();
    assert_eq!(
        review(&store, &scope)
            .row
            .stored
            .state
            .pending_check(&target.operation),
        pending.as_ref()
    );
    recovery::complete(
        &mut store,
        &completion,
        recovery::ResultKind::Nonpositive(KnownReadFailure::RateLimited),
        800_000,
    )
    .unwrap();
    drop(store);
    let mut store = db.open();
    assert_eq!(
        recovery::deadline(&store.0, &scope.key.team).unwrap(),
        800_000
    );
    let permit = takeover(&mut store, &scope);
    let expected = review(&store, &scope);
    assert!(recovery::reserve(&mut store, &permit, &expected, &target.operation, 799_999).is_err());
}
#[test]
fn clear_and_rejected_return_are_preservation_not_dispatch() {
    let (mut store, scope, lease) = setup();
    let target = sent(&mut store, &scope, &lease);
    facade::record(&mut store, &target, &facade::Outcome::Rejected).unwrap();
    storage::mark_gap(&mut store, &lease, "offline").unwrap();
    let expected = review(&store, &scope);
    let id = expected.row.stored.state.consumption().unwrap().id.clone();
    facade::control(
        &mut store,
        &expected,
        facade::Control::ReturnConsumption(id),
    )
    .unwrap();
    assert_eq!(
        review(&store, &scope).review.local.composer.text,
        "original"
    );
    let expected = review(&store, &scope);
    facade::control(&mut store, &expected, facade::Control::Clear).unwrap();
    assert!(review(&store, &scope).review.local.composer.text.is_empty());
}
#[test]
fn migration_imports_v5_pending_check_without_issuing_a_new_read() {
    let mut store = DraftStore::open(":memory:").unwrap();
    let (scope, _) = seed(&mut store);
    let old = store.read_local_review(&scope).unwrap();
    let mut state = old.stored.as_ref().unwrap().state.clone();
    state.edit(
        &scope,
        Some(codec::encode_composer(&scope.key, &local("requested").composer).unwrap()),
    );
    let op = state.begin_operation(&scope).unwrap();
    state.mark_uncertain(&scope, &op.id);
    let check = state.reserve_check(&op, 10_000).unwrap();
    // Fixture seeds the legitimate reducer state; admission APIs are tested above.
    store
        .0
        .execute(
            "UPDATE composer_draft_sync SET state=?1",
            [serde_json::to_string(&state).unwrap()],
        )
        .unwrap();
    storage::activate(&mut store, &scope.account_id).unwrap();
    let attempts = recovery::attempts(&store.0).unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[check.id()].state, recovery::State::Pending);
    let permit = takeover(&mut store, &scope);
    let expected = review(&store, &scope);
    assert!(recovery::reserve(&mut store, &permit, &expected, &op, 1_000_000).is_err());
    assert_eq!(expected.row.stored.state.pending_check(&op), Some(&check));
}
#[test]
fn v6_upgrade_preserves_pending_baseline_and_rejects_legacy_write() {
    let mut store = DraftStore::open(":memory:").unwrap();
    let (scope, _) = seed(&mut store);
    // Build the exact prior envelope/schema shape, including a real admitted
    // unresolved baseline, without borrowing another module's private APIs.
    storage::activate(&mut store, &scope.account_id).unwrap();
    let lease = open(&mut store, &scope);
    let expected = review(&store, &scope);
    let _ticket = readback::reserve(&mut store, &lease, &expected, 1000).unwrap();
    let mut envelope: Value = serde_json::from_str(&wire::encode(&expected.row).unwrap()).unwrap();
    envelope.as_object_mut().unwrap().remove("basis");
    envelope.as_object_mut().unwrap().remove("origin");
    store.0.execute_batch("DROP TRIGGER composer_draft_require_v7_insert;
        DROP TRIGGER composer_draft_require_v7_update;
        DROP TABLE composer_draft_continuity_format;
        CREATE TABLE composer_draft_continuity_format(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL CHECK(version=6));
        INSERT INTO composer_draft_continuity_format VALUES(1,6);
        DROP TABLE composer_draft_recovery_owners; DROP TABLE composer_draft_status_reads;
        DROP TABLE composer_draft_read_abandonment; DROP TABLE composer_draft_read_late;
        DROP TABLE composer_draft_protected_archive; DROP TABLE composer_draft_recovery_editing;
        DROP TABLE composer_draft_release_head;").unwrap();
    store
        .0
        .execute(
            "UPDATE composer_draft_sync SET format_version=6,state=?1",
            [envelope.to_string()],
        )
        .unwrap();
    store.0.execute_batch("CREATE TRIGGER composer_draft_require_v6_insert BEFORE INSERT ON composer_draft_sync WHEN NEW.format_version != 6 BEGIN SELECT RAISE(ABORT,'protected draft format required'); END;
        CREATE TRIGGER composer_draft_require_v6_update BEFORE UPDATE ON composer_draft_sync WHEN NEW.format_version != 6 BEGIN SELECT RAISE(ABORT,'protected draft format required'); END;").unwrap();
    let raw: String = store
        .0
        .query_row(
            "SELECT content FROM composer_draft_continuity_reads",
            [],
            |r| r.get(0),
        )
        .unwrap();
    migration::upgrade_v6(&mut store, &scope.account_id).unwrap();
    assert_eq!(
        store
            .0
            .query_row(
                "SELECT content FROM composer_draft_continuity_reads",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        raw
    );
    assert_eq!(
        review(&store, &scope).row.ownership.origin,
        Origin::Imported
    );
    assert!(
        store
            .load_synced(&scope.account_id, LoadMode::Current)
            .is_err()
    );
}

#[test]
fn known_mutation_rate_limit_and_uncertain_outcome_commit_together() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    let before = review(&store, &scope);
    let before_team = storage::team(&store.0, &scope.key.team).unwrap();
    store.0.execute_batch("CREATE TRIGGER fail_row BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(
        facade::record_with_cooldown(&mut store, &target, &facade::Outcome::Uncertain, 700_000)
            .is_err()
    );
    assert!(before.matches(&review(&store, &scope)));
    assert_eq!(recovery::deadline(&store.0, &scope.key.team).unwrap(), 0);
    assert_eq!(
        storage::team(&store.0, &scope.key.team).unwrap(),
        before_team
    );
    store.0.execute_batch("DROP TRIGGER fail_row;").unwrap();
    facade::record_with_cooldown(&mut store, &target, &facade::Outcome::Uncertain, 700_000)
        .unwrap();
    let permit = takeover(&mut store, &scope);
    let expected = review(&store, &scope);
    assert!(recovery::reserve(&mut store, &permit, &expected, &target.operation, 699_999).is_err());
    assert_eq!(
        expected
            .row
            .stored
            .state
            .check_retry_at_ms(&target.operation),
        Some(700_000)
    );
}
#[test]
fn failed_reviewed_schedule_clear_rolls_back_intent_and_ownership() {
    let (mut store, scope, lease) = setup();
    let before = review(&store, &scope);
    let raw = before.row.stored.state.desired().cloned().unwrap();
    store.0.execute_batch("CREATE TRIGGER fail_clear BEFORE DELETE ON composer_drafts BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(facade::reviewed_schedule(&mut store, &lease, &before, &raw, 2_000_000_000).is_err());
    assert!(before.matches(&review(&store, &scope)));
    store.0.execute_batch("DROP TRIGGER fail_clear;").unwrap();
    facade::reviewed_schedule(&mut store, &lease, &before, &raw, 2_000_000_000).unwrap();
    assert!(review(&store, &scope).review.local.composer.text.is_empty());
}
#[test]
fn superseded_positive_response_remains_an_explicit_exact_operation_recovery() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    let (_, old) = check(&mut store, &scope, &target);
    let permit = takeover(&mut store, &scope);
    let before = review(&store, &scope);
    let ticket = before
        .row
        .stored
        .state
        .pending_check(&target.operation)
        .unwrap()
        .clone();
    recovery::supersede_unknown(&mut store, &permit, &before, &ticket, 200_000).unwrap();
    recovery::complete(
        &mut store,
        &old,
        recovery::ResultKind::Positive(positive(&target.operation)),
        0,
    )
    .unwrap();
    let before = review(&store, &scope);
    assert_eq!(before.row.stored.state.pending(), Some(&target.operation));
    recovery::accept_late_positive(&mut store, &before, ticket.id()).unwrap();
    assert!(review(&store, &scope).row.stored.state.pending().is_none());
}
#[test]
fn late_receipt_cannot_touch_byte_identical_reenrolled_owner() {
    let (mut store, scope, lease) = setup();
    let target = update(&mut store, &scope, &lease);
    let before = review(&store, &scope);
    let mut changed = before.row.clone();
    changed.ownership.enrollment_id = uuid::Uuid::new_v4().to_string();
    storage::write_row(&store.0, &changed).unwrap();
    assert!(facade::record(&mut store, &target, &positive(&target.operation)).is_err());
    assert_eq!(
        review(&store, &scope).row.stored.state.pending(),
        Some(&target.operation)
    );
}

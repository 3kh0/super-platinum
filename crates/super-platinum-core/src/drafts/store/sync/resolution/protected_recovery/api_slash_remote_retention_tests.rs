//! SQLite retention, rollback and late-receipt tests.
use super::*;
#[test]
fn v3_migration_preserves_v2_binding_completion_attestation_and_local_history_bytes() {
    let f = remote_fixture();
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::NotSent);
    let before = [
        raw(&f, "composer_draft_slash_bindings", "content"),
        raw(&f, "composer_draft_slash_completions", "content"),
        raw(&f, "composer_draft_slash_attestations", "content"),
    ];
    r::migrate(&f.store, &f.scope.account_id).unwrap();
    r::migrate(&f.store, &f.scope.account_id).unwrap();
    assert_eq!(
        before,
        [
            raw(&f, "composer_draft_slash_bindings", "content"),
            raw(&f, "composer_draft_slash_completions", "content"),
            raw(&f, "composer_draft_slash_attestations", "content")
        ]
    );
    assert_eq!(count(&f, "composer_draft_slash_remote_jobs"), 0);
}
#[test]
fn migration_marker_failure_rolls_back_new_tables() {
    let f = remote_fixture();
    setup(&f);
    f.store.0.execute_batch("CREATE TRIGGER fail_remote BEFORE UPDATE ON composer_draft_slash_format BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(r::migrate(&f.store, &f.scope.account_id).is_err());
    let n: i64 = f
        .store
        .0
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'composer_draft_slash_remote_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
}
#[test]
fn partial_or_orphan_v3_schema_never_disables_existing_guard() {
    let f = start(true);
    f.store
        .0
        .execute_batch(
            "DROP TABLE composer_draft_slash_format; DROP TABLE composer_draft_slash_bindings;",
        )
        .unwrap();
    assert!(guard(&f.store.0, &f.scope).is_err());
    let f = start(true);
    f.store
        .0
        .execute_batch("DROP TABLE composer_draft_slash_remote_jobs;")
        .unwrap();
    assert!(r::audit(&f.store.0, &f.scope.account_id).is_err());
    assert!(guard(&f.store.0, &f.scope).is_err());
}
#[test]
fn job_cas_failure_rolls_back_read_admission_and_team_pending() {
    let f = start(true);
    let records = inventory::reads(&f.store.0).unwrap();
    let before = storage::team(&f.store.0, &f.scope.key.team).unwrap();
    f.store.0.execute_batch("CREATE TRIGGER fail_job BEFORE INSERT ON composer_draft_slash_remote_jobs BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(
        r::reserve(
            &f.store,
            &f.stream,
            &current(&f),
            &f.writer,
            2,
            f.next.id(),
            1_000_000
        )
        .is_err()
    );
    assert_eq!(
        storage::team(&f.store.0, &f.scope.key.team).unwrap(),
        before
    );
    assert_eq!(inventory::reads(&f.store.0).unwrap(), records);
}
#[test]
fn cleanup_receipt_failure_rolls_back_reducer_and_cooldown() {
    let f = start(false);
    let claim = cleanup(&f);
    let before = current(&f);
    f.store.0.execute_batch("CREATE TRIGGER fail_job BEFORE UPDATE ON composer_draft_slash_remote_jobs BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    let report = r::cleanup_report(
        &claim,
        r::CleanupKind::Response,
        Some("{\"ok\":true}".into()),
        5_000_000,
    );
    assert!(r::retain_cleanup(&f.store, &claim, &report).is_err());
    assert!(current(&f).same(&before));
}
#[test]
fn archive_statement_and_commit_failures_roll_back_row_journal_and_move() {
    for trigger in [
        "CREATE TRIGGER fail_archive BEFORE INSERT ON composer_draft_slash_remote_archives BEGIN SELECT RAISE(ABORT,'fixture'); END;",
        "CREATE TRIGGER fail_archive BEFORE DELETE ON composer_draft_slash_bindings BEGIN SELECT RAISE(ABORT,'fixture'); END;",
        "PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(parent INTEGER REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fail_archive AFTER INSERT ON composer_draft_slash_remote_archives BEGIN INSERT INTO c VALUES(9); END;",
    ] {
        let f = start(true);
        let raw = read(&f);
        let choice = r::choice(raw, f.stream.clone(), r::Action::Preserve);
        let before = current(&f);
        let journal = slash_journal::protected_inventory(&f.store.0, &f.scope.account_id).unwrap();
        f.store.0.execute_batch(trigger).unwrap();
        assert!(r::preserve(&f.store, &choice).is_err());
        assert!(current(&f).same(&before));
        assert_eq!(
            slash_journal::protected_inventory(&f.store.0, &f.scope.account_id).unwrap(),
            journal
        );
        assert_eq!(count(&f, "composer_draft_slash_remote_archives"), 0);
        assert!(guard(&f.store.0, &f.scope).is_err());
    }
}
#[test]
fn changed_job_or_archive_evidence_fails_closed() {
    let f = start(true);
    preserved(&f);
    f.store.0.execute("UPDATE composer_draft_slash_remote_jobs SET content=replace(content,'\"user\":\"U1\"','\"user\":\"U_OTHER\"')",[]).unwrap();
    assert!(r::audit(&f.store.0, &f.scope.account_id).is_err());
    assert!(guard(&f.store.0, &f.scope).is_err());
}
#[test]
fn oversized_evidence_is_refused_without_dropping_active_binding() {
    let f = start(true);
    read(&f);
    f.store
        .0
        .execute(
            "UPDATE composer_draft_slash_remote_jobs SET content=?1",
            [" ".repeat(17 * 1024 * 1024)],
        )
        .unwrap();
    assert!(r::audit(&f.store.0, &f.scope.account_id).is_err());
    assert_eq!(count(&f, "composer_draft_slash_bindings"), 1);
}

#[test]
fn typing_after_positive_cleanup_before_archival_keeps_new_input_and_can_finish() {
    let f = start(false);
    let claim = cleanup(&f);
    let raw = complete(&f, &claim);
    let newer = checkpoint(&f, 3, &local("typed after successful cleanup"));
    r::archive_cleanup(&f.store, &f.stream, &newer, &f.writer, 3, &raw).unwrap();
    assert!(same_local(current(&f).local(), newer.local()));
    guard(&f.store.0, &f.scope).unwrap();
}
#[test]
fn positive_cleanup_reducer_rolls_back_when_job_cas_fails_then_same_receipt_can_flush() {
    let f = start(false);
    let claim = cleanup(&f);
    let before = current(&f);
    let report = r::cleanup_report(
        &claim,
        r::CleanupKind::Response,
        Some("{\"ok\":true}".into()),
        0,
    );
    f.store.0.execute_batch("CREATE TRIGGER fail_job BEFORE UPDATE ON composer_draft_slash_remote_jobs BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(r::retain_cleanup(&f.store, &claim, &report).is_err());
    assert!(current(&f).same(&before));
    f.store.0.execute_batch("DROP TRIGGER fail_job;").unwrap();
    r::retain_cleanup(&f.store, &claim, &report).unwrap();
    assert!(current(&f).row().state().acknowledged().is_none());
}
#[test]
fn replaced_enrollment_late_ack_is_historical_only() {
    let f = start(false);
    let claim = cleanup(&f);
    let before = current(&f);
    let raw = raw(&f, "composer_draft_sync", "state");
    let replaced = raw.replace(
        before.row().lineage().enrollment(),
        &uuid::Uuid::new_v4().to_string(),
    );
    assert_ne!(raw, replaced);
    f.store
        .0
        .execute("UPDATE composer_draft_sync SET state=?1", [replaced])
        .unwrap();
    let replacement = current(&f);
    let raw = complete(&f, &claim);
    assert!(current(&f).same(&replacement));
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(v["reduced_after"].is_null());
}
#[test]
fn thread_cleanup_uses_positive_owned_baseline_and_preserves_thread_scope() {
    let f = fixture::remote_at(":memory:", Some("100.000001".into()));
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::Reply(json!({"ok":true})));
    let draft = current(&f).local().clone();
    checkpoint(&f, 2, &draft);
    r::migrate(&f.store, &f.scope.account_id).unwrap();
    let claim = cleanup(&f);
    assert_eq!(
        current(&f)
            .row()
            .state()
            .pending()
            .unwrap()
            .scope
            .key
            .thread
            .as_deref(),
        Some("100.000001")
    );
    let raw = complete(&f, &claim);
    archived_cleanup(&f, &raw, 3);
    guard(&f.store.0, &f.scope).unwrap();
}
#[test]
fn disk_restart_keeps_remote_archive_and_never_restores_dispatch_handle() {
    let db = crate::drafts::store::sync::tests::Database::new();
    let f = fixture::remote_at(&db.path, None);
    let (_, target) = setup(&f);
    finish(
        &f,
        &target,
        Delivery::Reply(json!({"ok":true,"keep_input":true})),
    );
    let draft = current(&f).local().clone();
    checkpoint(&f, 2, &draft);
    r::migrate(&f.store, &f.scope.account_id).unwrap();
    preserved(&f);
    let scope = f.scope.clone();
    f.stream.revoke();
    drop(f);
    let store = db.open();
    r::audit(&store.0, &scope.account_id).unwrap();
    guard(&store.0, &scope).unwrap();
}
#[test]
fn positive_cleanup_before_restart_can_archive_with_fresh_stream_and_accepted_writer() {
    let db = crate::drafts::store::sync::tests::Database::new();
    let (scope, job, old_team) = prepare_cleanup_before_restart(&db.path);
    archive_cleanup_after_restart(&db, scope, job, old_team);
}
// Keep the two process-lifetime phases in separate debug stack frames. In
// particular, the original Fixture and its temporaries must not occupy the
// reopened-store phase's frame after drop(f).
#[inline(never)]
fn prepare_cleanup_before_restart(path: &std::path::Path) -> (Scope, String, Team) {
    let f = fixture::remote_at(path, None);
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::Reply(json!({"ok":true})));
    let draft = current(&f).local().clone();
    checkpoint(&f, 2, &draft);
    r::migrate(&f.store, &f.scope.account_id).unwrap();
    let claim = cleanup(&f);
    let job = complete(&f, &claim);
    let scope = f.scope.clone();
    let old_team = storage::team(&f.store.0, &scope.key.team).unwrap();
    f.stream.revoke();
    drop(claim);
    drop(f);
    (scope, job, old_team)
}
#[inline(never)]
fn archive_cleanup_after_restart(
    db: &crate::drafts::store::sync::tests::Database,
    scope: Scope,
    job: String,
    old_team: Team,
) {
    let store = db.open();
    let state = operations::stream_state(&store, &scope.account_id, &scope.key.team).unwrap();
    let stream =
        operations::open_stream(&store, &scope.account_id, &scope.key.team, &state).unwrap();
    let view = views::review(&store.0, &scope).unwrap();
    let writer = writers::register(&store, &view).unwrap();
    let WriteResult::Applied(view) =
        writers::checkpoint(&store, &writer, 1, Some(&view), view.local(), None).unwrap()
    else {
        panic!("fresh accepted writer")
    };
    let before = view.clone();
    r::archive_cleanup(&store, &stream, &view, &writer, 1, &job).unwrap();
    let after = views::review(&store.0, &scope).unwrap();
    assert!(same_local(after.local(), before.local()));
    assert_eq!(after.row().state(), before.row().state());
    let new_team = storage::team(&store.0, &scope.key.team).unwrap();
    assert!(new_team.epoch > old_team.epoch);
    assert_ne!(new_team.lease_id, old_team.lease_id);
    assert_eq!(new_team.sequence, old_team.sequence);
    let retained: String = store
        .0
        .query_row(
            "SELECT content FROM composer_draft_slash_remote_jobs",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, job);
    r::audit(&store.0, &scope.account_id).unwrap();
    guard(&store.0, &scope).unwrap();
}
#[test]
fn archival_only_fresh_stream_rejects_revoked_owner_and_malformed_historical_team() {
    let f = start(false);
    let claim = cleanup(&f);
    let job = complete(&f, &claim);
    let draft = current(&f).local().clone();
    let view = checkpoint(&f, 3, &draft);
    let state = operations::stream_state(&f.store, &f.scope.account_id, &f.scope.key.team).unwrap();
    let stream =
        operations::open_stream(&f.store, &f.scope.account_id, &f.scope.key.team, &state).unwrap();
    assert!(r::archive_cleanup(&f.store, &f.stream, &view, &f.writer, 3, &job).is_err());
    r::archive_cleanup(&f.store, &stream, &view, &f.writer, 3, &job).unwrap();
    let original = raw(&f, "composer_draft_slash_remote_archives", "content");
    for (key, value) in [
        ("epoch", json!(0)),
        ("sequence", json!(0)),
        ("read_retry_at_ms", json!(0)),
        ("lease_id", json!("invalid")),
        ("last_interrupted_id", json!("invalid")),
    ] {
        let mut v: serde_json::Value = serde_json::from_str(&original).unwrap();
        v["team"][key] = value;
        f.store
            .0
            .execute(
                "UPDATE composer_draft_slash_remote_archives SET content=?1",
                [v.to_string()],
            )
            .unwrap();
        assert!(r::audit(&f.store.0, &f.scope.account_id).is_err(), "{key}");
    }
    for key in ["pending", "last_interrupted_id", "gap_reason"] {
        let mut v: serde_json::Value = serde_json::from_str(&original).unwrap();
        v["team"].as_object_mut().unwrap().remove(key);
        f.store
            .0
            .execute(
                "UPDATE composer_draft_slash_remote_archives SET content=?1",
                [v.to_string()],
            )
            .unwrap();
        assert!(
            r::audit(&f.store.0, &f.scope.account_id).is_err(),
            "missing {key}"
        );
    }
    f.store
        .0
        .execute(
            "UPDATE composer_draft_slash_remote_archives SET content=?1",
            [original],
        )
        .unwrap();
    r::audit(&f.store.0, &f.scope.account_id).unwrap();
}
#[test]
fn nested_transactions_never_return_uncommitted_read_or_cleanup_authority() {
    let f = start(true);
    f.store.0.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(
        r::reserve(
            &f.store,
            &f.stream,
            &current(&f),
            &f.writer,
            2,
            f.next.id(),
            1_000_000
        )
        .is_err()
    );
    assert!(r::migrate(&f.store, &f.scope.account_id).is_err());
    f.store.0.execute_batch("ROLLBACK").unwrap();
    assert_eq!(count(&f, "composer_draft_slash_remote_jobs"), 0);
}

#[test]
fn one_use_read_and_cleanup_claims_reject_duplicate_process_tickets() {
    let f = start(false);
    let ticket = r::reserve(
        &f.store,
        &f.stream,
        &current(&f),
        &f.writer,
        2,
        f.next.id(),
        1_000_000,
    )
    .unwrap();
    let duplicate = r::duplicate_read_ticket(&ticket);
    let claim = r::claim_read(&f.store, ticket).unwrap();
    assert!(r::claim_read(&f.store, duplicate).is_err());
    let report = r::read_report(&claim, r::ReadKind::Response, Some(body(&f)), 0);
    let raw = r::retain_read(&f.store, &claim, &report).unwrap();
    let choice = r::choice(raw, f.stream.clone(), r::Action::Cleanup);
    let ticket = r::reserve_cleanup(&f.store, &choice).unwrap();
    let duplicate = r::duplicate_cleanup_ticket(&ticket);
    r::claim_cleanup(&f.store, ticket).unwrap();
    assert!(r::claim_cleanup(&f.store, duplicate).is_err());
}
#[test]
fn cas_cannot_change_immutable_known_deadline_or_accept_noncanonical_replacement() {
    let f = start(true);
    let ticket = r::reserve(
        &f.store,
        &f.stream,
        &current(&f),
        &f.writer,
        2,
        f.next.id(),
        1_000_000,
    )
    .unwrap();
    let raw = raw_job(&f);
    assert!(r::mutate_reserved_deadline(&f.store, &raw).is_err());
    assert_eq!(raw_job(&f), raw);
    f.store
        .0
        .execute(
            "UPDATE composer_draft_slash_remote_jobs SET content=?1",
            [format!(" {raw} ")],
        )
        .unwrap();
    assert!(r::claim_read(&f.store, ticket).is_err());
}
#[test]
fn migration_preserves_existing_local_archive_bytes_and_v1_evidence() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::NotSent);
    let view = refreshed(&f);
    a::archive_known(&f.store, &view).unwrap();
    let before = raw(&f, "composer_draft_slash_archives", "content");
    r::migrate(&f.store, &f.scope.account_id).unwrap();
    assert_eq!(raw(&f, "composer_draft_slash_archives", "content"), before);
    guard(&f.store.0, &f.scope).unwrap();
}
#[test]
fn post_cleanup_lifecycle_change_cannot_use_typing_only_followup() {
    let f = start(false);
    let claim = cleanup(&f);
    let job = complete(&f, &claim);
    let original = current(&f);
    let raw = raw(&f, "composer_draft_sync", "state");
    let changed = raw.replace(
        original.row().state().client_msg_id(),
        &uuid::Uuid::new_v4().to_string(),
    );
    f.store
        .0
        .execute("UPDATE composer_draft_sync SET state=?1", [changed])
        .unwrap();
    let draft = current(&f).local().clone();
    let fresh = checkpoint(&f, 3, &draft);
    assert!(r::archive_cleanup(&f.store, &f.stream, &fresh, &f.writer, 3, &job).is_err());
    assert!(guard(&f.store.0, &f.scope).is_err());
}

#[path = "api_slash_remote_ledger_tests.rs"]
mod ledger_tests;

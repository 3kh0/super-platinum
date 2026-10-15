//! Actual SQLite regressions for the coupled remote and continuity ledgers.
use super::*;

fn ledger_claim(f: &Fixture) -> r::model::ReadClaim {
    let ticket = r::reserve(
        &f.store,
        &f.stream,
        &current(f),
        &f.writer,
        2,
        f.next.id(),
        1_000_000,
    )
    .unwrap();
    r::claim_read(&f.store, ticket).unwrap()
}
fn ledger_id(f: &Fixture) -> String {
    let job: serde_json::Value = serde_json::from_str(&raw_job(f)).unwrap();
    job["id"].as_str().unwrap().into()
}
fn ledger_raw(f: &Fixture) -> String {
    f.store
        .0
        .query_row(
            "SELECT content FROM composer_draft_continuity_reads WHERE id=?1",
            [ledger_id(f)],
            |r| r.get(0),
        )
        .unwrap()
}
#[test]
fn remote_read_claim_and_retention_share_exact_generic_continuity_ledger() {
    let f = start(true);
    let claim = ledger_claim(&f);
    let id = ledger_id(&f);
    let original = inventory::reads(&f.store.0).unwrap().remove(&id).unwrap();
    assert_eq!(original.state, ReadState::Unresolved);
    assert_eq!(
        storage::team(&f.store.0, &f.scope.key.team)
            .unwrap()
            .pending
            .unwrap()
            .id,
        id
    );
    let report = r::read_report(&claim, r::ReadKind::Response, Some(body(&f)), 0);
    r::retain_read(&f.store, &claim, &report).unwrap();
    let retained = inventory::reads(&f.store.0).unwrap().remove(&id).unwrap();
    assert_eq!(retained.state, ReadState::Finished);
    assert_eq!(
        retained.completion,
        Some(ReadCompletion::RetainedSlashRemote { version: 1 })
    );
    assert_eq!(retained.lease_id, original.lease_id);
    assert_eq!(retained.sequence, original.sequence);
    assert!(
        storage::team(&f.store.0, &f.scope.key.team)
            .unwrap()
            .pending
            .is_none()
    );
    r::audit(&f.store.0, &f.scope.account_id).unwrap();
}
#[test]
fn missing_or_substituted_continuity_admission_still_refuses_read_claim() {
    for remove in [true, false] {
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
        let id = ledger_id(&f);
        let job = raw_job(&f);
        if remove {
            f.store
                .0
                .execute(
                    "DELETE FROM composer_draft_continuity_reads WHERE id=?1",
                    [id],
                )
                .unwrap();
        } else {
            let mut value: serde_json::Value = serde_json::from_str(&ledger_raw(&f)).unwrap();
            value["enrollment_id"] = json!(uuid::Uuid::new_v4().to_string());
            f.store
                .0
                .execute(
                    "UPDATE composer_draft_continuity_reads SET content=?1 WHERE id=?2",
                    params![value.to_string(), id],
                )
                .unwrap();
        }
        assert!(r::claim_read(&f.store, ticket).is_err());
        assert_eq!(raw_job(&f), job);
        assert!(storage::team(&f.store.0, &f.scope.key.team).is_err());
    }
}
#[test]
fn retained_remote_marker_cannot_outlive_or_substitute_its_exact_job() {
    let f = start(true);
    read(&f);
    let id = ledger_id(&f);
    let original = raw_job(&f);
    let mut value: serde_json::Value = serde_json::from_str(&original).unwrap();
    value["team"]["lease_id"] = json!(uuid::Uuid::new_v4().to_string());
    f.store
        .0
        .execute(
            "UPDATE composer_draft_slash_remote_jobs SET content=?1",
            [value.to_string()],
        )
        .unwrap();
    assert!(inventory::reads(&f.store.0).is_err());
    f.store
        .0
        .execute(
            "DELETE FROM composer_draft_slash_remote_jobs WHERE id=?1",
            [id],
        )
        .unwrap();
    assert!(inventory::reads(&f.store.0).is_err());
}
#[test]
fn reservation_ledger_insert_failure_rolls_back_team_and_job() {
    let f = start(true);
    let before = storage::team(&f.store.0, &f.scope.key.team).unwrap();
    let records = inventory::reads(&f.store.0).unwrap();
    f.store.0.execute_batch("CREATE TRIGGER fail_read BEFORE INSERT ON composer_draft_continuity_reads BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
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
    assert_eq!(count(&f, "composer_draft_slash_remote_jobs"), 0);
}
#[test]
fn read_receipt_ledger_and_job_failures_roll_back_all_coupled_evidence() {
    for table in [
        "composer_draft_continuity_reads",
        "composer_draft_slash_remote_jobs",
    ] {
        let f = start(true);
        let claim = ledger_claim(&f);
        let old_job = raw_job(&f);
        let old_ledger = ledger_raw(&f);
        let old_team = storage::team(&f.store.0, &f.scope.key.team).unwrap();
        let old_deadline = recovery::deadline(&f.store.0, &f.scope.key.team).unwrap();
        f.store.0.execute_batch(&format!("CREATE TRIGGER fail_receipt BEFORE UPDATE ON {table} BEGIN SELECT RAISE(ABORT,'fixture'); END;")).unwrap();
        let report = r::read_report(&claim, r::ReadKind::Response, Some(body(&f)), 5_000_000);
        assert!(r::retain_read(&f.store, &claim, &report).is_err());
        assert_eq!(raw_job(&f), old_job);
        assert_eq!(ledger_raw(&f), old_ledger);
        assert_eq!(
            storage::team(&f.store.0, &f.scope.key.team).unwrap(),
            old_team
        );
        assert_eq!(
            recovery::deadline(&f.store.0, &f.scope.key.team).unwrap(),
            old_deadline
        );
        f.store
            .0
            .execute_batch("DROP TRIGGER fail_receipt;")
            .unwrap();
        r::retain_read(&f.store, &claim, &report).unwrap();
        assert_ne!(ledger_raw(&f), old_ledger);
    }
}
#[test]
fn unknown_report_keeps_generic_record_unresolved_and_retry_floor_after_new_stream() {
    let f = start(true);
    let claim = ledger_claim(&f);
    let id = ledger_id(&f);
    let original = inventory::reads(&f.store.0).unwrap().remove(&id).unwrap();
    let report = r::read_report(&claim, r::ReadKind::Unknown, None, 5_000_000);
    r::retain_read(&f.store, &claim, &report).unwrap();
    assert_eq!(
        inventory::reads(&f.store.0).unwrap().remove(&id).unwrap(),
        original
    );
    let state = operations::stream_state(&f.store, &f.scope.account_id, &f.scope.key.team).unwrap();
    let stream =
        operations::open_stream(&f.store, &f.scope.account_id, &f.scope.key.team, &state).unwrap();
    assert!(readback::unresolved(&f.store.0, &f.scope.key.team).unwrap());
    assert!(
        r::reserve(
            &f.store,
            &stream,
            &current(&f),
            &f.writer,
            2,
            f.next.id(),
            6_000_000
        )
        .is_err()
    );
    r::audit(&f.store.0, &f.scope.account_id).unwrap();
}
#[test]
fn original_read_report_after_real_supersession_retains_recovery_provenance() {
    let f = start(true);
    let claim = ledger_claim(&f);
    let id = ledger_id(&f);
    let payload = body(&f);
    let permit =
        recovery::take_over(&f.store, &f.scope.account_id, &f.scope.key.team, None).unwrap();
    let view = current(&f);
    recovery::supersede_baseline(&f.store, &permit, &view.inner, &id, 2_000_000).unwrap();
    let previous = inventory::reads(&f.store.0).unwrap().remove(&id).unwrap();
    assert_eq!(previous.state, ReadState::SupersededUnknown);
    let abandonment = raw(&f, "composer_draft_read_abandonment", "content");
    let report = r::read_report(&claim, r::ReadKind::Response, Some(payload), 0);
    let job = r::retain_read(&f.store, &claim, &report).unwrap();
    let after = inventory::reads(&f.store.0).unwrap().remove(&id).unwrap();
    assert_eq!(after.state, ReadState::SupersededUnknown);
    assert_eq!(after.completion, previous.completion);
    assert_eq!(
        raw(&f, "composer_draft_read_abandonment", "content"),
        abandonment
    );
    assert!(current(&f).same(&view));
    r::audit(&f.store.0, &f.scope.account_id).unwrap();
    assert!(
        r::preserve(
            &f.store,
            &r::choice(job, f.stream.clone(), r::Action::Preserve)
        )
        .is_err()
    );
}

#[test]
fn unknown_remote_read_survives_disk_restart_as_unresolved_without_dispatch_handle() {
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
    let claim = ledger_claim(&f);
    let id = ledger_id(&f);
    let report = r::read_report(&claim, r::ReadKind::Unknown, None, 5_000_000);
    let job = r::retain_read(&f.store, &claim, &report).unwrap();
    let ledger = ledger_raw(&f);
    let scope = f.scope.clone();
    f.stream.revoke();
    drop(claim);
    drop(f);
    let store = db.open();
    let state = operations::stream_state(&store, &scope.account_id, &scope.key.team).unwrap();
    let _stream =
        operations::open_stream(&store, &scope.account_id, &scope.key.team, &state).unwrap();
    let retained: String = store
        .0
        .query_row(
            "SELECT content FROM composer_draft_continuity_reads WHERE id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, ledger);
    let retained: String = store
        .0
        .query_row(
            "SELECT content FROM composer_draft_slash_remote_jobs WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, job);
    assert!(readback::unresolved(&store.0, &scope.key.team).unwrap());
    r::audit(&store.0, &scope.account_id).unwrap();
    assert!(guard(&store.0, &scope).is_err());
}

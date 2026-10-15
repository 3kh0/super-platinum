//! Actual SQLite regressions. Every report/choice is explicit cfg(test) evidence.
use super::*;
#[path = "api_slash_remote_fixture.rs"]
mod fixture;
#[path = "api_slash_remote_retention_tests.rs"]
mod retention;
use super::super::super::remote as r;
fn start(keep: bool) -> Fixture {
    let f = remote_fixture();
    let (_, target) = setup(&f);
    finish(
        &f,
        &target,
        Delivery::Reply(json!({"ok":true,"keep_input":keep})),
    );
    let local = current(&f).local().clone();
    checkpoint(&f, 2, &local);
    r::migrate(&f.store, &f.scope.account_id).unwrap();
    f
}
fn body(f: &Fixture) -> String {
    let remote = current(f).row().state().acknowledged().unwrap().clone();
    json!({"ok":true,"draft":{"id":remote.id,"client_msg_id":remote.client_msg_id,"last_updated_ts":"12.000001",
 "team_id":f.scope.key.team,"user_id":"U1","is_deleted":false,"is_sent":false,"date_scheduled":0,
 "blocks":remote.snapshot.blocks,"file_ids":remote.snapshot.file_ids,"destinations":remote.snapshot.destinations}}).to_string()
}
fn read(f: &Fixture) -> String {
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
    let claim = r::claim_read(&f.store, ticket).unwrap();
    let report = r::read_report(&claim, r::ReadKind::Response, Some(body(f)), 0);
    r::retain_read(&f.store, &claim, &report).unwrap()
}
fn preserved(f: &Fixture) -> String {
    let raw = read(f);
    let choice = r::choice(raw, f.stream.clone(), r::Action::Preserve);
    r::preserve(&f.store, &choice).unwrap();
    raw_job(f)
}
fn raw_job(f: &Fixture) -> String {
    raw(f, "composer_draft_slash_remote_jobs", "content")
}
fn cleanup(f: &Fixture) -> r::model::CleanupClaim {
    let raw = read(f);
    let choice = r::choice(raw, f.stream.clone(), r::Action::Cleanup);
    let ticket = r::reserve_cleanup(&f.store, &choice).unwrap();
    r::claim_cleanup(&f.store, ticket).unwrap()
}
fn complete(f: &Fixture, claim: &r::model::CleanupClaim) -> String {
    let report = r::cleanup_report(
        claim,
        r::CleanupKind::Response,
        Some(json!({"ok":true}).to_string()),
        0,
    );
    r::retain_cleanup(&f.store, claim, &report).unwrap()
}
fn archived_cleanup(f: &Fixture, job: &str, serial: u64) {
    let local = current(f).local().clone();
    let before = checkpoint(f, serial, &local);
    r::archive_cleanup(&f.store, &f.stream, &before, &f.writer, serial, job).unwrap();
}
#[test]
fn verified_remote_read_does_not_manufacture_gap_or_change_basis() {
    let f = start(true);
    let before = current(&f);
    let team = storage::team(&f.store.0, &f.scope.key.team).unwrap();
    read(&f);
    assert!(current(&f).same(&before));
    let after = storage::team(&f.store.0, &f.scope.key.team).unwrap();
    assert_eq!(team.epoch, after.epoch);
    assert_eq!(team.lease_id, after.lease_id);
    assert_eq!(after.mode, Mode::Open);
    assert_eq!(current(&f).row().lineage().basis(), Basis::Remote);
}
#[test]
fn named_read_accepts_legitimately_unverified_epoch_without_fake_gap() {
    let f = start(true);
    operations::mark_gap(&f.store, &f.stream, GapReason::Disconnect).unwrap();
    let expected =
        operations::stream_state(&f.store, &f.scope.account_id, &f.scope.key.team).unwrap();
    let stream =
        operations::open_stream(&f.store, &f.scope.account_id, &f.scope.key.team, &expected)
            .unwrap();
    assert!(
        r::reserve(
            &f.store,
            &stream,
            &current(&f),
            &f.writer,
            2,
            f.next.id(),
            1_000_000
        )
        .is_ok()
    );
}
#[test]
fn wrong_intent_writer_serial_and_same_text_aba_refuse_reservation() {
    let f = start(true);
    let before = current(&f);
    assert!(
        r::reserve(
            &f.store,
            &f.stream,
            &before,
            &f.writer,
            1,
            f.next.id(),
            1_000_000
        )
        .is_err()
    );
    assert!(
        r::reserve(
            &f.store,
            &f.stream,
            &before,
            &f.writer,
            2,
            &uuid::Uuid::new_v4().to_string(),
            1_000_000
        )
        .is_err()
    );
    f.store
        .0
        .execute("UPDATE composer_drafts SET content=content", [])
        .unwrap();
    assert!(
        r::reserve(
            &f.store,
            &f.stream,
            &before,
            &f.writer,
            2,
            f.next.id(),
            1_000_000
        )
        .is_err()
    );
    assert_eq!(count(&f, "composer_draft_slash_remote_jobs"), 0);
}
#[test]
fn preserve_archives_clean_remote_without_clearing_or_deleting_and_retry_is_historical() {
    let f = start(true);
    let raw = read(&f);
    let choice = r::choice(raw, f.stream.clone(), r::Action::Preserve);
    let before = current(&f);
    let done = r::preserve(&f.store, &choice).unwrap();
    let after = current(&f);
    assert!(same_local(before.local(), after.local()));
    assert_eq!(after.row().lineage().basis(), Basis::Remote);
    assert_eq!(after.row().state().acknowledged().unwrap().id, "D1");
    assert!(after.row().state().pending().is_none());
    guard(&f.store.0, &f.scope).unwrap();
    assert_eq!(count(&f, "composer_draft_slash_remote_archives"), 1);
    assert!(done.historical().unwrap().same(&after));
    checkpoint(&f, 3, &local("later typing"));
    let later = current(&f);
    r::preserve(&f.store, &choice).unwrap();
    assert!(current(&f).same(&later));
}
#[test]
fn dirty_or_empty_preserve_never_enables_implicit_remote_write() {
    for keep in [false, true] {
        let f = start(keep);
        if keep {
            checkpoint(&f, 3, &local("newer"));
        }
        let serial = if keep { 3 } else { 2 };
        let ticket = r::reserve(
            &f.store,
            &f.stream,
            &current(&f),
            &f.writer,
            serial,
            f.next.id(),
            1_000_000,
        )
        .unwrap();
        let claim = r::claim_read(&f.store, ticket).unwrap();
        let report = r::read_report(&claim, r::ReadKind::Response, Some(body(&f)), 0);
        let raw = r::retain_read(&f.store, &claim, &report).unwrap();
        let choice = r::choice(raw, f.stream.clone(), r::Action::Preserve);
        let before = current(&f);
        assert!(r::preserve(&f.store, &choice).is_err());
        assert!(current(&f).same(&before));
        assert!(guard(&f.store.0, &f.scope).is_err());
    }
}
#[test]
fn malformed_wrong_owner_lifecycle_and_absence_never_become_choice() {
    for mutation in 0..6 {
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
        let claim = r::claim_read(&f.store, ticket).unwrap();
        let mut raw: serde_json::Value = serde_json::from_str(&body(&f)).unwrap();
        match mutation {
            0 => raw["draft"]["id"] = json!("D_OTHER"),
            1 => raw["draft"]["user_id"] = json!("U_OTHER"),
            2 => raw["draft"]["is_deleted"] = json!(true),
            3 => raw["draft"]["date_scheduled"] = json!(10),
            4 => raw = json!({"ok":false,"error":"draft_not_found"}),
            _ => raw["error"] = json!("contradiction"),
        }
        let report = r::read_report(&claim, r::ReadKind::Response, Some(raw.to_string()), 0);
        let raw = r::retain_read(&f.store, &claim, &report).unwrap();
        let choice = r::choice(raw, f.stream.clone(), r::Action::Preserve);
        assert!(r::preserve(&f.store, &choice).is_err());
        assert!(guard(&f.store.0, &f.scope).is_err());
    }
}
#[test]
fn unknown_read_retains_pending_and_cannot_be_superseded_by_new_read() {
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
    let claim = r::claim_read(&f.store, ticket).unwrap();
    let report = r::read_report(&claim, r::ReadKind::Unknown, None, 0);
    r::retain_read(&f.store, &claim, &report).unwrap();
    assert!(
        storage::team(&f.store.0, &f.scope.key.team)
            .unwrap()
            .pending
            .is_some()
    );
    assert!(
        r::reserve(
            &f.store,
            &f.stream,
            &current(&f),
            &f.writer,
            2,
            f.next.id(),
            9_000_000
        )
        .is_err()
    );
}
#[test]
fn read_cooldown_and_exact_receipt_retry_are_durable() {
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
    let claim = r::claim_read(&f.store, ticket).unwrap();
    let report = r::read_report(&claim, r::ReadKind::NotRequested, None, 5_000_000);
    let first = r::retain_read(&f.store, &claim, &report).unwrap();
    assert_eq!(r::retain_read(&f.store, &claim, &report).unwrap(), first);
    assert!(
        r::reserve(
            &f.store,
            &f.stream,
            &current(&f),
            &f.writer,
            2,
            f.next.id(),
            4_999_999
        )
        .is_err()
    );
    assert!(
        r::reserve(
            &f.store,
            &f.stream,
            &current(&f),
            &f.writer,
            2,
            f.next.id(),
            5_000_000
        )
        .is_ok()
    );
}
#[test]
fn explicit_rejected_choice_is_distinct_from_ack_or_not_sent() {
    let f = remote_fixture();
    let (_, target) = setup(&f);
    finish(
        &f,
        &target,
        Delivery::Reply(json!({"ok":false,"error":"invalid_command"})),
    );
    let local = current(&f).local().clone();
    checkpoint(&f, 2, &local);
    r::migrate(&f.store, &f.scope.account_id).unwrap();
    let raw = read(&f);
    let wrong = r::choice(raw.clone(), f.stream.clone(), r::Action::Preserve);
    assert!(r::preserve(&f.store, &wrong).is_err());
    let explicit = r::choice(raw, f.stream.clone(), r::Action::PreserveRejected);
    r::preserve(&f.store, &explicit).unwrap();
    guard(&f.store.0, &f.scope).unwrap();
}
#[test]
fn unknown_command_never_acquires_remote_recovery_admission() {
    let f = remote_fixture();
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::Unknown);
    let draft = current(&f).local().clone();
    checkpoint(&f, 2, &draft);
    r::migrate(&f.store, &f.scope.account_id).unwrap();
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
}
#[test]
fn cleanup_reserves_real_delete_without_editing_empty_or_nonempty_current_input() {
    for keep in [false, true] {
        let f = start(keep);
        let before = current(&f);
        let claim = cleanup(&f);
        let after = current(&f);
        let op = after.row().state().pending().unwrap();
        assert_eq!(op.kind, crate::drafts::sync::OperationKind::Delete);
        assert!(op.snapshot.is_none());
        assert_eq!(op.base.as_ref().unwrap().id, "D1");
        assert_eq!(
            before.row().state().desired(),
            after.row().state().desired()
        );
        assert!(same_local(before.local(), after.local()));
        let result = complete(&f, &claim);
        archived_cleanup(&f, &result, 3);
        guard(&f.store.0, &f.scope).unwrap();
    }
}
#[test]
fn positive_cleanup_preserves_newer_input_and_retired_identity_then_explicit_release() {
    let f = start(false);
    let claim = cleanup(&f);
    checkpoint(&f, 3, &local("newer during delete"));
    let before = current(&f);
    let raw = complete(&f, &claim);
    let after = current(&f);
    assert!(same_local(before.local(), after.local()));
    assert_eq!(
        after.row().state().desired(),
        before.row().state().desired()
    );
    assert!(after.row().state().acknowledged().is_none());
    assert!(after.row().state().has_remote_identity_history());
    assert_eq!(after.row().lineage().basis(), Basis::Unverified);
    assert_eq!(after.row().lineage().verified_epoch(), 0);
    assert!(guard(&f.store.0, &f.scope).is_err());
    archived_cleanup(&f, &raw, 4);
    let current = current(&f);
    assert!(same_local(current.local(), before.local()));
    super::super::archive::release(&f.store, &current.inner).unwrap();
    assert_eq!(count(&f, "composer_draft_protected_archive"), 1);
}
#[test]
fn cleanup_unknown_rejected_malformed_and_not_requested_never_archive_or_replay() {
    for (kind, body) in [
        (r::CleanupKind::Unknown, None),
        (r::CleanupKind::NotRequested, None),
        (r::CleanupKind::Response, Some("{bad".into())),
        (
            r::CleanupKind::Response,
            Some(json!({"ok":false,"error":"invalid_draft"}).to_string()),
        ),
        (
            r::CleanupKind::Response,
            Some(json!({"ok":true,"draft_id":"OTHER"}).to_string()),
        ),
    ] {
        let f = start(false);
        let claim = cleanup(&f);
        let report = r::cleanup_report(&claim, kind, body, 0);
        let raw = r::retain_cleanup(&f.store, &claim, &report).unwrap();
        let view = current(&f);
        let draft = view.local().clone();
        let fresh = checkpoint(&f, 3, &draft);
        assert!(r::archive_cleanup(&f.store, &f.stream, &fresh, &f.writer, 3, &raw).is_err());
        assert!(guard(&f.store.0, &f.scope).is_err());
        assert!(current(&f).row().state().pending().is_some());
    }
}
#[test]
fn stale_read_choice_or_cleanup_claim_never_rebases_over_input() {
    for stage in 0..2 {
        let f = start(true);
        let raw = read(&f);
        let choice = r::choice(raw, f.stream.clone(), r::Action::Cleanup);
        if stage == 0 {
            checkpoint(&f, 3, &local("typed"));
            assert!(r::reserve_cleanup(&f.store, &choice).is_err());
        } else {
            let ticket = r::reserve_cleanup(&f.store, &choice).unwrap();
            checkpoint(&f, 3, &local("typed"));
            assert!(r::claim_cleanup(&f.store, ticket).is_err());
        }
        assert!(guard(&f.store.0, &f.scope).is_err());
    }
}
#[test]
fn revoked_source_late_cleanup_is_historical_and_never_writes_successor() {
    let f = start(false);
    let claim = cleanup(&f);
    f.stream.revoke();
    let before = current(&f);
    let raw = complete(&f, &claim);
    assert!(current(&f).same(&before));
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(value["reduced_after"].is_null());
    assert!(value["result"].is_object());
}
#[test]
fn replaced_pending_operation_late_ack_is_retained_without_touching_replacement() {
    let f = start(false);
    let claim = cleanup(&f);
    let (key, raw): (String, String) = f
        .store
        .0
        .query_row(
            "SELECT destination,state FROM composer_draft_sync",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let op = current(&f).row().state().pending().unwrap().id.clone();
    let replacement = raw.replace(&op, &uuid::Uuid::new_v4().to_string());
    assert_ne!(replacement, raw);
    f.store
        .0
        .execute(
            "UPDATE composer_draft_sync SET state=?1 WHERE destination=?2",
            params![replacement, key],
        )
        .unwrap();
    let before = current(&f);
    let raw = complete(&f, &claim);
    assert!(current(&f).same(&before));
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(value["reduced_after"].is_null());
}
#[test]
fn cleanup_exact_retry_after_newer_input_never_reduces_twice() {
    let f = start(false);
    let claim = cleanup(&f);
    let report = r::cleanup_report(
        &claim,
        r::CleanupKind::Response,
        Some("{\"ok\":true}".into()),
        0,
    );
    let first = r::retain_cleanup(&f.store, &claim, &report).unwrap();
    checkpoint(&f, 3, &local("later"));
    let before = current(&f);
    assert_eq!(r::retain_cleanup(&f.store, &claim, &report).unwrap(), first);
    assert!(current(&f).same(&before));
}
#[test]
fn cleanup_archive_retry_returns_history_without_touching_later_input() {
    let f = start(false);
    let claim = cleanup(&f);
    let raw = complete(&f, &claim);
    let draft = current(&f).local().clone();
    let before = checkpoint(&f, 3, &draft);
    r::archive_cleanup(&f.store, &f.stream, &before, &f.writer, 3, &raw).unwrap();
    checkpoint(&f, 4, &local("later"));
    let later = current(&f);
    r::archive_cleanup(&f.store, &f.stream, &before, &f.writer, 3, &raw).unwrap();
    assert!(current(&f).same(&later));
}

#[test]
fn slice2a_inspection_preserves_actual_v3_remote_jobs_archives_and_read_ledger() {
    let f = start(true);
    read(&f);
    crate::drafts::store::protected::upgrade::audit_test_store(&f.store, &f.scope.account_id)
        .unwrap();
    let f = start(false);
    let claim = cleanup(&f);
    let raw = complete(&f, &claim);
    let draft = current(&f).local().clone();
    let before = checkpoint(&f, 3, &draft);
    r::archive_cleanup(&f.store, &f.stream, &before, &f.writer, 3, &raw).unwrap();
    crate::drafts::store::protected::upgrade::audit_test_store(&f.store, &f.scope.account_id)
        .unwrap();
}

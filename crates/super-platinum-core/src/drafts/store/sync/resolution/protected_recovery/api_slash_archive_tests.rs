//! Actual SQLite ownership regressions; no HTTP, production store or UI consent.
#[path = "api_slash_remote_tests.rs"]
mod remote_tests;
use super::super::{archive as a, archive_storage as evidence, completion as c, history as h};
use super::*;
use crate::slash_commands::controller::{self, Delivery, ManagedTerminal};
fn outcome(f: &Fixture, delivery: Delivery) -> ManagedTerminal {
    controller::isolated_terminal(&f.next, delivery, None)
}
fn setup(f: &Fixture) -> (Claim, c::Target) {
    c::install(&f.store, &f.scope.account_id).unwrap();
    h::migrate(&f.store, &f.scope.account_id).unwrap();
    let claim = f.claim().unwrap();
    let target = c::target(&claim);
    (claim, target)
}
fn finish(f: &Fixture, target: &c::Target, delivery: Delivery) -> ManagedTerminal {
    let value = outcome(f, delivery);
    c::complete_attested(&f.store, target, &value).unwrap();
    value
}
fn current(f: &Fixture) -> Review {
    views::review(&f.store.0, &f.scope).unwrap()
}
fn checkpoint(f: &Fixture, serial: u64, draft: &LocalDraft) -> Review {
    let before = current(f);
    let WriteResult::Applied(after) =
        writers::checkpoint(&f.store, &f.writer, serial, Some(&before), draft, None).unwrap()
    else {
        panic!("accepted current writer")
    };
    after
}
fn inspected(f: &Fixture, serial: u64) -> a::ArchiveReview {
    a::inspect(
        &f.store,
        &f.stream,
        &current(f),
        &f.writer,
        serial,
        f.next.id(),
    )
    .unwrap()
}
fn refreshed(f: &Fixture) -> a::ArchiveReview {
    let local = current(f).local().clone();
    checkpoint(f, 2, &local);
    inspected(f, 2)
}
fn count(f: &Fixture, table: &str) -> i64 {
    f.store
        .0
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn raw(f: &Fixture, table: &str, column: &str) -> String {
    f.store
        .0
        .query_row(&format!("SELECT {column} FROM {table} LIMIT 1"), [], |r| {
            r.get(0)
        })
        .unwrap()
}
fn active(f: &Fixture) {
    assert!(guard(&f.store.0, &f.scope).is_err());
    assert_eq!(count(f, "composer_draft_slash_bindings"), 1);
    assert_eq!(count(f, "composer_draft_slash_archives"), 0);
}
#[test]
fn ack_false_archives_exact_empty_pair_without_recreating_or_clearing_input() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::Reply(json!({"ok":true})));
    assert!(current(&f).local().empty());
    assert!(current(&f).row().state().desired().is_none());
    let review = refreshed(&f);
    let before = current(&f);
    let writer = raw(&f, "composer_draft_writer_lineage", "content");
    let result = a::archive_known(&f.store, &review).unwrap();
    assert_eq!(result.reason(), a::Reason::LocalAcknowledged);
    assert_eq!(result.original_intent(), f.next.id());
    let after = current(&f);
    assert_eq!(after.row().state(), before.row().state());
    assert_eq!(after.row().revision(), before.row().revision() + 1);
    assert!(same_local(after.local(), before.local()));
    assert!(after.same(&result.historical_review().unwrap()));
    assert_eq!(writer, raw(&f, "composer_draft_writer_lineage", "content"));
    assert_eq!(count(&f, "composer_draft_slash_bindings"), 0);
    assert_eq!(count(&f, "composer_draft_slash_archives"), 1);
    guard(&f.store.0, &f.scope).unwrap();
}
#[test]
fn keep_input_and_newer_input_are_preserved_for_both_ack_values() {
    for keep in [false, true] {
        let f = Fixture::new();
        let (_, target) = setup(&f);
        finish(
            &f,
            &target,
            Delivery::Reply(json!({"ok":true,"keep_input":keep})),
        );
        let newer = checkpoint(&f, 2, &local("newer after response"));
        let review = inspected(&f, 2);
        a::archive_known(&f.store, &review).unwrap();
        assert!(same_local(current(&f).local(), newer.local()));
        assert_eq!(current(&f).row().state(), newer.row().state());
    }
}
#[test]
fn post_clear_requires_current_accepted_serial_not_original_writer_receipt() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::Reply(json!({"ok":true})));
    assert!(matches!(
        a::inspect(&f.store, &f.stream, &current(&f), &f.writer, 1, f.next.id()),
        Err(a::ArchiveError::Stale)
    ));
    let reviewed = refreshed(&f);
    a::archive_known(&f.store, &reviewed).unwrap();
}
#[test]
fn true_not_sent_is_report_derived_and_preserves_local_input() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    let terminal = finish(&f, &target, Delivery::NotSent);
    assert!(matches!(terminal.terminal().phase(), Phase::NotSent));
    let reviewed = refreshed(&f);
    let before = current(&f);
    let result = a::archive_known(&f.store, &reviewed).unwrap();
    assert_eq!(result.reason(), a::Reason::ProvenNotSent);
    assert!(same_local(current(&f).local(), before.local()));
}
#[test]
fn decoded_record_cannot_backfill_delivery_provenance() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    let attested = outcome(&f, Delivery::NotSent);
    c::complete(&f.store, &target, attested.terminal()).unwrap();
    assert!(c::complete_attested(&f.store, &target, &attested).is_err());
    assert_eq!(count(&f, "composer_draft_slash_attestations"), 0);
    let draft = current(&f).local().clone();
    checkpoint(&f, 2, &draft);
    assert!(matches!(
        a::inspect(&f.store, &f.stream, &current(&f), &f.writer, 2, f.next.id()),
        Err(a::ArchiveError::Pending)
    ));
    active(&f);
}
#[test]
fn rejected_requires_separate_exact_explicit_choice_and_never_not_sent() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    finish(
        &f,
        &target,
        Delivery::Reply(json!({"ok":false,"error":"invalid_command"})),
    );
    let reviewed = refreshed(&f);
    assert!(matches!(
        a::archive_known(&f.store, &reviewed),
        Err(a::ArchiveError::ChoiceRequired)
    ));
    active(&f);
    let choice = a::isolated_rejection_choice(reviewed);
    let before = current(&f);
    let result = a::archive_rejection(&f.store, &choice).unwrap();
    assert_eq!(result.reason(), a::Reason::RejectedPreserveRetire);
    assert!(same_local(before.local(), current(&f).local()));
    a::archive_rejection(&f.store, &choice).unwrap();
    assert_eq!(count(&f, "composer_draft_slash_archives"), 1);
}
#[test]
fn stale_rejection_choice_cannot_retire_after_newer_input() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    finish(
        &f,
        &target,
        Delivery::Reply(json!({"ok":false,"error":"invalid_command"})),
    );
    let choice = a::isolated_rejection_choice(refreshed(&f));
    checkpoint(&f, 3, &local("newer"));
    assert!(matches!(
        a::archive_rejection(&f.store, &choice),
        Err(a::ArchiveError::Stale)
    ));
    active(&f);
}
#[test]
fn all_unknown_delivery_classes_remain_active_even_with_explicit_choice() {
    let deliveries = [
        Delivery::Unknown,
        Delivery::HttpFailure(429),
        Delivery::Reply(json!({"ok":false,"error":"dispatch_failed"})),
        Delivery::Reply(json!({"ok":true,"keep_input":null})),
    ];
    for delivery in deliveries {
        let f = Fixture::new();
        let (_, target) = setup(&f);
        finish(&f, &target, delivery);
        let review = refreshed(&f);
        assert!(matches!(
            a::archive_known(&f.store, &review),
            Err(a::ArchiveError::Pending)
        ));
        let choice = a::isolated_rejection_choice(review);
        assert!(matches!(
            a::archive_rejection(&f.store, &choice),
            Err(a::ArchiveError::Pending)
        ));
        active(&f);
    }
}
#[test]
fn migration_preserves_existing_v1_bytes_without_attesting_or_retiring() {
    let f = Fixture::new();
    c::install(&f.store, &f.scope.account_id).unwrap();
    let claim = f.claim().unwrap();
    let target = c::target(&claim);
    let terminal = outcome(&f, Delivery::NotSent);
    c::complete(&f.store, &target, terminal.terminal()).unwrap();
    let binding = raw(&f, "composer_draft_slash_bindings", "content");
    let settlement = raw(&f, "composer_draft_slash_completions", "content");
    let before = current(&f);
    h::migrate(&f.store, &f.scope.account_id).unwrap();
    h::migrate(&f.store, &f.scope.account_id).unwrap();
    assert_eq!(binding, raw(&f, "composer_draft_slash_bindings", "content"));
    assert_eq!(
        settlement,
        raw(&f, "composer_draft_slash_completions", "content")
    );
    assert!(current(&f).same(&before));
    assert_eq!(count(&f, "composer_draft_slash_attestations"), 0);
    active(&f);
}
#[test]
fn migration_failure_rolls_back_marker_and_new_tables() {
    let f = Fixture::new();
    c::install(&f.store, &f.scope.account_id).unwrap();
    f.store.0.execute_batch("CREATE TRIGGER fail_retention_marker BEFORE UPDATE ON composer_draft_slash_format BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(h::migrate(&f.store, &f.scope.account_id).is_err());
    let n:i64=f.store.0.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('composer_draft_slash_archives','composer_draft_slash_attestations')",[],|r|r.get(0)).unwrap();
    assert_eq!(n, 0);
    assert!(!h::v2(&f.store.0).unwrap());
}
#[test]
fn partial_future_wrong_account_and_missing_completion_migration_fail_closed() {
    for mutation in [
        "CREATE TABLE composer_draft_slash_archives(x TEXT)",
        "UPDATE composer_draft_slash_format SET version=3",
        "UPDATE composer_draft_slash_format SET account='other'",
        "DROP TABLE composer_draft_slash_completions",
    ] {
        let f = Fixture::new();
        c::install(&f.store, &f.scope.account_id).unwrap();
        f.store.0.execute_batch(mutation).unwrap();
        assert!(h::migrate(&f.store, &f.scope.account_id).is_err());
    }
}
#[test]
fn attestation_insert_failure_rolls_back_terminal_cooldown_clear_and_receipt() {
    for rate in [false, true] {
        let f = Fixture::new();
        let (claim, target) = setup(&f);
        f.store.0.execute_batch("CREATE TRIGGER fail_attestation BEFORE INSERT ON composer_draft_slash_attestations BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
        let terminal = if rate {
            controller::isolated_terminal(&f.next, Delivery::HttpFailure(429), Some(25))
        } else {
            outcome(&f, Delivery::Reply(json!({"ok":true})))
        };
        assert!(c::complete_attested(&f.store, &target, &terminal).is_err());
        assert!(current(&f).same(claim.review()));
        assert_eq!(count(&f, "composer_draft_slash_completions"), 0);
        assert_eq!(count(&f, "composer_draft_slash_attestations"), 0);
        assert_eq!(count(&f, "sp_slash_cooldowns"), 0);
        let records = slash_journal::protected_inventory(&f.store.0, &f.scope.account_id).unwrap();
        assert_eq!(
            records.iter().find(|r| r.id() == f.next.id()),
            Some(&f.next)
        );
    }
}
#[test]
fn archive_failures_roll_back_journal_move_and_row_successor() {
    for trigger in [
        "CREATE TRIGGER fail_archive BEFORE INSERT ON composer_draft_slash_archives BEGIN SELECT RAISE(ABORT,'fixture'); END",
        "CREATE TRIGGER fail_archive BEFORE DELETE ON composer_draft_slash_bindings BEGIN SELECT RAISE(ABORT,'fixture'); END",
        "CREATE TRIGGER fail_archive BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'fixture'); END",
        "CREATE TRIGGER fail_archive BEFORE UPDATE ON sp_slash_intents BEGIN SELECT RAISE(ABORT,'fixture'); END",
    ] {
        let f = Fixture::new();
        let (_, target) = setup(&f);
        let end = finish(&f, &target, Delivery::NotSent);
        let review = refreshed(&f);
        let before = current(&f);
        f.store.0.execute_batch(trigger).unwrap();
        assert!(a::archive_known(&f.store, &review).is_err());
        assert!(current(&f).same(&before));
        active(&f);
        let records = slash_journal::protected_inventory(&f.store.0, &f.scope.account_id).unwrap();
        assert_eq!(
            records.iter().find(|r| r.id() == f.next.id()),
            Some(end.terminal())
        );
    }
}
#[test]
fn same_text_serial_selection_and_external_sql_aba_refuse_old_archive_review() {
    for change in 0..3 {
        let f = Fixture::new();
        let (_, target) = setup(&f);
        finish(&f, &target, Delivery::NotSent);
        let review = refreshed(&f);
        match change {
            0 => {
                let draft = current(&f).local().clone();
                checkpoint(&f, 3, &draft);
            }
            1 => {
                let mut draft = current(&f).local().clone();
                draft.composer.set_selection(0, 1);
                checkpoint(&f, 3, &draft);
            }
            _ => {
                f.store
                    .0
                    .execute("UPDATE composer_drafts SET content=content", [])
                    .unwrap();
            }
        }
        assert!(matches!(
            a::archive_known(&f.store, &review),
            Err(a::ArchiveError::Stale)
        ));
        active(&f);
    }
}
#[test]
fn stream_revoke_replacement_and_gap_refuse_archival() {
    for change in 0..3 {
        let f = Fixture::new();
        let (_, target) = setup(&f);
        finish(&f, &target, Delivery::NotSent);
        let review = refreshed(&f);
        match change {
            0 => f.stream.revoke(),
            1 => {
                let state =
                    operations::stream_state(&f.store, &f.scope.account_id, &f.scope.key.team)
                        .unwrap();
                operations::open_stream(&f.store, &f.scope.account_id, &f.scope.key.team, &state)
                    .unwrap();
            }
            _ => operations::mark_gap(&f.store, &f.stream, GapReason::Disconnect).unwrap(),
        }
        assert!(a::archive_known(&f.store, &review).is_err());
        active(&f);
    }
}
#[test]
fn archived_retry_preserves_newer_input_and_cannot_remove_a_new_active_intent() {
    let mut f = Fixture::new();
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::NotSent);
    let old = refreshed(&f);
    a::archive_known(&f.store, &old).unwrap();
    let old_id = f.next.id().to_owned();
    f.review = checkpoint(&f, 3, &local("/example another"));
    let (prior, next) = records(&f.review);
    f.prior = prior;
    f.next = next;
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, None, &f.prior).unwrap();
    let claim = claim(
        &f.store, &f.stream, &f.review, &f.writer, 3, &f.prior, &f.next,
    )
    .unwrap();
    let after = claim.review().clone();
    let retry = a::archive_known(&f.store, &old).unwrap();
    assert_eq!(retry.original_intent(), old_id);
    assert!(current(&f).same(&after));
    assert_eq!(count(&f, "composer_draft_slash_bindings"), 1);
    assert_eq!(count(&f, "composer_draft_slash_archives"), 1);
    assert!(guard(&f.store.0, &f.scope).is_err());
}
#[test]
fn archived_history_keeps_completion_lookup_and_unattested_retry_historical() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    let end = finish(&f, &target, Delivery::Reply(json!({"ok":true})));
    let reviewed = refreshed(&f);
    a::archive_known(&f.store, &reviewed).unwrap();
    let after = checkpoint(&f, 3, &local("new input"));
    c::complete_attested(&f.store, &target, &end).unwrap();
    assert!(current(&f).same(&after));
    let loaded = c::recover(&f.store, &f.scope, f.next.id()).unwrap();
    assert!(loaded.record().retired());
    assert!(guard(&f.store.0, &f.scope).is_ok());
}
#[test]
fn duplicate_active_archived_intent_corruption_cannot_bypass_guard() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::NotSent);
    let reviewed = refreshed(&f);
    a::archive_known(&f.store, &reviewed).unwrap();
    f.store.0.execute_batch("INSERT INTO composer_draft_slash_bindings SELECT intent,destination,enrollment,binding FROM composer_draft_slash_archives").unwrap();
    assert!(guard(&f.store.0, &f.scope).is_err());
    assert!(evidence::audit(&f.store.0, &f.scope.account_id).is_err());
}
#[test]
fn archive_attestation_and_reason_corruption_fail_closed() {
    for which in [0, 1, 2] {
        let f = Fixture::new();
        let (_, target) = setup(&f);
        finish(&f, &target, Delivery::NotSent);
        let reviewed = refreshed(&f);
        a::archive_known(&f.store, &reviewed).unwrap();
        match which {
            0 => {
                f.store
                    .0
                    .execute(
                        "UPDATE composer_draft_slash_archives SET destination='other'",
                        [],
                    )
                    .unwrap();
            }
            1 => {
                f.store
                    .0
                    .execute("DELETE FROM composer_draft_slash_attestations", [])
                    .unwrap();
            }
            _ => {
                let mut value: serde_json::Value =
                    serde_json::from_str(&raw(&f, "composer_draft_slash_archives", "content"))
                        .unwrap();
                value["reason"] = json!("LocalAcknowledged");
                f.store
                    .0
                    .execute(
                        "UPDATE composer_draft_slash_archives SET content=?1",
                        [value.to_string()],
                    )
                    .unwrap();
            }
        }
        assert!(guard(&f.store.0, &f.scope).is_err());
        assert!(a::archive_known(&f.store, &reviewed).is_err());
    }
}
#[test]
fn disk_restart_keeps_history_without_live_archive_or_execution_authority() {
    let db = crate::drafts::store::sync::tests::Database::new();
    let f = Fixture::at(&db.path);
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::NotSent);
    let reviewed = refreshed(&f);
    a::archive_known(&f.store, &reviewed).unwrap();
    let scope = f.scope.clone();
    let id = f.next.id().to_owned();
    f.stream.revoke();
    drop(reviewed);
    drop(target);
    drop(f);
    let store = db.open();
    evidence::audit(&store.0, &scope.account_id).unwrap();
    guard(&store.0, &scope).unwrap();
    let recovered = c::recover(&store, &scope, &id).unwrap();
    assert!(recovered.record().retired());
    assert!(
        controller::Controller::restore(recovered.record().clone())
            .unwrap()
            .recovery_required()
    );
}
#[test]
fn combined_inventory_bound_refuses_archive_without_dropping_active() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::NotSent);
    let reviewed = refreshed(&f);
    f.store
        .0
        .execute(
            "UPDATE composer_draft_slash_attestations SET content=?1",
            [" ".repeat(16 * 1024 * 1024 + 1)],
        )
        .unwrap();
    assert!(a::archive_known(&f.store, &reviewed).is_err());
    assert_eq!(count(&f, "composer_draft_slash_bindings"), 1);
    assert_eq!(count(&f, "composer_draft_slash_archives"), 0);
}
#[test]
fn missing_pair_and_nested_transaction_never_archive() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::NotSent);
    let reviewed = refreshed(&f);
    f.store.0.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(a::archive_known(&f.store, &reviewed).is_err());
    f.store.0.execute_batch("ROLLBACK").unwrap();
    active(&f);
    // Simulate mismatched local text without changing desired; exact current
    // writer proof is also stale, and no empty snapshot is synthesized.
    f.store
        .0
        .execute("DELETE FROM composer_drafts", [])
        .unwrap();
    assert!(a::pair(&current(&f)).is_err());
    assert!(a::archive_known(&f.store, &reviewed).is_err());
    active(&f);
}

fn remote_fixture() -> Fixture {
    let mut f = Fixture::new();
    let ticket = operations::prepare(&f.store, &f.stream, &f.review).unwrap();
    let (op, receipt) = operations::claim(&f.store, ticket).unwrap().into_parts();
    let remote = crate::drafts::sync::RemoteDraft {
        scope: f.scope.clone(),
        id: "D1".into(),
        client_msg_id: op.client_msg_id.clone(),
        last_updated_ts: "10.000001".into(),
        snapshot: op.snapshot.clone().unwrap(),
    };
    let review =
        operations::record(&f.store, &receipt, &Outcome::Active(remote.clone()), 0).unwrap();
    // A newly acknowledged identity makes successor invalidate the prior basis.
    // Only a positive baseline can verify that remote identity for mutation.
    assert_eq!(review.row().lineage().basis(), Basis::Unverified);
    assert_eq!(review.row().lineage().verified_epoch(), 0);
    let error = facade::admitted(&f.store.0, &f.stream.0, &review.inner).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("new mutation requires current verified basis")
    );
    f.writer = writers::register(&f.store, &review).unwrap();
    let WriteResult::Applied(unverified) =
        writers::checkpoint(&f.store, &f.writer, 1, Some(&review), review.local(), None).unwrap()
    else {
        panic!("pre-baseline writer");
    };
    f.review = unverified;
    let (prior, next) = records(&f.review);
    f.prior = prior;
    f.next = next;
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, None, &f.prior).unwrap();
    assert!(matches!(f.claim(), Err(Error::Storage)));
    f.unchanged();

    // Use the existing gap/new-stream positive baseline protocol. No manual
    // basis edits, fabricated command receipt or weakened admission guard.
    operations::mark_gap(&f.store, &f.stream, GapReason::Disconnect).unwrap();
    let stream_state =
        operations::stream_state(&f.store, &f.scope.account_id, &f.scope.key.team).unwrap();
    f.stream = operations::open_stream(
        &f.store,
        &f.scope.account_id,
        &f.scope.key.team,
        &stream_state,
    )
    .unwrap();
    let review = current(&f);
    let ticket = baseline::reserve(&f.store, &f.stream, &review, 1000).unwrap();
    let BaselineClaim::Ready { completion, .. } = baseline::claim(&f.store, ticket, 1000).unwrap()
    else {
        panic!("fresh fixture baseline");
    };
    let raw = json!({"ok":true,"draft":{"id":remote.id,"client_msg_id":remote.client_msg_id,
        "last_updated_ts":"11.000001","team_id":"T1","user_id":"U1",
        "is_deleted":false,"is_sent":false,"date_scheduled":0,
        "blocks":remote.snapshot.blocks,"file_ids":remote.snapshot.file_ids,
        "destinations":remote.snapshot.destinations}});
    let BaselineResult::Retained(evidence) = baseline::retain(
        &f.store,
        &completion,
        BaselineResponse::Positive {
            raw,
            user: "U1".into(),
        },
        0,
    )
    .unwrap() else {
        panic!("positive remote baseline");
    };
    let remote = evidence.remote().clone();
    let review = baseline::finish(
        &f.store,
        &evidence,
        &RemoteChoice::Local(LocalChoice::Current),
    )
    .unwrap();
    assert_eq!(review.row().lineage().basis(), Basis::Remote);
    assert_eq!(review.row().state().acknowledged(), Some(&remote));
    f.writer = writers::register(&f.store, &review).unwrap();
    let WriteResult::Applied(review) =
        writers::checkpoint(&f.store, &f.writer, 1, Some(&review), review.local(), None).unwrap()
    else {
        panic!("new receipt")
    };
    f.review = review;
    let (prior, next) = records(&f.review);
    f.prior = prior;
    f.next = next;
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, None, &f.prior).unwrap();
    f
}

#[test]
fn every_remote_owned_outcome_remains_guarded_in_first_archival_slice() {
    for delivery in [
        Delivery::NotSent,
        Delivery::Reply(json!({"ok":true})),
        Delivery::Reply(json!({"ok":true,"keep_input":true})),
        Delivery::Reply(json!({"ok":false,"error":"invalid_command"})),
    ] {
        let f = remote_fixture();
        let (_, target) = setup(&f);
        finish(&f, &target, delivery);
        let draft = current(&f).local().clone();
        checkpoint(&f, 2, &draft);
        assert!(matches!(
            a::inspect(&f.store, &f.stream, &current(&f), &f.writer, 2, f.next.id()),
            Err(a::ArchiveError::NeedsRemoteReview)
        ));
        active(&f);
        assert!(operations::prepare(&f.store, &f.stream, &current(&f)).is_err());
    }
}
#[test]
fn managed_evidence_rejects_wrong_origin_frozen_identity_and_duplicate_fields() {
    let f = Fixture::new();
    let original = outcome(&f, Delivery::NotSent);
    let encoded = original.encoded().unwrap();
    for field in ["origin", "version", "id"] {
        let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        match field {
            "origin" => value["origin"] = json!("SlackReply"),
            "version" => value["terminal"]["version"] = json!(99),
            _ => value["terminal"]["id"] = json!(uuid::Uuid::new_v4().to_string()),
        }
        assert!(controller::ManagedEvidence::decode(&value.to_string()).is_err());
    }
    let duplicate = encoded.replacen("\"version\":1", "\"version\":1,\"version\":1", 1);
    assert_ne!(duplicate, encoded);
    assert!(controller::ManagedEvidence::decode(&duplicate).is_err());
}

#[test]
fn deferred_commit_failure_rolls_back_every_archival_participant() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    let terminal = finish(&f, &target, Delivery::NotSent);
    let reviewed = refreshed(&f);
    let before = current(&f);
    f.store.0.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE commit_parent(id INTEGER PRIMARY KEY); CREATE TABLE commit_child(parent INTEGER REFERENCES commit_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER fail_archive_commit AFTER INSERT ON composer_draft_slash_archives BEGIN INSERT INTO commit_child VALUES(99); END;").unwrap();
    assert!(a::archive_known(&f.store, &reviewed).is_err());
    assert!(current(&f).same(&before));
    active(&f);
    assert_eq!(count(&f, "commit_child"), 0);
    let records = slash_journal::protected_inventory(&f.store.0, &f.scope.account_id).unwrap();
    assert_eq!(
        records.iter().find(|r| r.id() == f.next.id()),
        Some(terminal.terminal())
    );
}
#[test]
fn archival_and_exact_retry_preserve_known_cooldown_bytes() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    let terminal = controller::isolated_terminal(
        &f.next,
        Delivery::Reply(json!({"ok":true,"keep_input":true})),
        Some(37),
    );
    c::complete_attested(&f.store, &target, &terminal).unwrap();
    let reviewed = refreshed(&f);
    let query = "SELECT quote(scope)||quote(permanent)||quote(duration_ms)||quote(run)||quote(until_mono)||quote(until_wall) FROM sp_slash_cooldowns";
    let before: String = f.store.0.query_row(query, [], |r| r.get(0)).unwrap();
    a::archive_known(&f.store, &reviewed).unwrap();
    a::archive_known(&f.store, &reviewed).unwrap();
    let after: String = f.store.0.query_row(query, [], |r| r.get(0)).unwrap();
    assert_eq!(before, after);
}

#[test]
fn separately_persisted_terminal_cannot_acquire_same_commit_attestation() {
    for delivery in [
        Delivery::NotSent,
        Delivery::Reply(json!({"ok":true})),
        Delivery::Reply(json!({"ok":false,"error":"invalid_command"})),
    ] {
        let mut f = Fixture::new();
        let (_, target) = setup(&f);
        let terminal = outcome(&f, delivery);
        slash_journal::compare(
            &mut f.store.0,
            &f.scope.account_id,
            Some(&f.next),
            terminal.terminal(),
        )
        .unwrap();
        let before = current(&f);
        assert!(c::complete_attested(&f.store, &target, &terminal).is_err());
        assert!(current(&f).same(&before));
        assert_eq!(count(&f, "composer_draft_slash_completions"), 0);
        assert_eq!(count(&f, "composer_draft_slash_attestations"), 0);
        let records = slash_journal::protected_inventory(&f.store.0, &f.scope.account_id).unwrap();
        assert_eq!(
            records.iter().find(|r| r.id() == f.next.id()),
            Some(terminal.terminal())
        );
        active(&f);
    }
}
#[test]
fn orphan_history_or_completion_schema_never_means_no_active_guard() {
    let f = Fixture::new();
    let (_, target) = setup(&f);
    finish(&f, &target, Delivery::NotSent);
    let reviewed = refreshed(&f);
    a::archive_known(&f.store, &reviewed).unwrap();
    f.store
        .0
        .execute_batch(
            "DROP TABLE composer_draft_slash_bindings; DROP TABLE composer_draft_slash_format;",
        )
        .unwrap();
    assert!(guard(&f.store.0, &f.scope).is_err());
    assert_eq!(count(&f, "composer_draft_slash_archives"), 1);
    assert_eq!(count(&f, "composer_draft_slash_attestations"), 1);
    for table in [
        "composer_draft_slash_archives",
        "composer_draft_slash_attestations",
        "composer_draft_slash_completion_format",
        "composer_draft_slash_completions",
    ] {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!("CREATE TABLE {table}(content TEXT);"))
            .unwrap();
        assert!(guard(&conn, &f.scope).is_err());
    }
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    guard(&conn, &f.scope).unwrap();
}

#[path = "api_slash_audit_index_tests.rs"]
mod audit_index_tests;

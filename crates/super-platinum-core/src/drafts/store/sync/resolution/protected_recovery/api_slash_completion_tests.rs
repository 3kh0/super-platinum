//! SQLite regressions only. No production account, HTTP, replay or worker hookup.
use super::super::completion::{self as c, Effect};
use super::*;
fn terminal(f: &Fixture, phase: Phase, status: Option<u16>, retry: Option<u64>) -> SlashRecord {
    let mut value = serde_json::to_value(&f.next).unwrap();
    value["version"] = json!(3);
    value["phase"] = serde_json::to_value(phase).unwrap();
    value["http_status"] = json!(status);
    value["retry_after_secs"] = json!(retry);
    SlashRecord::decode(&serde_json::to_vec(&value).unwrap()).unwrap()
}
fn response(f: &Fixture, raw: serde_json::Value) -> SlashRecord {
    terminal(
        f,
        Phase::Completed(crate::slash_commands::api::decode(raw)),
        Some(200),
        None,
    )
}
fn ready(f: &Fixture) -> (Claim, c::Target) {
    c::install(&f.store, &f.scope.account_id).unwrap();
    let claim = f.claim().unwrap();
    let target = c::target(&claim);
    (claim, target)
}
fn current(f: &Fixture) -> Review {
    views::review(&f.store.0, &f.scope).unwrap()
}
fn journal(f: &Fixture) -> SlashRecord {
    slash_journal::protected_inventory(&f.store.0, &f.scope.account_id)
        .unwrap()
        .into_iter()
        .find(|r| r.id() == f.next.id())
        .unwrap()
}
fn receipt_count(f: &Fixture) -> i64 {
    f.store
        .0
        .query_row(
            "SELECT COUNT(*) FROM composer_draft_slash_completions",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
fn applied(f: &Fixture, before: &Review, draft: &LocalDraft) -> Review {
    let WriteResult::Applied(view) =
        writers::checkpoint(&f.store, &f.writer, 2, Some(before), draft, None).unwrap()
    else {
        panic!("newer canonical input")
    };
    view
}
#[test]
fn ack_false_clears_exact_local_only_and_preserves_binding_guard() {
    let f = Fixture::new();
    let (claim, target) = ready(&f);
    let ack = response(&f, json!({"ok":true}));
    let result = c::complete(&f.store, &target, &ack).unwrap();
    assert_eq!(result.effect(), Effect::ClearedExactAcknowledged);
    assert!(!result.pending());
    assert_eq!(result.record(), &ack);
    let after = current(&f);
    assert!(after.local().empty());
    assert!(after.row().state().desired().is_none());
    assert_eq!(
        after.row().state().client_msg_id(),
        claim.review().row().state().client_msg_id()
    );
    assert_eq!(after.row().lineage(), claim.review().row().lineage());
    assert_eq!(
        after.row().state().local_revision(),
        claim.review().row().state().local_revision() + 1
    );
    assert!(guard(&f.store.0, &f.scope).is_err());
    assert!(operations::consume(&f.store, &f.stream, &after, ConsumeKind::Send).is_err());
    assert!(operations::prepare(&f.store, &f.stream, &after).is_err());
    assert!(archive::release(&f.store, &after.inner).is_err());
    assert!(storage::release(&f.store, &after.inner).is_err());
}
#[test]
fn keep_input_retains_exact_snapshot_without_restore() {
    let f = Fixture::new();
    let (claim, target) = ready(&f);
    let ack = response(
        &f,
        json!({"ok":true,"keep_input":true,"response":"synthetic feedback"}),
    );
    let result = c::complete(&f.store, &target, &ack).unwrap();
    assert_eq!(result.effect(), Effect::RetainedKeepInput);
    assert!(current(&f).same(claim.review()));
    assert_eq!(journal(&f), ack);
}
#[test]
fn newer_input_and_same_text_new_serial_are_preserved_for_both_keep_values() {
    for keep in [false, true] {
        for same_text in [false, true] {
            let f = Fixture::new();
            let (claim, target) = ready(&f);
            let draft = if same_text {
                claim.review().local().clone()
            } else {
                local("new input")
            };
            let newer = applied(&f, claim.review(), &draft);
            let ack = response(&f, json!({"ok":true,"keep_input":keep}));
            c::complete(&f.store, &target, &ack).unwrap();
            assert!(current(&f).same(&newer));
        }
    }
}
#[test]
fn same_byte_external_sql_aba_retires_clear_but_preserves_receipt() {
    let f = Fixture::new();
    let (_, target) = ready(&f);
    f.store
        .0
        .execute("UPDATE composer_drafts SET content=content", [])
        .unwrap();
    let newer = current(&f);
    let ack = response(&f, json!({"ok":true}));
    assert_eq!(
        c::complete(&f.store, &target, &ack).unwrap().effect(),
        Effect::PreservedCurrent
    );
    assert!(current(&f).same(&newer));
    assert_eq!(journal(&f), ack);
}
#[test]
fn revoked_or_replaced_stream_records_history_without_clearing() {
    for replace in [false, true] {
        let f = Fixture::new();
        let (claim, target) = ready(&f);
        if replace {
            let state =
                operations::stream_state(&f.store, &f.scope.account_id, &f.scope.key.team).unwrap();
            let _ =
                operations::open_stream(&f.store, &f.scope.account_id, &f.scope.key.team, &state)
                    .unwrap();
        } else {
            f.stream.revoke();
        }
        let ack = response(&f, json!({"ok":true}));
        assert_eq!(
            c::complete(&f.store, &target, &ack).unwrap().effect(),
            Effect::PreservedCurrent
        );
        assert!(current(&f).same(claim.review()));
        assert_eq!(journal(&f), ack);
    }
}
#[test]
fn unknowns_remain_pending_and_cannot_be_replaced_by_ack() {
    for raw in [
        json!({"ok":false,"error":"dispatch_failed"}),
        json!({"ok":true,"keep_input":null}),
    ] {
        let f = Fixture::new();
        let (claim, target) = ready(&f);
        let unknown = response(&f, raw);
        let result = c::complete(&f.store, &target, &unknown).unwrap();
        assert_eq!(result.effect(), Effect::PendingUnknown);
        assert!(result.pending());
        assert!(current(&f).same(claim.review()));
        assert!(c::complete(&f.store, &target, &response(&f, json!({"ok":true}))).is_err());
        let recovery = c::recover(&f.store, &f.scope, f.next.id()).unwrap();
        assert!(recovery.pending());
        assert_eq!(recovery.original(), f.next.original());
    }
}
#[test]
fn not_sent_and_rejection_retain_original_and_do_not_unlock() {
    for rejected in [false, true] {
        let f = Fixture::new();
        let (claim, target) = ready(&f);
        let end = if rejected {
            response(&f, json!({"ok":false,"error":"invalid_command"}))
        } else {
            terminal(&f, Phase::NotSent, None, None)
        };
        c::complete(&f.store, &target, &end).unwrap();
        assert!(current(&f).same(claim.review()));
        assert!(guard(&f.store.0, &f.scope).is_err());
    }
}
#[test]
fn receipt_insert_failure_rolls_back_local_row_journal_and_cooldown() {
    for rate in [false, true] {
        let f = Fixture::new();
        let (claim, target) = ready(&f);
        f.store.0.execute_batch("CREATE TRIGGER fail_completion BEFORE INSERT ON composer_draft_slash_completions BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
        let end = if rate {
            terminal(&f, Phase::Unknown, Some(429), Some(31))
        } else {
            response(&f, json!({"ok":true}))
        };
        assert!(c::complete(&f.store, &target, &end).is_err());
        assert_eq!(journal(&f), f.next);
        assert!(current(&f).same(claim.review()));
        assert_eq!(receipt_count(&f), 0);
        let count: i64 = f
            .store
            .0
            .query_row("SELECT COUNT(*) FROM sp_slash_cooldowns", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
#[test]
fn row_failure_rolls_back_clear_and_terminal_receipt() {
    let f = Fixture::new();
    let (claim, target) = ready(&f);
    f.store.0.execute_batch("CREATE TRIGGER fail_completion_row BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(c::complete(&f.store, &target, &response(&f, json!({"ok":true}))).is_err());
    assert!(current(&f).same(claim.review()));
    assert_eq!(journal(&f), f.next);
    assert_eq!(receipt_count(&f), 0);
}
#[test]
fn idempotent_ack_cannot_repeat_clear_over_new_input() {
    let f = Fixture::new();
    let (_, target) = ready(&f);
    let ack = response(&f, json!({"ok":true}));
    c::complete(&f.store, &target, &ack).unwrap();
    let cleared = current(&f);
    let newer = applied(&f, &cleared, &local("new input after receipt"));
    assert_eq!(
        c::complete(&f.store, &target, &ack).unwrap().effect(),
        Effect::ClearedExactAcknowledged
    );
    assert!(current(&f).same(&newer));
    assert_eq!(receipt_count(&f), 1);
}
#[test]
fn repeated_rate_receipt_does_not_extend_cooldown() {
    let f = Fixture::new();
    let (_, target) = ready(&f);
    let end = terminal(&f, Phase::Unknown, Some(429), None);
    c::complete(&f.store, &target, &end).unwrap();
    let before:String=f.store.0.query_row("SELECT quote(scope)||quote(permanent)||quote(duration_ms)||quote(run)||quote(until_mono)||quote(until_wall) FROM sp_slash_cooldowns",[],|r|r.get(0)).unwrap();
    c::complete(&f.store, &target, &end).unwrap();
    let after:String=f.store.0.query_row("SELECT quote(scope)||quote(permanent)||quote(duration_ms)||quote(run)||quote(until_mono)||quote(until_wall) FROM sp_slash_cooldowns",[],|r|r.get(0)).unwrap();
    assert_eq!(before, after);
}
#[test]
fn recovery_is_inert_and_dispatch_possible_cannot_be_settled() {
    let f = Fixture::new();
    let (claim, _) = ready(&f);
    let loaded = c::recover(&f.store, &f.scope, f.next.id()).unwrap();
    assert!(loaded.pending());
    assert_eq!(loaded.effect(), None);
    assert!(loaded.current().unwrap().same(claim.review()));
    assert!(matches!(
        c::preserve_recovered(&f.store, &loaded),
        Err(Error::Pending)
    ));
    assert_eq!(journal(&f), f.next);
    assert_eq!(receipt_count(&f), 0);
}
#[test]
fn terminal_recovery_preserves_input_and_refuses_changed_review() {
    let mut f = Fixture::new();
    let (claim, _) = ready(&f);
    let ack = response(&f, json!({"ok":true}));
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, Some(&f.next), &ack).unwrap();
    let loaded = c::recover(&f.store, &f.scope, f.next.id()).unwrap();
    let newer = applied(&f, claim.review(), &local("newer"));
    assert!(matches!(
        c::preserve_recovered(&f.store, &loaded),
        Err(Error::Stale)
    ));
    assert_eq!(receipt_count(&f), 0);
    let fresh = c::recover(&f.store, &f.scope, f.next.id()).unwrap();
    assert_eq!(
        c::preserve_recovered(&f.store, &fresh).unwrap().effect(),
        Effect::RecoveredPreserved
    );
    assert!(current(&f).same(&newer));
}
#[test]
fn reopen_keeps_raw_evidence_and_recovery_never_clears() {
    let db = crate::drafts::store::sync::tests::Database::new();
    let mut f = Fixture::at(&db.path);
    let (_, target) = ready(&f);
    let ack = response(&f, json!({"ok":true,"response":"synthetic result"}));
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, Some(&f.next), &ack).unwrap();
    let scope = f.scope.clone();
    let original = f.next.original().clone();
    f.stream.revoke();
    drop(target);
    drop(f);
    let store = db.open();
    let recovery = c::recover(&store, &scope, ack.id()).unwrap();
    assert_eq!(recovery.record(), &ack);
    assert_eq!(recovery.original(), &original);
    c::preserve_recovered(&store, &recovery).unwrap();
    assert!(!views::review(&store.0, &scope).unwrap().local().empty());
    assert!(guard(&store.0, &scope).is_err());
}
#[test]
fn partial_future_wrong_account_schema_and_additive_failure_do_not_repair() {
    for sql in [
        "DROP TABLE composer_draft_slash_completions",
        "UPDATE composer_draft_slash_completion_format SET version=2",
        "UPDATE composer_draft_slash_completion_format SET account='other'",
    ] {
        let f = Fixture::new();
        let (_, target) = ready(&f);
        f.store.0.execute_batch(sql).unwrap();
        assert!(c::install(&f.store, &f.scope.account_id).is_err());
        assert!(c::complete(&f.store, &target, &response(&f, json!({"ok":true}))).is_err());
        assert_eq!(journal(&f), f.next);
    }
    let f = Fixture::new();
    f.store
        .0
        .execute_batch("CREATE VIEW composer_draft_slash_completions AS SELECT 1")
        .unwrap();
    assert!(c::install(&f.store, &f.scope.account_id).is_err());
    let n:i64=f.store.0.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name='composer_draft_slash_completion_format'",[],|r|r.get(0)).unwrap();
    assert_eq!(n, 0);
}
#[test]
fn receipt_contradictions_missing_fields_and_wrong_index_fail_closed() {
    for field in ["effect", "before", "binding", "index"] {
        let f = Fixture::new();
        let (_, target) = ready(&f);
        let ack = response(&f, json!({"ok":true}));
        c::complete(&f.store, &target, &ack).unwrap();
        let raw: String = f
            .store
            .0
            .query_row(
                "SELECT content FROM composer_draft_slash_completions",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        match field {
            "effect" => v["effect"] = json!("PendingUnknown"),
            "before" => {
                v.as_object_mut().unwrap().remove("before");
            }
            "binding" => v["binding"] = json!("{}"),
            _ => {
                f.store
                    .0
                    .execute(
                        "UPDATE composer_draft_slash_completions SET destination='other'",
                        [],
                    )
                    .unwrap();
            }
        }
        if field != "index" {
            f.store
                .0
                .execute(
                    "UPDATE composer_draft_slash_completions SET content=?1",
                    [v.to_string()],
                )
                .unwrap();
        }
        assert!(c::recover(&f.store, &f.scope, f.next.id()).is_err());
        assert!(c::complete(&f.store, &target, &ack).is_err());
    }
}
#[test]
fn nested_transaction_and_wrong_intent_cannot_settle() {
    let f = Fixture::new();
    let (_, target) = ready(&f);
    let ack = response(&f, json!({"ok":true}));
    f.store.0.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(c::complete(&f.store, &target, &ack).is_err());
    f.store.0.execute_batch("ROLLBACK").unwrap();
    let mut v = serde_json::to_value(&ack).unwrap();
    v["id"] = json!(uuid::Uuid::new_v4().to_string());
    let wrong = SlashRecord::decode(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(c::complete(&f.store, &target, &wrong).is_err());
    assert_eq!(journal(&f), f.next);
}
#[test]
fn acknowledged_remote_base_is_preserved_and_delete_stays_guarded() {
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
    let (claim, target) = ready(&f);
    c::complete(&f.store, &target, &response(&f, json!({"ok":true}))).unwrap();
    let after = current(&f);
    assert_eq!(after.row().state().acknowledged(), Some(&remote));
    assert_eq!(
        after.row().state().acknowledged_local_revision(),
        claim.review().row().state().acknowledged_local_revision()
    );
    assert_eq!(after.row().state().client_msg_id(), remote.client_msg_id);
    assert!(after.local().empty());
    assert!(operations::prepare(&f.store, &f.stream, &after).is_err());
    assert!(guard(&f.store.0, &f.scope).is_err());
}

#[test]
fn exact_thread_settlement_preserves_parent_destination() {
    let mut f = Fixture::new();
    f.scope.key.thread = Some("1.000001".into());
    f.store
        .save([(f.scope.key.clone(), local("/example thread"))])
        .unwrap();
    let inventory = views::local(&f.store.0, &f.scope).unwrap();
    let review = operations::enroll(&f.store, &f.scope, &inventory).unwrap();
    let review = operations::verify(&f.store, &f.stream, &review).unwrap();
    f.writer = writers::register(&f.store, &review).unwrap();
    let WriteResult::Applied(review) =
        writers::checkpoint(&f.store, &f.writer, 1, Some(&review), review.local(), None).unwrap()
    else {
        panic!("thread writer")
    };
    f.review = review;
    let (prior, next) = records(&f.review);
    f.prior = prior;
    f.next = next;
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, None, &f.prior).unwrap();
    let (_, target) = ready(&f);
    c::complete(&f.store, &target, &response(&f, json!({"ok":true}))).unwrap();
    assert!(current(&f).local().empty());
    assert_eq!(current(&f).scope().key.thread.as_deref(), Some("1.000001"));
    let mut parent = f.scope.clone();
    parent.key.thread = None;
    assert!(!views::review(&f.store.0, &parent).unwrap().local().empty());
}
#[test]
fn retired_terminal_receipt_is_historical_and_never_reclears() {
    let mut f = Fixture::new();
    let (_, target) = ready(&f);
    let ack = response(&f, json!({"ok":true}));
    c::complete(&f.store, &target, &ack).unwrap();
    let mut value = serde_json::to_value(&ack).unwrap();
    value["version"] = json!(4);
    value["retired"] = json!(true);
    let retired = SlashRecord::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, Some(&ack), &retired).unwrap();
    let newer = applied(&f, &current(&f), &local("newer"));
    let loaded = c::recover(&f.store, &f.scope, f.next.id()).unwrap();
    assert_eq!(loaded.record(), &retired);
    c::preserve_recovered(&f.store, &loaded).unwrap();
    assert!(current(&f).same(&newer));
    assert!(guard(&f.store.0, &f.scope).is_err());
}

//! Authored transaction regressions. No HTTP, production store or Admission.
use super::super::archive;
use super::*;
#[path = "api_slash_archive_tests.rs"]
mod archive_tests;
#[path = "api_slash_completion_tests.rs"]
mod completion_tests;
use crate::drafts::store::sync::tests::local;
use crate::drafts::sync::ConsumeKind;
use crate::slash_commands::{Owner, api::Frozen, catalog::Catalog, controller::Phase};
use serde_json::json;

struct Fixture {
    store: DraftStore,
    scope: Scope,
    stream: Stream,
    review: Review,
    writer: Writer,
    prior: SlashRecord,
    next: SlashRecord,
}
fn records(review: &Review) -> (SlashRecord, SlashRecord) {
    let scope = review.scope();
    let owner = Owner {
        account: scope.account_id.clone(),
        team: scope.key.team.clone(),
        user: "U1".into(),
        channel: scope.key.channel.clone(),
        thread: scope.key.thread.clone(),
    };
    let catalog=Catalog::decode(owner.clone(),json!({"ok":true,"commands":[{"name":"/example","canonical_name":"/example","type":"app","app":"A1","teams":[scope.key.team],"is_runnable_from_thread":true}],"app_actions":[],"cache_ts":"opaque"})).unwrap();
    let frozen = Frozen::capture(
        owner,
        review.row().revision(),
        original(review),
        "/example".into(),
        None,
        &catalog.choice(0).unwrap(),
        &catalog,
    )
    .unwrap();
    let mut value = json!({"id":uuid::Uuid::new_v4().to_string(),"version":1,"frozen":frozen,"phase":"Prepared","retry_after_secs":null,"http_status":null,"retired":false});
    let prior = SlashRecord::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    value["version"] = json!(2);
    value["phase"] = json!("DispatchPossible");
    (
        prior,
        SlashRecord::decode(&serde_json::to_vec(&value).unwrap()).unwrap(),
    )
}
impl Fixture {
    fn new() -> Self {
        Self::at(":memory:")
    }
    fn at(path: impl AsRef<std::path::Path>) -> Self {
        let mut store = DraftStore::open(path).unwrap();
        let scope = Scope {
            account_id: uuid::Uuid::new_v4().to_string(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            },
        };
        storage::activate(&store, &scope.account_id).unwrap();
        adapter_schema::install(&store, &scope.account_id).unwrap();
        store
            .save([(scope.key.clone(), local("/example argument"))])
            .unwrap();
        let inventory = views::local(&store.0, &scope).unwrap();
        let review = operations::enroll(&store, &scope, &inventory).unwrap();
        let state = operations::stream_state(&store, &scope.account_id, &scope.key.team).unwrap();
        let stream =
            operations::open_stream(&store, &scope.account_id, &scope.key.team, &state).unwrap();
        let review = operations::verify(&store, &stream, &review).unwrap();
        let writer = writers::register(&store, &review).unwrap();
        let WriteResult::Applied(review) =
            writers::checkpoint(&store, &writer, 1, Some(&review), review.local(), None).unwrap()
        else {
            panic!("canonical writer expected")
        };
        slash_journal::install(&mut store.0, &scope.account_id).unwrap();
        install(&store, &scope.account_id).unwrap();
        let (prior, next) = records(&review);
        slash_journal::compare(&mut store.0, &scope.account_id, None, &prior).unwrap();
        Self {
            store,
            scope,
            stream,
            review,
            writer,
            prior,
            next,
        }
    }
    fn claim(&self) -> Result<Claim, Error> {
        claim(
            &self.store,
            &self.stream,
            &self.review,
            &self.writer,
            1,
            &self.prior,
            &self.next,
        )
    }
    fn count(&self) -> i64 {
        self.store
            .0
            .query_row(
                "SELECT COUNT(*) FROM composer_draft_slash_bindings",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn unchanged(&self) {
        assert!(
            views::review(&self.store.0, &self.scope)
                .unwrap()
                .same(&self.review)
        );
        assert_eq!(self.count(), 0);
        let records =
            slash_journal::protected_inventory(&self.store.0, &self.scope.account_id).unwrap();
        assert_eq!(
            records.iter().find(|record| record.id() == self.prior.id()),
            Some(&self.prior)
        );
    }
}

#[test]
fn atomic_claim_binds_full_review_row_writer_stream_and_retains_input() {
    let f = Fixture::new();
    let before = serde_json::to_vec(f.review.local()).unwrap();
    let receipt = serde_json::to_vec(&writers::matching(&f.store.0, &f.writer).unwrap()).unwrap();
    let claimed = f.claim().unwrap();
    assert_eq!(claimed.record(), &f.next);
    assert!(claimed.stream_live());
    assert_eq!(
        claimed.review().row().revision(),
        f.review.row().revision() + 1
    );
    assert_eq!(claimed.review().row().state(), f.review.row().state());
    assert_eq!(
        serde_json::to_vec(claimed.review().local()).unwrap(),
        before
    );
    assert_eq!(
        serde_json::to_vec(&writers::matching(&f.store.0, &f.writer).unwrap()).unwrap(),
        receipt
    );
    assert_eq!(f.count(), 1);
    assert!(crate::slash_commands::controller::managed_admission_unavailable().is_err());
}
#[test]
fn binding_insert_failure_rolls_back_journal_row_and_binding() {
    let f = Fixture::new();
    f.store.0.execute_batch("CREATE TRIGGER fail_slash_insert BEFORE INSERT ON composer_draft_slash_bindings BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(f.claim().is_err());
    f.unchanged();
}
#[test]
fn row_write_failure_rolls_back_prior_journal_and_binding_writes() {
    let f = Fixture::new();
    f.store.0.execute_batch("CREATE TRIGGER fail_slash_row BEFORE UPDATE ON composer_draft_sync BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(f.claim().is_err());
    f.unchanged();
}
#[test]
fn same_text_clock_aba_rejects_old_review_before_journal_update() {
    let f = Fixture::new();
    // The real managed save API refuses this write; simulate an external
    // same-byte SQL write so the installed trigger must retire the Review.
    f.store
        .0
        .execute(
            "UPDATE composer_drafts SET content=content WHERE destination=?1",
            [serde_json::to_string(&f.scope.key).unwrap()],
        )
        .unwrap();
    assert!(f.claim().is_err());
    assert_eq!(f.count(), 0);
    assert!(matches!(
        slash_journal::protected_inventory(&f.store.0, &f.scope.account_id).unwrap()[0].phase(),
        Phase::Prepared
    ));
}
#[test]
fn writer_serial_zero_future_and_wrong_registered_writer_are_rejected() {
    let f = Fixture::new();
    for serial in [0, 2] {
        assert!(
            claim(
                &f.store, &f.stream, &f.review, &f.writer, serial, &f.prior, &f.next
            )
            .is_err()
        );
        f.unchanged();
    }
    let other = writers::register(&f.store, &f.review).unwrap();
    assert!(claim(&f.store, &f.stream, &f.review, &other, 1, &f.prior, &f.next).is_err());
    f.unchanged();
}
#[test]
fn stream_revocation_and_replacement_do_not_issue_claims() {
    let f = Fixture::new();
    f.stream.revoke();
    assert!(f.claim().is_err());
    f.unchanged();
    let g = Fixture::new();
    let state = operations::stream_state(&g.store, &g.scope.account_id, &g.scope.key.team).unwrap();
    let _replacement =
        operations::open_stream(&g.store, &g.scope.account_id, &g.scope.key.team, &state).unwrap();
    assert!(g.claim().is_err());
    assert_eq!(g.count(), 0);
}
#[test]
fn changed_original_and_destination_cannot_borrow_protected_review() {
    for change in ["original", "channel", "thread", "account"] {
        let mut f = Fixture::new();
        let mut value = serde_json::to_value(&f.prior).unwrap();
        value["id"] = json!(uuid::Uuid::new_v4().to_string());
        match change {
            "original" => {
                value["frozen"]["original"] = json!({"ops":[{"insert":"same-looking text"}]})
            }
            "channel" => value["frozen"]["owner"]["channel"] = json!("C2"),
            "thread" => value["frozen"]["owner"]["thread"] = json!("1.2"),
            _ => value["frozen"]["owner"]["account"] = json!(uuid::Uuid::new_v4().to_string()),
        }
        let prior = SlashRecord::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
        value["version"] = json!(2);
        value["phase"] = json!("DispatchPossible");
        let next = SlashRecord::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
        // Account mismatch need not be inserted to prove refusal before SQL CAS.
        if change != "account" {
            slash_journal::compare(&mut f.store.0, &f.scope.account_id, None, &prior).unwrap();
        }
        assert!(claim(&f.store, &f.stream, &f.review, &f.writer, 1, &prior, &next).is_err());
        f.unchanged();
    }
}
#[test]
fn dropped_claim_repeated_cas_and_nested_transaction_never_reissue() {
    let f = Fixture::new();
    f.store.0.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(f.claim().is_err());
    f.store.0.execute_batch("ROLLBACK").unwrap();
    f.unchanged();
    drop(f.claim().unwrap());
    assert!(f.claim().is_err());
    let tx = adapter_atomic::begin(&f.store.0).unwrap();
    assert_eq!(
        slash_journal::protected_dispatch(&tx, &f.scope.account_id, &f.prior, &f.next),
        Err(crate::slash_commands::Failure::NoReplay)
    );
    drop(tx);
    assert_eq!(f.count(), 1);
}
#[test]
fn ordinary_mutations_and_both_release_paths_are_blocked_but_new_input_is_preserved() {
    let f = Fixture::new();
    let claim = f.claim().unwrap();
    assert!(operations::consume(&f.store, &f.stream, claim.review(), ConsumeKind::Send).is_err());
    assert!(operations::prepare(&f.store, &f.stream, claim.review()).is_err());
    assert!(archive::release(&f.store, &claim.review().inner).is_err());
    assert!(storage::release(&f.store, &claim.review().inner).is_err());
    let newer = local("new input after explicit command");
    let WriteResult::Applied(review) =
        writers::checkpoint(&f.store, &f.writer, 2, Some(claim.review()), &newer, None).unwrap()
    else {
        panic!("new input must stay canonical")
    };
    assert!(same_local(review.local(), &newer));
    assert_eq!(f.count(), 1);
    assert!(guard(&f.store.0, &f.scope).is_err());
    assert!(operations::prepare(&f.store, &f.stream, &review).is_err());
}
#[test]
fn disk_restart_retains_binding_and_loaded_receipt_has_no_replay() {
    let db = crate::drafts::store::sync::tests::Database::new();
    let f = Fixture::at(&db.path);
    let claimed = f.claim().unwrap();
    f.stream.revoke();
    assert!(!claimed.stream_live());
    let account = f.scope.account_id.clone();
    let scope = f.scope.clone();
    drop(claimed);
    drop(f);
    let mut store = db.open();
    assert!(guard(&store.0, &scope).is_err());
    let loaded = slash_journal::load(&mut store.0, &account).unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(
        crate::slash_commands::controller::Controller::restore(loaded[0].clone())
            .unwrap()
            .recovery_required()
    );
}
#[test]
fn partial_future_and_wrong_account_schema_fail_closed_without_repair() {
    for mutation in [
        "DROP TABLE composer_draft_slash_bindings",
        "UPDATE composer_draft_slash_format SET version=2",
        "UPDATE composer_draft_slash_format SET account='other'",
    ] {
        let f = Fixture::new();
        f.store.0.execute_batch(mutation).unwrap();
        assert!(f.claim().is_err());
        assert!(install(&f.store, &f.scope.account_id).is_err());
        assert!(guard(&f.store.0, &f.scope).is_err());
        assert!(matches!(
            slash_journal::protected_inventory(&f.store.0, &f.scope.account_id).unwrap()[0].phase(),
            Phase::Prepared
        ));
    }
}
#[test]
fn indexed_binding_corruption_cannot_hide_an_owned_intent() {
    let f = Fixture::new();
    drop(f.claim().unwrap());
    f.store
        .0
        .execute(
            "UPDATE composer_draft_slash_bindings SET destination=?1",
            [serde_json::to_string(&DraftKey {
                channel: "C2".into(),
                ..f.scope.key.clone()
            })
            .unwrap()],
        )
        .unwrap();
    assert!(guard(&f.store.0, &f.scope).is_err());
    assert!(install(&f.store, &f.scope.account_id).is_err());
}

#[test]
fn retained_writer_receipt_cannot_be_substituted_or_relabelled() {
    let f = Fixture::new();
    drop(f.claim().unwrap());
    let raw: String = f
        .store
        .0
        .query_row(
            "SELECT content FROM composer_draft_slash_bindings",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut binding: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mut writer: serde_json::Value =
        serde_json::from_str(binding["writer_receipt"].as_str().unwrap()).unwrap();
    writer["last"]["preserved"] = json!(true);
    binding["writer_receipt"] = json!(serde_json::to_string(&writer).unwrap());
    f.store
        .0
        .execute(
            "UPDATE composer_draft_slash_bindings SET content=?1",
            [binding.to_string()],
        )
        .unwrap();
    assert!(guard(&f.store.0, &f.scope).is_err());
}

#[test]
fn another_destination_unresolved_receipt_blocks_atomic_claim_without_row_change() {
    let mut f = Fixture::new();
    let mut value = serde_json::to_value(&f.prior).unwrap();
    value["id"] = json!(uuid::Uuid::new_v4().to_string());
    value["frozen"]["owner"]["channel"] = json!("C2");
    let prior = SlashRecord::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, None, &prior).unwrap();
    value["phase"] = json!("DispatchPossible");
    value["version"] = json!(2);
    let next = SlashRecord::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, Some(&prior), &next).unwrap();
    assert!(f.claim().is_err());
    f.unchanged();
}

#[test]
fn additive_migration_ddl_failure_rolls_back_its_marker() {
    let f = Fixture::new();
    f.store.0.execute_batch("DROP TABLE composer_draft_slash_bindings; DROP TABLE composer_draft_slash_format; CREATE VIEW composer_draft_slash_bindings AS SELECT 1").unwrap();
    assert!(install(&f.store, &f.scope.account_id).is_err());
    let n: i64 = f
        .store
        .0
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='composer_draft_slash_format'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
    assert!(views::review(&f.store.0, &f.scope).unwrap().same(&f.review));
}

#[test]
fn terminal_receipt_never_implicitly_unlocks_consumption_or_release() {
    let mut f = Fixture::new();
    let claimed = f.claim().unwrap();
    let mut value = serde_json::to_value(&f.next).unwrap();
    value["version"] = json!(3);
    value["phase"] = json!("NotSent");
    let terminal = SlashRecord::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    slash_journal::compare(
        &mut f.store.0,
        &f.scope.account_id,
        Some(&f.next),
        &terminal,
    )
    .unwrap();
    assert!(guard(&f.store.0, &f.scope).is_err());
    assert!(archive::release(&f.store, &claimed.review().inner).is_err());
    assert_eq!(f.count(), 1);
}

#[path = "api_slash_fresh_clear_tests.rs"]
mod fresh_clear_tests;

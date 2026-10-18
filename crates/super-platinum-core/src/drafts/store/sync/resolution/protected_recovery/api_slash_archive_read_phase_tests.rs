//! Actual archive transactions, retained history and post-write rollback.
//! Deterministic validation counts are not an elapsed-time speedup claim.
use super::*;
fn ready(history_count: usize) -> (Fixture, Vec<String>, u64) {
    let (mut f, ids) = history(history_count);
    let serial = 2 * history_count as u64 + 1;
    f.review = checkpoint(&f, serial, &local("/example next"));
    (f.prior, f.next) = records(&f.review);
    slash_journal::compare(&mut f.store.0, &f.scope.account_id, None, &f.prior).unwrap();
    let claimed = claim(
        &f.store, &f.stream, &f.review, &f.writer, serial, &f.prior, &f.next,
    )
    .unwrap();
    finish(
        &f,
        &c::target(&claimed),
        Delivery::Reply(json!({"ok":true,"keep_input":true})),
    );
    let local = current(&f).local().clone();
    checkpoint(&f, serial + 1, &local);
    (f, ids, serial + 1)
}
#[test]
fn archive_read_phases_validate_all_history_once_then_reaudit_after_write() {
    for historical in [1, 4, 8] {
        let (f, _, serial) = ready(historical);
        let total = historical + 1;
        super::super::super::super::bindings::take_validation_work();
        let review = inspected(&f, serial);
        assert_eq!(
            super::super::super::super::bindings::take_validation_work(),
            total
        );
        a::archive_known(&f.store, &review).unwrap();
        assert_eq!(
            super::super::super::super::bindings::take_validation_work(),
            2 * total
        );
        assert_eq!(count(&f, "composer_draft_slash_archives"), total as i64);
        assert_eq!(count(&f, "composer_draft_slash_bindings"), 0);
        // Repeated archive still revalidates current history and returns the
        // exact immutable archived record, without advancing local input.
        let before = current(&f);
        a::archive_known(&f.store, &review).unwrap();
        assert_eq!(
            super::super::super::super::bindings::take_validation_work(),
            total
        );
        assert!(current(&f).same(&before));
    }
}
#[test]
fn completed_read_phase_never_hides_later_unrelated_corruption() {
    let (f, ids, serial) = ready(2);
    let review = inspected(&f, serial);
    for (table, column) in [
        ("composer_draft_slash_archives", "binding"),
        ("composer_draft_slash_completions", "content"),
        ("composer_draft_slash_attestations", "content"),
        ("composer_draft_slash_archives", "content"),
    ] {
        let query = format!("SELECT {column} FROM {table} WHERE intent=?1");
        let original: String = f
            .store
            .0
            .query_row(&query, [&ids[0]], |r| r.get(0))
            .unwrap();
        f.store
            .0
            .execute(
                &format!("UPDATE {table} SET {column}='{{}}' WHERE intent=?1"),
                [&ids[0]],
            )
            .unwrap();
        assert!(
            a::inspect(
                &f.store,
                &f.stream,
                &current(&f),
                &f.writer,
                serial,
                f.next.id()
            )
            .is_err()
        );
        assert!(a::archive_known(&f.store, &review).is_err());
        assert_eq!(count(&f, "composer_draft_slash_archives"), 2);
        assert_eq!(count(&f, "composer_draft_slash_bindings"), 1);
        f.store
            .0
            .execute(
                &format!("UPDATE {table} SET {column}=?1 WHERE intent=?2"),
                params![original, ids[0]],
            )
            .unwrap();
        evidence::audit(&f.store.0, &f.scope.account_id).unwrap();
    }
    a::archive_known(&f.store, &review).unwrap();
}
#[test]
fn fresh_post_write_audit_rolls_back_triggered_history_corruption() {
    let (f, ids, serial) = ready(2);
    let review = inspected(&f, serial);
    let before = current(&f);
    let original: String = f
        .store
        .0
        .query_row(
            "SELECT content FROM composer_draft_slash_attestations WHERE intent=?1",
            [&ids[0]],
            |r| r.get(0),
        )
        .unwrap();
    f.store.0.execute_batch("CREATE TRIGGER corrupt_archive_history AFTER INSERT ON composer_draft_slash_archives BEGIN UPDATE composer_draft_slash_attestations SET content='{}' WHERE intent<>NEW.intent; END;").unwrap();
    assert!(a::archive_known(&f.store, &review).is_err());
    assert!(current(&f).same(&before));
    assert_eq!(count(&f, "composer_draft_slash_archives"), 2);
    assert_eq!(count(&f, "composer_draft_slash_bindings"), 1);
    let retained: String = f
        .store
        .0
        .query_row(
            "SELECT content FROM composer_draft_slash_attestations WHERE intent=?1",
            [&ids[0]],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, original);
    f.store
        .0
        .execute_batch("DROP TRIGGER corrupt_archive_history")
        .unwrap();
    evidence::audit(&f.store.0, &f.scope.account_id).unwrap();
    a::archive_known(&f.store, &review).unwrap();
}
#[test]
fn read_phase_requires_transaction_and_exact_account_and_scope() {
    let (f, _, _) = ready(1);
    assert!(evidence::read_phase(&f.store.0, &f.scope.account_id).is_err());
    let tx = adapter_atomic::begin(&f.store.0).unwrap();
    assert!(evidence::read_phase(&tx, "foreign-account").is_err());
    let phase = evidence::read_phase(&tx, &f.scope.account_id).unwrap();
    let mut foreign = f.scope.clone();
    foreign.account_id = "foreign-account".into();
    assert!(phase.binding(&foreign, f.next.id()).is_err());
    assert!(phase.settlement(&foreign, f.next.id()).is_err());
    assert!(phase.binding(&f.scope, f.next.id()).is_ok());
    assert!(phase.settlement(&f.scope, f.next.id()).is_ok());
    assert!(phase.attestation(f.next.id()).is_some());
    drop(phase);
    tx.commit().unwrap();
}

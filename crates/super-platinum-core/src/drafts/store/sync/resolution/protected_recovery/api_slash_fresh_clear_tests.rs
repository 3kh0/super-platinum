//! Actual SQLite first-commit evidence; historical Effect is never a capability.
use super::super::{completion as c, history};
use super::*;
use crate::slash_commands::controller::{self, Delivery};
fn setup(f: &Fixture) -> c::Target {
    c::install(&f.store, &f.scope.account_id).unwrap();
    history::migrate(&f.store, &f.scope.account_id).unwrap();
    c::target(&f.claim().unwrap())
}
#[test]
fn first_attested_clear_witness_binds_original_prepared_and_cannot_repeat_on_retry() {
    let f = Fixture::new();
    let target = setup(&f);
    let terminal =
        controller::isolated_terminal(&f.next, Delivery::Reply(json!({"ok":true})), None);
    let mut first = c::complete_attested(&f.store, &target, &terminal).unwrap();
    let witness = first.take_fresh_clear().unwrap();
    assert!(witness.matches(&f.prior, first.record()));
    let (different, _) = records(&f.review);
    assert!(!witness.matches(&different, first.record()));
    assert!(first.take_fresh_clear().is_none());
    let mut retry = c::complete_attested(&f.store, &target, &terminal).unwrap();
    assert_eq!(retry.effect(), c::Effect::ClearedExactAcknowledged);
    assert!(retry.take_fresh_clear().is_none());
    let recovered = c::recover(&f.store, &f.scope, f.next.id()).unwrap();
    assert!(
        c::preserve_recovered(&f.store, &recovered)
            .unwrap()
            .take_fresh_clear()
            .is_none()
    );
}
#[test]
fn decoded_completion_cannot_issue_or_backfill_fresh_native_clear() {
    let f = Fixture::new();
    let target = setup(&f);
    let terminal =
        controller::isolated_terminal(&f.next, Delivery::Reply(json!({"ok":true})), None);
    let mut unsealed = c::complete(&f.store, &target, terminal.terminal()).unwrap();
    assert_eq!(unsealed.effect(), c::Effect::ClearedExactAcknowledged);
    assert!(unsealed.take_fresh_clear().is_none());
    assert!(c::complete_attested(&f.store, &target, &terminal).is_err());
}
#[test]
fn newer_checkpoint_preserves_input_and_receives_no_fresh_clear_witness() {
    let f = Fixture::new();
    let target = setup(&f);
    let current = views::review(&f.store.0, &f.scope).unwrap();
    let newer = local("newer exact input");
    let WriteResult::Applied(_) =
        writers::checkpoint(&f.store, &f.writer, 2, Some(&current), &newer, None).unwrap()
    else {
        panic!("writer accepted")
    };
    let terminal =
        controller::isolated_terminal(&f.next, Delivery::Reply(json!({"ok":true})), None);
    let mut settled = c::complete_attested(&f.store, &target, &terminal).unwrap();
    assert_eq!(settled.effect(), c::Effect::PreservedCurrent);
    assert!(settled.take_fresh_clear().is_none());
    assert_eq!(
        serde_json::to_value(views::review(&f.store.0, &f.scope).unwrap().local()).unwrap(),
        serde_json::to_value(newer).unwrap()
    );
}

//! Copy into the exact isolated core's tests directory; compile with
//! cargo test --locked --features protected-test-support --test slash_fixture_external --no-run
//! External crate privacy check only. No network/production/native activation.
#![cfg(feature = "protected-test-support")]
#![allow(dead_code)]
use super_platinum_core::drafts::store::protected::owned_store::{
    OwnedRoute, SlashCompletion, SlashFixture, SlashFixtureOutcome, SlashFixtureTarget,
    SlashFreshClear,
};
use super_platinum_core::slash_commands::controller::Record;

fn witness(c: &SlashCompletion, prepared: &Record, terminal: &Record) {
    let value: Option<SlashFreshClear> = c.take_fresh_clear();
    if let Some(value) = value {
        let _ = value.belongs_to(c);
        let _ = value.matches(prepared, terminal);
        let _: Result<(), SlashFreshClear> = value.consume(c, prepared, terminal);
    }
}
fn context() -> (OwnedRoute, SlashFixture) {
    OwnedRoute::fresh_slash_fixture(SlashFixtureTarget::Thread, SlashFixtureOutcome::Clear).unwrap()
}
#[test]
fn external_signatures_compile_without_exposing_storage_witness() {
    let _witness: fn(&SlashCompletion, &Record, &Record) = witness;
    let _context: fn() -> (OwnedRoute, SlashFixture) = context;
}

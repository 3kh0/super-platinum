//! Typed protected storage boundary. No activation, SQL closure or raw state save.
use super::*;
#[path = "api_types.rs"]
mod types;
pub use types::*;
pub use types::{Basis, KnownReadFailure, LoadMode, Loaded, Release};

#[path = "api_local.rs"]
mod local;
#[path = "api_inventory.rs"]
mod views;
#[path = "api_writers.rs"]
mod writers;

#[path = "api_operations.rs"]
mod operations;

#[path = "api_baseline.rs"]
mod baseline;

#[path = "api_journal.rs"]
mod journal;

#[path = "api_checks.rs"]
mod checks;
#[path = "api_load.rs"]
mod loading;
#[path = "api_observation.rs"]
mod observation;
fn required_option<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

#[path = "api_client.rs"]
mod client;
pub use client::Client;
#[path = "api_commands.rs"]
pub(crate) mod commands;

pub(in crate::drafts::store) fn touch_local(
    conn: &Connection,
    key: &DraftKey,
) -> Result<(), AppError> {
    adapter_schema::touch_local_if_installed(conn, key)
}
#[cfg(any(test, feature = "protected-test-support"))]
#[path = "api_test_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "api_tests.rs"]
mod adapter_tests;

#[path = "api_faults.rs"]
mod faults;

#[path = "api_lifecycle.rs"]
mod lifecycle;

#[path = "api_slash_intent.rs"]
mod slash_intent;
// Private decoder bridge; validates retained remote-read identity without recursion.
pub(super) fn validate_slash_remote_read(
    conn: &Connection,
    id: &str,
    record: &ReadRecord,
) -> Result<(), AppError> {
    slash_intent::validate_remote_read(conn, id, record)
}
pub(super) fn slash_guard(conn: &Connection, scope: &Scope) -> Result<(), AppError> {
    slash_intent::guard(conn, scope)
}

#[path = "api_store_upgrade.rs"]
pub(in crate::drafts::store) mod upgrade;

#[path = "api_owned_store.rs"]
pub mod owned_store;

pub(crate) use slash_intent::runtime as slash_runtime;
pub(crate) use slash_intent::{Claim as SlashClaim, FreshClear, SlashSettlement, SlashTarget};

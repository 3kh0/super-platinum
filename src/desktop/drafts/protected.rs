//! Unwired protected desktop preparation. No legacy persistence conversion.
#![allow(dead_code)]
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::drafts::{DraftKey, LocalDraft, store::protected as durable, sync::Scope};

mod account;
mod actions;
#[cfg(feature = "protected-native-fixture")]
pub(crate) mod adoption_fixture;
mod baseline;
mod baseline_choice;
pub(crate) mod delivery;
mod discovery;
mod dispatch;
pub(crate) mod editor;
mod editor_open;
mod editor_proof;
#[cfg(feature = "protected-native-fixture")]
mod enrollment_consent;
#[cfg(feature = "protected-native-fixture")]
pub(crate) mod enrollment_fixture;
mod journal;
pub(crate) mod lifecycle;
mod local_writer;
mod model;
#[cfg(feature = "protected-native-fixture")]
pub(crate) mod native_fixture;
mod observations;
pub(crate) mod operation;
mod outcome;
mod owned_baseline;
mod owned_editor;
#[cfg(feature = "protected-native-fixture")]
pub(crate) mod owned_native;
mod owned_release;
mod owned_route;
#[cfg(test)]
mod owned_route_tests;
mod owned_send;
pub(crate) mod owned_slash;
pub(crate) mod owned_upload;
mod owner;
mod ownership;
pub(crate) mod projection;
mod recovery;
mod refresh;
pub(crate) mod registry;
mod release;
mod retention;
mod session;
mod settlement;
mod slash_editor;
#[cfg(feature = "protected-native-fixture")]
pub(crate) mod slash_native;
mod source;
pub(crate) mod status_ui;
#[cfg(test)]
mod tests;
mod visit;

pub(crate) use account::Account;
pub(crate) use actions::{Action, Applied, NativeClear};
pub(crate) use baseline::{BaselineRun, Evidence};
pub(crate) use dispatch::Dispatched;
pub(crate) use editor_proof::ReviewLease;
pub(crate) use model::{Inventory, View};
pub(crate) use observations::{ObservationQueue, Observed};
pub(crate) use owner::Owner;
pub(crate) use recovery::{CheckRun, Checked};
pub(crate) use session::Session;
pub(crate) use source::Source;
pub(crate) use visit::Visit;

fn same_local(left: &LocalDraft, right: &LocalDraft) -> bool {
    matches!((serde_json::to_value(left), serde_json::to_value(right)),
        (Ok(left), Ok(right)) if left == right)
}

fn barrier_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .expect("projection identity exhausted");
    format!("protected-{id}")
}

pub(crate) mod production;
mod root_factory;

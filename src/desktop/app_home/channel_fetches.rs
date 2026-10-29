//! Shared initialization and ordered channel publication, independent of a view.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, SlackClient, Transport},
    state::directory_channels::{Facts, Observation, Origin, RendererReply},
};

#[cfg(test)]
pub(crate) mod calendar_member_test_support;
pub(crate) mod canonical;
pub(crate) mod conditional;
pub(super) mod context;
mod drain;
mod emoji;
mod events;
pub(crate) mod fixture;
mod initial;
mod members;
mod payload;
pub(crate) use payload::file_saved_search as file_saved_parameters;
pub(crate) use payload::file_search as file_search_parameters;
pub(crate) use payload::people as people_parameters;
pub(crate) use payload::plan_info;
mod render;
pub(crate) mod search;
pub(crate) mod teams;
mod topics;
pub(crate) mod unknown;
use context::Context;
pub(crate) use emoji::{
    EmojiStandard, emoji_standard, fixture_emoji_assignments, fixture_emoji_guard_probe,
};
pub(crate) use events::request_initialization;
pub(crate) use events::{after_event, before_event, connected, resume};
pub(crate) use initial::{initialize, resync};
pub(crate) use members::{MemberFlags, member_flags};
pub(crate) use render::fields_guarded as member_fields;

const GENERATION: &str = "shared-channel-initialization";
type Key = (String, String, String);
fn unavailable() -> Error {
    Error::Api("channel_initialization_unavailable".into())
}

#[derive(Default)]
pub(crate) struct Registry {
    slots: BTreeMap<Key, Slot>,
    topics: topics::Queue<u64>,
    topic_owners: BTreeMap<u64, Owner>,
    topic_timer: Option<Arc<()>>,
    bursts: super_platinum_core::state::directory_channels::freshness::InvalidationBursts,
    fixture: Option<fixture::Model>,
    startup: BTreeMap<Key, (Arc<Context>, bool)>,
    conditional: conditional::Registry,
    unknown: unknown::Registry,
    posting: Vec<search::posting::Pending>,
    opened: Vec<search::open::Pending>,
}
struct Slot {
    serial: u64,
    context: Arc<Context>,
    token: Arc<()>,
    changes: tokio::sync::watch::Sender<u64>,
    publication: Arc<Observation>,
    transformations: Arc<members::Transformations>,
    drain: Option<Arc<()>>,
    retry: Option<Arc<()>>,
    retry_at: Option<Instant>,
    initial: Option<initial::Attempt>,
    payload: payload::State,
    errors: Vec<Arc<Error>>,
}
impl Slot {
    fn notify(&self) {
        self.changes.send_modify(|n| *n = n.wrapping_add(1));
    }
}
#[derive(Clone)]
struct Owner {
    context: Arc<Context>,
    token: Arc<()>,
}
impl Owner {
    fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && shell
                .app_home
                .channel_fetches
                .slots
                .get(&self.context.key())
                .is_some_and(|slot| {
                    Arc::ptr_eq(&slot.token, &self.token)
                        && Arc::ptr_eq(&slot.context, &self.context)
                })
    }
    fn publication_current(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && shell
                .app_home
                .channel_fetches
                .slots
                .get(&self.context.key())
                .is_some_and(|slot| observed_current(shell, &self.context, &slot.publication))
    }
}
fn observe(shell: &ShellState, context: &Context) -> Result<Observation, Error> {
    if !context.current(shell) {
        return Err(unavailable());
    }
    let w = shell
        .core
        .workspaces
        .get(&context.session.team_id)
        .ok_or_else(unavailable)?;
    w.directory_channels
        .observation(
            &context.account,
            &context.session,
            context.epoch,
            GENERATION,
            &w.channels,
        )
        .map_err(|_| unavailable())
}
fn observed_current(shell: &ShellState, context: &Context, observed: &Observation) -> bool {
    context.current(shell)
        && shell
            .core
            .workspaces
            .get(&context.session.team_id)
            .is_some_and(|w| {
                observed.current(
                    &w.directory_channels,
                    &context.account,
                    &context.session,
                    context.epoch,
                    GENERATION,
                    &w.channels,
                )
            })
}
// Only call in the same serial turn as an accepted source/paired-native change.
fn advance(shell: &mut ShellState, owner: &Owner) -> Result<(), Error> {
    if !owner.current(shell) {
        return Err(unavailable());
    }
    let next = Arc::new(observe(shell, &owner.context)?);
    let slot = shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .unwrap();
    slot.publication = next;
    slot.notify();
    Ok(())
}
fn failed(shell: &mut ShellState, owner: &Owner, error: Error) {
    if !owner.current(shell) {
        return;
    }
    let slot = shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .unwrap();
    if let Error::RateLimited { retry_after_secs } = &error {
        let at = Instant::now() + Duration::from_secs(retry_after_secs.unwrap_or(60));
        slot.retry_at = Some(slot.retry_at.map_or(at, |old| old.max(at)));
    }
    slot.errors.push(Arc::new(error));
    if slot.errors.len() > 32 {
        slot.errors.remove(0);
    }
    slot.notify();
}
/// Carry timer/history custody without granting the replacement account access.
pub(crate) fn carry(previous: &mut ShellState, loaded: &mut ShellState) {
    loaded.app_home.channel_fetches = std::mem::take(&mut previous.app_home.channel_fetches);
}

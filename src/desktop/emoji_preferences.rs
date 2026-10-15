//! Owner-bound emoji preference reads and acknowledged usage persistence.
use crate::{scheduled::Context, state::ShellState};
use dioxus::prelude::*;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use super_platinum_core::{
    composer_emoji::{admission::Usage, api},
    config::WorkspaceSession,
};

mod boundary;
#[cfg(test)]
mod tests;
mod writer;
use boundary::{Boundary, Native};

fn retry_delay(error: &super_platinum_core::slack::Error) -> Option<Duration> {
    use super_platinum_core::slack::Error;
    match error {
        Error::RateLimited { retry_after_secs } => {
            Some(Duration::from_secs(retry_after_secs.unwrap_or(60)))
        }
        Error::HttpStatus {
            retry_after_secs: Some(seconds),
            ..
        } => Some(Duration::from_secs(*seconds)),
        _ => None,
    }
}

pub(crate) fn same_session(a: &WorkspaceSession, b: &WorkspaceSession) -> bool {
    a.team_id == b.team_id
        && a.user_id == b.user_id
        && a.token == b.token
        && a.url == b.url
        && a.enterprise_id == b.enterprise_id
        && a.name == b.name
}
struct Owner {
    context: Context,
    account: Option<String>,
}
impl Owner {
    fn capture(shell: &ShellState) -> Option<Self> {
        Some(Self {
            context: Context::capture(shell)?,
            account: shell.core.active_account.clone(),
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && self.account == shell.core.active_account
            && shell
                .core
                .session
                .as_ref()
                .and_then(|s| s.workspaces.get(&self.context.session.team_id))
                .is_some_and(|s| same_session(s, &self.context.session))
    }
    fn same(&self, other: &Self) -> bool {
        self.context.epoch == other.context.epoch
            && self.account == other.account
            && same_session(&self.context.session, &other.context.session)
            && Arc::ptr_eq(&self.context.transport, &other.context.transport)
    }
}
struct Read {
    owner: Arc<Owner>,
    attempted: Instant,
    not_before: Option<Instant>,
    loading: bool,
}
#[derive(Clone)]
struct Visit {
    usage: Usage,
    now_ms: f64,
}
struct Writer {
    owner: Arc<Owner>,
    order: writer::Order,
    pending: Vec<Visit>,
    latest: Option<String>,
    applied_revision: u64,
    seq: u64,
    running: bool,
    last_dispatch: Option<Instant>,
    not_before: Option<Instant>,
    blocked: bool,
}
impl Writer {
    fn new(owner: Arc<Owner>) -> Self {
        Self {
            owner,
            order: Default::default(),
            pending: vec![],
            latest: None,
            applied_revision: 0,
            seq: 0,
            running: false,
            last_dispatch: None,
            not_before: None,
            blocked: false,
        }
    }
    fn wait(&self, now: Instant) -> Duration {
        let throttle = self
            .last_dispatch
            .and_then(|at| at.checked_add(Duration::from_secs(60)));
        throttle
            .into_iter()
            .chain(self.not_before)
            .map(|at| at.saturating_duration_since(now))
            .max()
            .unwrap_or_default()
    }
}
#[derive(Default)]
pub(crate) struct State {
    reads: HashMap<String, Read>,
    writers: HashMap<String, Writer>,
}
impl State {
    fn retire_read(&mut self, owner: &Arc<Owner>) {
        let team = &owner.context.session.team_id;
        if self
            .reads
            .get(team)
            .is_some_and(|r| Arc::ptr_eq(&r.owner, owner))
        {
            self.reads.remove(team);
        }
    }
    fn retire_writer(&mut self, owner: &Arc<Owner>) {
        let team = &owner.context.session.team_id;
        if self
            .writers
            .get(team)
            .is_some_and(|w| Arc::ptr_eq(&w.owner, owner))
        {
            self.writers.remove(team);
        }
    }
}

pub(crate) fn ensure(state: Signal<ShellState>) {
    if crate::emoji_native::preferences_held() {
        return;
    }
    ensure_with(Native, state);
}
fn ensure_with<B: Boundary>(boundary: B, mut state: Signal<ShellState>) {
    let (owner, revision) = {
        let mut shell = state.write();
        let Some(team) = shell.core.active_team.clone() else {
            return;
        };
        let Some(workspace) = shell.core.workspaces.get(&team) else {
            return;
        };
        if workspace.emoji_preferences.loaded() {
            return;
        }
        let revision = workspace.emoji_preferences.revision();
        if boundary.is_fixture() {
            shell
                .core
                .workspaces
                .get_mut(&team)
                .unwrap()
                .emoji_preferences
                .replace(&Default::default());
            return;
        }
        let Some(owner) = Owner::capture(&shell).map(Arc::new) else {
            return;
        };
        let now = boundary.now();
        if shell.emoji_preferences.reads.get(&team).is_some_and(|r| {
            r.owner.same(&owner)
                && (r.loading
                    || now.saturating_duration_since(r.attempted) < Duration::from_secs(30)
                    || r.not_before.is_some_and(|at| now < at))
        }) {
            return;
        }
        shell.emoji_preferences.reads.insert(
            team,
            Read {
                owner: owner.clone(),
                attempted: now,
                not_before: None,
                loading: true,
            },
        );
        (owner, revision)
    };
    boundary.clone().spawn(Box::pin(async move {
        let admitted = {
            let shell = state.peek();
            owner.current(&shell)
                && shell
                    .emoji_preferences
                    .reads
                    .get(&owner.context.session.team_id)
                    .is_some_and(|read| Arc::ptr_eq(&read.owner, &owner))
        };
        if !admitted {
            state.write().emoji_preferences.retire_read(&owner);
            return;
        }
        let request = api::preferences_request(&owner.context.client, &owner.context.session);
        let result = boundary
            .execute(owner.clone(), request)
            .await
            .and_then(|v| api::parse_preferences(&v));
        if !owner.current(&state.peek()) {
            state.write().emoji_preferences.retire_read(&owner);
            return;
        }
        let team = &owner.context.session.team_id;
        let mut shell = state.write();
        // Full credentials can return to earlier values. Only this exact read
        // slot may complete; an old A response cannot replace a newer A/B/A read.
        if shell
            .emoji_preferences
            .reads
            .get(team)
            .is_none_or(|r| !Arc::ptr_eq(&r.owner, &owner))
        {
            return;
        }
        if let Some(read) = shell
            .emoji_preferences
            .reads
            .get_mut(team)
            .filter(|r| Arc::ptr_eq(&r.owner, &owner))
        {
            read.loading = false;
            if let Some(delay) = result.as_ref().err().and_then(retry_delay) {
                // An unrepresentable deadline must not become immediate retry.
                read.not_before = boundary.now().checked_add(delay);
                if read.not_before.is_none() {
                    read.loading = true;
                }
            }
        }
        match result {
            Ok(values) => {
                let prefs = &mut shell
                    .core
                    .workspaces
                    .get_mut(team)
                    .unwrap()
                    .emoji_preferences;
                if prefs.revision() == revision {
                    prefs.replace(&values);
                } else {
                    shell.emoji_preferences.reads.remove(team);
                }
                drop(shell);
                boundary.persist(state, team);
            }
            Err(error) => {
                drop(shell);
                boundary.failure(state, error, "Could not load emoji preferences");
            }
        }
    }));
}

pub(crate) fn record(
    state: Signal<ShellState>,
    usage: Usage,
) -> impl std::future::Future<Output = ()> {
    writer::record_with(Native, state, usage)
}

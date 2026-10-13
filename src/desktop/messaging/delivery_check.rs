//! Explicit, bounded reads of retained upload receipts. Never resends a file.
use super::{
    ShellState,
    ownership::{UploadReadback, ingest_upload_page},
    upload_recovery,
};
use dioxus::prelude::*;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};
use super_platinum_core::{
    domain::PendingFileMessage,
    slack::{Error, Transport, models::HistoryPage},
};
pub(crate) mod check;
pub(crate) mod fixture;
mod network;
#[cfg(test)]
mod tests;
const MAX_CHECKS: usize = 32;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    account: Option<String>,
    team: String,
    channel: String,
    client: String,
}
#[derive(Clone)]
pub(crate) struct Action {
    key: Key,
    sender: String,
    ts: String,
    root: Option<String>,
    files: Vec<String>,
    broadcast: bool,
    epoch: u64,
    transport: Option<Arc<Transport>>,
    credential: u64,
    channel_generation: u64,
    thread_generation: Option<u64>,
}
#[derive(Clone)]
struct Attempt {
    action: Action,
    serial: u64,
}
#[derive(Default)]
pub(crate) struct State {
    entries: HashMap<Key, Entry>,
    cooldowns: HashMap<ReadOwner, Deadline>,
    serial: u64,
    pub(super) fixture: Option<fixture::Fixture>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ReadOwner {
    account: Option<String>,
    team: String,
    sender: String,
    credential: u64,
    replies: bool,
}
#[derive(Default)]
struct Deadline {
    at: Option<Instant>,
    overflow: bool,
}
impl Deadline {
    fn active(&self, now: Instant) -> bool {
        self.overflow || self.at.is_some_and(|at| now < at)
    }
    fn extend(&mut self, now: Instant, delay: Duration) -> Option<Duration> {
        match now.checked_add(delay) {
            None => self.overflow = true,
            Some(at) => self.at = Some(self.at.map_or(at, |old| old.max(at))),
        }
        if self.overflow {
            None
        } else {
            self.at.map(|at| at.saturating_duration_since(now))
        }
    }
}
struct Entry {
    serial: u64,
    busy: bool,
    owner: ReadOwner,
    message: &'static str,
}
pub(crate) struct Presentation {
    pub action: Action,
    pub busy: bool,
    pub disabled: bool,
    pub message: &'static str,
}
fn stamp(shell: &ShellState, team: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.workspaces.get(team))
        .map(|s| (&s.user_id, &s.token))
        .hash(&mut h);
    shell
        .core
        .transport
        .as_ref()
        .map(|t| t.d_cookie())
        .hash(&mut h);
    h.finish()
}
impl Action {
    fn read_owner(&self) -> ReadOwner {
        ReadOwner {
            account: self.key.account.clone(),
            team: self.key.team.clone(),
            sender: self.sender.clone(),
            credential: self.credential,
            replies: self.root.is_some(),
        }
    }

    fn pending<'a>(&self, shell: &'a ShellState) -> Option<&'a PendingFileMessage> {
        shell.core.pending_file_messages.iter().find(|p| {
            p.team == self.key.team
                && p.channel == self.key.channel
                && p.client_msg_id == self.key.client
                && p.message_ts == self.ts
                && p.thread_ts == self.root
                && p.reply_broadcast == self.broadcast
                && p.finalized.as_ref().is_some_and(|r| {
                    r.account == self.key.account
                        && r.sender == self.sender
                        && r.file_ids == self.files
                })
        })
    }
    fn owner_current(&self, shell: &ShellState) -> bool {
        self.key.account == shell.core.active_account
            && shell.core.account_epoch == self.epoch
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && stamp(shell, &self.key.team) == self.credential
            && shell
                .core
                .workspaces
                .get(&self.key.team)
                .is_some_and(|w| w.self_user_id == self.sender)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.owner_current(shell)
            && shell.core.active_team.as_ref() == Some(&self.key.team)
            && shell.core.active_channel.as_ref() == Some(&self.key.channel)
            && shell.channel_generation == self.channel_generation
            && self
                .thread_generation
                .is_none_or(|g| shell.thread_generation == g)
            && self.pending(shell).is_some()
    }
    fn expected(&self) -> UploadReadback {
        UploadReadback {
            team: self.key.team.clone(),
            channel: self.key.channel.clone(),
            thread_ts: self.root.clone(),
            reply_broadcast: self.broadcast,
            sender: self.sender.clone(),
            file_ids: self.files.clone(),
        }
    }
}
pub(crate) fn presentation(shell: &ShellState, ts: &str) -> Option<Presentation> {
    if shell.pending_delivery.get(ts) != Some(&crate::state::PendingDeliveryStatus::Unconfirmed) {
        return None;
    }
    let pending = shell.core.pending_file_messages.iter().find(|p| {
        p.message_ts == ts
            && Some(&p.team) == shell.core.active_team.as_ref()
            && Some(&p.channel) == shell.core.active_channel.as_ref()
    })?;
    let receipt = pending.finalized.as_ref()?;
    if receipt.account != shell.core.active_account
        || !shell
            .core
            .workspaces
            .get(&pending.team)
            .is_some_and(|w| w.self_user_id == receipt.sender)
    {
        return None;
    }
    let action = Action {
        key: Key {
            account: receipt.account.clone(),
            team: pending.team.clone(),
            channel: pending.channel.clone(),
            client: pending.client_msg_id.clone(),
        },
        sender: receipt.sender.clone(),
        ts: ts.into(),
        root: pending.thread_ts.clone(),
        files: receipt.file_ids.clone(),
        broadcast: pending.reply_broadcast,
        epoch: shell.core.account_epoch,
        transport: shell.core.transport.clone(),
        credential: stamp(shell, &pending.team),
        channel_generation: shell.channel_generation,
        thread_generation: pending.thread_ts.as_ref().map(|_| shell.thread_generation),
    };
    let entry = shell.upload_checks.entries.get(&action.key);
    let busy = entry.is_some_and(|e| e.busy);
    let cooling = shell
        .upload_checks
        .cooldowns
        .get(&action.read_owner())
        .is_some_and(|d| d.active(Instant::now()));
    let available = fixture::enabled(shell)
        || (shell.core.transport.is_some()
            && shell
                .core
                .session
                .as_ref()
                .is_some_and(|s| s.workspaces.contains_key(&pending.team)));
    Some(Presentation {
        action,
        busy,
        disabled: busy || cooling || !available,
        message: if busy {
            "Checking the conversation…"
        } else if cooling {
            "Slack asked us to wait before checking again."
        } else if !available {
            "Reconnect to check delivery."
        } else {
            entry.map_or("Checks the conversation without uploading again.", |e| {
                e.message
            })
        },
    })
}
fn begin(shell: &mut ShellState, action: Action, now: Instant) -> Option<Attempt> {
    if !action.current(shell)
        || shell.pending_delivery.get(&action.ts)
            != Some(&crate::state::PendingDeliveryStatus::Unconfirmed)
    {
        return None;
    }
    let owner = action.read_owner();
    let checks = &mut shell.upload_checks;
    checks.entries.retain(|key, e| {
        e.busy
            || shell.core.pending_file_messages.iter().any(|p| {
                p.team == key.team && p.channel == key.channel && p.client_msg_id == key.client
            })
    });
    checks.cooldowns.retain(|owner, d| {
        d.active(now) || checks.entries.values().any(|e| e.busy && &e.owner == owner)
    });
    if checks.entries.get(&action.key).is_some_and(|e| e.busy)
        || checks.cooldowns.get(&owner).is_some_and(|d| d.active(now))
    {
        return None;
    }
    if (!checks.entries.contains_key(&action.key) && checks.entries.len() >= MAX_CHECKS)
        || (!checks.cooldowns.contains_key(&owner) && checks.cooldowns.len() >= MAX_CHECKS)
    {
        shell.show_toast("Finish an existing delivery check before starting another.");
        return None;
    }
    checks.cooldowns.entry(owner.clone()).or_default();
    shell.upload_checks.serial = shell.upload_checks.serial.wrapping_add(1);
    let serial = shell.upload_checks.serial;
    shell.upload_checks.entries.insert(
        action.key.clone(),
        Entry {
            serial,
            busy: true,
            owner,
            message: "Checking the conversation…",
        },
    );
    Some(Attempt { action, serial })
}
fn retry_delay(error: Option<&Error>) -> Option<Duration> {
    match error {
        Some(Error::RateLimited { retry_after_secs }) => {
            Some(Duration::from_secs(retry_after_secs.unwrap_or(5).max(1)))
        }
        Some(Error::Api(code)) if matches!(code.as_str(), "ratelimited" | "rate_limited") => {
            Some(Duration::from_secs(30))
        }
        Some(Error::HttpStatus {
            retry_after_secs: Some(s),
            ..
        }) => Some(Duration::from_secs((*s).max(1))),
        _ => None,
    }
}
fn finish(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<HistoryPage, Error>,
    now: Instant,
) -> Option<Duration> {
    let entry = shell
        .upload_checks
        .entries
        .get_mut(&attempt.action.key)
        .filter(|e| e.serial == attempt.serial && e.busy)?;
    entry.busy = false;
    let owner = entry.owner.clone();
    let delay = retry_delay(result.as_ref().err());
    entry.message = match &result {
        Err(Error::Offline(_)) => "Reconnect, then check again. Delivery is still unconfirmed.",
        Err(_) => "Could not confirm delivery. Check again when ready.",
        Ok(_) => "Not found in the checked messages. Delivery is still unconfirmed.",
    };
    let delay = delay.and_then(|d| {
        shell
            .upload_checks
            .cooldowns
            .entry(owner)
            .or_default()
            .extend(now, d)
    });
    if attempt.action.current(shell) {
        if let Ok(page) = result {
            let ids = page
                .messages
                .iter()
                .filter_map(|m| m.ts.clone())
                .collect::<Vec<_>>();
            ingest_upload_page(shell, &attempt.action.expected(), page);
            upload_recovery::reconcile(
                shell,
                &attempt.action.key.team,
                &attempt.action.key.channel,
                Some(&ids),
            );
            shell.refresh_from_core();
        }
    }
    if attempt.action.pending(shell).is_none() {
        shell.upload_checks.entries.remove(&attempt.action.key);
    }
    delay
}
pub(crate) fn check(mut state: Signal<ShellState>, action: Action) {
    let attempt = begin(&mut state.write(), action, Instant::now());
    let Some(attempt) = attempt else {
        return;
    };
    if fixture::enabled(&state.peek()) {
        fixture::dispatch(&mut state.write(), attempt);
        return;
    }
    // Root-owned task records Retry-After even when the message row unmounts.
    dioxus::core::spawn_forever(async move {
        let result = network::run(state, &attempt).await;
        complete_attempt(state, attempt, result);
    });
}
fn complete_attempt(
    mut state: Signal<ShellState>,
    attempt: Attempt,
    result: Result<HistoryPage, Error>,
) {
    let delay = finish(&mut state.write(), &attempt, result, Instant::now());
    if let Some(delay) = delay {
        dioxus::core::spawn_forever(async move {
            tokio::time::sleep(delay).await;
            let mut shell = state.write();
            let owner = attempt.action.read_owner();
            if shell
                .upload_checks
                .cooldowns
                .get(&owner)
                .is_some_and(|d| !d.active(Instant::now()))
            {
                shell.upload_checks.cooldowns.remove(&owner);
            }
        });
    }
}

fn read_allowed(shell: &ShellState, attempt: &Attempt, now: Instant) -> bool {
    attempt.action.current(shell)
        && shell
            .upload_checks
            .entries
            .get(&attempt.action.key)
            .is_some_and(|e| e.busy && e.serial == attempt.serial)
        && !shell
            .upload_checks
            .cooldowns
            .get(&attempt.action.read_owner())
            .is_some_and(|d| d.active(now))
}

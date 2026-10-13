//! Stable-owner notification mutations; only explicit actions dispatch writes.
use crate::{scheduled::Context, state::ShellState};
use dioxus::prelude::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use super_platinum_core::slack::{Error, api};
mod model;
use model::Owner;
pub(crate) use model::Token;
use model::*;
pub(crate) mod check;
pub(crate) mod fixture;
mod network;
#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
pub(crate) struct State {
    entries: BTreeMap<Owner, Entry>,
    cooldowns: BTreeMap<RateOwner, i64>,
    fixture: Option<fixture::Fixture>,
}
pub(crate) struct Presentation {
    pub label: String,
    pub muted: bool,
    pub busy: bool,
    pub uncertain: bool,
    pub can_change: bool,
    pub can_check: bool,
    pub can_temporary: bool,
    pub can_mute_until_tomorrow: bool,
    pub temporary_until: Option<i64>,
    pub temporary_reason: Option<String>,
    pub message: Option<String>,
    pub token: Token,
}
fn now() -> i64 {
    super_platinum_core::state::now_secs()
}
fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
pub(crate) fn presentation(shell: &ShellState, channel: &str) -> Option<Presentation> {
    if !eligible(shell, channel) {
        return None;
    }
    let token = Token::capture(shell, channel)?;
    let config = &shell.core.workspaces[&token.owner.team].notifications;
    let muted = config.muted(channel, now());
    let permanent = config
        .channels
        .get(channel)
        .and_then(|p| p.get("muted"))
        .and_then(Value::as_bool)
        == Some(true);
    let entry = shell.channel_mute.entries.get(&token.owner);
    let operation = entry.and_then(|e| e.operation.as_ref());
    let busy = operation
        .is_some_and(|o| matches!(o.phase, Phase::Preparing | Phase::Saving | Phase::Checking));
    let uncertain =
        operation.is_some_and(|o| matches!(o.phase, Phase::Uncertain | Phase::AwaitingRead));
    let ready =
        entry.is_none_or(|e| e.retry_at <= now()) && shared_retry_at(shell, &token) <= now();
    Some(Presentation {
        label: if muted {
            "Unmute channel"
        } else {
            "Mute channel"
        }
        .into(),
        muted,
        busy,
        uncertain,
        can_change: config.loaded && operation.is_none() && ready,
        can_check: uncertain && ready,
        can_temporary: config.loaded && !permanent && operation.is_none() && ready,
        can_mute_until_tomorrow: config.loaded
            && !permanent
            && operation.is_none()
            && ready
            && config.temporary_schedule_loaded
            && temporary_expiration(
                api::TemporaryMuteDuration::UntilTomorrow,
                &config.user,
                now(),
            )
            .is_ok(),
        temporary_until: config
            .temporary_mute_expiration(channel)
            .filter(|expiry| *expiry > now()),
        temporary_reason: if permanent {
            Some("Unmute this channel before choosing a temporary mute.".into())
        } else if !config.temporary_schedule_loaded {
            Some("Refresh the workspace to load the schedule for Until tomorrow.".into())
        } else {
            None
        },
        message: if !config.loaded {
            Some("Refresh the workspace to load notification settings.".into())
        } else if !ready {
            Some("Slack is limiting requests. Check again after the retry delay.".into())
        } else {
            entry.and_then(|entry| {
                let stale_success = entry.operation.is_none()
                    && entry.settled.is_some_and(|(saved, was_muted)| {
                        saved.muted != permanent
                            || saved.temp_mute_expiration_ts
                                != config.temporary_mute_expiration(channel)
                            || was_muted != muted
                    });
                (!stale_success).then(|| entry.message.clone()).flatten()
            })
        },
        token,
    })
}
pub(crate) fn change(mut state: Signal<ShellState>, token: Token) {
    let Some(attempt) = begin(&mut state.write(), token, now()) else {
        return;
    };
    if fixture::dispatch(state, attempt.clone()) {
        return;
    }
    dioxus::core::spawn_forever(network::run(state, attempt));
}
fn temporary_expiration(
    duration: api::TemporaryMuteDuration,
    prefs: &BTreeMap<String, Value>,
    now: i64,
) -> Result<i64, Error> {
    if let Some((clock, offset)) = fixture::temporary_clock() {
        duration.expires_at_fixed_offset(clock, offset, prefs)
    } else {
        duration.expires_at_local(now, prefs)
    }
}
pub(crate) fn temporarily_mute(
    mut state: Signal<ShellState>,
    token: Token,
    duration: api::TemporaryMuteDuration,
) {
    let attempt = begin_temporary(&mut state.write(), token, duration, now());
    let Some(attempt) = attempt else {
        return;
    };
    if fixture::dispatch(state, attempt.clone()) {
        return;
    }
    dioxus::core::spawn_forever(network::run(state, attempt));
}
fn begin_temporary(
    shell: &mut ShellState,
    token: Token,
    duration: api::TemporaryMuteDuration,
    now: i64,
) -> Option<Attempt> {
    if !token.current(shell) {
        return None;
    }
    let config = &shell.core.workspaces[&token.owner.team].notifications;
    if config
        .channels
        .get(&token.owner.channel)
        .and_then(|p| p.get("muted"))
        .and_then(Value::as_bool)
        == Some(true)
        || (duration == api::TemporaryMuteDuration::UntilTomorrow
            && !config.temporary_schedule_loaded)
    {
        return None;
    }
    let expiry = temporary_expiration(duration, &config.user, now).ok()?;
    let schedule =
        (duration == api::TemporaryMuteDuration::UntilTomorrow).then(|| config.user.clone());
    begin_change(
        shell,
        token,
        Some(api::ChannelMuteChange::TemporaryUntil(expiry)),
        schedule,
        now,
    )
}
pub(crate) fn check(mut state: Signal<ShellState>, token: Token) {
    let Some(read) = begin_check(&mut state.write(), token, now()) else {
        return;
    };
    if fixture::read(state, read.clone()) {
        return;
    }
    dioxus::core::spawn_forever(network::read(state, read));
}
fn begin(shell: &mut ShellState, token: Token, now: i64) -> Option<Attempt> {
    begin_change(shell, token, None, None, now)
}
fn begin_change(
    shell: &mut ShellState,
    token: Token,
    change: Option<api::ChannelMuteChange>,
    schedule: Option<BTreeMap<String, Value>>,
    now: i64,
) -> Option<Attempt> {
    if token.operation.is_some()
        || !token.current(shell)
        || !eligible(shell, &token.owner.channel)
        || shared_retry_at(shell, &token) > now
    {
        return None;
    }
    let config = &shell.core.workspaces[&token.owner.team].notifications;
    if !config.loaded {
        return None;
    }
    let change = change.unwrap_or(api::ChannelMuteChange::Permanent(
        !config.muted(&token.owner.channel, now),
    ));
    let desired = !matches!(change, api::ChannelMuteChange::Permanent(false));
    if shell
        .channel_mute
        .entries
        .get(&token.owner)
        .is_some_and(|e| e.operation.is_some() || e.retry_at > now)
    {
        return None;
    }
    let context = context(shell, &token);
    if context.is_none() && !fixture::enabled() && !cfg!(test) {
        return None;
    }
    if shell.channel_mute.entries.len() >= 32
        && !shell.channel_mute.entries.contains_key(&token.owner)
    {
        if let Some(idle) = shell
            .channel_mute
            .entries
            .iter()
            .find(|(_, entry)| entry.operation.is_none() && entry.retry_at <= now)
            .map(|(key, _)| key.clone())
        {
            shell.channel_mute.entries.remove(&idle);
        } else {
            return None;
        }
    }
    let baseline = target(shell, &token.owner);
    let id = next_id();
    shell.channel_mute.entries.insert(
        token.owner.clone(),
        Entry {
            operation: Some(Operation {
                id,
                change,
                phase: Phase::Preparing,
                read_id: None,
            }),
            message: Some("Checking current notification settings…".into()),
            retry_at: 0,
            settled: None,
        },
    );
    Some(Attempt {
        id,
        token,
        desired,
        change,
        context,
        baseline,
        schedule,
    })
}
fn operation_mut<'a>(shell: &'a mut ShellState, attempt: &Attempt) -> Option<&'a mut Entry> {
    shell
        .channel_mute
        .entries
        .get_mut(&attempt.token.owner)
        .filter(|e| e.operation.as_ref().is_some_and(|o| o.id == attempt.id))
}
fn mark_sending(shell: &mut ShellState, attempt: &Attempt) -> bool {
    if !attempt.token.current(shell)
        || !eligible(shell, &attempt.token.owner.channel)
        || shared_retry_at(shell, &attempt.token) > now()
    {
        if let Some(entry) = operation_mut(shell, attempt) {
            if entry
                .operation
                .as_ref()
                .is_some_and(|o| o.phase == Phase::Preparing)
            {
                entry.operation = None;
                entry.message = None;
            }
        }
        return false;
    }
    let Some(entry) = operation_mut(shell, attempt) else {
        return false;
    };
    let operation = entry.operation.as_mut().unwrap();
    if operation.phase != Phase::Preparing {
        return false;
    }
    operation.phase = Phase::Saving;
    entry.message = Some(
        if attempt.desired {
            "Muting channel…"
        } else {
            "Unmuting channel…"
        }
        .into(),
    );
    true
}
fn preflight_failed(shell: &mut ShellState, attempt: &Attempt, error: Option<&Error>, now: i64) {
    if let Some(error) = error {
        remember_cooldown(shell, &attempt.token, error, now);
    }
    let Some(entry) = operation_mut(shell, attempt) else {
        return;
    };
    if entry
        .operation
        .as_ref()
        .is_none_or(|o| o.phase != Phase::Preparing)
    {
        return;
    }
    entry.operation = None;
    entry.retry_at = entry.retry_at.max(error.map_or(0, |e| cooldown(e, now)));
    entry.message = Some("Notification settings could not be checked. Nothing was changed.".into());
}
fn finish_write(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<(), Error>,
    now: i64,
) -> bool {
    if let Err(error) = &result {
        remember_cooldown(shell, &attempt.token, error, now);
    }
    let Some(entry) = operation_mut(shell, attempt) else {
        return false;
    };
    if entry
        .operation
        .as_ref()
        .is_none_or(|o| o.phase != Phase::Saving)
    {
        return false;
    }
    match result {
        Ok(()) => {
            entry.operation.as_mut().unwrap().phase = Phase::AwaitingRead;
            entry.message = Some("Checking the updated notification settings…".into());
        }
        Err(error) => {
            entry.retry_at = entry.retry_at.max(cooldown(&error, now));
            if known_rejection(&error) && entry.retry_at <= now {
                entry.operation = None;
                entry.message = Some(
                    if matches!(error, Error::Offline(_)) {
                        "Reconnect to Slack. Nothing was changed."
                    } else {
                        "Slack did not change this channel's notification settings."
                    }
                    .into(),
                );
                return false;
            }
            entry.operation.as_mut().unwrap().phase = Phase::Uncertain;
            entry.message = Some(
                "The change could not be confirmed. Check status before changing it again.".into(),
            );
        }
    }
    attempt.token.current(shell)
}
fn begin_check(shell: &mut ShellState, token: Token, now: i64) -> Option<Read> {
    if !token.current(shell) || shared_retry_at(shell, &token) > now {
        return None;
    }
    let context = context(shell, &token);
    if context.is_none() && !fixture::enabled() && !cfg!(test) {
        return None;
    }
    let baseline = target(shell, &token.owner);
    let entry = shell.channel_mute.entries.get_mut(&token.owner)?;
    let operation = entry.operation.as_mut()?;
    if token.operation != Some(operation.id)
        || !matches!(operation.phase, Phase::AwaitingRead | Phase::Uncertain)
        || entry.retry_at > now
    {
        return None;
    }
    let id = next_id();
    operation.phase = Phase::Checking;
    operation.read_id = Some(id);
    entry.message = Some("Checking notification settings…".into());
    Some(Read {
        id,
        operation: operation.id,
        token,
        context,
        baseline,
    })
}
fn finish_read(
    shell: &mut ShellState,
    read: &Read,
    result: Result<api::NotificationPreferences, Error>,
    now: i64,
) -> bool {
    if let Err(error) = &result {
        remember_cooldown(shell, &read.token, error, now);
    }
    let current = read.token.current(shell);
    let same = target(shell, &read.token.owner) == read.baseline;
    let Some(entry) = shell.channel_mute.entries.get_mut(&read.token.owner) else {
        return false;
    };
    let Some(operation) = entry
        .operation
        .as_mut()
        .filter(|o| o.id == read.operation && o.read_id == Some(read.id))
    else {
        return false;
    };
    operation.read_id = None;
    operation.phase = Phase::Uncertain;
    if !current {
        entry.message = Some("Return to this channel and check the change's status.".into());
        return false;
    }
    let prefs = match result {
        Ok(prefs) => prefs,
        Err(error) => {
            entry.retry_at = entry.retry_at.max(cooldown(&error, now));
            entry.message =
                Some("Notification settings could not be checked. No change was retried.".into());
            return false;
        }
    };
    let Ok(canonical) = prefs.channel_state(&read.token.owner.channel) else {
        entry.message =
            Some("Slack returned incomplete notification settings. Check status again.".into());
        return false;
    };
    if !same {
        entry.message =
            Some("Notification settings changed during the check. Check status again.".into());
        return false;
    }
    let change = operation.change;
    let settled = change.confirms(canonical, now);
    if settled {
        entry.operation = None;
        entry.settled = Some((canonical, canonical.effective_muted(now)));
        entry.message = Some(
            if canonical.effective_muted(now) {
                if matches!(change, api::ChannelMuteChange::TemporaryUntil(_)) {
                    "Temporary mute saved."
                } else {
                    "Channel muted."
                }
            } else {
                "Channel unmuted."
            }
            .into(),
        );
    } else {
        entry.message=Some("Slack has not confirmed the requested mute state. Check status again; the change will not be retried automatically.".into());
    }
    adopt(shell, &read.token.owner, &prefs, now);
    settled
}
fn adopt(shell: &mut ShellState, owner: &Owner, prefs: &api::NotificationPreferences, now: i64) {
    let Some(workspace) = shell
        .core
        .workspaces
        .get_mut(&owner.team)
        .filter(|w| w.self_user_id == owner.user)
    else {
        return;
    };
    let canonical = prefs
        .raw()
        .get("channels")
        .and_then(|channels| channels.get(&owner.channel));
    let target = workspace
        .notifications
        .channels
        .entry(owner.channel.clone())
        .or_default();
    for key in ["muted", "temp_mute_expiration_ts"] {
        if let Some(value) = canonical.and_then(|channel| channel.get(key)) {
            target.insert(key.into(), value.clone());
        } else {
            target.remove(key);
        }
    }
    workspace
        .sidebar
        .sync_notification_mutes(&workspace.notifications, now);
    shell.refresh_from_core();
}

fn shared_retry_at(shell: &ShellState, token: &Token) -> i64 {
    shell
        .channel_mute
        .cooldowns
        .get(&token.rate_owner())
        .copied()
        .unwrap_or_default()
}
fn remember_cooldown(shell: &mut ShellState, token: &Token, error: &Error, now: i64) {
    let until = cooldown(error, now);
    if until > now {
        let saved = shell
            .channel_mute
            .cooldowns
            .entry(token.rate_owner())
            .or_default();
        *saved = (*saved).max(until);
    }
}
fn known_rejection(error: &Error) -> bool {
    matches!(error, Error::Offline(_))
        || matches!(error,Error::Api(code) if matches!(code.as_str(),"invalid_auth"|"not_authed"|"token_revoked"|"account_inactive"|"channel_not_found"|"not_in_channel"|"invalid_arguments"|"missing_scope"|"permission_denied"|"restricted_action"|"is_archived"))
}
pub(crate) fn poll(shell: &mut ShellState) -> bool {
    poll_at(shell, now())
}
fn poll_at(shell: &mut ShellState, now: i64) -> bool {
    let mut projection_changed = false;
    for workspace in shell.core.workspaces.values_mut() {
        projection_changed |= workspace
            .sidebar
            .refresh_notification_mutes_if_due(&workspace.notifications, now);
    }
    if projection_changed {
        shell.refresh_from_core();
    }
    let before = shell.channel_mute.cooldowns.len();
    shell.channel_mute.cooldowns.retain(|_, until| *until > now);
    let mut changed = projection_changed || before != shell.channel_mute.cooldowns.len();
    for entry in shell.channel_mute.entries.values_mut() {
        if entry.retry_at > 0 && entry.retry_at <= now {
            entry.retry_at = 0;
            changed = true;
        }
    }
    changed
}

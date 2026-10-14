//! Explicit, owner-bound changes to Slack's two conversation reading preferences.
use crate::state::{Overlay, SettingsSection, ShellState};
use dioxus::prelude::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use super_platinum_core::{
    slack::{Error, api},
    state::{ReadMode, ReadPreferences},
};
mod model;
use model::Owner;
use model::*;
mod lifecycle;
use lifecycle::*;
pub(crate) mod check;
pub(crate) mod fixture;
mod network;
#[cfg(test)]
mod tests;
pub(crate) mod view;

#[derive(Clone, Default)]
pub(crate) struct State {
    view: Option<View>,
    entries: BTreeMap<Owner, Entry>,
    fixture: Option<fixture::Fixture>,
}
/// Shell reloads replace account data, but an outstanding write belongs to the
/// application lifetime. Move only receipts; never transfer a pane or its prefs.
pub(crate) fn carry_pending(previous: &mut ShellState, replacement: &mut ShellState) {
    crate::read_shortcuts::bulk::carry_pending(previous, replacement);
    crate::read_shortcuts::carry_pending(previous, replacement);
    for (owner, entry) in std::mem::take(&mut previous.read_settings.entries) {
        if entry
            .pending
            .as_ref()
            .is_some_and(|p| p.phase != Phase::Preparing)
        {
            replacement
                .read_settings
                .entries
                .entry(owner)
                .or_insert(entry);
        }
    }
    crate::channel_mute::carry_prefs_cooldowns(previous, replacement);
}
pub(crate) fn next_id() -> u64 {
    static ID: AtomicU64 = AtomicU64::new(1);
    ID.fetch_add(1, Ordering::Relaxed)
}
pub(crate) fn mount_key(s: &ShellState) -> String {
    Token::capture(s)
        .map(|t| {
            format!(
                "{}:{}:{}:{}:{}:{}",
                t.epoch, t.owner.account, t.owner.team, t.owner.user, t.credential, t.transport
            )
        })
        .unwrap_or_default()
}
fn visible(s: &ShellState) -> bool {
    s.overlay == Some(Overlay::Settings) && s.settings_section == SettingsSection::Reading
}
fn current(s: &ShellState, t: &Token, visit: u64) -> bool {
    visible(s)
        && t.current(s)
        && s.read_settings
            .view
            .as_ref()
            .is_some_and(|v| v.visit == visit && v.token == *t)
}
fn cooling(s: &ShellState) -> bool {
    crate::channel_mute::prefs_retry_at(s) > super_platinum_core::state::now_secs()
}
fn remember(s: &mut ShellState, t: &Token, e: &Error) {
    crate::channel_mute::remember_owned_prefs_cooldown(s, &t.rate_owner, e);
}
pub(crate) struct Presentation {
    pub mode: Option<ReadMode>,
    pub busy: bool,
    pub can_change: bool,
    pub can_read: bool,
    pub uncertain: bool,
    pub message: Option<String>,
}
pub(crate) fn presentation(s: &ShellState, visit: u64) -> Option<Presentation> {
    let v = s
        .read_settings
        .view
        .as_ref()
        .filter(|v| current(s, &v.token, visit))?;
    let e = s.read_settings.entries.get(&v.token.owner);
    let pending = e.and_then(|e| e.pending.as_ref());
    let busy = v.loading || pending.is_some_and(|p| p.phase != Phase::Uncertain);
    Some(Presentation {
        mode: v.snapshot.as_ref().and_then(|p| p.mode().ok()),
        busy,
        can_change: !busy
            && !cooling(s)
            && pending.is_none()
            && v.snapshot.is_some()
            && observed(s, &v.token) == v.snapshot,
        can_read: !busy && !cooling(s),
        uncertain: pending.is_some(),
        message: if cooling(s) {
            Some("Slack asked us to wait before checking reading preferences again.".into())
        } else if v.snapshot.is_some() && observed(s, &v.token) != v.snapshot {
            Some("Reading preferences changed in Slack. Reload to review them.".into())
        } else {
            v.error
                .clone()
                .or_else(|| e.and_then(|e| e.message.clone()))
        },
    })
}
pub(crate) fn open(mut state: Signal<ShellState>, visit: u64) {
    let r = { open_shell(&mut state.write(), visit) };
    if let Some(r) = r {
        dispatch_read(state, r);
    }
}
fn open_shell(s: &mut ShellState, visit: u64) -> Option<Read> {
    let token = Token::capture(s)?;
    if !visible(s) {
        return None;
    }
    // Retain unresolved receipts across closing the pane or switching accounts.
    if s.read_settings.entries.len() >= 16 && !s.read_settings.entries.contains_key(&token.owner) {
        s.read_settings.entries.retain(|_, e| e.pending.is_some());
        if s.read_settings.entries.len() >= 16 {
            return None;
        }
    }
    s.read_settings.view = Some(View {
        visit,
        token: token.clone(),
        snapshot: None,
        loading: false,
        error: None,
        read: 0,
    });
    if s.read_settings
        .entries
        .get(&token.owner)
        .is_some_and(|e| e.pending.is_some())
        || cooling(s)
    {
        return None;
    }
    begin_read(s, visit)
}
pub(crate) fn close(mut state: Signal<ShellState>, visit: u64) {
    close_shell(&mut state.write(), visit);
}
fn close_shell(s: &mut ShellState, visit: u64) {
    let Some(v) = s.read_settings.view.as_ref().filter(|v| v.visit == visit) else {
        return;
    };
    if let Some(e) = s.read_settings.entries.get_mut(&v.token.owner) {
        if e.pending
            .as_ref()
            .is_some_and(|a| a.phase == Phase::Preparing)
        {
            e.pending = None;
            e.message = Some("Nothing was changed.".into());
        }
    }
    s.read_settings.view = None;
}
pub(crate) fn reload(mut state: Signal<ShellState>, visit: u64) {
    let r = { begin_read(&mut state.write(), visit) };
    if let Some(r) = r {
        dispatch_read(state, r);
    }
}
fn dispatch_read(state: Signal<ShellState>, r: Read) {
    if fixture::read(state, r.clone()) {
        return;
    }
    dioxus::core::spawn_forever(network::read(state, r));
}
pub(crate) fn change(mut state: Signal<ShellState>, visit: u64, mode: ReadMode) {
    let a = { begin(&mut state.write(), visit, mode) };
    if let Some(a) = a {
        if fixture::dispatch(state, a.clone()) {
            return;
        }
        dioxus::core::spawn_forever(network::save(state, a));
    }
}
fn begin(s: &mut ShellState, visit: u64, mode: ReadMode) -> Option<Attempt> {
    if presentation(s, visit).is_none_or(|v| !v.can_change) {
        return None;
    }
    let v = s.read_settings.view.as_ref()?;
    let token = v.token.clone();
    let snapshot = v.snapshot.as_ref()?;
    if snapshot.mode().ok() == Some(mode) {
        return None;
    }
    let session = session(s, &token)?;
    let intent = api::ReadPreferenceIntent::capture(&session, snapshot, mode).ok()?;
    let a = Attempt {
        id: next_id(),
        visit,
        token: token.clone(),
        intent: Arc::new(intent),
        observed: observed(s, &token),
        phase: Phase::Preparing,
        read: None,
        context: context(s, &token),
    };
    s.read_settings.entries.insert(
        token.owner,
        Entry {
            pending: Some(a.clone()),
            message: Some("Checking current reading preferences…".into()),
        },
    );
    Some(a)
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    if let Some(visit) = s
        .read_settings
        .view
        .as_ref()
        .filter(|v| !current(s, &v.token, v.visit))
        .map(|v| v.visit)
    {
        close_shell(s, visit);
        return true;
    }
    false
}

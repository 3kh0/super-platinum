//! Recurring wall-clock schedule: one explicit selection, one guarded mutation.
use crate::state::{Overlay, SettingsSection, ShellState};
use dioxus::prelude::*;
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
use model::*;
mod lifecycle;
use lifecycle::*;
pub(crate) mod check;
pub(crate) mod fixture;
mod network;
#[cfg(test)]
mod tests;
pub(crate) use api::{
    ScheduleAction as Action, ScheduleBound as Bound, ScheduleDay as Day, ScheduleMode as Mode,
};
#[derive(Clone, Default)]
pub(crate) struct State {
    view: Option<View>,
    entries: BTreeMap<Owner, Entry>,
    pub(crate) fixture: Option<fixture::Fixture>,
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
                s.core.account_epoch,
                t.owner.account,
                t.owner.team,
                t.owner.user,
                t.credential,
                t.transport
            )
        })
        .unwrap_or_default()
}
fn now() -> i64 {
    super_platinum_core::state::now_secs()
}
fn visible(s: &ShellState) -> bool {
    s.overlay == Some(Overlay::Settings) && s.settings_section == SettingsSection::Notifications
}
fn current(s: &ShellState, t: &Token, visit: u64) -> bool {
    visible(s)
        && t.current(s)
        && s.notification_schedule
            .view
            .as_ref()
            .is_some_and(|v| v.visit == visit && v.token == *t)
}
fn cooling(s: &ShellState) -> bool {
    crate::channel_mute::prefs_retry_at(s) > now()
}
fn remember(s: &mut ShellState, t: &Token, e: &Error) {
    crate::channel_mute::remember_owned_prefs_cooldown(s, &t.rate_owner, e);
}
pub(crate) struct Presentation {
    pub snapshot: Option<api::NotificationSchedule>,
    pub busy: bool,
    pub can_change: bool,
    pub can_read: bool,
    pub uncertain: bool,
    pub message: Option<String>,
}
pub(crate) fn presentation(s: &ShellState, visit: u64) -> Option<Presentation> {
    let v = s
        .notification_schedule
        .view
        .as_ref()
        .filter(|v| current(s, &v.token, visit))?;
    let entry = s.notification_schedule.entries.get(&v.token.owner);
    let pending = entry.and_then(|e| e.pending.as_ref());
    let busy = v.loading || pending.is_some_and(|a| a.phase != Phase::Uncertain);
    Some(Presentation {
        snapshot: v.snapshot.clone(),
        busy,
        can_change: !busy && !cooling(s) && pending.is_none() && v.snapshot.is_some(),
        can_read: !busy && !cooling(s),
        uncertain: pending.is_some(),
        message: if cooling(s) {
            Some("Slack asked us to wait before checking notification preferences again.".into())
        } else {
            v.error
                .clone()
                .or_else(|| entry.and_then(|e| e.message.clone()))
        },
    })
}
pub(crate) fn open(mut state: Signal<ShellState>, visit: u64) {
    let read = { open_shell(&mut state.write(), visit) };
    if let Some(read) = read {
        dispatch_read(state, read);
    }
}
fn open_shell(s: &mut ShellState, visit: u64) -> Option<Read> {
    let token = Token::capture(s)?;
    if !visible(s) {
        return None;
    }
    if s.notification_schedule.entries.len() >= 16
        && !s.notification_schedule.entries.contains_key(&token.owner)
    {
        s.notification_schedule
            .entries
            .retain(|_, e| e.pending.is_some());
        if s.notification_schedule.entries.len() >= 16 {
            return None;
        }
    }
    let snapshot = None;
    s.notification_schedule.view = Some(View {
        visit,
        token: token.clone(),
        snapshot,
        loading: false,
        error: None,
        read: 0,
    });
    let pending = s
        .notification_schedule
        .entries
        .get(&token.owner)
        .and_then(|e| e.pending.as_ref());
    if pending
        .is_some_and(|a| matches!(a.phase, Phase::Saving | Phase::Preparing | Phase::Checking))
    {
        return None;
    }
    // Reopening an uncertain receipt does not issue another write; user chooses Check.
    if pending.is_some() || cooling(s) {
        return None;
    }
    begin_read(s, visit)
}
pub(crate) fn close(mut state: Signal<ShellState>, visit: u64) {
    close_shell(&mut state.write(), visit);
}
fn close_shell(s: &mut ShellState, visit: u64) {
    let Some(v) = s
        .notification_schedule
        .view
        .as_ref()
        .filter(|v| v.visit == visit)
    else {
        return;
    };
    let owner = v.token.owner.clone();
    if let Some(e) = s.notification_schedule.entries.get_mut(&owner) {
        if e.pending
            .as_ref()
            .is_some_and(|a| a.phase == Phase::Preparing)
        {
            e.pending = None;
            e.message = Some("Nothing was changed.".into());
        }
    }
    s.notification_schedule.view = None;
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
pub(crate) fn change(mut state: Signal<ShellState>, visit: u64, action: Action) {
    let a = { begin(&mut state.write(), visit, action) };
    if let Some(a) = a {
        if fixture::dispatch(state, a.clone()) {
            return;
        }
        dioxus::core::spawn_forever(network::save(state, a));
    }
}
fn begin(s: &mut ShellState, visit: u64, action: Action) -> Option<Attempt> {
    if presentation(s, visit).is_none_or(|v| !v.can_change) {
        return None;
    }
    let v = s.notification_schedule.view.as_ref()?;
    let token = v.token.clone();
    let snapshot = v.snapshot.as_ref()?;
    let session = session(s, &token)?;
    let intent = match api::NotificationScheduleIntent::capture(&session, snapshot, action) {
        Ok(i) => i,
        Err(_) => {
            s.notification_schedule.view.as_mut()?.error =
                Some("Choose an available schedule option.".into());
            return None;
        }
    };
    if intent.confirms(snapshot) {
        return None;
    }
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
    s.notification_schedule.entries.insert(
        token.owner,
        Entry {
            pending: Some(a.clone()),
            message: Some("Checking current notification schedule…".into()),
        },
    );
    Some(a)
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    let stale = s
        .notification_schedule
        .view
        .as_ref()
        .is_some_and(|v| !current(s, &v.token, v.visit));
    let mut changed = false;
    if stale {
        let visit = s.notification_schedule.view.as_ref().unwrap().visit;
        close_shell(s, visit);
        changed = true;
    }
    // No polling requests; a retained receipt is checked only by explicit user action.
    changed
}

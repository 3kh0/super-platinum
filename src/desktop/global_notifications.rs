//! Global defaults: immediate guarded controls and an explicit, retained keyword draft.
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
mod keywords;
mod network;
pub(crate) use keywords::{discard_keywords, edit_keywords, save_keywords};
#[cfg(test)]
mod tests;
pub(crate) use api::{ChannelNotificationMode as Mode, GlobalNotificationAction as Action};
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
#[cfg(test)]
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
        && s.global_notifications
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
    pub snapshot: Option<api::NotificationPreferences>,
    pub busy: bool,
    pub can_change: bool,
    pub can_read: bool,
    pub uncertain: bool,
    pub message: Option<String>,
    pub linked: bool,
    pub keywords: String,
    pub keyword_seed: u64,
    pub keyword_dirty: bool,
    pub keyword_error: Option<String>,
}
pub(crate) fn presentation(s: &ShellState, visit: u64) -> Option<Presentation> {
    let v = s
        .global_notifications
        .view
        .as_ref()
        .filter(|v| current(s, &v.token, visit))?;
    let entry = s.global_notifications.entries.get(&v.token.owner);
    let pending = entry.and_then(|e| e.pending.as_ref());
    let busy = v.loading || pending.is_some_and(|a| a.phase != Phase::Uncertain);
    Some(Presentation {
        linked: v.linked.unwrap_or(false),
        keywords: entry
            .and_then(|e| e.keywords.as_ref())
            .map(|d| d.text.clone())
            .unwrap_or_default(),
        keyword_seed: entry
            .and_then(|e| e.keywords.as_ref())
            .map_or(0, |d| d.seed),
        keyword_dirty: entry
            .and_then(|e| e.keywords.as_ref())
            .is_some_and(|d| d.dirty),
        keyword_error: entry
            .and_then(|e| e.keywords.as_ref())
            .and_then(|d| api::normalize_notification_keywords(&d.text).err())
            .map(|_| "Use no more than 100 comma-separated keywords.".into()),
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
    if s.global_notifications.entries.len() >= 16
        && !s.global_notifications.entries.contains_key(&token.owner)
    {
        s.global_notifications
            .entries
            .retain(|_, e| e.pending.is_some() || e.keywords.as_ref().is_some_and(|d| d.dirty));
        if s.global_notifications.entries.len() >= 16 {
            return None;
        }
    }
    let snapshot = None;
    s.global_notifications.view = Some(View {
        visit,
        token: token.clone(),
        snapshot,
        loading: false,
        error: None,
        read: 0,
        linked: None,
    });
    let pending = s
        .global_notifications
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
        .global_notifications
        .view
        .as_ref()
        .filter(|v| v.visit == visit)
    else {
        return;
    };
    let owner = v.token.owner.clone();
    if let Some(e) = s.global_notifications.entries.get_mut(&owner) {
        if e.pending
            .as_ref()
            .is_some_and(|a| a.phase == Phase::Preparing)
        {
            e.pending = None;
            e.message = Some("Nothing was changed.".into());
        }
    }
    s.global_notifications.view = None;
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
    let v = s.global_notifications.view.as_ref()?;
    let token = v.token.clone();
    let snapshot = v.snapshot.as_ref()?;
    let session = session(s, &token)?;
    let linked = v.linked?;
    if matches!(&action, Action::DesktopMode { linked_mobile, .. } if *linked_mobile != linked) {
        return None;
    }
    let next_linked = match &action {
        Action::RelinkMobile | Action::MobilePush(false) => Some(true),
        _ => None,
    };
    let keyword_generation = if matches!(&action, Action::Keywords(_)) {
        let draft = s
            .global_notifications
            .entries
            .get(&token.owner)?
            .keywords
            .as_ref()?;
        if snapshot.global_settings().ok()?.keywords != draft.baseline {
            s.global_notifications.view.as_mut()?.error = Some(
                "Keywords changed in Slack. Discard this draft to review them before saving."
                    .into(),
            );
            return None;
        }
        Some(draft.generation)
    } else {
        None
    };
    let intent = match api::GlobalNotificationIntent::capture(&session, snapshot, action) {
        Ok(i) => i,
        Err(_) => {
            s.global_notifications.view.as_mut()?.error =
                Some("Choose an available notification option.".into());
            return None;
        }
    };
    if intent.confirms(snapshot) {
        if let Some(linked) = next_linked {
            s.global_notifications.view.as_mut()?.linked = Some(linked);
        }
        if let Some(generation) = keyword_generation {
            if let Some(draft) = s
                .global_notifications
                .entries
                .get_mut(&token.owner)
                .and_then(|e| e.keywords.as_mut())
                .filter(|d| d.generation == generation)
            {
                draft.dirty = false;
                draft.text = api::notification_keywords_display(&draft.baseline);
                draft.seed = next_id();
            }
        }
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
        linked,
        next_linked,
        keyword_generation,
    };
    let entry = s
        .global_notifications
        .entries
        .entry(token.owner)
        .or_default();
    entry.pending = Some(a.clone());
    entry.message = Some("Checking current notification defaults…".into());
    Some(a)
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    let stale = s
        .global_notifications
        .view
        .as_ref()
        .is_some_and(|v| !current(s, &v.token, v.visit));
    let mut changed = false;
    if stale {
        let visit = s.global_notifications.view.as_ref().unwrap().visit;
        close_shell(s, visit);
        changed = true;
    }
    // No polling requests; a retained receipt is checked only by explicit user action.
    changed
}

/// Showing independent mobile choices is local mounted state, never a write.
pub(crate) fn separate_mobile(mut state: Signal<ShellState>, visit: u64) {
    let mut s = state.write();
    if presentation(&s, visit).is_some_and(|v| {
        v.can_change
            && v.snapshot
                .as_ref()
                .and_then(|p| p.global_settings().ok())
                .is_some_and(|p| p.mobile_enabled)
    }) {
        s.global_notifications.view.as_mut().unwrap().linked = Some(false);
    }
}

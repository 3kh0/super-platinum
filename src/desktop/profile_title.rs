//! Header-section title edits have their own transaction and canonical receipt.
//! Never combines a section update with users.profile.set or resends on readback.
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use super_platinum_core::{
    profile_text,
    slack::{Error, api},
};
mod lifecycle;
mod model;
mod network;
use lifecycle::*;
use model::Owner;
use model::*;
pub(crate) mod check;
pub(crate) mod fixture;
#[cfg(test)]
mod tests;
#[derive(Clone, Default)]
pub(crate) struct State {
    draft: Option<Draft>,
    entries: BTreeMap<Owner, Entry>,
    pub(crate) fixture: Option<fixture::Fixture>,
}
fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
fn now() -> i64 {
    super_platinum_core::state::now_secs()
}
pub(crate) fn blocks_profile_writes(shell: &ShellState) -> bool {
    Owner::capture(shell)
        .and_then(|o| shell.profile_title.entries.get(&o))
        .is_some_and(|e| e.pending.is_some())
}
fn competing(shell: &ShellState) -> bool {
    crate::profile_edit::blocks_status(shell) || crate::self_status::blocks_profile_edit(shell)
}
pub(crate) struct Presentation {
    pub visit: u64,
    pub value: String,
    pub busy: bool,
    pub loading: bool,
    pub uncertain: bool,
    pub allowed: bool,
    pub can_save: bool,
    pub can_check: bool,
    pub can_retire: bool,
    pub can_reload: bool,
    pub message: Option<String>,
    pub reason: Option<String>,
    pub validation: Option<String>,
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let draft = shell
        .profile_title
        .draft
        .as_ref()
        .filter(|d| d.token.current(shell))?;
    let entry = shell.profile_title.entries.get(&draft.token.owner);
    let pending = entry.and_then(|e| e.pending.as_ref());
    let busy = pending.is_some_and(|a| a.phase != Phase::Uncertain);
    let uncertain = pending.is_some_and(|a| matches!(a.phase, Phase::Uncertain | Phase::Checking));
    let capability = draft.snapshot.as_ref().map(|s| s.title_capability());
    let allowed = capability.as_ref().is_some_and(|c| c.allowed());
    let validation = api::normalize_profile_title(&draft.value)
        .err()
        .map(|_| "Use 249 characters or fewer for your title.".into());
    let competing = competing(shell);
    Some(Presentation {
        visit: draft.visit,
        value: draft.value.clone(),
        busy,
        loading: draft.loading,
        uncertain,
        allowed,
        can_save: allowed
            && !draft.loading
            && pending.is_none()
            && !competing
            && validation.is_none()
            && draft.value != draft.baseline
            && entry.is_none_or(|e| e.retry_at <= now()),
        can_check: uncertain && !busy && entry.is_none_or(|e| e.retry_at <= now()),
        can_retire: uncertain && !busy && entry.is_some_and(|e| e.checked),
        can_reload: !draft.loading
            && pending.is_none()
            && draft.snapshot.is_none()
            && entry.is_none_or(|e| e.retry_at <= now()),
        validation,
        reason: capability
            .as_ref()
            .and_then(|c| c.reason())
            .map(str::to_owned),
        message: if competing {
            Some(
                "Finish or check the pending profile or status change before saving your title."
                    .into(),
            )
        } else {
            draft
                .error
                .clone()
                .or_else(|| entry.and_then(|e| e.message.clone()))
        },
    })
}
pub(crate) fn open(mut state: Signal<ShellState>) {
    let started = { open_shell(&mut state.write()) };
    if let Some((token, visit)) = started {
        if fixture::enabled(&state.peek()) {
            fixture::load(&mut state.write(), &token, visit);
        } else {
            dioxus::core::spawn_forever(network::load(state, token, visit));
        }
    }
}
fn open_shell(shell: &mut ShellState) -> Option<(Token, u64)> {
    poll(shell);
    let token = Token::capture(shell)?;
    if shell.profile_title.entries.len() >= 16
        && !shell.profile_title.entries.contains_key(&token.owner)
    {
        shell.show_toast("Check an earlier title update before opening another editor.");
        return None;
    }
    let pending = shell
        .profile_title
        .entries
        .get(&token.owner)
        .and_then(|e| e.pending.as_ref());
    let retained = pending.is_some();
    let observed = token.owner.title(shell);
    let value = pending
        .map(|a| a.intent.desired_title().to_owned())
        .unwrap_or_else(|| observed.clone());
    let cooling = shell
        .profile_title
        .entries
        .get(&token.owner)
        .is_some_and(|e| e.retry_at > now());
    let visit = next_id();
    shell.profile_title.draft = Some(Draft {
        visit,
        token: token.clone(),
        observed: observed.clone(),
        baseline: observed,
        value,
        snapshot: None,
        loading: !retained && !cooling,
        error: cooling.then(|| "Slack asked us to wait before loading again.".into()),
    });
    shell.overlay = Some(Overlay::ProfileTitle);
    shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    (!retained && !cooling).then_some((token, visit))
}
pub(crate) fn close(mut state: Signal<ShellState>, visit: u64) {
    close_visit(&mut state.write(), Some(visit));
}
pub(crate) fn close_shell(shell: &mut ShellState) {
    close_visit(shell, None);
}
fn close_visit(shell: &mut ShellState, visit: Option<u64>) {
    let Some(draft) = shell.profile_title.draft.as_ref() else {
        return;
    };
    if visit.is_some_and(|v| v != draft.visit) {
        return;
    }
    let owner = draft.token.owner.clone();
    if let Some(entry) = shell.profile_title.entries.get_mut(&owner) {
        if entry
            .pending
            .as_ref()
            .is_some_and(|a| matches!(a.phase, Phase::Saving | Phase::Checking))
        {
            return;
        }
        if entry
            .pending
            .as_ref()
            .is_some_and(|a| a.phase == Phase::Preparing)
        {
            entry.pending = None;
        }
    }
    shell.profile_title.draft = None;
    if shell.overlay == Some(Overlay::ProfileTitle) {
        shell.overlay = None;
    }
}
pub(crate) fn change(mut state: Signal<ShellState>, visit: u64, value: String) {
    let mut shell = state.write();
    if presentation(&shell)
        .is_none_or(|v| v.visit != visit || v.busy || v.loading || v.uncertain || !v.allowed)
    {
        return;
    }
    let draft = shell.profile_title.draft.as_mut().unwrap();
    draft.value = profile_text::normalize(&value);
    draft.error = None;
}
pub(crate) fn save(mut state: Signal<ShellState>, visit: u64) {
    let started = { begin(&mut state.write(), visit) };
    if let Some(attempt) = started {
        if fixture::enabled(&state.peek()) {
            fixture::save(&mut state.write(), attempt);
        } else {
            dioxus::core::spawn_forever(network::save(state, attempt));
        }
    }
}
pub(crate) fn check_status(mut state: Signal<ShellState>, visit: u64) {
    let started = { begin_check(&mut state.write(), visit) };
    if let Some((token, id, read)) = started {
        if fixture::enabled(&state.peek()) {
            fixture::check(&mut state.write(), token, id, read);
        } else {
            dioxus::core::spawn_forever(network::check(state, token, id, read));
        }
    }
}
pub(crate) fn reload(mut state: Signal<ShellState>, visit: u64) {
    let Some(view) = presentation(&state.peek()).filter(|v| v.visit == visit && v.can_reload)
    else {
        return;
    };
    let _ = view;
    let token = {
        let mut shell = state.write();
        let d = shell.profile_title.draft.as_mut().unwrap();
        d.loading = true;
        d.token.clone()
    };
    if fixture::enabled(&state.peek()) {
        fixture::load(&mut state.write(), &token, visit);
    } else {
        dioxus::core::spawn_forever(network::load(state, token, visit));
    }
}
pub(crate) fn keep_current(mut state: Signal<ShellState>, visit: u64) {
    let mut shell = state.write();
    if presentation(&shell).is_none_or(|v| v.visit != visit || !v.can_retire) {
        return;
    }
    let owner = shell
        .profile_title
        .draft
        .as_ref()
        .unwrap()
        .token
        .owner
        .clone();
    shell.profile_title.entries.get_mut(&owner).unwrap().pending = None;
    close_visit(&mut shell, Some(visit));
}
pub(crate) fn poll(shell: &mut ShellState) -> bool {
    let stale = shell
        .profile_title
        .draft
        .as_ref()
        .is_some_and(|d| !d.token.current(shell));
    if stale {
        if let Some(draft) = shell.profile_title.draft.take() {
            if let Some(entry) = shell.profile_title.entries.get_mut(&draft.token.owner) {
                if entry
                    .pending
                    .as_ref()
                    .is_some_and(|a| a.phase == Phase::Preparing)
                {
                    entry.pending = None;
                }
                if let Some(a) = entry
                    .pending
                    .as_mut()
                    .filter(|a| a.phase == Phase::Checking)
                {
                    a.phase = Phase::Uncertain;
                    a.read_id = None;
                }
            }
        }
        if shell.overlay == Some(Overlay::ProfileTitle) {
            shell.overlay = None;
        }
    }
    let mut changed = stale;
    let visible = shell
        .profile_title
        .draft
        .as_ref()
        .map(|d| d.token.owner.clone());
    let before = shell.profile_title.entries.len();
    for entry in shell.profile_title.entries.values_mut() {
        if entry.retry_at > 0 && entry.retry_at <= now() {
            entry.retry_at = 0;
            changed = true;
        }
    }
    shell
        .profile_title
        .entries
        .retain(|o, e| Some(o) == visible.as_ref() || e.pending.is_some() || e.retry_at > now());
    changed || before != shell.profile_title.entries.len()
}

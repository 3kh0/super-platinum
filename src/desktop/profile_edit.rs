//! Own-profile drafts and one-shot text updates. Status and unknown fields never
//! enter the write payload; uncertain writes retain an owner-bound receipt.
use crate::state::{Overlay, ShellState};
use dioxus::prelude::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use super_platinum_core::{
    profile_text::{self, Field},
    slack::{Error, api, models::UserProfile},
};
mod lifecycle;
mod model;
mod network;
pub(crate) mod photo;
pub(crate) mod recording;
pub(crate) mod sections;
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
    photo: photo::State,
    sections: sections::State,
    recording: recording::State,
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
pub(crate) struct FieldView {
    pub field: Field,
    pub value: String,
    pub allowed: bool,
    pub reason: Option<String>,
    pub error: Option<String>,
}
pub(crate) struct Presentation {
    pub visit: u64,
    pub fields: Vec<FieldView>,
    pub loading: bool,
    pub busy: bool,
    pub uncertain: bool,
    pub can_save: bool,
    pub can_reload: bool,
    pub can_check: bool,
    pub can_retire: bool,
    pub error: Option<String>,
    pub status_conflict: bool,
}
pub(crate) fn blocks_status(shell: &ShellState) -> bool {
    sections::pending(shell)
        || recording::pending(shell)
        || photo::pending(shell)
        || Owner::capture(shell)
            .and_then(|o| shell.profile_edit.entries.get(&o))
            .is_some_and(|e| e.pending.is_some())
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let draft = shell
        .profile_edit
        .draft
        .as_ref()
        .filter(|d| d.token.current(shell))?;
    let entry = shell.profile_edit.entries.get(&draft.token.owner);
    let pending = entry.and_then(|e| e.pending.as_ref());
    let busy = pending.is_some_and(|a| a.phase != Phase::Uncertain);
    let uncertain = pending.is_some_and(|a| matches!(a.phase, Phase::Uncertain | Phase::Checking));
    let status_conflict = recording::active(shell)
        || recording::pending(shell)
        || sections::active(shell)
        || sections::pending(shell)
        || recording::pending(shell)
        || photo::pending(shell)
        || crate::self_status::blocks_profile_edit(shell)
        || crate::profile_title::blocks_profile_writes(shell);
    let fields = FIELDS
        .into_iter()
        .map(|field| {
            let capability = draft.capabilities.as_ref().map(|c| c.capability(field));
            FieldView {
                field,
                value: draft.values.get(field).into(),
                allowed: capability.is_some_and(|c| c.allowed()),
                reason: capability.and_then(|c| c.reason()).map(str::to_owned),
                error: capability
                    .filter(|c| c.allowed())
                    .and_then(|_| profile_text::validate(field, draft.values.get(field)).err())
                    .map(|e| validation(e).into()),
            }
        })
        .collect::<Vec<_>>();
    let can_save = !draft.loading
        && pending.is_none()
        && !status_conflict
        && entry.is_none_or(|e| e.retry_at <= now())
        && fields.iter().all(|f| f.error.is_none())
        && draft.values != draft.baseline
        && fields
            .iter()
            .filter(|f| f.value != draft.baseline.get(f.field))
            .all(|f| f.allowed);
    Some(Presentation {
        visit: draft.visit,
        fields,
        loading: draft.loading,
        busy,
        uncertain,
        can_save,
        can_reload: draft.capabilities.is_none() && !draft.loading && !uncertain,
        can_check: uncertain && !busy && entry.is_none_or(|e| e.retry_at <= now()),
        can_retire: uncertain && !busy && entry.is_some_and(|e| e.checked),
        status_conflict,
        error: draft
            .error
            .clone()
            .or_else(|| entry.and_then(|e| e.message.clone())),
    })
}
pub(crate) fn open(mut state: Signal<ShellState>) {
    let load = open_shell(&mut state.write());
    if let Some((token, visit)) = load {
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
    if shell.profile_edit.entries.len() >= 16
        && !shell.profile_edit.entries.contains_key(&token.owner)
    {
        shell.show_toast("Check an earlier profile change before opening another profile editor.");
        return None;
    }
    let observed = token.owner.values(shell);
    let retained = shell
        .profile_edit
        .entries
        .get(&token.owner)
        .and_then(|e| e.pending.clone());
    let cooling = shell
        .profile_edit
        .entries
        .get(&token.owner)
        .is_some_and(|e| e.retry_at > now());
    let visit = next_id();
    let values = retained
        .as_ref()
        .map(|a| a.desired.clone())
        .unwrap_or_else(|| observed.clone());
    let baseline = retained
        .as_ref()
        .map(|a| a.baseline.clone())
        .unwrap_or_else(|| observed.clone());
    shell.profile_edit.draft = Some(Draft {
        token: token.clone(),
        visit,
        observed,
        baseline,
        values,
        capabilities: None,
        loading: retained.is_none() && !cooling,
        error: cooling.then(|| "Slack asked us to wait before loading again.".into()),
    });
    shell.overlay = Some(Overlay::ProfileEdit);
    shell.shortcut_generation = shell.shortcut_generation.wrapping_add(1);
    (retained.is_none() && !cooling).then_some((token, visit))
}
pub(crate) fn close(mut state: Signal<ShellState>, visit: u64) {
    close_visit(&mut state.write(), Some(visit));
}
pub(crate) fn close_shell(shell: &mut ShellState) {
    close_visit(shell, None);
}
fn close_visit(shell: &mut ShellState, visit: Option<u64>) {
    if recording::intercept_close(shell, visit)
        || sections::intercept_close(shell, visit)
        || photo::intercept_close(shell, visit)
    {
        return;
    }
    let Some(draft) = shell.profile_edit.draft.as_ref() else {
        return;
    };
    if visit.is_some_and(|v| v != draft.visit) {
        return;
    }
    let owner = draft.token.owner.clone();
    if shell
        .profile_edit
        .entries
        .get(&owner)
        .and_then(|e| e.pending.as_ref())
        .is_some_and(|a| matches!(a.phase, Phase::Saving | Phase::Checking))
    {
        return;
    }
    if let Some(entry) = shell.profile_edit.entries.get_mut(&owner) {
        if entry
            .pending
            .as_ref()
            .is_some_and(|a| a.phase == Phase::Preparing)
        {
            entry.pending = None;
        }
    }
    shell.profile_edit.draft = None;
    if shell.overlay == Some(Overlay::ProfileEdit) {
        shell.overlay = None;
    }
}
pub(crate) fn change(mut state: Signal<ShellState>, visit: u64, field: Field, value: String) {
    let mut shell = state.write();
    let Some(view) =
        presentation(&shell).filter(|v| v.visit == visit && !v.loading && !v.busy && !v.uncertain)
    else {
        return;
    };
    if !view.fields.iter().any(|f| f.field == field && f.allowed) {
        return;
    }
    if let Some(draft) = shell.profile_edit.draft.as_mut() {
        draft.values.0[index(field)] = profile_text::normalize(&value);
        draft.error = None;
    }
}
pub(crate) fn save(mut state: Signal<ShellState>, visit: u64) {
    let attempt = begin(&mut state.write(), visit);
    if let Some(attempt) = attempt {
        if fixture::enabled(&state.peek()) {
            fixture::save(&mut state.write(), attempt);
        } else {
            dioxus::core::spawn_forever(network::save(state, attempt));
        }
    }
}
pub(crate) fn check_status(mut state: Signal<ShellState>, visit: u64) {
    let check = begin_check(&mut state.write(), visit);
    if let Some((token, id, read)) = check {
        if fixture::enabled(&state.peek()) {
            fixture::check(&mut state.write(), token, id, read);
        } else {
            dioxus::core::spawn_forever(network::check(state, token, id, read));
        }
    }
}
pub(crate) fn reload(mut state: Signal<ShellState>, visit: u64) {
    let Some(draft) = state
        .peek()
        .profile_edit
        .draft
        .clone()
        .filter(|d| d.visit == visit && d.token.current(&state.peek()))
    else {
        return;
    };
    if blocks_status(&state.peek()) || draft.loading {
        return;
    }
    if state
        .peek()
        .profile_edit
        .entries
        .get(&draft.token.owner)
        .is_some_and(|e| e.retry_at > now())
    {
        return;
    }
    state.write().profile_edit.draft.as_mut().unwrap().loading = true;
    if fixture::enabled(&state.peek()) {
        fixture::load(&mut state.write(), &draft.token, visit);
    } else {
        dioxus::core::spawn_forever(network::load(state, draft.token, visit));
    }
}
pub(crate) fn poll(shell: &mut ShellState) -> bool {
    let photo_changed = recording::poll(shell) | sections::poll(shell) | photo::poll(shell);
    let stale = shell
        .profile_edit
        .draft
        .as_ref()
        .is_some_and(|d| !d.token.current(shell));
    if stale {
        if let Some(draft) = shell.profile_edit.draft.take() {
            if let Some(entry) = shell.profile_edit.entries.get_mut(&draft.token.owner) {
                if entry
                    .pending
                    .as_ref()
                    .is_some_and(|a| a.phase == Phase::Preparing)
                {
                    entry.pending = None;
                }
                if let Some(pending) = entry
                    .pending
                    .as_mut()
                    .filter(|a| a.phase == Phase::Checking)
                {
                    pending.phase = Phase::Uncertain;
                    pending.read_id = None;
                }
            }
        }
        if shell.overlay == Some(Overlay::ProfileEdit) {
            shell.overlay = None;
        }
    }
    let mut expired = false;
    for entry in shell.profile_edit.entries.values_mut() {
        if entry.retry_at > 0 && entry.retry_at <= now() {
            entry.retry_at = 0;
            expired = true;
        }
    }
    let before = shell.profile_edit.entries.len();
    let visible = shell
        .profile_edit
        .draft
        .as_ref()
        .map(|d| d.token.owner.clone());
    shell.profile_edit.entries.retain(|owner, entry| {
        Some(owner) == visible.as_ref() || entry.pending.is_some() || entry.retry_at > now()
    });
    photo_changed || stale || expired || before != shell.profile_edit.entries.len()
}

pub(crate) fn keep_current(mut state: Signal<ShellState>, visit: u64) {
    let mut shell = state.write();
    let Some(view) = presentation(&shell).filter(|v| v.visit == visit && v.can_retire) else {
        return;
    };
    let _ = view;
    let owner = shell
        .profile_edit
        .draft
        .as_ref()
        .unwrap()
        .token
        .owner
        .clone();
    shell.profile_edit.entries.get_mut(&owner).unwrap().pending = None;
    close_visit(&mut shell, Some(visit));
}

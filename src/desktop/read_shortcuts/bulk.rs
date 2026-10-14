//! One workspace-wide clear/Undo operation, independent of loaded message bags.
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, SlackClient, Transport, api, client::PreparedRequest},
    state::ClearUnreadsPreference,
};
mod model;
use model::Owner;
use model::*;
mod lifecycle;
use lifecycle::*;
mod network;
mod preference;
mod ui;
pub(crate) use ui::view;
pub(crate) mod check;
pub(crate) mod fixture;
pub(crate) mod reconcile;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct State {
    entries: BTreeMap<Owner, Entry>,
    dialog: Option<Dialog>,
    individuals: BTreeMap<u64, (Option<String>, String, String)>,
    pub(crate) fixture: Option<fixture::Fixture>,
}
fn next_id() -> u64 {
    static ID: AtomicU64 = AtomicU64::new(1);
    ID.fetch_add(1, Ordering::Relaxed)
}
pub(crate) fn request(mut state: Signal<ShellState>) {
    let next = { begin(&mut state.write()) };
    if let Some(next) = next {
        dispatch(state, next);
    }
}
fn dispatch(state: Signal<ShellState>, next: Next) {
    match next {
        Next::Preference(read) => {
            if !fixture::read(state, read.clone()) {
                dioxus::core::spawn_forever(network::preference_read(state, read));
            }
        }
        Next::Bulk(work) => {
            if let Some(work) = fixture::write(state, work) {
                dioxus::core::spawn_forever(network::bulk(state, work));
            }
        }
    }
}
fn begin(s: &mut ShellState) -> Option<Next> {
    if s.bulk_read.dialog.is_some() {
        return None;
    }
    let context = match Context::capture(s) {
        Ok(c) => Arc::new(c),
        Err(_) => {
            s.show_toast("Clear unreads is unavailable for this workspace. Enterprise scope is not supported yet.");
            return None;
        }
    };
    if !context.visit_current(s) || cursor_busy(s, &context.owner.team) {
        return None;
    }
    let entry = s
        .bulk_read
        .entries
        .entry(context.owner.clone())
        .or_default();
    if entry.operation.is_some() || entry.reconciling.is_some() {
        entry.hidden = false;
        return None;
    }
    // A new explicit clear retires the old short-lived Undo, never clones it.
    entry.notice = None;
    entry.message = None;
    entry.hidden = false;
    let preference = observed(s, &context);
    match preference.requires_confirmation() {
        Ok(true) => {
            open_confirmation(s, context, preference);
            None
        }
        Ok(false) if !preference_pending(s, &context.owner) => {
            start_clear(s, context, false).map(Next::Bulk)
        }
        _ => {
            let id = next_id();
            s.bulk_read.dialog = Some(Dialog {
                id,
                context: context.clone(),
                preference: preference.clone(),
                phase: DialogPhase::Loading,
                dont_ask: false,
                error: None,
            });
            Some(Next::Preference(PrefRead {
                id,
                context,
                observed: preference,
            }))
        }
    }
}
fn open_confirmation(
    s: &mut ShellState,
    context: Arc<Context>,
    preference: ClearUnreadsPreference,
) {
    s.bulk_read.dialog = Some(Dialog {
        id: next_id(),
        context,
        preference,
        phase: DialogPhase::Confirm,
        dont_ask: false,
        error: None,
    });
}
pub(crate) fn cancel(mut state: Signal<ShellState>, id: u64) {
    let mut s = state.write();
    if s.bulk_read.dialog.as_ref().is_some_and(|d| d.id == id) {
        s.bulk_read.dialog = None;
    }
}
pub(crate) fn confirm(mut state: Signal<ShellState>, id: u64) {
    let (work, pref) = { confirm_shell(&mut state.write(), id) };
    if let Some(pref) = pref {
        if !fixture::preference_write(state, pref.clone()) {
            dioxus::core::spawn_forever(network::disable_confirmation(state, pref));
        }
    }
    if let Some(work) = work {
        dispatch(state, Next::Bulk(work));
    }
}
fn confirm_shell(s: &mut ShellState, id: u64) -> (Option<Work>, Option<PrefWork>) {
    let Some(d) =
        s.bulk_read.dialog.as_ref().filter(|d| {
            d.id == id && d.phase == DialogPhase::Confirm && d.context.visit_current(s)
        })
    else {
        return (None, None);
    };
    let context = d.context.clone();
    let baseline = d.preference.clone();
    let suppress = d.dont_ask;
    if cursor_busy(s, &context.owner.team) {
        return (None, None);
    }
    if observed(s, &context) != baseline {
        let d = s.bulk_read.dialog.as_mut().unwrap();
        d.phase = DialogPhase::Unavailable;
        d.error = Some("The confirmation preference changed. Check again before clearing.".into());
        return (None, None);
    }
    s.bulk_read.dialog = None;
    let work = start_clear(s, context.clone(), true);
    let pref = work.as_ref().and_then(|_| {
        suppress
            .then(|| preference::begin(s, context, baseline))
            .flatten()
    });
    (work, pref)
}
pub(crate) fn undo(mut state: Signal<ShellState>) {
    let work = { begin_undo(&mut state.write(), Instant::now()) };
    if let Some(work) = work {
        dispatch(state, Next::Bulk(work));
    }
}
pub(crate) fn refresh(mut state: Signal<ShellState>) {
    let context = Context::capture(&state.peek()).ok().map(Arc::new);
    if let Some(context) = context {
        let baseline = Arc::new(reconcile::Baseline::capture(&state.peek(), &context));
        let id = { start_reconcile(&mut state.write(), &context) };
        if let Some(id) = id {
            if !fixture::refresh(state, context.clone(), baseline.clone(), id) {
                dioxus::core::spawn_forever(reconcile::run(state, context, baseline, id));
            }
        }
    }
}
pub(crate) fn dismiss(mut state: Signal<ShellState>) {
    let mut s = state.write();
    if let Some(owner) = Owner::capture(&s) {
        if let Some(e) = s.bulk_read.entries.get_mut(&owner) {
            e.hidden = true;
            e.notice = None;
        }
    }
}
pub(crate) fn blocks_reads(s: &ShellState) -> bool {
    if s.bulk_read.dialog.is_some() {
        return true;
    }
    Owner::capture(s)
        .and_then(|owner| s.bulk_read.entries.get(&owner))
        .is_some_and(|e| {
            e.operation
                .as_ref()
                .is_some_and(|o| o.phase == Phase::Sending)
                || e.reconciling.is_some()
        })
}
pub(crate) fn dialog_open(s: &ShellState) -> bool {
    s.bulk_read.dialog.is_some()
}
pub(crate) fn begin_individual(s: &mut ShellState) -> Option<u64> {
    if blocks_reads(s) {
        return None;
    }
    let team = s.core.active_team.clone()?;
    let user = s.core.workspaces.get(&team)?.self_user_id.clone();
    let id = next_id();
    s.bulk_read
        .individuals
        .insert(id, (s.core.active_account.clone(), team, user));
    Some(id)
}
pub(crate) fn end_individual(s: &mut ShellState, id: u64) {
    s.bulk_read.individuals.remove(&id);
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    let mut changed = false;
    if s.bulk_read
        .dialog
        .as_ref()
        .is_some_and(|d| !d.context.visit_current(s))
    {
        s.bulk_read.dialog = None;
        changed = true;
    }
    let now = Instant::now();
    let stale: Vec<_> = s
        .bulk_read
        .entries
        .iter()
        .filter_map(|(owner, e)| {
            e.notice
                .as_ref()
                .filter(|n| !n.context.current(s))
                .map(|_| owner.clone())
        })
        .collect();
    for owner in stale {
        s.bulk_read.entries.get_mut(&owner).unwrap().notice = None;
        changed = true;
    }
    for entry in s.bulk_read.entries.values_mut() {
        if entry
            .notice
            .as_ref()
            .is_some_and(|n| now.duration_since(n.started) >= n.duration)
        {
            entry.notice = None;
            if entry.operation.is_none() && !entry.refresh_failed {
                entry.message = None;
            }
            changed = true;
        }
    }
    changed
}
pub(crate) fn carry_pending(previous: &mut ShellState, replacement: &mut ShellState) {
    replacement
        .bulk_read
        .individuals
        .extend(std::mem::take(&mut previous.bulk_read.individuals));
    for (owner, mut entry) in std::mem::take(&mut previous.bulk_read.entries) {
        entry.notice = None;
        entry.reconciling = None;
        if entry
            .preference
            .as_ref()
            .is_some_and(|p| p.phase == PrefPhase::Preparing)
        {
            entry.preference = None;
        }
        if entry.operation.is_some() || entry.preference.is_some() {
            let target = replacement.bulk_read.entries.entry(owner).or_default();
            // A fresh shell may already contain an idle presentation entry.
            // That must not silently discard an older outstanding receipt.
            if target.operation.is_none() && entry.operation.is_some() {
                target.operation = entry.operation;
                target.message = entry.message;
                target.hidden = entry.hidden;
                target.acknowledged = entry.acknowledged;
            }
            if target.preference.is_none() && entry.preference.is_some() {
                target.preference = entry.preference;
                target.preference_message = entry.preference_message;
            }
        }
    }
}

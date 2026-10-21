//! Existing-pill toggles retain one issued result independently of visible visits.
use super::fixture::reaction_actions as fixture;
use super::*;
use serde_json::json;
use std::sync::Mutex;
use super_platinum_core::{
    slack::{events::RtEvent, models::Message, transport::ListCommentReactionFailure},
    slack_lists::comments::{
        ReactionFamilyIntent, ReactionIntent, ReactionRecord, ReactionResponse,
        own_reaction_family_names, reaction_family_base,
    },
};
pub(crate) mod addition;
mod family;
mod model;
mod recovery;
mod run;
pub(crate) mod selection;
use model::{
    Attempt, MAX_ACTIONS, Phase, Work, begin, failure_deadline, finish, retire_preflight,
    same_target, settle,
};
pub(super) use recovery::{apply_readback, readback, readback_current};

#[derive(Clone, Default)]
pub(super) struct State(Arc<Mutex<Vec<Attempt>>>);

pub(crate) fn available(shell: &ShellState, generation: u64, ts: &str, name: &str) -> bool {
    if crate::fixture::is_fixture() && !fixture::enabled(shell) {
        return false;
    }
    let Some(entry) = shell.slack_lists.comments.entry.as_ref() else {
        return false;
    };
    if entry.owner.serial != generation
        || !entry.owner.active(shell)
        || entry.loading
        || selection::has_picker(shell)
        || addition::busy(shell)
        || cooldown(shell) > now()
        || draft::presentation(shell).is_some_and(|d| d.busy || d.loading)
        || entry.target.root_ts() == Some(ts)
        || entry.deleted.contains(ts)
        || !entry
            .rows
            .iter()
            .any(|r| r.ts == ts && r.reactions.iter().any(|v| v.name == name))
    {
        return false;
    }
    let Some(row) = message(entry, ts) else {
        return false;
    };
    let user = entry.owner.scope.stable_owner().2;
    let adding = family::needs_addition(row, &user, name);
    let plan = if adding {
        None
    } else {
        let Ok(plan) = family::selection(row, &user, name) else {
            return false;
        };
        plan
    };
    let needed = plan.as_ref().map_or(1, |(_, names)| names.len());
    let Ok(actions) = shell.slack_lists.comments.reaction_actions.0.lock() else {
        return false;
    };
    !actions
        .iter()
        .any(|a| a.belongs(&entry.owner, &entry.target) && a.retry_at > now())
        && !actions.iter().any(|a| {
            a.belongs(&entry.owner, &entry.target)
                && a.blocking()
                && (a.phase == Phase::Preparing
                    || (a.ts == ts
                        && (a.name == name
                            || ((plan.is_some() || adding)
                                && reaction_family_base(name).is_some()
                                && reaction_family_base(name) == reaction_family_base(&a.name))
                            || a.family.as_ref().is_some_and(|f| {
                                reaction_family_base(name) == Some(f.base.as_str())
                            }))))
        })
        && family::room(&actions, needed)
}
pub(crate) fn status(shell: &ShellState, generation: u64) -> Option<String> {
    let entry = shell.slack_lists.comments.entry.as_ref()?;
    if entry.owner.serial != generation || !entry.owner.active(shell) {
        return None;
    }
    if addition::busy(shell) {
        return addition::status(shell, generation);
    }
    let actions = shell.slack_lists.comments.reaction_actions.0.lock().ok()?;
    let matches = |a: &&Attempt| a.belongs(&entry.owner, &entry.target);
    let urgent = actions
        .iter()
        .rev()
        .filter(matches)
        .find(|a| matches!(a.phase, Phase::Preparing | Phase::Queued | Phase::Sent))
        .or_else(|| {
            actions
                .iter()
                .rev()
                .filter(matches)
                .find(|a| a.phase == Phase::Unknown)
        });
    if urgent.is_none() {
        if let Some(notice) = addition::status(shell, generation) {
            return Some(notice);
        }
    }
    urgent
        .or_else(|| {
            actions
                .iter()
                .rev()
                .find(|a| a.belongs(&entry.owner, &entry.target))
        })
        .map(|a| {
            if let Some(f) = &a.family {
                return family::status(&actions, f);
            }
            if matches!(a.phase, Phase::Confirmed | Phase::Desired)
                && (a.socket_observed || a.canonical_observed)
            {
                "Reaction response retained. Newer comment updates are shown.".into()
            } else {
                a.phase.notice().into()
            }
        })
}
pub(crate) fn toggle(state: Signal<ShellState>, generation: u64, ts: String, name: String) {
    if !available(&state.peek(), generation, &ts, &name) {
        return;
    }
    let adding = {
        let shell = state.peek();
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .and_then(|e| {
                message(e, &ts)
                    .map(|m| family::needs_addition(m, &e.owner.scope.stable_owner().2, &name))
            })
            .unwrap_or(false)
    };
    if adding {
        addition::toggle(state, generation, ts, name);
    } else {
        dioxus::core::spawn_forever(run::toggle(state, generation, ts, name));
    }
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let Ok(actions) = shell.slack_lists.comments.reaction_actions.0.lock() else {
        return Value::Null;
    };
    json!({"attempts":actions.iter().map(|a|{let mut row=json!({"id":a.id,"target":a.target,"ts":a.ts,"name":a.name,"user":a.user,"adding":a.adding,
        "phase":a.phase.label(),"raw":a.raw,"socketObserved":a.socket_observed,"socketRevision":a.socket_revision,"canonicalObserved":a.canonical_observed,"readbackResolved":a.readback,"published":a.published,"retryAt":a.retry_at,"fullReadback":a.full_readback});
        if let Some(f)=&a.family {row["parent"]=json!(f.parent);row["family"]=json!(f.base);row["selected"]=json!(f.selected);row["planned"]=json!(f.names);row["issued"]=json!(a.issued);row["failure"]=json!(a.failure);}row}).collect::<Vec<_>>(),
        "pending":actions.iter().filter(|a|a.blocking()).count()})
}
pub(super) fn observe_socket(shell: &ShellState, team: &str, generation: u64, event: &RtEvent) {
    let (channel, ts, user, name) = match event {
        RtEvent::ReactionAdded {
            channel,
            ts,
            user,
            reaction,
        }
        | RtEvent::ReactionRemoved {
            channel,
            ts,
            user,
            reaction,
        } => (channel, ts, user, reaction),
        _ => return,
    };
    let Some(entry) = shell.slack_lists.comments.entry.as_ref() else {
        return;
    };
    if !entry.owner.active(shell)
        || entry.owner.scope.team != team
        || entry.target.channel_id() != channel
        || shell
            .core
            .workspaces
            .get(team)
            .is_none_or(|w| w.rt_generation != generation)
        || !entry.rows.iter().any(|r| r.ts == *ts)
    {
        return;
    }
    if let Ok(mut actions) = shell.slack_lists.comments.reaction_actions.0.lock() {
        family::socket(&actions, &entry.owner, &entry.target, event);
        for a in actions.iter_mut().filter(|a| {
            a.belongs(&entry.owner, &entry.target)
                && !a.published
                && matches!(
                    a.phase,
                    Phase::Sent | Phase::Confirmed | Phase::Desired | Phase::Unknown
                )
                && a.ts == *ts
        }) {
            a.socket_revision = a.socket_revision.saturating_add(1);
            if a.user == *user && a.name == *name {
                a.socket_observed = true;
            }
        }
    }
}
pub(super) fn observe_page(shell: &ShellState, owner: &Owner, page: &Page) {
    if !owner.active(shell) {
        return;
    }
    if let Ok(mut actions) = shell.slack_lists.comments.reaction_actions.0.lock() {
        for a in actions
            .iter_mut()
            .filter(|a| a.belongs(owner, page.target()))
        {
            // A completed canonical replacement is later display evidence. Do
            // not let an earlier issued result repaint it, even when its
            // membership list is insufficient to resolve an uncertain write.
            if !a.published
                && matches!(
                    a.phase,
                    Phase::Sent | Phase::Confirmed | Phase::Desired | Phase::Unknown
                )
                && page.raw()["messages"].as_array().is_some_and(|rows| {
                    rows.iter().any(|m| m["ts"].as_str() == Some(a.ts.as_str()))
                })
            {
                a.canonical_observed = true;
            }
            if a.phase == Phase::Unknown
                && a.record
                    .as_ref()
                    .is_some_and(|r| r.observed(page) == Ok(true))
            {
                a.phase = Phase::Observed;
                a.readback = true;
            }
        }
    }
}
fn message<'a>(entry: &'a Entry, ts: &str) -> Option<&'a Message> {
    entry
        .page
        .as_ref()?
        .messages()
        .iter()
        .chain(&entry.accepted)
        .chain(&entry.live)
        .find(|m| m.ts.as_deref() == Some(ts))
}
fn member(message: &Message, user: &str, name: &str) -> bool {
    message
        .reactions
        .iter()
        .find(|r| r.name == name)
        .is_some_and(|r| r.users.iter().any(|u| u == user))
}
#[cfg(test)]
mod tests;

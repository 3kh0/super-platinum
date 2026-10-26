//! Shared missing-group selector ownership, independent of Profile editor leases.
use crate::{directory_member_fetches::Context, state::ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use super_platinum_core::{
    slack::Error,
    state::directory_usergroups::{
        Snapshot,
        unknown::{self as domain, Flight, Lookup, Manager, Scope},
    },
};
#[cfg(test)]
mod tests;
mod worker;
type Key = (String, String, String);
#[derive(Default)]
pub(crate) struct Registry {
    slots: BTreeMap<Key, Runtime>,
}
struct Runtime {
    owner: Arc<Context>,
    identity: Arc<()>,
    scope: Scope,
    manager: Manager,
    worker: Option<Arc<()>>,
    active: bool,
    selections: BTreeMap<String, Arc<Selection>>,
    started: u64,
    finished: u64,
    last: Value,
}
pub(crate) struct Selection {
    owner: Arc<Context>,
    scope: Scope,
    lookup: Lookup,
}
impl Selection {
    pub(crate) fn lookup(&self) -> &Lookup {
        &self.lookup
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.session.team_id)
                .is_some_and(|w| {
                    self.lookup
                        .current(&self.scope, &w.directory_usergroups, &w.usergroups)
                })
    }
    pub(crate) fn schedule(self: Arc<Self>, mut state: Signal<ShellState>) {
        if !self.current(&state.peek()) {
            return;
        }
        if self.lookup.is_source_hit() {
            let _ = super::freshness::hit_observed(state, self);
            return;
        }
        let wake = {
            let mut s = state.write();
            if !self.current(&s) {
                return;
            }
            arm(&mut s, &self.owner)
        };
        if let Some(wake) = wake {
            dioxus::core::spawn_forever(worker::run(state, wake));
        }
    }
    pub(super) fn owner(&self) -> Arc<Context> {
        self.owner.clone()
    }
}
fn unavailable() -> Error {
    Error::Api("usergroup_unknown_unavailable".into())
}
fn key(owner: &Context) -> Key {
    (
        owner.account.clone(),
        owner.session.team_id.clone(),
        owner.session.user_id.clone(),
    )
}
fn scope(owner: &Context) -> Result<Scope, Error> {
    Scope::new(
        &owner.account,
        &owner.session,
        owner.epoch,
        owner.realtime_generation(),
    )
    .map_err(|_| unavailable())
}
fn enabled() -> bool {
    !crate::fixture::is_fixture()
        || crate::profile_edit::sections::fixture::group_recovery::enabled()
}
/// Passive existing-source observation. This never registers absent work.
pub(crate) fn observed(shell: &ShellState, team: &str, id: &str) -> Option<Arc<Selection>> {
    let owner = Arc::new(Context::capture(shell, team).ok()?);
    let scope = scope(&owner).ok()?;
    let w = shell.core.workspaces.get(team)?;
    if let Ok(lookup) = Lookup::observed(&scope, &w.directory_usergroups, &w.usergroups, id) {
        return Some(Arc::new(Selection {
            owner,
            scope,
            lookup,
        }));
    }
    shell
        .group_unknown
        .slots
        .get(&key(&owner))?
        .selections
        .get(id)
        .filter(|s| s.current(shell))
        .cloned()
}
/// Original selector encounter. No await or metadata publication occurs here.
pub(crate) fn select(
    shell: &mut ShellState,
    team: &str,
    id: &str,
) -> Result<Arc<Selection>, Error> {
    if !enabled() {
        return Err(unavailable());
    }
    let owner = Arc::new(Context::capture(shell, team)?);
    let k = key(&owner);
    if shell
        .group_unknown
        .slots
        .get(&k)
        .is_none_or(|r| !r.owner.current(shell))
    {
        let scope = scope(&owner)?;
        shell.group_unknown.slots.insert(
            k.clone(),
            Runtime {
                owner: owner.clone(),
                identity: Arc::new(()),
                manager: Manager::new(scope.clone()),
                scope,
                worker: None,
                active: false,
                selections: BTreeMap::new(),
                started: 0,
                finished: 0,
                last: Value::Null,
            },
        );
    }
    let w = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
    let r = shell
        .group_unknown
        .slots
        .get_mut(&k)
        .ok_or_else(unavailable)?;
    let lookup = r
        .manager
        .lookup(&r.scope, &w.directory_usergroups, &w.usergroups, id)
        .map_err(|_| unavailable())?;
    let selected = Arc::new(Selection {
        owner,
        scope: r.scope.clone(),
        lookup,
    });
    r.selections.insert(id.into(), selected.clone());
    Ok(selected)
}
#[derive(Clone)]
struct Wake {
    owner: Arc<Context>,
    identity: Arc<()>,
    worker: Arc<()>,
}
impl Wake {
    fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && s.group_unknown
                .slots
                .get(&key(&self.owner))
                .is_some_and(|r| {
                    Arc::ptr_eq(&r.identity, &self.identity)
                        && r.worker
                            .as_ref()
                            .is_some_and(|w| Arc::ptr_eq(w, &self.worker))
                })
    }
    fn flight_current(&self, s: &ShellState, f: &Flight) -> bool {
        self.current(s)
            && s.group_unknown
                .slots
                .get(&key(&self.owner))
                .is_some_and(|r| r.manager.flight_current(&r.scope, f))
    }
}
fn arm(s: &mut ShellState, owner: &Arc<Context>) -> Option<Wake> {
    let r = s.group_unknown.slots.get_mut(&key(owner))?;
    if r.worker.is_some() || r.manager.pending().is_empty() {
        return None;
    }
    let worker = Arc::new(());
    r.worker = Some(worker.clone());
    Some(Wake {
        owner: owner.clone(),
        identity: r.identity.clone(),
        worker,
    })
}
fn clock() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}
fn snapshot(s: &ShellState, o: &Context) -> Result<Snapshot, Error> {
    if !o.current(s) {
        return Err(unavailable());
    }
    let w = s
        .core
        .workspaces
        .get(&o.session.team_id)
        .ok_or_else(unavailable)?;
    w.directory_usergroups
        .snapshot(&o.account, &o.session, o.epoch, &w.usergroups)
        .map_err(|_| unavailable())
}
fn current(s: &ShellState, o: &Context, source: &Snapshot) -> bool {
    o.current(s)
        && s.core.workspaces.get(&o.session.team_id).is_some_and(|w| {
            source.current(
                &w.directory_usergroups,
                &o.account,
                &o.session,
                o.epoch,
                &w.usergroups,
            )
        })
}
/// Fixture evidence is passive and never drains or fabricates source authority.
pub(crate) fn status(s: &ShellState) -> Value {
    if !crate::profile_edit::sections::fixture::group_recovery::enabled() {
        return Value::Null;
    }
    json!({"managers":s.group_unknown.slots.values().map(|r|json!({"account":r.owner.account,"team":r.owner.session.team_id,
        "epoch":r.owner.epoch,"socketGeneration":r.owner.realtime_generation(),"current":r.owner.current(s),
        "worker":r.worker.is_some(),"active":r.active,"pendingIds":r.manager.pending(),
        "started":r.started,"finished":r.finished,"lastSettlement":r.last})).collect::<Vec<_>>(),
        "quiescent":s.group_unknown.slots.values().all(|r|r.worker.is_none()&&!r.active)})
}

pub(crate) fn fixture_worker(s: &ShellState, team: &str) -> Option<std::sync::Weak<()>> {
    if !crate::profile_edit::sections::fixture::missing_group::named() {
        return None;
    }
    let owner = Context::capture(s, team).ok()?;
    s.group_unknown
        .slots
        .get(&key(&owner))?
        .worker
        .as_ref()
        .map(Arc::downgrade)
}

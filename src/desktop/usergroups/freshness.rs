//! Known-group selector freshness. An admitted editor keeps its original label;
//! this account-owned worker only refreshes canonical metadata in the background.
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
    state::directory_usergroups::freshness::{Batch, Clock, Manager, Observation, Scope},
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
    active: Option<Batch>,
    started: u64,
    finished: u64,
    observation: Observation,
    last: Value,
}
#[derive(Clone)]
struct Wake {
    owner: Arc<Context>,
    identity: Arc<()>,
    worker: Arc<()>,
}
impl Wake {
    fn current(&self, shell: &ShellState) -> bool {
        self.owner.account_current(shell)
            && shell
                .group_freshness
                .slots
                .get(&key(&self.owner))
                .is_some_and(|slot| {
                    Arc::ptr_eq(&slot.identity, &self.identity)
                        && slot
                            .worker
                            .as_ref()
                            .is_some_and(|v| Arc::ptr_eq(v, &self.worker))
                })
    }
    fn batch_current(&self, shell: &ShellState, batch: &Batch) -> bool {
        self.current(shell)
            && shell.group_freshness.slots[&key(&self.owner)]
                .manager
                .batch_current(&shell.group_freshness.slots[&key(&self.owner)].scope, batch)
    }
}
fn key(owner: &Context) -> Key {
    (
        owner.account.clone(),
        owner.session.team_id.clone(),
        owner.session.user_id.clone(),
    )
}
fn unavailable() -> Error {
    Error::Api("group_freshness_unavailable".into())
}
fn clock() -> Result<Clock, Error> {
    Clock::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| unavailable())?
            .as_millis()
            .try_into()
            .map_err(|_| unavailable())?,
    )
    .map_err(|_| unavailable())
}
fn facts(shell: &mut ShellState, team: &str) -> Result<Observation, Error> {
    let observed = crate::app_home::cfm_lifecycle::observed(shell, team)?;
    Ok(Observation {
        slow_connection_count: Some(observed.slow_connections),
        major_version: Some(observed.major),
        enabled: true,
    })
}
fn enabled() -> bool {
    !crate::fixture::is_fixture()
        || crate::profile_edit::sections::fixture::group_recovery::known_freshness::enabled()
}
fn ensure(
    shell: &mut ShellState,
    owner: Arc<Context>,
    observation: Observation,
) -> Result<(), Error> {
    if !owner.current(shell) {
        return Err(unavailable());
    }
    let key = key(&owner);
    if shell
        .group_freshness
        .slots
        .get(&key)
        .is_some_and(|slot| slot.owner.account_current(shell))
    {
        return Ok(());
    }
    let scope =
        Scope::new(&owner.account, &owner.session, owner.epoch).map_err(|_| unavailable())?;
    shell.group_freshness.slots.insert(
        key,
        Runtime {
            owner,
            identity: Arc::new(()),
            manager: Manager::new(scope.clone()),
            scope,
            worker: None,
            active: None,
            started: 0,
            finished: 0,
            observation,
            last: Value::Null,
        },
    );
    Ok(())
}
/// Called after the actual paired source/native metadata publication. Typed
/// presence, labels and membership-only operations never manufacture freshness.
pub(super) fn published(shell: &mut ShellState, owner: Arc<Context>, ids: &[String]) {
    if ids.is_empty() || !enabled() {
        return;
    }
    let _ = (|| -> Result<(), Error> {
        let observation = facts(shell, &owner.session.team_id)?;
        ensure(shell, owner.clone(), observation)?;
        let slot = shell
            .group_freshness
            .slots
            .get_mut(&key(&owner))
            .ok_or_else(unavailable)?;
        slot.manager
            .mark_fresh(&slot.scope, clock()?, observation, ids)
            .map_err(|_| unavailable())?;
        slot.observation = observation;
        Ok(())
    })();
}
/// The selector's side effect. No await and no new authority for the row already
/// returned to the rich editor; background errors do not make it unavailable.
pub(crate) fn hit(
    mut state: Signal<ShellState>,
    owner: Arc<Context>,
    id: &str,
) -> Result<(), Error> {
    if !enabled() {
        return Ok(());
    }
    let wake = {
        let mut shell = state.write();
        if !owner.current(&shell)
            || !shell
                .core
                .workspaces
                .get(&owner.session.team_id)
                .and_then(|w| w.usergroups.get(id))
                .is_some_and(|g| g.id == id && g.profile_label().is_some())
        {
            return Err(unavailable());
        }
        let observation = facts(&mut shell, &owner.session.team_id)?;
        ensure(&mut shell, owner.clone(), observation)?;
        let now = clock()?;
        let slot = shell
            .group_freshness
            .slots
            .get_mut(&key(&owner))
            .ok_or_else(unavailable)?;
        slot.manager
            .maybe_refetch(
                &slot.scope,
                now,
                observation,
                id,
                crate::app_home::cfm_lifecycle::paused(),
            )
            .map_err(|_| unavailable())?;
        slot.observation = observation;
        if slot.worker.is_some() || slot.manager.next_wake().is_none() {
            None
        } else {
            let worker = Arc::new(());
            slot.worker = Some(worker.clone());
            Some(Wake {
                owner,
                identity: slot.identity.clone(),
                worker,
            })
        }
    };
    if let Some(wake) = wake {
        dioxus::core::spawn_forever(worker::run(state, wake));
    }
    Ok(())
}

/// Observe the original worker's lifetime without keeping it alive. This closed
/// fixture witness grants neither publication authority nor cancellation.
pub(crate) fn fixture_worker(shell: &ShellState, team: &str) -> Option<std::sync::Weak<()>> {
    if !crate::profile_edit::sections::fixture::group_recovery::known_freshness::enabled() {
        return None;
    }
    let owner = Context::capture(shell, team).ok()?;
    let slot = shell.group_freshness.slots.get(&key(&owner))?;
    if !slot.owner.account_current(shell) {
        return None;
    }
    slot.worker.as_ref().map(Arc::downgrade)
}

/// Passive, credential-free fixture evidence. Reading this never resets stamps,
/// advances the queue or publishes a row.
pub(crate) fn snapshot(shell: &ShellState, team: &str) -> Value {
    if !crate::profile_edit::sections::fixture::group_recovery::known_freshness::enabled() {
        return Value::Null;
    }
    snapshot_inner(shell, team)
}
fn snapshot_inner(shell: &ShellState, team: &str) -> Value {
    let Ok(owner) = Context::capture(shell, team) else {
        return Value::Null;
    };
    let Some(slot) = shell
        .group_freshness
        .slots
        .get(&key(&owner))
        .filter(|slot| slot.owner.account_current(shell))
    else {
        return Value::Null;
    };
    json!({
        "scope":{"account":owner.account,"team":team,"user":owner.session.user_id,"epoch":owner.epoch},
        "freshIds":clock().ok().and_then(|now|slot.manager.fresh_ids(&slot.scope,now).ok()),
        "pending":slot.manager.pending(),
        "inFlight":slot.active.as_ref().map(|batch|batch.ids()),
        "worker":slot.worker.is_some(),"started":slot.started,"finished":slot.finished,
        "observations":{"slowConnectionCount":slot.observation.slow_connection_count,
            "major":slot.observation.major_version},
        "lastSettlement":slot.last,
    })
}

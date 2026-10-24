//! Shared missing selector acquisition. A display observation is not a server
//! absence claim, and only the core manager can authorize terminal markers.
use super::*;
use std::cell::{Cell, RefCell};
use super_platinum_core::state::directory_channels::{
    projection::ClassificationState,
    unknown::{self as domain, Flight, Lookup, LookupKind, Manager, Scope},
};
pub(crate) mod fixture;
mod publish;
mod read;
#[cfg(test)]
mod tests;
mod worker;

#[derive(Default)]
pub(super) struct Registry {
    slots: BTreeMap<Key, Runtime>,
    fixture: fixture::Model,
}
struct Runtime {
    owner: Owner,
    identity: Arc<()>,
    scope: Scope,
    manager: Manager,
    worker: Option<Arc<()>>,
    active: bool,
    selections: BTreeMap<String, Arc<Selection>>,
}
/// Consumers retain this opaque source observation across renderer work.
pub(crate) struct Selection {
    owner: Owner,
    identity: Arc<()>,
    lookup: Lookup,
}
impl Selection {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if !self.owner.current(shell) {
            return false;
        }
        let ctx = &self.owner.context;
        let Some(r) = shell.app_home.channel_fetches.unknown.slots.get(&ctx.key()) else {
            return false;
        };
        let Some(w) = shell.core.workspaces.get(&ctx.session.team_id) else {
            return false;
        };
        Arc::ptr_eq(&r.identity, &self.identity)
            && self.lookup.current(
                &r.manager,
                &r.scope,
                &w.directory_channels,
                GENERATION,
                &w.channels,
            )
    }
    pub(crate) fn lookup(&self) -> &Lookup {
        &self.lookup
    }
    pub(crate) fn schedule(self: Arc<Self>, mut state: Signal<ShellState>) {
        if !self.lookup.is_transient() {
            let _ = conditional::hit_selected(state, &self);
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
    pub(super) fn context(&self) -> Arc<Context> {
        self.owner.context.clone()
    }
}
fn owner(shell: &ShellState, team: &str) -> Result<Owner, Error> {
    let context = Context::capture(shell, team)?;
    let slot = shell
        .app_home
        .channel_fetches
        .slots
        .get(&context.key())
        .ok_or_else(unavailable)?;
    let owner = Owner {
        context: slot.context.clone(),
        token: slot.token.clone(),
    };
    if !owner.publication_current(shell) {
        return Err(unavailable());
    }
    Ok(owner)
}
/// Register a real selector encounter, without awaiting or publishing a fake row.
pub(crate) fn select(
    shell: &mut ShellState,
    team: &str,
    id: &str,
) -> Result<Arc<Selection>, Error> {
    if crate::fixture::is_fixture() && !fixture::enabled() {
        return Err(unavailable());
    }
    let owner = owner(shell, team)?;
    let ctx = &owner.context;
    // Preserve the same initialization/pause observation required by canonical selectors.
    let _ = crate::app_home::cfm_lifecycle::observed(shell, team)?;
    let observed = Arc::new(observe(shell, ctx)?);
    let key = ctx.key();
    if shell
        .app_home
        .channel_fetches
        .unknown
        .slots
        .get(&key)
        .is_none_or(|r| !r.owner.current(shell))
    {
        let scope = Scope::new(&ctx.account, &ctx.session, ctx.epoch, ctx.rt_generation)
            .map_err(|_| unavailable())?;
        shell.app_home.channel_fetches.unknown.slots.insert(
            key.clone(),
            Runtime {
                owner: owner.clone(),
                identity: Arc::new(()),
                manager: Manager::new(scope.clone()),
                scope,
                worker: None,
                active: false,
                selections: BTreeMap::new(),
            },
        );
    }
    let w = &shell.core.workspaces[team];
    let r = shell
        .app_home
        .channel_fetches
        .unknown
        .slots
        .get_mut(&key)
        .unwrap();
    let lookup = r
        .manager
        .lookup(
            &r.scope,
            &w.directory_channels,
            observed,
            GENERATION,
            &w.channels,
            id,
            true,
        )
        .map_err(|_| unavailable())?;
    if lookup.kind() == LookupKind::Unobserved {
        return Err(unavailable());
    }
    let selection = Arc::new(Selection {
        owner,
        identity: r.identity.clone(),
        lookup,
    });
    r.selections.insert(id.into(), selection.clone());
    Ok(selection)
}
/// Read-only use of a previously registered encounter; diagnostics do not fetch.
pub(crate) fn observed(shell: &ShellState, team: &str, id: &str) -> Option<Arc<Selection>> {
    let owner = owner(shell, team).ok()?;
    shell
        .app_home
        .channel_fetches
        .unknown
        .slots
        .get(&owner.context.key())?
        .selections
        .get(id)
        .filter(|s| s.current(shell))
        .cloned()
}
#[derive(Clone)]
struct Wake {
    owner: Owner,
    identity: Arc<()>,
    token: Arc<()>,
}
impl Wake {
    fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell
                .app_home
                .channel_fetches
                .unknown
                .slots
                .get(&self.owner.context.key())
                .is_some_and(|r| {
                    Arc::ptr_eq(&r.identity, &self.identity)
                        && r.worker
                            .as_ref()
                            .is_some_and(|t| Arc::ptr_eq(t, &self.token))
                })
    }
}
struct Attempt {
    wake: Wake,
    owner: Owner,
    flight: Flight,
    revision: u64,
    publication: RefCell<Arc<Observation>>,
    shadow: RefCell<ClassificationState>,
    refused_dispatch: Cell<bool>,
    // Actual returned objects plus non-unknown rows retained at read dispatch.
    // Ordered membership/unchanged actions alone never populate this ledger.
    accepted_ids: RefCell<std::collections::BTreeSet<String>>,
}
impl Attempt {
    fn current(&self, shell: &ShellState) -> bool {
        self.wake.current(shell)
            && self.owner.publication_current(shell)
            && Arc::ptr_eq(
                &self.publication.borrow(),
                &shell.app_home.channel_fetches.slots[&self.owner.context.key()].publication,
            )
            && shell.core.workspaces[&self.owner.context.session.team_id]
                .channel_lifecycle
                .revision()
                == self.revision
            && shell
                .app_home
                .channel_fetches
                .unknown
                .slots
                .get(&self.owner.context.key())
                .is_some_and(|r| r.manager.flight_current(&r.scope, &self.flight))
    }
    fn dispatch(&self, shell: &ShellState) -> bool {
        let current = self.admit(shell);
        if !current {
            self.refused_dispatch.set(true);
        }
        current
    }
    fn admit(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && shell.app_home.channel_fetches.slots[&self.owner.context.key()]
                .retry_at
                .is_none_or(|at| at <= Instant::now())
    }
}
fn arm(shell: &mut ShellState, owner: &Owner) -> Option<Wake> {
    let r = shell
        .app_home
        .channel_fetches
        .unknown
        .slots
        .get_mut(&owner.context.key())?;
    if r.worker.is_some() || r.manager.pending().is_empty() {
        return None;
    }
    let token = Arc::new(());
    r.worker = Some(token.clone());
    Some(Wake {
        owner: owner.clone(),
        identity: r.identity.clone(),
        token,
    })
}
pub(super) fn busy(shell: &ShellState, key: &Key) -> bool {
    shell
        .app_home
        .channel_fetches
        .unknown
        .slots
        .get(key)
        .is_some_and(|r| r.active && r.owner.current(shell))
}
fn observed_attempt(shell: &ShellState, a: &Attempt) -> Result<Observation, Error> {
    if !a.current(shell) {
        return Err(unavailable());
    }
    observe(shell, &a.owner.context)
}
fn failure(shell: &mut ShellState, a: &Attempt, error: Error) {
    if !a.current(shell) {
        return;
    }
    let ctx = &a.owner.context;
    crate::profile_edit::sections::rich::refresh_failed(
        shell,
        &ctx.account,
        &ctx.session.team_id,
        &ctx.session.user_id,
        a.flight.ids(),
    );
    failed(shell, &a.owner, error);
}
fn clock() -> Duration {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
}

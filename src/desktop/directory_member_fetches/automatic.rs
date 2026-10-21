//! Default connected-socket member freshness. Queuing is not acknowledgment;
//! mounted consumers retain their own immutable labels and native drafts.
use super::Key;
use super::*;
use crate::app_home::cfm_lifecycle;
use dioxus::prelude::*;
use super_platinum_core::{
    slack::{events::RtEvent, realtime::Connection},
    state::{
        RealtimeStatus,
        directory_members::{Observations, subscriptions as model},
    },
};
pub(crate) mod mode;
mod observer;
mod worker;
pub(crate) use observer::wait;
const GENERATION: &str = "automatic-known-member-hit";
pub(super) enum Failure {
    Refused,
    Failed,
}

#[derive(Default)]
pub(super) struct Registry {
    slots: BTreeMap<Key, Runtime>,
    mode: mode::Registry,
}
impl Registry {
    pub(super) fn retain(&mut self, shell: &ShellState) {
        self.slots.retain(|_, r| r.context.account_current(shell));
        self.mode.retain(shell);
    }
}
struct Runtime {
    context: Arc<Context>,
    identity: Arc<()>,
    scope: model::Scope,
    manager: model::Manager,
    worker: Option<Arc<()>>,
    changed: tokio::sync::watch::Sender<u64>,
    started: u64,
    finished: u64,
    http_finished: u64,
    acknowledgments: u64,
    failures: u64,
}
impl Runtime {
    fn notify(&self) {
        self.changed.send_modify(|v| *v = v.wrapping_add(1));
    }
}
#[derive(Clone)]
struct Wake {
    context: Arc<Context>,
    identity: Arc<()>,
    token: Arc<()>,
}
impl Wake {
    fn current(&self, shell: &ShellState) -> bool {
        self.context.account_current(shell)
            && shell
                .directory_member_fetches
                .automatic
                .slots
                .get(&self.context.key())
                .is_some_and(|r| {
                    Arc::ptr_eq(&self.identity, &r.identity)
                        && r.worker
                            .as_ref()
                            .is_some_and(|v| Arc::ptr_eq(v, &self.token))
                })
    }
}
pub(crate) fn fixture_enabled() -> bool {
    crate::profile_edit::sections::fixture::member_freshness::enabled()
}
fn socket(shell: &ShellState, team: &str) -> Result<Connection, Error> {
    match &shell.core.workspaces.get(team).ok_or_else(unavailable)?.rt {
        RealtimeStatus::Connected(connection) => Ok(connection.clone()),
        _ => Err(unavailable()),
    }
}
fn now(shell: &ShellState) -> Result<u64, Error> {
    shell.directory_member_fetches.millis(Instant::now())
}
fn facts(shell: &mut ShellState, team: &str) -> Result<model::Observation, Error> {
    mode::observed(shell, team)?;
    socket(shell, team)?;
    let observed = cfm_lifecycle::observed(shell, team)?;
    if observed.slow_connections == 0 {
        return Err(unavailable());
    }
    // Every accepted native socket is a full connection; no fast-resume path.
    Ok(model::Observation {
        connection_count: observed.slow_connections,
        slow_connection_count: observed.slow_connections,
        major_version: observed.major,
    })
}
fn ensure(shell: &mut ShellState, context: Arc<Context>) -> Result<(), Error> {
    let key = context.key();
    if shell
        .directory_member_fetches
        .automatic
        .slots
        .get(&key)
        .is_some_and(|r| r.context.account_current(shell))
    {
        return Ok(());
    }
    let scope = model::Scope::capture(&context.account, &context.session, context.epoch)?;
    shell.directory_member_fetches.automatic.slots.insert(
        key,
        Runtime {
            context,
            identity: Arc::new(()),
            manager: model::Manager::new(scope.clone()),
            scope,
            worker: None,
            changed: tokio::sync::watch::channel(0).0,
            started: 0,
            finished: 0,
            http_finished: 0,
            acknowledgments: 0,
            failures: 0,
        },
    );
    Ok(())
}
pub(crate) struct Hit {
    context: Arc<Context>,
    observed: Observations,
    id: String,
}
pub(crate) fn observe(shell: &ShellState, team: &str, id: &str) -> Result<Hit, Error> {
    if crate::fixture::is_fixture() && !fixture_enabled() {
        return Err(unavailable());
    }
    socket(shell, team)?;
    mode::observed(shell, team)?;
    let context = Arc::new(Context::capture(shell, team)?);
    let workspace = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
    let observed = workspace.directory_members.observations(
        &context.account,
        &context.session,
        context.epoch,
        GENERATION,
    )?;
    observed.rows(&workspace.directory_members, &[id.into()])?;
    Ok(Hit {
        context,
        observed,
        id: id.into(),
    })
}
impl Hit {
    pub(crate) fn schedule(self, mut state: Signal<ShellState>) {
        let work = (|| -> Result<Option<Wake>, Error> {
            let mut shell = state.write();
            let ctx = self.context;
            if !ctx.current(&shell)
                || !self.observed.current(
                    &shell
                        .core
                        .workspaces
                        .get(&ctx.session.team_id)
                        .ok_or_else(unavailable)?
                        .directory_members,
                    &ctx.account,
                    &ctx.session,
                    ctx.epoch,
                    GENERATION,
                )
            {
                return Err(unavailable());
            }
            let observed = facts(&mut shell, &ctx.session.team_id)?;
            ensure(&mut shell, ctx.clone())?;
            let at = now(&shell)?;
            let r = shell
                .directory_member_fetches
                .automatic
                .slots
                .get_mut(&ctx.key())
                .unwrap();
            r.manager
                .hit(&r.scope, at, observed, &self.id, cfm_lifecycle::paused())?;
            r.notify();
            Ok(arm(r))
        })();
        if let Ok(Some(wake)) = work {
            dioxus::core::spawn_forever(worker::run(state, wake));
        }
    }
}
fn arm(r: &mut Runtime) -> Option<Wake> {
    if r.worker.is_some()
        || r.manager.next_parent_due_ms().is_none() && r.manager.cycles().is_empty()
    {
        return None;
    }
    let token = Arc::new(());
    r.worker = Some(token.clone());
    Some(Wake {
        context: r.context.clone(),
        identity: r.identity.clone(),
        token,
    })
}
/// Wake only this account's existing work; this never schedules acquisition.
fn notify(shell: &ShellState, team: &str) {
    let Ok(context) = Context::capture(shell, team) else {
        return;
    };
    if let Some(runtime) = shell
        .directory_member_fetches
        .automatic
        .slots
        .get(&context.key())
        && runtime.context.account_current(shell)
    {
        runtime.notify();
    }
}
/// Called after the accepted native disconnect mutation. Pending work remains
/// owned by this account while the original socket operations retire.
pub(crate) fn disconnected(shell: &ShellState, team: &str, generation: u64) {
    if shell.core.workspaces.get(team).is_some_and(|workspace| {
        generation >= workspace.rt_generation
            && matches!(workspace.rt, RealtimeStatus::Disconnected)
    }) {
        notify(shell, team);
    }
}
/// Accepted actual connection; never derives a count from the generation.
pub(crate) fn connected(shell: &mut ShellState, team: &str, generation: u64) {
    if shell
        .core
        .workspaces
        .get(team)
        .is_none_or(|w| w.rt_generation != generation)
    {
        return;
    }
    let Ok(ctx) = Context::capture(shell, team) else {
        return;
    };
    let Ok(observed) = facts(shell, team) else {
        return;
    };
    let Ok(at) = now(shell) else { return };
    let Some(r) = shell
        .directory_member_fetches
        .automatic
        .slots
        .get_mut(&ctx.key())
    else {
        return;
    };
    if !r
        .manager
        .current(&model::Scope::capture(&ctx.account, &ctx.session, ctx.epoch).unwrap())
    {
        return;
    }
    let _ = r.manager.observe(&r.scope, at, observed);
    r.notify();
}
/// Called after real native event application. Socket payload is an invalidation,
/// never permission to replace canonical raw source or an open editor catalog.
pub(crate) fn event(shell: &mut ShellState, team: &str, generation: u64, event: &RtEvent) {
    if shell
        .core
        .workspaces
        .get(team)
        .is_none_or(|w| w.rt_generation != generation)
    {
        return;
    }
    let ids: Vec<String> = match event {
        RtEvent::UserChanged(user) => vec![user.id.clone()],
        RtEvent::Unknown(raw) if raw.kind == "user_invalidated" => raw
            .rest
            .get("user")
            .and_then(|user| user.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .into_iter()
            .collect(),
        _ => return,
    };
    if ids.is_empty() {
        return;
    }
    let Ok(ctx) = Context::capture(shell, team) else {
        return;
    };
    if let Some(r) = shell
        .directory_member_fetches
        .automatic
        .slots
        .get_mut(&ctx.key())
    {
        let _ = r.manager.invalidate(&r.scope, &ids);
        r.notify();
    }
}
/// Start pending same-account work after an accepted connection transition.
pub(crate) fn resume(mut state: Signal<ShellState>, team: &str) {
    let wake = {
        let mut shell = state.write();
        let Ok(ctx) = Context::capture(&shell, team) else {
            return;
        };
        if socket(&shell, team).is_err() {
            return;
        }
        let Some(r) = shell
            .directory_member_fetches
            .automatic
            .slots
            .get_mut(&ctx.key())
        else {
            return;
        };
        arm(r)
    };
    if let Some(wake) = wake {
        dioxus::core::spawn_forever(worker::run(state, wake));
    }
}
pub(crate) fn fixture_status(shell: &ShellState, team: &str) -> Value {
    if !fixture_enabled() && !crate::profile_edit::sections::fixture::group_recovery::enabled() {
        return Value::Null;
    }
    let Ok(ctx) = Context::capture(shell, team) else {
        return Value::Null;
    };
    let Some(r) = shell
        .directory_member_fetches
        .automatic
        .slots
        .get(&ctx.key())
    else {
        return serde_json::json!({"pending":[],"subscribed":[],"stale":[],"worker":false,"started":0,"finished":0,"http_finished":0,"acknowledgments":0,"failures":0});
    };
    serde_json::json!({"pending":r.manager.pending(),"subscribed":r.manager.subscribed(),"stale":r.manager.stale(),
        "worker":r.worker.is_some(),"started":r.started,"finished":r.finished,"http_finished":r.http_finished,
        "acknowledgments":r.acknowledgments,"failures":r.failures})
}

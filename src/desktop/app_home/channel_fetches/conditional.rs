//! Existing canonical selector-hit refresh; unknown acquisition is separate.
use super::*;
use std::cell::RefCell;
use super_platinum_core::state::directory_channels::{
    freshness::{self, Batch, Clock, Fresh, Manager, RowKind, Scope},
    projection::{ClassificationKind, ClassificationState},
};
pub(super) mod continuation;
pub(crate) mod fixture;
mod notice;
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
    context: Arc<Context>,
    identity: Arc<()>,
    scope: Scope,
    manager: Manager,
    worker: Option<Arc<()>>,
    active: bool,
    notice: notice::State,
    continuation: Option<continuation::Continuation>,
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
                .app_home
                .channel_fetches
                .conditional
                .slots
                .get(&self.context.key())
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
    batch: Batch,
    revision: u64,
    publication: RefCell<Arc<Observation>>,
    connection: Option<u64>,
    shadow: RefCell<ClassificationState>,
}
impl Attempt {
    fn accounting_current(&self, shell: &ShellState) -> bool {
        self.wake.current(shell)
            && shell
                .app_home
                .channel_fetches
                .conditional
                .slots
                .get(&self.owner.context.key())
                .is_some_and(|r| r.manager.batch_current(&r.scope, &self.batch))
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.accounting_current(shell)
            && self.owner.publication_current(shell)
            && Arc::ptr_eq(
                &self.publication.borrow(),
                &shell.app_home.channel_fetches.slots[&self.owner.context.key()].publication,
            )
            && shell.core.workspaces[&self.owner.context.session.team_id]
                .channel_lifecycle
                .revision()
                == self.revision
    }
    fn admit(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && shell.app_home.channel_fetches.slots[&self.owner.context.key()]
                .retry_at
                .is_none_or(|at| at <= Instant::now())
    }
}
fn clock() -> Result<Clock, Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| unavailable())?;
    Clock::new(now.as_millis().try_into().map_err(|_| unavailable())?).map_err(|_| unavailable())
}
fn facts(shell: &mut ShellState, ctx: &Context) -> Result<freshness::Observation, Error> {
    let snapshot = crate::app_home::cfm_lifecycle::observed(shell, &ctx.session.team_id)?;
    Ok(freshness::Observation {
        slow_connection_count: Some(snapshot.slow_connections),
        major_version: Some(snapshot.major),
        enabled: true,
    })
}
fn ensure(shell: &mut ShellState, context: Arc<Context>) -> Result<(), Error> {
    if !context.current(shell) {
        return Err(unavailable());
    }
    let key = context.key();
    if shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get(&key)
        .is_some_and(|r| r.context.account_current(shell))
    {
        return Ok(());
    }
    let scope =
        Scope::new(&context.account, &context.session, context.epoch).map_err(|_| unavailable())?;
    shell.app_home.channel_fetches.conditional.slots.insert(
        key,
        Runtime {
            context,
            identity: Arc::new(()),
            manager: Manager::new(scope.clone()),
            scope,
            worker: None,
            active: false,
            notice: notice::State::default(),
            continuation: None,
        },
    );
    Ok(())
}
fn owner(shell: &ShellState, ctx: &Context) -> Result<Owner, Error> {
    let slot = shell
        .app_home
        .channel_fetches
        .slots
        .get(&ctx.key())
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
pub(super) fn hit(mut state: Signal<ShellState>, ctx: Arc<Context>, id: &str) -> Result<(), Error> {
    if crate::fixture::is_fixture() && !fixture::enabled() {
        return Ok(());
    }
    let wake = {
        let mut s = state.write();
        if !ctx.current(&s) {
            return Err(unavailable());
        }
        let owner = owner(&s, &ctx)?;
        let row = observe(&s, &ctx)?
            .rows()
            .iter()
            .find(|r| r["id"] == id)
            .cloned()
            .ok_or_else(unavailable)?;
        // This milestone admits the existing ordinary-channel consumer only.
        canonical::ordinary_label(&serde_json::json!({"type":"channel","channel_id":id}), &row)?;
        let observation = facts(&mut s, &ctx)?;
        ensure(&mut s, ctx.clone())?;
        let now = now(&s)?;
        let r = s
            .app_home
            .channel_fetches
            .conditional
            .slots
            .get_mut(&ctx.key())
            .unwrap();
        r.manager
            .maybe_refetch(
                &r.scope,
                now,
                observation,
                id,
                crate::app_home::cfm_lifecycle::paused(),
            )
            .map_err(|_| unavailable())?;
        arm(&mut s, &owner)
    };
    if let Some(wake) = wake {
        dioxus::core::spawn_forever(worker::run(state, wake));
    }
    Ok(())
}
fn arm(shell: &mut ShellState, owner: &Owner) -> Option<Wake> {
    let r = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get_mut(&owner.context.key())?;
    if r.worker.is_some() || r.manager.next_wake().is_none() {
        return None;
    }
    let token = Arc::new(());
    r.worker = Some(token.clone());
    Some(Wake {
        context: owner.context.clone(),
        identity: r.identity.clone(),
        token,
    })
}
/// TTL invalidation leaves the canonical predecessor and current display intact.
pub(super) fn stale(shell: &mut ShellState, owner: &Owner, ids: &[String]) -> Result<(), Error> {
    if !owner.publication_current(shell) {
        return Err(unavailable());
    }
    let ctx = &owner.context;
    let observation = facts(shell, ctx)?;
    ensure(shell, ctx.clone())?;
    let now = now(shell)?;
    let registry = &mut shell.app_home.channel_fetches;
    let r = registry.conditional.slots.get_mut(&ctx.key()).unwrap();
    let effects = r
        .manager
        .mark_stale(&r.scope, now, observation, ids, &mut registry.bursts)
        .map_err(|_| unavailable())?;
    if effects.notify_throttled && r.notice.invoke(Instant::now()) {
        registry.slots[&ctx.key()].notify();
    }
    Ok(())
}
fn fresh(shadow: &ClassificationState, ids: &[String]) -> Result<Vec<Fresh>, Error> {
    ids.iter()
        .map(|id| {
            let kind = match shadow.row(id).map_err(|_| unavailable())?.kind {
                ClassificationKind::Existing => RowKind::Existing,
                ClassificationKind::NonExistent => RowKind::NonExistent,
                ClassificationKind::Absent => RowKind::Absent,
            };
            Ok(Fresh {
                id: id.clone(),
                kind,
                entropy: entropy(),
            })
        })
        .collect()
}
fn entropy() -> f64 {
    use std::hash::BuildHasher;
    let bits = std::collections::hash_map::RandomState::new().hash_one(0u8) >> 11;
    bits as f64 / ((1u64 << 53) as f64)
}
fn failure(shell: &mut ShellState, attempt: &Attempt, error: Error) {
    if !attempt.accounting_current(shell) {
        return;
    }
    // A full-height Profile editor can leave no safe space for the shell toast.
    // Preserve its local feedback only for this exact current read receipt.
    if attempt.current(shell) {
        let ctx = &attempt.owner.context;
        crate::profile_edit::sections::rich::refresh_failed(
            shell,
            &ctx.account,
            &ctx.session.team_id,
            &ctx.session.user_id,
            attempt.batch.ids(),
        );
    }
    if attempt.owner.current(shell) {
        failed(shell, &attempt.owner, error.clone());
    }
    shell.report_failure(&error, "", || "Channel metadata could not refresh.".into());
}

fn observed(shell: &ShellState, attempt: &Attempt) -> Result<Observation, Error> {
    if !attempt.current(shell) {
        return Err(unavailable());
    }
    observe(shell, &attempt.owner.context)
}
pub(super) fn busy(shell: &ShellState, key: &Key) -> bool {
    shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get(key)
        .is_some_and(|r| r.active && r.context.account_current(shell))
}
fn publication_advance(shell: &mut ShellState, attempt: &Attempt) -> Result<(), Error> {
    continuation::conditional(shell, attempt)?;
    *attempt.publication.borrow_mut() = shell.app_home.channel_fetches.slots
        [&attempt.owner.context.key()]
        .publication
        .clone();
    Ok(())
}

fn now(shell: &ShellState) -> Result<Clock, Error> {
    Clock::new(
        clock()?
            .millis()
            .checked_add(fixture::offset(shell))
            .ok_or_else(unavailable)?,
    )
    .map_err(|_| unavailable())
}

pub(super) fn fresh_ids(shell: &ShellState, owner: &Owner) -> Result<Vec<String>, Error> {
    if !owner.publication_current(shell) {
        return Err(unavailable());
    }
    let Some(r) = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get(&owner.context.key())
    else {
        return Ok(Vec::new());
    };
    if !r.context.account_current(shell) {
        return Err(unavailable());
    }
    r.manager
        .fresh_ids(&r.scope, now(shell)?)
        .map_err(|_| unavailable())
}

pub(super) fn source_values(
    shell: &ShellState,
    owner: &Owner,
    rows: &[Value],
) -> Result<Option<continuation::Input>, Error> {
    continuation::input(shell, owner, rows)
}
pub(super) fn source_fresh(
    shell: &mut ShellState,
    owner: &Owner,
    input: Option<continuation::Input>,
    facts: &Facts,
    boot: bool,
) -> Result<Option<continuation::Continuation>, Error> {
    continuation::processed(shell, owner, input, facts, boot)
}
fn mark_source_fresh(shell: &mut ShellState, owner: &Owner, values: &[Fresh]) -> Result<(), Error> {
    if !owner.current(shell) {
        return Err(unavailable());
    }
    let ctx = &owner.context;
    // Passive source effects never grant initialized read admission.
    let snapshot = crate::app_home::cfm_lifecycle::capture(shell, &ctx.session.team_id)?;
    let observation = freshness::Observation {
        slow_connection_count: Some(snapshot.slow_connections),
        major_version: Some(snapshot.major),
        enabled: true,
    };
    let now = now(shell)?;
    let r = shell
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get_mut(&ctx.key())
        .ok_or_else(unavailable)?;
    r.manager
        .mark_fresh(&r.scope, now, observation, values)
        .map_err(|_| unavailable())
}

pub(super) fn notifications(state: Signal<ShellState>) {
    notice::schedule(state);
}

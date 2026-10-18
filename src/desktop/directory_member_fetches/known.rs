//! Shared known-member read/adoption. Explicit Retry uses unconditional nzRy;
//! automatic subscription work supplies a conditional plan and completion.
use super::*;
use crate::app_home::channel_fetches::{self, MemberFlags};
use dioxus::prelude::*;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use super_platinum_core::{
    block_actions::directory_endpoint::member_info::Read, state::directory_members::Observations,
};
mod read;
mod stage;
const GENERATION: &str = "explicit-known-member-recovery";

struct Attempt {
    context: Arc<Context>,
    observed: Observations,
    native_before: BTreeMap<String, Value>,
    flags: MemberFlags,
    ids: Vec<String>,
    conditional_rows: Option<
        Vec<super_platinum_core::block_actions::workflow_management::member_hydration::Current>,
    >,
}
impl Attempt {
    fn capture(shell: &mut ShellState, team: &str, ids: &[String]) -> Result<Self, Error> {
        Self::capture_mode(shell, team, ids, false)
    }
    fn capture_mode(
        shell: &mut ShellState,
        team: &str,
        ids: &[String],
        conditional: bool,
    ) -> Result<Self, Error> {
        let fresh = Arc::new(Context::capture(shell, team)?);
        let key = fresh.key();
        let context = shell
            .directory_member_fetches
            .slots
            .get(&key)
            .filter(|slot| slot.context.current(shell))
            .map(|slot| slot.context.clone())
            .unwrap_or(fresh);
        let flags = channel_fetches::member_flags(shell, team)?;
        let workspace = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
        let observed = workspace.directory_members.observations(
            &context.account,
            &context.session,
            context.epoch,
            GENERATION,
        )?;
        let known: BTreeSet<_> = observed.source_ids().collect();
        let mut seen = BTreeSet::new();
        let ids: Vec<_> = ids
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect();
        if ids.is_empty()
            || ids.len() > 100_000
            || ids.iter().any(|id| !known.contains(id.as_str()))
        {
            return Err(unavailable());
        }
        let mut conditional_rows = conditional.then(Vec::new);
        for chunk in ids.chunks(100) {
            if let Some(rows) = conditional_rows.as_mut() {
                let requested = observed.rows(&workspace.directory_members, chunk)?;
                Read::conditional(chunk.to_vec(), &requested)?;
                rows.extend(requested);
            } else {
                Read::capture(chunk.to_vec())?;
            }
        }
        let native_before = context::native_snapshot(&workspace.users)?;
        // A forced read can precede the first absent selector. Reuse the same
        // account/team failure cell without registering any missing claim.
        if shell
            .directory_member_fetches
            .slots
            .get(&key)
            .is_none_or(|slot| !Arc::ptr_eq(&slot.context, &context))
        {
            shell.directory_member_fetches.slots.insert(
                key,
                Slot {
                    context: context.clone(),
                    manager: Default::default(),
                    changed: tokio::sync::watch::channel(0).0,
                    retry_at: None,
                    last_failure: None,
                },
            );
        }
        let attempt = Self {
            context,
            observed,
            native_before,
            flags,
            ids,
            conditional_rows,
        };
        if !attempt.current(shell) {
            return Err(unavailable());
        }
        Ok(attempt)
    }
    fn read(&self, ids: &[String]) -> Result<Read, Error> {
        match &self.conditional_rows {
            Some(rows) => Read::conditional(ids.to_vec(), rows),
            None => Read::capture(ids.to_vec()),
        }
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && self.flags.current(shell)
            && shell.directory_member_fetches.slot(&self.context).is_ok()
            && shell
                .core
                .workspaces
                .get(&self.context.session.team_id)
                .is_some_and(|workspace| {
                    self.observed.current(
                        &workspace.directory_members,
                        &self.context.account,
                        &self.context.session,
                        self.context.epoch,
                        GENERATION,
                    ) && context::native_snapshot(&workspace.users)
                        .is_ok_and(|native| native == self.native_before)
                })
    }
    fn cooling(&self, shell: &ShellState) -> bool {
        shell
            .directory_member_fetches
            .slot(&self.context)
            .is_ok_and(|slot| {
                slot.retry_at
                    .is_some_and(|at| at.is_none_or(|at| at > Instant::now()))
            })
    }
    fn admit(
        &self,
        shell: &ShellState,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<(), Error> {
        if !self.current(shell) || !current(shell) || self.cooling(shell) {
            return Err(unavailable());
        }
        Ok(())
    }
    fn failure(&self, shell: &mut ShellState, error: &Error) {
        // A returned failure belongs to the original account even if its UI or
        // source observation retired. It never grants source publication.
        if !self.context.current(shell) {
            return;
        }
        let Ok(slot) = shell.directory_member_fetches.slot_mut(&self.context) else {
            return;
        };
        if let Error::RateLimited { retry_after_secs } = error {
            let deadline =
                Instant::now().checked_add(Duration::from_secs(retry_after_secs.unwrap_or(60)));
            slot.retry_at = Some(match slot.retry_at {
                Some(Some(old)) => deadline.map(|new| new.max(old)),
                Some(None) => None,
                None => deadline,
            });
        }
        slot.last_failure = Some(Arc::new(error.clone()));
        if self.conditional_rows.is_none() {
            shell.report_failure(error, "", || {
                "Member details could not be refreshed.".into()
            });
        }
    }
}

static STARTED: AtomicU64 = AtomicU64::new(0);
static FINISHED: AtomicU64 = AtomicU64::new(0);
struct Running;
impl Drop for Running {
    fn drop(&mut self) {
        FINISHED.fetch_add(1, Ordering::Relaxed);
    }
}
/// Passive closed-fixture completion, including renderer/adoption or refusal.
pub(crate) fn fixture_status(_shell: &ShellState) -> Value {
    if !crate::profile_edit::sections::fixture::member_recovery::enabled() {
        return Value::Null;
    }
    let started = STARTED.load(Ordering::Relaxed);
    let finished = FINISHED.load(Ordering::Relaxed);
    serde_json::json!({"started":started,"finished":finished,"active":started.saturating_sub(finished)})
}

/// Caller selects only unavailable known refs from the original failed opening.
/// Every selected ID must still have actual canonical source at capture time.
pub(crate) async fn recover(
    mut state: Signal<ShellState>,
    team: &str,
    ids: &[String],
    current: impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    if crate::fixture::is_fixture()
        && !crate::profile_edit::sections::fixture::member_recovery::enabled()
    {
        return Err(unavailable());
    }
    STARTED.fetch_add(1, Ordering::Relaxed);
    let _running = Running;
    let attempt = {
        let mut shell = state.write();
        if !current(&shell) {
            return Err(unavailable());
        }
        Attempt::capture(&mut shell, team, ids)?
    };
    attempt.admit(&state.peek(), &current)?;
    let received = read::acquire(state, &attempt, &current).await?;
    stage::apply(state, &attempt, received, &current).await
}

/// Subscription and read are independent issued operations, as in source
/// Promise.all. Only their successful aggregate reaches private adoption.
/// The shared automatic owner supplies completion and all lifetime guards.
pub(super) async fn refresh(
    mut state: Signal<ShellState>,
    team: &str,
    ids: &[String],
    current: impl Fn(&ShellState) -> bool,
    subscription: impl std::future::Future<Output = Result<(), super::automatic::Failure>>,
    http_finished: impl Fn(),
) -> Result<(), super::automatic::Failure> {
    use super::automatic::Failure;
    if crate::fixture::is_fixture() && !super::automatic::fixture_enabled() {
        return Err(Failure::Refused);
    }
    let attempt = {
        let mut shell = state.write();
        if !current(&shell) {
            return Err(Failure::Refused);
        }
        Attempt::capture_mode(&mut shell, team, ids, true).map_err(|_| Failure::Refused)?
    };
    attempt
        .admit(&state.peek(), &current)
        .map_err(|_| Failure::Refused)?;
    let read = async {
        let result = read::acquire(state, &attempt, &current).await;
        http_finished();
        result.map_err(|_| {
            if attempt.current(&state.peek()) && current(&state.peek()) {
                Failure::Failed
            } else {
                Failure::Refused
            }
        })
    };
    let (received, ()) = tokio::try_join!(read, subscription)?;
    stage::apply(state, &attempt, received, &current)
        .await
        .map_err(|_| {
            if attempt.current(&state.peek()) && current(&state.peek()) {
                Failure::Failed
            } else {
                Failure::Refused
            }
        })
}

pub(super) fn automatic_admissible(
    shell: &mut ShellState,
    team: &str,
    ids: &[String],
) -> Result<(), Error> {
    let attempt = Attempt::capture_mode(shell, team, ids, true)?;
    attempt.admit(shell, &|_| true)
}

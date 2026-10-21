//! Owned Profile acquisition of raw group rows for absent mentions or explicit
//! Retry. The opening owns the bounded request; this is not a shared group manager.
use super::{hydration::Progress, ownership::GroupRead};
use crate::{directory_member_fetches::Context, state::ShellState};
use dioxus::prelude::*;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use super_platinum_core::slack::{Error, models::UserGroup};

mod rate;
mod read;
pub(crate) use rate::{
    LIMITED, READY, RateStatus, poll_rate, rate_elapsed_ms, rate_snapshot, rate_status,
};
#[cfg(test)]
mod tests;

fn unavailable() -> Error {
    Error::Api("profile_group_recovery_unavailable".into())
}

struct Attempt {
    request: GroupRead,
    ids: Vec<String>,
    quiet: bool,
    progress: RefreshProgress,
}
impl Attempt {
    fn capture(shell: &ShellState, team: &str, ids: &[String]) -> Result<Self, Error> {
        let owner = Arc::new(Context::capture(shell, team)?);
        let mut seen = BTreeSet::new();
        let ids: Vec<_> = ids
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect();
        if ids.is_empty() {
            return Err(unavailable());
        }
        let request = GroupRead::capture(shell, owner, &ids).ok_or_else(unavailable)?;
        Ok(Self {
            request,
            ids,
            quiet: false,
            progress: RefreshProgress::default(),
        })
    }
    fn current(&self, shell: &ShellState, current: &impl Fn(&ShellState) -> bool) -> bool {
        self.request.current(shell) && current(shell)
    }
    fn publish(
        &self,
        shell: &mut ShellState,
        rows: Vec<UserGroup>,
        current: &impl Fn(&ShellState) -> bool,
    ) -> Result<bool, Error> {
        if !self.current(shell, current) || rows.iter().any(|row| row.label_source.is_none()) {
            return Err(unavailable());
        }
        let mut progress = Progress::default();
        if !progress.receive(shell, &self.request, Ok(rows)) {
            return Err(unavailable());
        }
        Ok(progress.should_persist())
    }
}

#[derive(Clone, Default)]
pub(super) struct RefreshProgress(std::rc::Rc<std::cell::RefCell<BTreeSet<String>>>);
impl RefreshProgress {
    fn admitted(&self, ids: &[String]) {
        self.0.borrow_mut().extend(ids.iter().cloned());
    }
    pub(super) fn ids(&self, original: &[String]) -> Vec<String> {
        let admitted = self.0.borrow();
        original
            .iter()
            .filter(|id| admitted.contains(*id))
            .cloned()
            .collect()
    }
}
enum RefreshSource {
    Legacy,
    Tracked(super_platinum_core::state::directory_usergroups::Snapshot),
}
impl RefreshSource {
    fn capture(shell: &ShellState, owner: &Context) -> Result<Self, Error> {
        let w = shell
            .core
            .workspaces
            .get(&owner.session.team_id)
            .ok_or_else(unavailable)?;
        if w.directory_usergroups.is_legacy() {
            return Ok(Self::Legacy);
        }
        w.directory_usergroups
            .snapshot(&owner.account, &owner.session, owner.epoch, &w.usergroups)
            .map(Self::Tracked)
            .map_err(|_| unavailable())
    }
    fn current(&self, shell: &ShellState, owner: &Context) -> bool {
        owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&owner.session.team_id)
                .is_some_and(|w| match self {
                    Self::Tracked(source) => source.current(
                        &w.directory_usergroups,
                        &owner.account,
                        &owner.session,
                        owner.epoch,
                        &w.usergroups,
                    ),
                    Self::Legacy => w.directory_usergroups.is_legacy(),
                })
    }
}
/// Same ordered acquisition and original GroupRead as explicit recovery, but
/// background freshness never changes the description's availability or errors.
pub(super) async fn refresh(
    state: Signal<ShellState>,
    owner: Arc<Context>,
    ids: &[String],
    progress: &RefreshProgress,
    current: impl Fn(&ShellState) -> bool,
) -> Result<(bool, usize), Error> {
    let request = GroupRead::capture(&state.peek(), owner, ids).ok_or_else(unavailable)?;
    let attempt = Attempt {
        request,
        ids: ids.to_vec(),
        quiet: true,
        progress: progress.clone(),
    };
    let source = RefreshSource::capture(&state.peek(), &attempt.request.owner)?;
    let received = read::acquire(state, &attempt, &current).await?;
    {
        let shell = state.peek();
        if !source.current(&shell, &attempt.request.owner) {
            return Err(unavailable());
        }
    }
    let errors = received.errors.len();
    let mut progress = Progress::default();
    if !progress
        .receive_observed(
            state,
            &attempt.request,
            Ok(received.rows),
            Some(received.ordered_rows),
            &current,
        )
        .await
    {
        return Err(unavailable());
    }
    Ok((progress.should_persist(), errors))
}

static STARTED: AtomicU64 = AtomicU64::new(0);
static FINISHED: AtomicU64 = AtomicU64::new(0);
struct Running;
impl Drop for Running {
    fn drop(&mut self) {
        FINISHED.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn fixture_status(_shell: &ShellState) -> Value {
    if !crate::profile_edit::sections::fixture::group_recovery::enabled() {
        return Value::Null;
    }
    let started = STARTED.load(Ordering::Relaxed);
    let finished = FINISHED.load(Ordering::Relaxed);
    serde_json::json!({"started":started,"finished":finished,"active":started.saturating_sub(finished)})
}

/// The caller selects unavailable references and retains the original unopened
/// description across this call. Actual partial rows may land; reopening must
/// still pass ordinary label capture for every original token.
pub(crate) async fn recover(
    mut state: Signal<ShellState>,
    team: &str,
    ids: &[String],
    current: impl Fn(&ShellState) -> bool,
) -> Result<(), Error> {
    if crate::fixture::is_fixture()
        && !crate::profile_edit::sections::fixture::group_recovery::enabled()
    {
        return Err(unavailable());
    }
    STARTED.fetch_add(1, Ordering::Relaxed);
    let _running = Running;
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    let attempt = Attempt::capture(&state.peek(), team, ids)?;
    let received = read::acquire(state, &attempt, &current).await?;
    let mut progress = Progress::default();
    if !progress
        .receive_observed(
            state,
            &attempt.request,
            Ok(received.rows),
            Some(received.ordered_rows),
            &current,
        )
        .await
    {
        return Err(unavailable());
    }
    let persist = progress.should_persist();
    if persist && attempt.request.owner.current(&state.peek()) && current(&state.peek()) {
        crate::bootstrap::persist_workspace_metadata(&state, team);
    }
    drop((received.raw, received.errors));
    // Read bookkeeping is not a label availability certificate. The caller
    // reopens through the existing full-token capture after this publication.
    Ok(())
}

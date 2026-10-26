//! Root-lived IO, cancel-safe subscriber leases, and private renderer staging.
use super::*;
use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
mod read;
mod stage;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use read::project_received;
pub(crate) use stage::Stage;

#[derive(Default)]
pub(super) struct Activity {
    started: AtomicU64,
    finished: AtomicU64,
}
struct Running(Arc<Activity>);
impl Drop for Running {
    fn drop(&mut self) {
        self.0.finished.fetch_add(1, Ordering::Relaxed);
    }
}
/// Closed fixture observation only: finished means the shared worker returned,
/// not that a renderer elected/adopted its receipt. No source authority here.
pub(crate) fn fixture_status(shell: &ShellState) -> Value {
    if !crate::profile_edit::sections::fixture::member_recovery::enabled()
        && !crate::profile_edit::sections::fixture::group_recovery::enabled()
        && !crate::slack_lists::comments::fixture::completion::title_users::named()
    {
        return Value::Null;
    }
    let activity = &shell.directory_member_fetches.activity;
    let started = activity.started.load(Ordering::Relaxed);
    let finished = activity.finished.load(Ordering::Relaxed);
    serde_json::json!({"started":started,"finished":finished,"active":started.saturating_sub(finished)})
}

/// Consumer lifetime only. Dropping this never cancels the shared IO task.
pub(crate) struct Client {
    state: Signal<ShellState>,
    subscription: Option<Subscription>,
}
impl Client {
    pub(crate) fn begin(
        mut state: Signal<ShellState>,
        team: &str,
        ids: &[String],
    ) -> Result<Option<Self>, Error> {
        if crate::fixture::is_fixture()
            && !crate::profile_edit::sections::fixture::member_recovery::enabled()
            && !crate::slack_lists::comments::fixture::completion::title_users::named()
        {
            let shell = state.peek();
            if crate::profile_edit::sections::fixture::completion::files::enabled(&shell)
                || crate::profile_edit::sections::fixture::group_recovery::enabled()
                || (crate::profile_edit::sections::fixture::section_reference::named()
                    && crate::profile_edit::sections::fixture::section_reference::case()
                        == "success")
            {
                // Mixed file/group reopening may need metadata while all USER rows
                // are already settled. This grants no member dispatcher access:
                // no subscription, slot, root task or network read is created.
                settled_without_member_io(&shell, team, ids)?;
                return Ok(None);
            }
            return Err(unavailable());
        }
        let subscription = register(&mut state.write(), team, ids)?;
        Ok(subscription.map(|subscription| Self {
            state,
            subscription: Some(subscription),
        }))
    }
    /// None means this subscriber's IDs reached canonical settlement. The
    /// consumer must still capture its own fresh Ready/eligibility observation.
    pub(crate) async fn next(
        &mut self,
        current: impl Fn(&ShellState) -> bool,
    ) -> Result<Option<Election>, Error> {
        let sub = self.subscription.as_mut().ok_or_else(unavailable)?;
        loop {
            let next = {
                let mut shell = self.state.write();
                if !current(&shell) {
                    return Err(unavailable());
                }
                match status(&shell, sub) {
                    Status::Complete => return Ok(None),
                    Status::Retired => return Err(unavailable()),
                    Status::Pending => {}
                }
                claim_adoption(&mut shell, sub)?
            };
            if let Some((adoption, outcome)) = next {
                return Ok(Some(Election {
                    state: self.state,
                    adoption: Some(adoption),
                    outcome,
                }));
            }
            kick(self.state, sub)?;
            sub.changed().await?;
        }
    }
}

fn settled_without_member_io(shell: &ShellState, team: &str, ids: &[String]) -> Result<(), Error> {
    let context = Context::capture(shell, team)?;
    let workspace = shell.core.workspaces.get(team).ok_or_else(unavailable)?;
    let store = &workspace.directory_members;
    let observed = store.observations(
        &context.account,
        &context.session,
        context.epoch,
        GENERATION,
    )?;
    let absent = Capture::network_only(observed, ids)?;
    if !absent.ids().is_empty() {
        return Err(unavailable());
    }
    let subset = store.subset(
        &context.account,
        &context.session,
        context.epoch,
        GENERATION,
        &ids.iter().cloned().collect(),
        &workspace.users,
    )?;
    if !context.current(shell)
        || !subset.current(
            store,
            &context.account,
            &context.session,
            context.epoch,
            GENERATION,
            &workspace.users,
        )
    {
        return Err(unavailable());
    }
    Ok(())
}
impl Drop for Client {
    fn drop(&mut self) {
        if let Some(sub) = self.subscription.take() {
            let mut state = self.state;
            // Normally unborrowed. A caller dropping inside a serial write
            // defers cleanup to the root instead of abandoning the claim.
            if let Ok(mut shell) = state.try_write() {
                retire(&mut shell, &sub);
                return;
            }
            dioxus::core::spawn_forever(async move {
                retire(&mut state.write(), &sub);
            });
        }
    }
}
pub(crate) struct Election {
    state: Signal<ShellState>,
    adoption: Option<Adoption>,
    pub(crate) outcome: Arc<Outcome>,
}
impl Election {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.adoption.as_ref().is_some_and(|a| a.current(shell)) && self.outcome.current(shell)
    }
}
impl Drop for Election {
    fn drop(&mut self) {
        if let Some(adoption) = self.adoption.take() {
            let mut state = self.state;
            if let Ok(mut shell) = state.try_write() {
                let _ = release_adoption(&mut shell, &adoption);
                return;
            }
            dioxus::core::spawn_forever(async move {
                let _ = release_adoption(&mut state.write(), &adoption);
            });
        }
    }
}

fn kick(mut state: Signal<ShellState>, sub: &Subscription) -> Result<(), Error> {
    let wake = {
        let mut shell = state.write();
        let slot = shell.directory_member_fetches.slot(&sub.context)?;
        if matches!(slot.retry_at, Some(None)) {
            return Err(unavailable());
        }
        arm(&mut shell, sub, Instant::now())?
    };
    let Some(wake) = wake else {
        return Ok(());
    };
    // Pinned Dioxus57d6794 spawn_forever uses Runtime::with_scope(ROOT).
    // Root Shell signal outlives every picker, dialog and subscriber.
    let activity = state.peek().directory_member_fetches.activity.clone();
    activity.started.fetch_add(1, Ordering::Relaxed);
    spawn_root(run(state, wake, Running(activity)));
    Ok(())
}
fn spawn_root(future: impl std::future::Future<Output = ()> + 'static) {
    dioxus::core::spawn_forever(future);
}
async fn run(mut state: Signal<ShellState>, wake: Wake, _running: Running) {
    let due = {
        let shell = state.peek();
        let Ok(slot) = shell.directory_member_fetches.slot(&wake.context) else {
            return;
        };
        match slot.retry_at {
            Some(None) => {
                drop(shell);
                let _ = cancel_wake(&mut state.write(), &wake);
                return;
            }
            Some(Some(deadline)) => wake.due().max(deadline),
            None => wake.due(),
        }
    };
    // Includes the source zero-delay first callback, permitting same-turn merge.
    tokio::time::sleep_until(due.into()).await;
    let started = {
        let mut shell = state.write();
        dispatch(&mut shell, &wake, Instant::now())
    };
    let dispatch = match started {
        Ok(value) => value,
        Err(_) => {
            let _ = cancel_wake(&mut state.write(), &wake);
            return;
        }
    };
    loop {
        if !dispatch.current(&state.peek()) {
            let _ = refuse(&mut state.write(), &dispatch);
            return;
        }
        let result = tokio::time::timeout(
            Duration::from_millis(ATTEMPT_TIMEOUT_MS),
            read::acquire(state, dispatch.clone()),
        )
        .await;
        let terminal = match result {
            Ok(Ok(received)) => received,
            Ok(Err(_)) => {
                // Chunk IO already retains its original failure. A local
                // aggregate refusal must not overwrite that evidence.
                let _ = refuse(&mut state.write(), &dispatch);
                return;
            }
            Err(_) => match {
                let mut shell = state.write();
                timeout(&mut shell, &dispatch)
            } {
                Ok(Timeout::Retry) => continue,
                Ok(Timeout::Exhausted) => read::Received::timed_out(dispatch.token.ids().to_vec()),
                Err(_) => {
                    let _ = refuse(&mut state.write(), &dispatch);
                    return;
                }
            },
        };
        if publish_observed_completed(
            &mut state.write(),
            dispatch.clone(),
            terminal.completed,
            terminal.terminal,
            terminal.pages,
            terminal.errors,
        )
        .is_err()
        {
            let _ = refuse(&mut state.write(), &dispatch);
        }
        return;
    }
}
fn cooling(shell: &ShellState, dispatch: &Dispatch) -> bool {
    shell
        .directory_member_fetches
        .slot(&dispatch.context)
        .is_ok_and(|slot| {
            slot.retry_at
                .is_some_and(|at| at.is_none_or(|at| at > Instant::now()))
        })
}
fn retain_failure(shell: &mut ShellState, dispatch: &Dispatch, error: Arc<Error>) {
    // Retain actual account-owned failure before any consumer query check.
    if !dispatch.context.current(shell) {
        return;
    }
    let Ok(slot) = shell.directory_member_fetches.slot_mut(&dispatch.context) else {
        return;
    };
    if let Error::RateLimited { retry_after_secs } = error.as_ref() {
        let deadline =
            Instant::now().checked_add(Duration::from_secs(retry_after_secs.unwrap_or(60)));
        slot.retry_at = Some(match slot.retry_at {
            Some(Some(old)) => deadline.map(|new| new.max(old)),
            Some(None) => None,
            None => deadline,
        });
    }
    slot.last_failure = Some(error.clone());
    shell.report_failure(error.as_ref(), "", || {
        "Member details could not be refreshed.".into()
    });
}

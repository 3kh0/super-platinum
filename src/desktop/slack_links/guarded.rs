//! Caller-owned channel navigation. A workflow occurrence grants only this
//! route; it cannot grant a delayed read permission to replace a newer view.
use super::*;
use crate::app_home::inputs::native::Release;
use std::{future::Future, pin::Pin, rc::Rc, task::Poll, time::Duration};
use super_platinum_core::error::NetworkFailure;
use super_platinum_core::slack::Error;

fn scoped_failure(shell: &mut ShellState, feedback: &FailureFeedback, message: &str) {
    if let Some(feedback) = feedback {
        feedback(shell, message);
    } else {
        failure(shell, feedback, message);
    }
}

#[derive(Clone)]
pub(crate) struct NavigationGuard {
    pub current: Rc<dyn Fn(&ShellState) -> bool>,
    pub lock: Rc<dyn Fn() -> Pin<Box<dyn Future<Output = Option<Release>>>>>,
    pub read: Option<ReadLifecycle>,
}

/// Optional caller feedback, scoped to an actually admitted missing read.
/// This does not reserve a native lease or grant request/navigation authority.
#[derive(Clone)]
pub(crate) struct ReadLifecycle {
    pub started: Rc<dyn Fn() -> Pin<Box<dyn Future<Output = bool>>>>,
    pub settled: Rc<dyn Fn()>,
}

struct PendingRead(Option<ReadLifecycle>);
impl PendingRead {
    async fn begin(lifecycle: Option<&ReadLifecycle>, current: impl Fn() -> bool) -> Option<Self> {
        if !current() {
            return None;
        }
        // Arm cleanup before awaiting: even a dropped acknowledgment future may
        // already have painted its original pending notice.
        let pending = Self(lifecycle.cloned());
        if let Some(lifecycle) = &pending.0
            && !(lifecycle.started)().await
        {
            return None;
        }
        current().then_some(pending)
    }
}
impl Drop for PendingRead {
    fn drop(&mut self) {
        if let Some(lifecycle) = &self.0 {
            (lifecycle.settled)();
        }
    }
}

struct NativeLease(Release);
impl Drop for NativeLease {
    fn drop(&mut self) {
        self.0.release();
    }
}

async fn commit_lease(
    initial: Option<NativeLease>,
    guard: &NavigationGuard,
) -> Option<NativeLease> {
    if let Some(lease) = initial {
        return Some(lease);
    }
    (guard.lock)().await.map(NativeLease)
}

struct Attempt {
    visit: Visit,
    guard: NavigationGuard,
}
impl Attempt {
    fn begin(
        shell: &mut ShellState,
        team: &str,
        channel: &str,
        guard: NavigationGuard,
    ) -> Result<Option<Self>, &'static str> {
        if !(guard.current)(shell) {
            return Ok(None);
        }
        // This route belongs to an existing caller in this workspace. It must
        // never switch accounts/workspaces while reserving its link intent.
        if shell.core.active_team.as_deref() != Some(team) {
            return Err(UNAVAILABLE);
        }
        if channel.is_empty()
            || channel.len() > 255
            || !channel
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err("This conversation link is invalid.");
        }
        let visit = Visit::begin(shell, &WorkspaceRef::TeamId(team.into()), false)?;
        Ok(Some(Self { visit, guard }))
    }

    fn current(&self, shell: &ShellState) -> bool {
        self.visit.current(shell) && (self.guard.current)(shell)
    }

    fn resolve(&self, shell: &mut ShellState, id: &str, info: Channel, revision: u64) -> bool {
        self.current(shell) && self.visit.resolve(shell, id, info, revision)
    }

    fn failed(&self, shell: &mut ShellState, feedback: &FailureFeedback, error: &Error) {
        if self.current(shell) {
            let offline = "Connect to Slack to open this conversation.";
            let detail = "Could not open this conversation. It may no longer be accessible.";
            if let Some(feedback) = feedback {
                feedback(shell, if error.is_offline() { offline } else { detail });
            } else {
                report_failure(shell, feedback, error, offline, detail);
            }
        }
    }

    fn select(&self, shell: &mut ShellState, channel: &str, workflows: bool) -> Option<Visit> {
        if !self.current(shell) || !select_target(shell, channel, None) {
            return None;
        }
        if workflows {
            // The explicit source route selects this destination without adding
            // a persistent tab record or creating a second navigation intent.
            shell.channel_tab = ChannelTab::Workflows;
            crate::channel_tabs::selection::record(
                shell,
                crate::channel_tabs::selection::Route::Resource(ChannelTab::Workflows),
                None,
            );
        }
        // From this point the original caller is intentionally no longer
        // current. History belongs to the successor conversation visit.
        Visit::capture(shell).ok()
    }
}

/// Check before every poll, including the poll that resumes an offline hold.
/// The timer exists only for this pending read, so dropping a retired future
/// cancels an otherwise quiet network/fixture wait without a global supervisor.
async fn while_current<T>(
    future: impl Future<Output = T>,
    current: impl Fn() -> bool,
) -> Option<T> {
    let mut future = std::pin::pin!(future);
    let guarded = std::future::poll_fn(|cx| {
        if !current() {
            return Poll::Ready(None);
        }
        future.as_mut().poll(cx).map(Some)
    });
    let retired = async {
        loop {
            if !current() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::select! {
        biased;
        _ = retired => None,
        result = guarded => result,
    }
}

pub(crate) async fn open_channel_guarded(
    state: Signal<ShellState>,
    team: String,
    channel: String,
    guard: NavigationGuard,
    feedback: FailureFeedback,
) {
    open_destination(state, team, channel, guard, feedback, false).await;
}

pub(crate) async fn open_channel_workflows_guarded(
    state: Signal<ShellState>,
    team: String,
    channel: String,
    guard: NavigationGuard,
    feedback: FailureFeedback,
) {
    open_destination(state, team, channel, guard, feedback, true).await;
}

async fn open_destination(
    mut state: Signal<ShellState>,
    team: String,
    channel: String,
    guard: NavigationGuard,
    feedback: FailureFeedback,
    workflows: bool,
) {
    if !(guard.current)(&state.peek()) {
        return;
    }
    let Some(release) = (guard.lock)().await else {
        return;
    };
    let mut initial = Some(NativeLease(release));
    // Even a no-op write borrow can render controlled inputs. A native lease
    // from a retired DOM must not schedule a render into its replacement.
    if !(guard.current)(&state.peek()) {
        return;
    }
    let attempt = {
        let mut shell = state.write();
        match Attempt::begin(&mut shell, &team, &channel, guard.clone()) {
            Ok(Some(attempt)) => attempt,
            Ok(None) => return,
            Err(message) => {
                if (guard.current)(&shell) {
                    scoped_failure(&mut shell, &feedback, message);
                }
                return;
            }
        }
    };
    let (known, revision, client) = {
        let shell = state.peek();
        if !attempt.current(&shell) {
            return;
        }
        let workspace = &shell.core.workspaces[&team];
        (
            workspace.channels.contains_key(&channel),
            workspace.channel_lifecycle.revision(),
            shell.core.client.clone(),
        )
    };
    // Ingress keeps the original pre-request authority. No native lock is held
    // during I/O, and fixture reads can never fall through to a real transport.
    let ingress = attempt.visit.transport.as_ref().map(|transport| {
        crate::bootstrap::posting_ingress::capture(&state, transport, &attempt.visit.session)
            .for_channels([channel.as_str()])
    });
    let source = if known {
        None
    } else {
        crate::app_home::channel_fetches::search::direct::Read::capture(
            state,
            &team,
            &channel,
            &|s| attempt.current(s),
        )
        .ok()
    };
    let mut pending_read = None;
    let result = if known {
        None
    } else {
        let Some(pending) = PendingRead::begin(attempt.guard.read.as_ref(), || {
            let shell = state.peek();
            attempt.current(&shell) && source.as_ref().is_none_or(|r| r.current(&shell))
        })
        .await
        else {
            return;
        };
        pending_read = Some(pending);
        // Known selection has no I/O and keeps this original lease. Only a
        // missing-channel read releases it; its final native acquisition waits
        // for that exact previous lease to close before checking fresh buffers.
        drop(initial.take());
        let read = async {
            if crate::fixture::is_fixture() {
                crate::app_home::about::management::activity::fixture::channel::fetch_raw(
                    state,
                    &client,
                    &attempt.visit.session,
                    &channel,
                )
                .await
            } else if let Some(transport) = attempt.visit.transport.as_ref() {
                api::fetch_conversation_info_raw(
                    transport,
                    &client,
                    &attempt.visit.session,
                    channel.clone(),
                )
                .await
            } else {
                Err(Error::Offline("No current Slack transport".into()))
            }
        };
        let Some(result) = while_current(read, || {
            let shell = state.peek();
            attempt.current(&shell) && source.as_ref().is_none_or(|r| r.current(&shell))
        })
        .await
        else {
            return;
        };
        Some(result)
    };
    if !attempt.current(&state.peek()) {
        return;
    }
    let mut canonical = None;
    let mut legacy = None;
    let mut failure = None;
    if let Some(result) = result {
        match result {
            Ok(raw) if raw["id"].as_str() == Some(channel.as_str()) => {
                if !attempt.current(&state.peek()) {
                    return;
                }
                if let Some(source) = source {
                    // Publish the original row through the shared processor
                    // before acquiring the final native lease. Never overwrite
                    // its paired-native result with a typed wire projection.
                    match source.publish(state, raw, |s| attempt.current(s)).await {
                        Ok(published) => canonical = Some(published),
                        Err(error) => failure = Some(error),
                    }
                } else {
                    match serde_json::from_value::<Channel>(raw) {
                        Ok(info) => legacy = Some(info),
                        Err(_) => {
                            failure = Some(Error::Api("conversation_info_response_invalid".into()));
                        }
                    }
                }
            }
            Ok(_) => {
                failure = Some(Error::Api("conversation_info_id_mismatch".into()));
            }
            Err(error) => failure = Some(error),
        }
    }
    let Some(commit) = commit_lease(initial, &attempt.guard).await else {
        return;
    };
    {
        let shell = state.peek();
        if !attempt.current(&shell) || canonical.as_ref().is_some_and(|c| !c.current(&shell)) {
            return;
        }
    }
    if let Some(error) = failure {
        attempt.failed(&mut state.write(), &feedback, &error);
        return;
    }
    if let Some(info) = legacy {
        if let Some(ingress) = ingress {
            ingress.channels(&state, std::slice::from_ref(&info));
        }
        if !attempt.resolve(&mut state.write(), &channel, info, revision) {
            return;
        }
    }
    let successor = {
        let mut shell = state.write();
        let successor = attempt.select(&mut shell, &channel, workflows);
        if successor.is_none() && attempt.current(&shell) {
            scoped_failure(&mut shell, &feedback, "This conversation is unavailable.");
        }
        successor
    };
    drop(commit);
    drop(pending_read);
    if let Some(successor) = successor
        && successor.current(&state.peek())
    {
        if !workflows {
            crate::bootstrap::refresh_selected_channel(state).await;
        }
    }
}

#[cfg(test)]
mod tests;

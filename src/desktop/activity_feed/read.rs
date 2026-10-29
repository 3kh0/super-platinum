//! Exact Activity read recovery, independent of filtered display rows.
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use std::{future::Future, sync::Arc};
use super_platinum_core::{
    activity_feed::{ActivityRequest, ActivityRetry},
    config::WorkspaceSession,
    slack::{Error, SlackClient, Transport, api, models::ActivityFeedPage},
};

#[derive(Clone, Copy)]
pub(crate) enum Mode {
    Refresh,
    Older,
    Retry,
    Resume,
}

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    scoped: bool,
    sequence: u64,
    loading: bool,
    loaded: bool,
    failed: bool,
    next: Option<String>,
    retry: Option<ActivityRetry>,
}
impl Stamp {
    fn capture(shell: &ShellState, team: &str) -> Self {
        let feed = &shell.core.activity;
        Self {
            scoped: feed.matches_scope(shell.core.account_epoch, team),
            sequence: feed.load_seq,
            loading: feed.loading,
            loaded: feed.loaded,
            failed: feed.failed,
            next: feed.next_cursor.clone(),
            retry: feed.failed_read().cloned(),
        }
    }
}

// Native credentials are never Debug, projected, serialized or persisted here.
#[derive(Clone)]
pub(crate) struct Source {
    owner: Arc<crate::bootstrap::ReadOwner>,
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    epoch: u64,
    team: String,
    thread_generation: u64,
    stamp: Stamp,
    reconnect: Option<crate::bootstrap::reconnect::Visit>,
}
impl Source {
    pub(crate) fn capture(shell: &ShellState) -> Option<Self> {
        Self::capture_with(shell, None)
    }
    fn capture_with(
        shell: &ShellState,
        reconnect: Option<crate::bootstrap::reconnect::Visit>,
    ) -> Option<Self> {
        if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() || !shell.signed_in {
            return None;
        }
        let team = shell.core.active_team.clone()?;
        let transport = shell.core.transport.clone()?;
        let session = shell.core.session.as_ref()?.workspaces.get(&team)?.clone();
        let owner = Arc::new(crate::bootstrap::ReadOwner::capture(
            &shell.core,
            &transport,
            &session,
        )?);
        let source = Self {
            owner,
            transport,
            client: shell.core.client.clone(),
            session,
            epoch: shell.core.account_epoch,
            stamp: Stamp::capture(shell, &team),
            team,
            thread_generation: shell.thread_generation,
            reconnect,
        };
        source.visit_current(shell).then_some(source)
    }
    fn route_current(&self, shell: &ShellState) -> bool {
        shell.signed_in
            && shell.core.active_team.as_ref() == Some(&self.team)
            && self.owner.current(&shell.core)
            && shell.main_view == MainView::Activity
            && shell.thread_generation == self.thread_generation
    }
    fn visit_current(&self, shell: &ShellState) -> bool {
        self.route_current(shell) && self.reconnect.as_ref().is_none_or(|v| v.current(shell))
    }
    fn admitted_current(&self, shell: &ShellState) -> bool {
        self.route_current(shell)
            && self
                .reconnect
                .as_ref()
                .is_none_or(|v| v.admitted_current(shell))
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.visit_current(shell)
            && self.stamp == Stamp::capture(shell, &self.team)
            && !self.stamp.loading
    }
    pub(crate) fn start(self, state: Signal<ShellState>, mode: Mode) {
        if !state.try_peek().is_ok_and(|s| self.current(&s)) {
            return;
        }
        dioxus::core::spawn_forever(self.run(state, mode));
    }
    async fn run(self, state: Signal<ShellState>, mode: Mode) {
        if let Some(sequence) = self.run_with(state, mode, fetch).await {
            if state
                .try_peek()
                .is_ok_and(|s| self.admitted_current(&s) && s.core.activity.load_seq == sequence)
            {
                crate::bootstrap::hydrate_current_surface(state).await;
            }
        }
    }
    async fn run_with<F, Fut>(
        &self,
        mut state: Signal<ShellState>,
        mode: Mode,
        mut fetch: F,
    ) -> Option<u64>
    where
        F: FnMut(Request) -> Fut,
        Fut: Future<Output = Result<ActivityFeedPage, Error>>,
    {
        if !state.try_peek().is_ok_and(|s| self.current(&s)) {
            return None;
        }
        let retry = matches!(mode, Mode::Retry)
            || (matches!(mode, Mode::Resume) && self.stamp.retry.is_some());
        // Recovery retries exactly one failed page. Only a new explicit/initial
        // refresh performs the existing bounded unread-fill (at most five pages).
        let fill = !retry && matches!(mode, Mode::Refresh | Mode::Resume);
        let target = state
            .peek()
            .core
            .workspaces
            .get(&self.team)
            .and_then(|w| w.activity_unread_count)
            .unwrap_or(0);
        let mut completed = 0;
        loop {
            let request = {
                let mut shell = state.try_write().ok()?;
                if !self.admitted_current(&shell) {
                    return None;
                }
                shell.core.activity.ensure_scope(self.epoch, &self.team);
                if retry && completed == 0 {
                    shell.core.activity.begin_retry()
                } else {
                    shell
                        .core
                        .activity
                        .begin(matches!(mode, Mode::Older) || completed > 0)
                }?
            };
            let _lease = Lease {
                state,
                request: request.clone(),
            };
            let result = fetch(Request {
                transport: self.transport.clone(),
                client: self.client.clone(),
                session: self.session.clone(),
                cursor: request.cursor.clone(),
            })
            .await;
            let (sequence, more) = {
                let mut shell = state.try_write().ok()?;
                if !self.admitted_current(&shell) || !shell.core.activity.accepts(&request) {
                    shell.core.activity.cancel(&request);
                    return None;
                }
                match result {
                    Ok(page) => {
                        if !shell.core.activity.apply_page(&request, page) {
                            return None;
                        }
                        completed += 1;
                        (
                            shell.core.activity.load_seq,
                            fill && completed < 5
                                && shell.core.activity.next_cursor.is_some()
                                && shell.core.activity.unread_count() < target,
                        )
                    }
                    Err(error) => {
                        shell.core.activity.fail(&request);
                        shell.report_failure(&error, "Activity needs a connection", || {
                            "Could not load activity. Please try again.".into()
                        });
                        return None;
                    }
                }
            };
            if !more {
                return Some(sequence);
            }
        }
    }
}

struct Lease {
    state: Signal<ShellState>,
    request: ActivityRequest,
}
impl Drop for Lease {
    fn drop(&mut self) {
        // Request pointer + scope + sequence authorize only local release.
        // Conflicting-borrow/disposed-signal cleanup remains best-effort.
        if let Ok(mut shell) = self.state.try_write() {
            shell.core.activity.cancel(&self.request);
        }
    }
}

struct Request {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    cursor: Option<String>,
}
async fn fetch(request: Request) -> Result<ActivityFeedPage, Error> {
    api::fetch_activity_feed(
        &request.transport,
        &request.client,
        &request.session,
        20,
        request.cursor,
        false,
    )
    .await
}
pub async fn load(state: Signal<ShellState>, append: bool) {
    let source = { state.try_peek().ok().and_then(|s| Source::capture(&s)) };
    if let Some(source) = source {
        source
            .run(state, if append { Mode::Older } else { Mode::Refresh })
            .await;
    }
}
pub(crate) async fn load_reconnect(
    state: Signal<ShellState>,
    visit: crate::bootstrap::reconnect::Visit,
) {
    let source = {
        state
            .try_peek()
            .ok()
            .and_then(|s| Source::capture_with(&s, Some(visit)))
    };
    if let Some(source) = source {
        source.run(state, Mode::Resume).await;
    }
}

/// Called only from the resource-detail early-return branch. Capture now,
/// before its resource loader can yield/change the original visit.
pub(crate) fn schedule_failed_after_outage(
    state: Signal<ShellState>,
    visit: crate::bootstrap::reconnect::Visit,
) {
    let source = {
        state.try_peek().ok().and_then(|shell| {
            let team = shell.core.active_team.as_deref()?;
            let feed = &shell.core.activity;
            if !feed.matches_scope(shell.core.account_epoch, team)
                || feed.failed_read().is_none()
                || feed.loading
            {
                return None;
            }
            Source::capture_with(&shell, Some(visit))
        })
    };
    if let Some(source) = source {
        source.start(state, Mode::Resume);
    }
}

#[cfg(test)]
mod tests;

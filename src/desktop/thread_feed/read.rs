//! Captured followed-Threads reads; exact failed coordinate and original cursor owner.
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use std::{future::Future, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, SlackClient, Transport, api, models::ThreadsViewPage},
    thread_feed::FeedRequest,
};

#[derive(Default)]
pub(crate) struct Reads {
    incarnation: Arc<()>,
    pending: Option<Arc<Ticket>>,
}
struct Ticket {
    epoch: u64,
    team: String,
    request: FeedRequest,
}
impl Ticket {
    fn owns(&self, shell: &ShellState) -> bool {
        shell
            .thread_feed_reads
            .pending
            .as_ref()
            .is_some_and(|p| std::ptr::eq(p.as_ref(), self))
    }
    fn active(&self, shell: &ShellState) -> bool {
        self.owns(shell)
            && shell
                .core
                .threads_view
                .matches_scope(self.epoch, &self.team)
            && shell.core.threads_view.accepts(&self.request)
    }
    fn release(&self, shell: &mut ShellState) {
        if !self.owns(shell) {
            return;
        }
        if shell
            .core
            .threads_view
            .matches_scope(self.epoch, &self.team)
        {
            shell.core.threads_view.cancel(&self.request);
        }
        shell.thread_feed_reads.pending = None;
    }
}
struct Lease {
    state: Signal<ShellState>,
    ticket: Arc<Ticket>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        // Pointer authorizes local cleanup only, including token retirement.
        // Conflicting signal borrow/disposal is still best-effort.
        if let Ok(mut shell) = self.state.try_write() {
            self.ticket.release(&mut shell);
        }
    }
}
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
    has_more: bool,
    cursor: Option<String>,
    retry: Option<FeedRequest>,
}
impl Stamp {
    fn capture(shell: &ShellState, team: &str) -> Self {
        let feed = &shell.core.threads_view;
        Self {
            scoped: feed.matches_scope(shell.core.account_epoch, team),
            sequence: feed.load_seq,
            loading: feed.loading,
            loaded: feed.loaded,
            failed: feed.failed,
            has_more: feed.has_more,
            cursor: feed.next_cursor.clone(),
            retry: feed.failed_read().cloned(),
        }
    }
}
// Never Debug/serialize/project credentials or the original native read owner.
#[derive(Clone)]
pub(crate) struct Source {
    root: Arc<()>,
    owner: Arc<crate::bootstrap::ReadOwner>,
    cursor_owner: Arc<crate::realtime::activity_counts::CursorOwner>,
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
        let cursor_owner = Arc::new(crate::realtime::activity_counts::CursorOwner::capture(
            &shell.core,
            &transport,
            &session,
        )?);
        let source = Self {
            root: shell.thread_feed_reads.incarnation.clone(),
            owner,
            cursor_owner,
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
            && shell.main_view == MainView::Threads
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell.thread_generation == self.thread_generation
            && Arc::ptr_eq(&self.root, &shell.thread_feed_reads.incarnation)
            && self.owner.current(&shell.core)
            && self.cursor_owner.current(&shell.core)
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
            let current = state.try_peek().is_ok_and(|s| {
                self.admitted_current(&s)
                    && s.core.threads_view.matches_scope(self.epoch, &self.team)
                    && s.core.threads_view.load_seq == sequence
                    && s.core.threads_view.loaded
                    && !s.core.threads_view.loading
            });
            if current {
                crate::bootstrap::persist_workspace(&state, &self.team);
                crate::bootstrap::hydrate_current_surface(state).await;
            }
        }
    }
    async fn run_with<F, Fut>(
        &self,
        mut state: Signal<ShellState>,
        mode: Mode,
        fetch: F,
    ) -> Option<u64>
    where
        F: FnOnce(Request) -> Fut,
        Fut: Future<Output = Result<ThreadsViewPage, Error>>,
    {
        let (ticket, cursors, counts_revision) = {
            let mut shell = state.try_write().ok()?;
            if !self.current(&shell) || (matches!(mode, Mode::Retry) && !self.stamp.scoped) {
                return None;
            }
            shell.core.threads_view.ensure_scope(self.epoch, &self.team);
            let retry = matches!(mode, Mode::Retry)
                || (matches!(mode, Mode::Resume)
                    && self.stamp.scoped
                    && self.stamp.retry.is_some());
            let request = if retry {
                shell.core.threads_view.begin_retry()
            } else {
                shell.core.threads_view.begin(matches!(mode, Mode::Older))
            }?;
            let cursors = shell
                .core
                .threads
                .iter()
                .filter(|(key, _)| key.0 == self.team)
                .map(|(key, bag)| (key.clone(), bag.read_revision))
                .collect();
            let counts_revision = shell.core.workspaces[&self.team].thread_counts.revision();
            let ticket = Arc::new(Ticket {
                epoch: self.epoch,
                team: self.team.clone(),
                request,
            });
            shell.thread_feed_reads.pending = Some(ticket.clone());
            (ticket, cursors, counts_revision)
        };
        let _lease = Lease {
            state,
            ticket: ticket.clone(),
        };
        let result = fetch(Request {
            transport: self.transport.clone(),
            client: self.client.clone(),
            session: self.session.clone(),
            cursor: ticket.request.cursor.clone(),
        })
        .await;
        let mut shell = state.try_write().ok()?;
        if !self.admitted_current(&shell) || !ticket.active(&shell) {
            ticket.release(&mut shell);
            return None;
        }
        match result {
            Ok(mut page) => {
                let counts = page.threads_state.take();
                let keys = page
                    .threads
                    .iter()
                    .filter_map(|item| {
                        Some((
                            self.team.clone(),
                            item.channel()?.clone(),
                            item.root_ts()?.clone(),
                        ))
                    })
                    .collect::<Vec<_>>();
                if !shell
                    .core
                    .apply_thread_feed_page(&self.team, &ticket.request, page, &cursors)
                {
                    return None;
                }
                super::reconcile_cursors(&mut shell.core, &keys);
                if let Some(counts) = counts {
                    shell
                        .core
                        .workspaces
                        .get_mut(&self.team)
                        .unwrap()
                        .thread_counts
                        .apply_snapshot(counts, counts_revision);
                }
                Some(shell.core.threads_view.load_seq)
            }
            Err(error) => {
                // All original native/cursor ownership and Core acceptance
                // precede both failure settlement and visible error reporting.
                shell.core.threads_view.fail(&ticket.request);
                shell.report_failure(&error, "Threads need a connection", || {
                    "Could not load threads. Please try again.".into()
                });
                None
            }
        }
    }
}
struct Request {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    cursor: Option<String>,
}
async fn fetch(request: Request) -> Result<ThreadsViewPage, Error> {
    api::fetch_threads_view(
        &request.transport,
        &request.client,
        &request.session,
        30,
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

#[cfg(test)]
mod tests;

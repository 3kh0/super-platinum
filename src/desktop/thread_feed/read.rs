//! Captured followed-Threads reads; exact failed coordinate and original cursor owner.
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, SlackClient, Transport, api, models::ThreadsViewPage},
    thread_feed::FeedRequest,
};

#[derive(Default)]
pub(crate) struct Reads {
    incarnation: Arc<()>,
    pending: Option<Arc<Ticket>>,
    accepted: Option<Arc<row::Accepted>>,
    owned_sequence: Option<u64>,
    pub(super) presentation: Option<super::unread::Presentation>,
    bulk: bulk::State,
    profiles: Option<Arc<profiles::Scope>>,
    document_names: Option<Arc<document_names::Scope>>,
    windows: Option<Arc<windows::Scope>>,
    context: Option<Arc<windows::ContextScope>>,
    earlier: Option<Arc<earlier::Scope>>,
    applied_read_barrier: Option<super_platinum_core::slack::realtime::ReadBarrier>,
    pub(super) earlier_layout: bool,
    copy_link: copy_link::State,
    timestamp: timestamp::State,
    reaction_toggle: reaction_toggle::State,
}
mod bulk;
pub(crate) mod copy_link;
mod document_names;
mod earlier;
mod profiles;
pub(crate) mod reaction_toggle;
mod row;
pub(crate) mod timestamp;
mod windows;
pub(crate) use bulk::{
    Action as MarkAll, has_unreads as has_unread_threads, status as mark_all_status,
};
pub(crate) use document_names::{document_author_id, schedule as schedule_document_names};
pub(crate) use earlier::accept_barrier as accept_read_barrier;
pub(crate) use profiles::Member as ProfileMember;
pub(crate) use profiles::Open as ProfileOpen;
pub(crate) use profiles::publish_refresh as publish_profile_refresh;
pub(crate) use windows::{ContextPrepared, finish_context, prepare_context};
pub(crate) fn reply_visible_limit(
    shell: &ShellState,
    item: &super_platinum_core::slack::models::ThreadViewItem,
) -> Option<usize> {
    windows::context_visible_limit(shell, item).or_else(|| windows::visible_limit(shell, item))
}
pub(crate) fn unread_marker(
    shell: &ShellState,
    item: &super_platinum_core::slack::models::ThreadViewItem,
    replies: &[&str],
) -> Option<(usize, bool)> {
    let cue = if let Some(accepted) = shell.thread_feed_reads.accepted.as_ref() {
        accepted.unread_marker(shell, item, replies)?
    } else {
        shell
            .thread_feed_reads
            .presentation
            .as_ref()?
            .cue(shell, item, replies)?
    };
    Some((cue.index, cue.ghost))
}
pub(super) fn retire_after_native_mutation(shell: &ShellState) {
    if let Some(accepted) = shell.thread_feed_reads.accepted.as_ref() {
        accepted.retire_missing(shell);
    }
    if let Some(windows) = shell.thread_feed_reads.windows.as_ref() {
        windows.retire_missing(shell);
    }
    profiles::retire_deferred(shell);
    document_names::retire_missing(shell);
    windows::retire_contexts(shell);
    earlier::retire_missing(shell);
}
struct Ticket {
    epoch: u64,
    team: String,
    request: FeedRequest,
    settled: AtomicBool,
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
        let active = self.active(shell);
        if shell
            .core
            .threads_view
            .matches_scope(self.epoch, &self.team)
        {
            shell.core.threads_view.cancel(&self.request);
        }
        // An externally settled public Core request is not a native row
        // publication. Exact pointer ownership authorizes local release only.
        if !active
            && !self.settled.load(Ordering::Acquire)
            && shell.thread_feed_reads.owned_sequence == Some(self.request.sequence)
        {
            shell.thread_feed_reads.owned_sequence = None;
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
    pub(crate) fn root_first_add(
        &self,
        shell: &ShellState,
        item: &super_platinum_core::slack::models::ThreadViewItem,
    ) -> Option<reaction_toggle::first_add::Open> {
        reaction_toggle::first_add::opener(self, shell, item)
    }
    pub(crate) fn root_reaction(
        &self,
        shell: &ShellState,
        item: &super_platinum_core::slack::models::ThreadViewItem,
        name: &str,
    ) -> Option<reaction_toggle::Action> {
        reaction_toggle::action(self, shell, item, name)
    }
    pub(crate) fn timestamp(
        &self,
        shell: &ShellState,
        item: &super_platinum_core::slack::models::ThreadViewItem,
    ) -> Option<timestamp::Action> {
        timestamp::action(self, shell, item)
    }
    pub(crate) fn copy_link(
        &self,
        shell: &ShellState,
        item: &super_platinum_core::slack::models::ThreadViewItem,
    ) -> Option<copy_link::Action> {
        copy_link::action(self, shell, item)
    }
    pub(crate) fn reading_member_name(&self, shell: &ShellState, id: &str) -> Option<String> {
        profiles::reading_name(self, shell, id)
    }
    pub(crate) fn reading_document_name(
        &self,
        shell: &ShellState,
        item: &super_platinum_core::slack::models::ThreadViewItem,
        message: &super_platinum_core::slack::models::Message,
    ) -> Option<String> {
        document_names::label(self, shell, item, message)
    }
    pub(crate) fn earlier_replies(
        &self,
        shell: &ShellState,
        item: &super_platinum_core::slack::models::ThreadViewItem,
    ) -> Option<windows::Action> {
        windows::Action::capture(self, shell, item)
    }
    pub(crate) fn network_earlier_replies(
        &self,
        shell: &ShellState,
        item: &super_platinum_core::slack::models::ThreadViewItem,
    ) -> Option<earlier::Action> {
        earlier::Action::capture(self, shell, item)
    }
    pub(crate) fn profile(
        &self,
        shell: &ShellState,
        item: &super_platinum_core::slack::models::ThreadViewItem,
        message: &super_platinum_core::slack::models::Message,
    ) -> Option<ProfileOpen> {
        profiles::Open::capture(self, shell, item, message)
    }
    pub(crate) fn row(
        &self,
        shell: &ShellState,
        item: &super_platinum_core::slack::models::ThreadViewItem,
    ) -> Option<row::Row> {
        row::Row::capture(self, shell, item)
    }
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
                document_names::schedule(state);
                profiles::load(self.clone(), state).await;
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
        let (ticket, cursors, counts_revision, previous_markers, copy_carry, timestamp_carry) = {
            let mut shell = state.try_write().ok()?;
            if !self.current(&shell) || (matches!(mode, Mode::Retry) && !self.stamp.scoped) {
                return None;
            }
            // Do not re-authorize a public/untracked Core replacement merely
            // by starting another native request at a new scalar coordinate.
            if shell
                .thread_feed_reads
                .accepted
                .as_ref()
                .is_some_and(|p| !p.current(&shell))
            {
                shell.thread_feed_reads.accepted = None;
            }
            // Retain visual memory only from a record just checked above.
            // It carries no row capability or message/canonical body.
            let copy_carry = copy_link::carry_roster(&shell);
            let timestamp_carry = timestamp::carry_roster(&shell);
            let previous_markers = shell
                .thread_feed_reads
                .accepted
                .as_ref()
                .map(|p| p.markers.clone());
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
                settled: AtomicBool::new(false),
            });
            shell.thread_feed_reads.owned_sequence = Some(ticket.request.sequence);
            shell.thread_feed_reads.pending = Some(ticket.clone());
            let previous_markers = ticket.request.cursor.as_ref().and(previous_markers);
            (
                ticket,
                cursors,
                counts_revision,
                previous_markers,
                copy_carry,
                timestamp_carry,
            )
        };
        let _lease = Lease {
            state,
            ticket: ticket.clone(),
        };
        let reaction_raw = reaction_toggle::Sink::default();
        let first_add_raw = reaction_toggle::first_add::Sink::default();
        let result = fetch(Request {
            transport: self.transport.clone(),
            client: self.client.clone(),
            session: self.session.clone(),
            cursor: ticket.request.cursor.clone(),
            reaction_raw: reaction_raw.clone(),
            first_add_raw: first_add_raw.clone(),
        })
        .await;
        let mut shell = state.try_write().ok()?;
        if !self.admitted_current(&shell) || !ticket.active(&shell) {
            ticket.release(&mut shell);
            return None;
        }
        match result {
            Ok(mut page) => {
                let copy_candidates = copy_link::candidates(&page);
                let timestamp_candidates = timestamp::candidates(&page);
                let reply_windows = windows::Candidates::from_page(&page, &self.team);
                let earlier_candidates = earlier::Candidates::from_page(&page, &self.team);
                let profile_candidates = profiles::Candidates::from_page(&page, &self.team);
                let document_candidates = document_names::Candidates::from_page(&page, &self.team);
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
                if ticket.request.cursor.is_none() {
                    bulk::after_refresh(&mut shell);
                }
                if let Some(counts) = counts {
                    shell
                        .core
                        .workspaces
                        .get_mut(&self.team)
                        .unwrap()
                        .thread_counts
                        .apply_snapshot(counts, counts_revision);
                }
                // Only this successful original native-ticket acceptance can
                // publish row ownership. Every success retires the old record,
                // even byte-equal data or unavailable new qualification.
                shell.thread_feed_reads.accepted =
                    row::Accepted::capture(self, &shell, previous_markers.as_deref()).map(Arc::new);
                shell.thread_feed_reads.profiles = profiles::Scope::capture(
                    self,
                    &shell,
                    profile_candidates,
                    ticket.request.cursor.is_some(),
                )
                .map(Arc::new);
                shell.thread_feed_reads.document_names = document_names::Scope::capture(
                    self,
                    &shell,
                    document_candidates,
                    ticket.request.cursor.is_some(),
                )
                .map(Arc::new);
                shell.thread_feed_reads.windows = windows::Scope::capture(
                    self,
                    &shell,
                    reply_windows,
                    ticket.request.cursor.is_some(),
                )
                .map(Arc::new);
                shell.thread_feed_reads.earlier = earlier::Scope::capture(
                    self,
                    &shell,
                    earlier_candidates,
                    ticket.request.cursor.is_some(),
                )
                .map(Arc::new);
                let copy_carry = ticket.request.cursor.as_ref().and(copy_carry);
                copy_link::publish(self, &mut shell, &ticket, copy_candidates, copy_carry);
                let timestamp_carry = ticket.request.cursor.as_ref().and(timestamp_carry);
                timestamp::publish(
                    self,
                    &mut shell,
                    &ticket,
                    timestamp_candidates,
                    timestamp_carry,
                );
                reaction_toggle::publish(self, &mut shell, &reaction_raw);
                reaction_toggle::first_add::publish(self, &mut shell, &first_add_raw);
                windows::context_page(&mut shell, &keys, ticket.request.cursor.is_some());
                ticket.settled.store(true, Ordering::Release);
                Some(shell.core.threads_view.load_seq)
            }
            Err(error) => {
                // All original native/cursor ownership and Core acceptance
                // precede both failure settlement and visible error reporting.
                shell.core.threads_view.fail(&ticket.request);
                ticket.settled.store(true, Ordering::Release);
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
    reaction_raw: reaction_toggle::Sink,
    first_add_raw: reaction_toggle::first_add::Sink,
}
async fn fetch(request: Request) -> Result<ThreadsViewPage, Error> {
    let transport = request.transport.clone();
    let sink = request.reaction_raw;
    let first_add_sink = request.first_add_raw;
    api::fetch_threads_view_with(
        &request.client,
        &request.session,
        30,
        request.cursor,
        false,
        |prepared| async move {
            let raw = reaction_toggle::get_view(transport, prepared, sink).await?;
            first_add_sink.observe(&raw);
            Ok(raw)
        },
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

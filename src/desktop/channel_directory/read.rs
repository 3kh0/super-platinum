use super::*;
use std::future::Future;

#[derive(Clone, Copy)]
pub(super) enum Mode {
    Search,
    Refresh,
    Next,
    Previous,
    Retry,
}
pub(super) struct Ticket {
    pub(super) active: AtomicBool,
    pub(super) revision: u64,
    query: Query,
    realtime: u64,
    channels: u64,
}
impl Ticket {
    fn owns(&self, s: &ShellState) -> bool {
        s.channel_directory
            .pending
            .as_ref()
            .is_some_and(|p| std::ptr::eq(p.as_ref(), self))
    }
}
pub(super) struct Lease {
    state: Signal<ShellState>,
    ticket: Arc<Ticket>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.ticket.active.store(false, Ordering::Release);
        if let Ok(mut s) = self.state.try_write() {
            if self.ticket.owns(&s) {
                if let Some(entry) = s.channel_directory.entry.clone() {
                    entry.recover_stopped_read(&mut s, false);
                }
                s.channel_directory.pending = None;
            }
        }
    }
}

pub(super) struct Request {
    state: Signal<ShellState>,
    entry: Entry,
    ticket: Arc<Ticket>,
    reconnect: Option<crate::bootstrap::reconnect::Visit>,
}
impl Request {
    fn current_shell(&self, s: &ShellState) -> bool {
        let q = &self.ticket.query;
        self.entry.current(s)
            && self.ticket.owns(s)
            && self.ticket.active.load(Ordering::Acquire)
            && s.channel_directory.revision == self.ticket.revision
            && s.channel_directory.draft == q.text
            && s.channel_directory.kind == q.kind
            && s.channel_directory.sort() == q.sort
            && self
                .reconnect
                .as_ref()
                .is_none_or(|v| v.admitted_current(s))
            && s.core
                .workspaces
                .get(&self.entry.family.session.team_id)
                .is_some_and(|w| {
                    w.rt_generation == self.ticket.realtime
                        && w.channel_lifecycle.revision() == self.ticket.channels
                })
    }
    pub(super) fn current(&self) -> bool {
        self.state.try_peek().is_ok_and(|s| self.current_shell(&s))
    }
    pub(super) fn query(&self) -> &Query {
        &self.ticket.query
    }
}
async fn fetch(request: Request) -> Result<Option<Page>, Error> {
    if !request.current() {
        return Ok(None);
    }
    let f = &request.entry.family;
    let prepared = api::request(&f.client, &f.session, &request.ticket.query)?;
    let result = f
        .transport
        .execute_admitted(
            prepared,
            || request.current(),
            || f.ready(),
            |result| f.observe(result),
        )
        .await?;
    result
        .map(|v| api::decode(v, &request.ticket.query))
        .transpose()
}

impl Entry {
    // Retain retry intent only; this never publishes data or grants authority.
    // A restore retires the interrupted request before reserving its successor.
    pub(super) fn recover_stopped_read(&self, s: &mut ShellState, restore: bool) {
        if !self.current(s) {
            return;
        }
        let pending = s.channel_directory.pending.clone().filter(|p| {
            p.revision == s.channel_directory.revision
                && p.query.text == s.channel_directory.draft
                && p.query.kind == s.channel_directory.kind
                && p.query.sort == s.channel_directory.sort()
                && (restore || !p.active.load(Ordering::Acquire))
        });
        if let Some(pending) = pending {
            pending.active.store(false, Ordering::Release);
            s.channel_directory.failed = Some(pending.query.clone());
            s.channel_directory.error = Some("Loading was interrupted. Refresh to try again.");
            s.channel_directory.pending = None;
        }
    }
    fn coordinate(&self, s: &ShellState, mode: Mode) -> Option<Query> {
        let d = &s.channel_directory;
        match mode {
            Mode::Search => Some(d.query()),
            Mode::Retry => d
                .failed
                .clone()
                .filter(|q| q.text == d.draft && q.kind == d.kind && q.sort == d.sort()),
            Mode::Refresh => Some(
                d.loaded
                    .as_ref()
                    .filter(|p| p.current(s))
                    .map(|p| p.query.clone())
                    .unwrap_or_else(|| d.query()),
            ),
            Mode::Next | Mode::Previous => {
                let loaded = d.loaded.as_ref().filter(|p| p.current(s))?;
                let mut query = loaded.query.clone();
                if matches!(mode, Mode::Next) {
                    if loaded.page.items.is_empty() || query.page >= loaded.page.pages {
                        return None;
                    }
                    query.page = query.page.checked_add(1)?;
                    query.cursor = loaded.page.next_cursor.clone();
                } else {
                    query.page = query.page.checked_sub(1).filter(|n| *n > 0)?;
                    query.cursor = d.cursors.get(&query.page)?.clone();
                }
                Some(query)
            }
        }
    }
    pub(super) fn begin(
        &self,
        mut state: Signal<ShellState>,
        mode: Mode,
        reconnect: Option<crate::bootstrap::reconnect::Visit>,
    ) -> Option<(Request, Lease)> {
        let mut s = state.try_write().ok()?;
        if !self.current(&s) || reconnect.as_ref().is_some_and(|v| !v.current(&s)) {
            return None;
        }
        self.recover_stopped_read(&mut s, false);
        if s.channel_directory.busy() {
            return None;
        }
        let query = self.coordinate(&s, mode)?;
        if !self.family.ready() {
            s.channel_directory.failed = Some(query);
            s.channel_directory.error = Some("Slack asked us to wait. Try refreshing later.");
            return None;
        }
        let workspace = s.core.workspaces.get(&self.family.session.team_id)?;
        let (realtime, channels) = (
            workspace.rt_generation,
            workspace.channel_lifecycle.revision(),
        );
        let d = &mut s.channel_directory;
        d.revision = d.revision.wrapping_add(1);
        let ticket = Arc::new(Ticket {
            active: AtomicBool::new(true),
            revision: d.revision,
            query,
            realtime,
            channels,
        });
        d.pending = Some(ticket.clone());
        d.selected = None;
        d.error = None;
        d.failed = None;
        drop(s);
        let request = Request {
            state,
            entry: self.clone(),
            ticket: ticket.clone(),
            reconnect,
        };
        Some((request, Lease { state, ticket }))
    }
    pub(super) fn start(&self, state: Signal<ShellState>, mode: Mode) {
        self.start_with_visit(state, mode, None);
    }
    pub(super) fn start_with_visit(
        &self,
        state: Signal<ShellState>,
        mode: Mode,
        visit: Option<crate::bootstrap::reconnect::Visit>,
    ) {
        let Some((request, lease)) = self.begin(state, mode, visit) else {
            return;
        };
        dioxus::core::spawn_forever(Self::run_with(request, lease, fetch));
    }
    pub(super) async fn run_with<F, Fut>(request: Request, _lease: Lease, fetch: F)
    where
        F: FnOnce(Request) -> Fut,
        Fut: Future<Output = Result<Option<Page>, Error>>,
    {
        if !request.current() {
            return;
        }
        let mut state = request.state;
        let ticket = request.ticket.clone();
        let entry = request.entry.clone();
        let reconnect = request.reconnect.clone();
        let result = fetch(request).await;
        let current = Request {
            state,
            entry: entry.clone(),
            ticket: ticket.clone(),
            reconnect,
        };
        let Ok(mut s) = state.try_write() else { return };
        if !current.current_shell(&s) {
            return;
        }
        let d = &mut s.channel_directory;
        d.pending = None;
        match result {
            Ok(Some(page)) => {
                if ticket.query.page == 1 {
                    d.cursors.clear();
                }
                d.cursors
                    .insert(ticket.query.page, ticket.query.cursor.clone());
                d.loaded = Some(Arc::new(Loaded {
                    entry,
                    query: ticket.query.clone(),
                    page,
                    realtime: ticket.realtime,
                    channels: ticket.channels,
                }));
                d.presentation = None;
                d.error = None;
                d.failed = None;
            }
            Ok(None) => {
                d.failed = Some(ticket.query.clone());
                d.error = Some("Slack asked us to wait. Try refreshing later.");
            }
            Err(error) => {
                d.failed = Some(ticket.query.clone());
                d.error = Some(if d.loaded.is_some() {
                    "Could not load your channels. Your loaded results are still available."
                } else {
                    "Could not load your channels. Refresh when you reconnect."
                });
                s.report_failure(&error, "Your channels need a connection", || {
                    "Could not load your channels. Refresh to try again.".into()
                });
            }
        }
    }
}

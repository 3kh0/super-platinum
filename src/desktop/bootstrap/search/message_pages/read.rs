//! Original-owner read tickets; manual cache lifetime differs from recovery UI.
use super::*;
#[cfg(test)]
use std::rc::Rc;
use std::sync::Arc;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{SlackClient, Transport},
};

pub(super) struct Frame {
    identity: recovery::Identity,
    pub(super) epoch: u64,
    pub(super) team: String,
    pub(super) raw: String,
    sort: super_platinum_core::search::SearchSort,
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
}
impl Frame {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.identity.account_current(shell)
            && self
                .identity
                .matches_session(&self.transport, &self.session)
            && shell.search_query == self.raw
            && shell.search_pager.kind == SearchKind::Messages
            && shell.search_pager.sort == self.sort
            && shell.search_pager.query == self.raw.trim()
    }
}

#[derive(Clone)]
pub(in crate::bootstrap::search) struct Initial {
    frame: Arc<Frame>,
    request: SearchRequest,
}
#[derive(Clone)]
pub(in crate::bootstrap::search) struct Ticket {
    pub(super) frame: Arc<Frame>,
    pub(super) activation: Arc<()>,
    pub(super) request: SearchRequest,
    pub(super) cursor: recovery::Cursor,
    pub(super) restored: Option<(super::super::super::reconnect::Visit, u64)>,
}
impl Ticket {
    fn owns(&self, shell: &ShellState) -> bool {
        shell.search_message_pages.as_ref().is_some_and(|book| {
            Arc::ptr_eq(&book.frame, &self.frame)
                && Arc::ptr_eq(&book.activation, &self.activation)
                && book.pending.as_ref() == Some(&self.request)
        })
    }
}
#[derive(Clone)]
pub(in crate::bootstrap::search) enum Read {
    Initial(Initial),
    Page(Ticket),
}

pub(in crate::bootstrap::search) fn initial(
    shell: &ShellState,
    request: &SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
) -> Option<Read> {
    if request.page != 1
        || request.kind != SearchKind::Messages
        || shell.overlay != Some(Overlay::Search)
        || !shell.core.client.same_configuration(client)
    {
        return None;
    }
    let identity = recovery::Identity::capture(shell)?;
    if !identity.matches_session(transport, session) {
        return None;
    }
    let frame = Arc::new(Frame {
        identity,
        epoch: request.epoch,
        team: request.team.clone(),
        raw: shell.search_query.clone(),
        sort: request.sort,
        transport: transport.clone(),
        client: client.clone(),
        session: session.clone(),
    });
    let read = Read::Initial(Initial {
        frame,
        request: request.clone(),
    });
    read.current(shell, request).then_some(read)
}
impl Read {
    pub(in crate::bootstrap::search) fn matches_arguments(
        &self,
        transport: &Arc<Transport>,
        client: &SlackClient,
        session: &WorkspaceSession,
    ) -> bool {
        let frame = match self {
            Self::Initial(initial) => &initial.frame,
            Self::Page(ticket) => &ticket.frame,
        };
        Arc::ptr_eq(&frame.transport, transport)
            && frame.client.same_configuration(client)
            && frame.identity.matches_session(transport, session)
    }
    pub(in crate::bootstrap::search) fn current(
        &self,
        shell: &ShellState,
        request: &SearchRequest,
    ) -> bool {
        let (frame, original) = match self {
            Self::Initial(initial) => (&initial.frame, &initial.request),
            Self::Page(ticket) => {
                if !ticket.owns(shell)
                    || !ticket.cursor.current(shell)
                    || !shell
                        .search_message_pages
                        .as_ref()
                        .is_some_and(|book| book.current(shell))
                {
                    return false;
                }
                if ticket.restored.as_ref().is_some_and(|(visit, dialog)| {
                    !visit.admitted_current(shell)
                        || shell.search_open_generation != *dialog
                        || shell.overlay != Some(Overlay::Search)
                }) {
                    return false;
                }
                (&ticket.frame, &ticket.request)
            }
        };
        original == request
            && frame.current(shell)
            && super::super::current(shell, request, &frame.transport)
    }
    pub(in crate::bootstrap::search) fn valid_metadata(
        &self,
        metadata: Option<&SearchPagination>,
    ) -> bool {
        match self {
            Self::Initial(_) => true, // Absence keeps the existing legacy path.
            Self::Page(ticket) => metadata.is_some_and(|p| {
                p.page == Some(ticket.request.page) && p.page_count.is_none_or(|bound| bound > 0)
            }),
        }
    }
    pub(in crate::bootstrap::search) fn retire(&self, shell: &mut ShellState) {
        let request = match self {
            Self::Initial(initial) => &initial.request,
            Self::Page(ticket) => {
                if ticket.owns(shell) {
                    let book = shell.search_message_pages.as_mut().unwrap();
                    book.pending = None;
                    book.failed = true;
                }
                &ticket.request
            }
        };
        shell.search_pager.fail(request);
    }
    /// Called synchronously after currentness/metadata validation and the exact
    /// pager completion; no await or successor-owner recapture occurs here.
    pub(in crate::bootstrap::search) fn publish(
        &self,
        shell: &mut ShellState,
        rows: Vec<SearchResultVm>,
        metadata: Option<&SearchPagination>,
    ) {
        match self {
            Self::Initial(initial) => {
                let known = metadata.filter(|p| {
                    p.page == Some(1)
                        && p.page_count.is_some_and(|n| n >= 1)
                        && p.total_count.is_some()
                });
                if let Some(p) = known {
                    let bound = p.page_count.unwrap();
                    let total = p.total_count.unwrap();
                    shell.search_message_pages = Some(Book {
                        frame: initial.frame.clone(),
                        activation: Arc::new(()),
                        pages: BTreeMap::from([(1, rows.clone())]),
                        bound,
                        total,
                        shown: 1,
                        target: 1,
                        revision: shell.search_pager.revision(),
                        pending: None,
                        failed: false,
                    });
                    shell.search_pager.page_count = bound;
                    shell.search_pager.total = total;
                }
                shell.search_results = rows;
            }
            Self::Page(ticket) => {
                if !ticket.owns(shell) {
                    return;
                }
                let p = metadata.expect("validated owned page metadata");
                let book = shell.search_message_pages.as_mut().unwrap();
                book.pages.insert(ticket.request.page, rows.clone());
                book.shown = ticket.request.page;
                book.target = ticket.request.page;
                book.total = p.total_count.unwrap_or(book.total);
                book.pending = None;
                book.failed = false;
                book.revision = shell.search_pager.revision();
                shell.search_pager.page_count = book.bound;
                shell.search_pager.total = book.total;
                shell.search_results = rows;
            }
        }
    }
}

pub(in crate::bootstrap::search) struct Lease {
    state: Signal<ShellState>,
    read: Read,
}
impl Lease {
    pub(in crate::bootstrap::search) fn new(state: Signal<ShellState>, read: Read) -> Self {
        Self { state, read }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut shell) = self.state.try_write() {
            self.read.retire(&mut shell);
        }
    }
}

pub(super) struct Work {
    pub(super) ticket: Ticket,
}
pub(super) async fn run(state: Signal<ShellState>, work: Work) {
    let frame = work.ticket.frame.clone();
    let request = work.ticket.request.clone();
    #[cfg(test)]
    if let Some(io) = try_consume_context::<Rc<super::super::message_scope_io::Io>>() {
        let execute = io.clone();
        super::super::messages::load_owned_with_profiles(
            state,
            request,
            &frame.transport,
            &frame.client,
            &frame.session,
            None,
            Some(Read::Page(work.ticket)),
            move |request| async move { execute.execute(request).await },
            move |ids| io.lookup(ids),
        )
        .await;
        return;
    }
    #[cfg(test)]
    {
        // Tests must explicitly supply a scoped synthetic executor. Never fall
        // through to Slack when a headless callback lacks its test provider.
        let io = try_consume_context::<Rc<super::test_io::Io>>()
            .expect("Messages page tests require their scoped synthetic executor");
        super::super::messages::load_owned_with_profiles(
            state,
            request,
            &frame.transport,
            &frame.client,
            &frame.session,
            None,
            Some(Read::Page(work.ticket)),
            |request| io.execute(request),
            |ids| io.lookup(ids),
        )
        .await;
    }
    #[cfg(not(test))]
    super::super::messages::load_owned_with_profiles(
        state,
        request,
        &frame.transport,
        &frame.client,
        &frame.session,
        None,
        Some(Read::Page(work.ticket)),
        |request| frame.transport.execute(request),
        |ids| api::fetch_users_info(&frame.transport, &frame.client, &frame.session, ids),
    )
    .await;
}

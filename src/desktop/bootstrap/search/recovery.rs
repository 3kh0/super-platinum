//! One failed Search page per restored connection, without replacing the query.
use super::*;
use crate::state::Overlay;
use std::{future::Future, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    search::{SearchKind, SearchSort},
    slack::{Error, PreparedRequest, SlackClient, Transport},
};

// Credentials stay private and ephemeral; never Debug, persist or report them.
#[derive(Clone)]
pub(super) struct Identity {
    account: String,
    epoch: u64,
    team: String,
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    cookie: String,
    raw_query: String,
    dialog: u64,
}
impl Identity {
    pub(super) fn capture_account(shell: &ShellState) -> Option<Self> {
        let team = shell.core.active_team.clone()?;
        let session = shell.core.session.as_ref()?;
        let identity = Identity {
            account: shell.core.active_account.clone()?,
            epoch: shell.core.account_epoch,
            transport: shell.core.transport.clone()?,
            client: shell.core.client.clone(),
            session: session.workspaces.get(&team)?.clone(),
            cookie: session.d_cookie.clone(),
            team,
            raw_query: shell.search_query.clone(),
            dialog: shell.search_open_generation,
        };
        identity.account_current(shell).then_some(identity)
    }
    pub(super) fn capture(shell: &ShellState) -> Option<Self> {
        Self::capture_account(shell).filter(|identity| identity.current(shell))
    }
    pub(super) fn team(&self) -> &str {
        &self.team
    }
    pub(super) fn matches_session(
        &self,
        transport: &Arc<Transport>,
        session: &WorkspaceSession,
    ) -> bool {
        Arc::ptr_eq(&self.transport, transport)
            && transport.d_cookie() == self.cookie
            && self.session.team_id == session.team_id
            && self.session.user_id == session.user_id
            && self.session.token == session.token
            && self.session.url == session.url
            && self.session.enterprise_id == session.enterprise_id
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.account_current(shell)
            && shell.overlay == Some(Overlay::Search)
            && shell.search_query == self.raw_query
            && shell.search_open_generation == self.dialog
    }
    pub(super) fn account_current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_ref() == Some(&self.account)
            && shell.core.client.same_configuration(&self.client)
            && shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|w| w.self_user_id == self.session.user_id)
            && shell.core.session.as_ref().is_some_and(|s| {
                s.d_cookie == self.cookie
                    && s.workspaces.get(&self.team).is_some_and(|w| {
                        w.team_id == self.team
                            && w.user_id == self.session.user_id
                            && w.token == self.session.token
                            && w.url == self.session.url
                            && w.enterprise_id == self.session.enterprise_id
                    })
            })
    }
}

#[derive(Clone)]
pub(super) struct Cursor {
    kind: SearchKind,
    sort: SearchSort,
    revision: u64,
    page: u32,
    pages: u32,
    total: u64,
}
impl Cursor {
    pub(super) fn capture(shell: &ShellState) -> Self {
        let pager = &shell.search_pager;
        Self {
            kind: pager.kind,
            sort: pager.sort,
            revision: pager.revision(),
            page: pager.page,
            pages: pager.page_count,
            total: pager.total,
        }
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        let pager = &shell.search_pager;
        self.kind == pager.kind
            && self.sort == pager.sort
            && self.revision == pager.revision()
            && self.page == pager.page
            && self.pages == pager.page_count
            && self.total == pager.total
    }
}

pub(super) struct Candidate {
    identity: Identity,
    cursor: Cursor,
}
impl Candidate {
    fn capture(shell: &ShellState) -> Option<Self> {
        if super::message_pages::has_scope(shell) || super::file_pages::has_scope(shell) {
            return None;
        }
        if shell
            .search_people_module
            .as_ref()
            .is_some_and(|m| m.has_results_scope())
        {
            return None;
        }
        if shell.overlay != Some(Overlay::Search)
            || !shell.search_pager.failed
            || shell.search_pager.loading
            || shell.search_query.trim().is_empty()
            || shell.search_pager.query != shell.search_query.trim()
        {
            return None;
        }
        Some(Self {
            identity: Identity::capture(shell)?,
            cursor: Cursor::capture(shell),
        })
    }
}

pub(super) struct Scope {
    identity: Identity,
    cursor: Cursor,
}
impl Scope {
    pub(super) fn current(&self, shell: &ShellState, request: &SearchRequest) -> bool {
        self.identity.current(shell)
            && self.cursor.current(shell)
            && request.page == self.cursor.page.saturating_add(1)
            && super::current(shell, request, &self.identity.transport)
    }
}

pub(super) struct Lease {
    state: Signal<ShellState>,
    request: SearchRequest,
}
impl Lease {
    pub(super) fn new(state: Signal<ShellState>, request: &SearchRequest) -> Self {
        Self {
            state,
            request: request.clone(),
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        // Publication may be retired by a closed dialog or equivalent raw edit.
        // Release only this exact request's slot, including on task cancellation.
        if let Ok(mut shell) = self.state.try_write() {
            shell.search_pager.fail(&self.request);
        }
    }
}

pub(in crate::bootstrap) fn schedule(
    state: Signal<ShellState>,
    visit: super::super::reconnect::Visit,
) {
    let Some(candidate) = state
        .try_read()
        .ok()
        .and_then(|shell| Candidate::capture(&shell))
    else {
        return;
    };
    spawn(async move {
        let transport = candidate.identity.transport.clone();
        let client = candidate.identity.client.clone();
        let session = candidate.identity.session.clone();
        recover_continued_with_profiles(
            state,
            visit,
            candidate,
            |request| transport.execute(request),
            |ids| api::fetch_users_info(&transport, &client, &session, ids),
        )
        .await;
    });
}

async fn recover_with_profiles<F, Fut, U, UFut>(
    state: Signal<ShellState>,
    visit: super::super::reconnect::Visit,
    candidate: Candidate,
    execute: F,
    lookup: U,
) where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
    U: FnMut(Vec<String>) -> UFut,
    UFut: Future<Output = Result<Vec<super_platinum_core::slack::models::User>, Error>>,
{
    let mut once = Some(execute);
    recover_using(
        state,
        visit,
        candidate,
        false,
        move |request| {
            let execute = once.take();
            async move {
                match execute {
                    Some(execute) => execute(request).await,
                    None => Err(Error::Transport(
                        "Single-request Search executor was already consumed".into(),
                    )),
                }
            }
        },
        lookup,
    )
    .await;
}

async fn recover_continued_with_profiles<F, Fut, U, UFut>(
    state: Signal<ShellState>,
    visit: super::super::reconnect::Visit,
    candidate: Candidate,
    execute: F,
    lookup: U,
) where
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
    U: FnMut(Vec<String>) -> UFut,
    UFut: Future<Output = Result<Vec<super_platinum_core::slack::models::User>, Error>>,
{
    recover_using(state, visit, candidate, true, execute, lookup).await;
}

async fn recover_using<F, Fut, U, UFut>(
    mut state: Signal<ShellState>,
    visit: super::super::reconnect::Visit,
    candidate: Candidate,
    continued: bool,
    execute: F,
    lookup: U,
) where
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
    U: FnMut(Vec<String>) -> UFut,
    UFut: Future<Output = Result<Vec<super_platinum_core::slack::models::User>, Error>>,
{
    let admitted = {
        let Ok(mut shell) = state.try_write() else {
            return;
        };
        if !visit.current(&shell)
            || !candidate.identity.current(&shell)
            || !candidate.cursor.current(&shell)
        {
            return;
        }
        let Some(request) = shell.search_pager.retry_failed(
            candidate.identity.epoch,
            &candidate.identity.team,
            &candidate.identity.raw_query,
        ) else {
            return;
        };
        let cursor = Cursor::capture(&shell);
        (
            request,
            Scope {
                identity: candidate.identity,
                cursor,
            },
        )
    };
    let (request, scope) = admitted;
    let _lease = Lease {
        state,
        request: request.clone(),
    };
    // Once admitted, a newer reconnect does not retire a valid REST response.
    let transport = scope.identity.transport.clone();
    let client = scope.identity.client.clone();
    let session = scope.identity.session.clone();
    match request.kind {
        SearchKind::People => {
            if continued {
                super::people::load_continued_with(
                    state,
                    request,
                    &transport,
                    &client,
                    &session,
                    Some(scope),
                    execute,
                )
                .await
            } else {
                super::people::load_with(
                    state,
                    request,
                    &transport,
                    &client,
                    &session,
                    Some(scope),
                    execute,
                )
                .await
            }
        }
        SearchKind::Files => {
            super::files::load_with_profiles(
                state,
                request,
                &transport,
                &client,
                &session,
                Some(scope),
                execute,
                lookup,
            )
            .await
        }
        SearchKind::Messages => {
            super::messages::load_with_profiles(
                state,
                request,
                &transport,
                &client,
                &session,
                Some(scope),
                execute,
                lookup,
            )
            .await
        }
    }
}

#[cfg(test)]
async fn recover_with<F, Fut>(
    state: Signal<ShellState>,
    visit: super::super::reconnect::Visit,
    candidate: Candidate,
    execute: F,
) where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    recover_with_profiles(state, visit, candidate, execute, |_| {
        std::future::ready(Ok(Vec::new()))
    })
    .await;
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod continuation_tests;

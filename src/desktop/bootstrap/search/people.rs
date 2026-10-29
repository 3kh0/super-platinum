//! One bounded people lookup. Returned records never seed canonical membership.
use super::recovery::{Cursor, Identity};
use super::*;
use crate::{bootstrap::profile_navigation::ProfileVisit, state::Overlay};
use std::{collections::HashSet, future::Future, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    search::SearchKind,
    slack::{Error, PreparedRequest, SlackClient, Transport, models::User},
};
pub(crate) mod numbered;

pub(crate) struct Results {
    owner: Identity,
    cursor: Cursor,
    // Immutable retained allocations let a rendered selection own the exact
    // record without serializing whole profiles again on every media tick.
    users: Vec<Arc<User>>,
}
impl Results {
    fn current(&self, shell: &ShellState) -> bool {
        self.owner.account_current(shell)
            && self.cursor.current(shell)
            && shell.search_pager.kind == SearchKind::People
            && shell.search_pager.page == 1
            && !shell.search_pager.loading
            && !shell.search_pager.failed
            && shell.search_pager.query == shell.search_query.trim()
    }
}
pub(crate) fn rows(shell: &ShellState) -> &[Arc<User>] {
    shell
        .search_people_results
        .as_ref()
        .filter(|r| r.current(shell))
        .map(|r| r.users.as_slice())
        .unwrap_or_default()
}
pub(crate) fn retired(shell: &ShellState) -> bool {
    if numbered::retired(shell) {
        return true;
    }
    shell.search_pager.kind == SearchKind::People
        && shell
            .search_people_results
            .as_ref()
            .is_some_and(|r| !r.current(shell))
}
pub(super) async fn load(
    state: Signal<ShellState>,
    request: SearchRequest,
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
) {
    if numbered::start(state, &request, &transport, &client, &session).await {
        return;
    }
    load_continued_with(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        |request| transport.execute(request),
    )
    .await;
}

// Legacy single-request callers keep FnOnce and their exact result contract.
// Admission/finalization are shared with production continuation, not copied.
pub(super) async fn load_with<F, Fut>(
    state: Signal<ShellState>,
    request: SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
    scope: Option<super::recovery::Scope>,
    execute: F,
) where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    let mut once = Some(execute);
    run_with(
        state,
        request,
        transport,
        client,
        session,
        scope,
        false,
        move |request| {
            let execute = once.take();
            async move {
                match execute {
                    Some(execute) => execute(request).await,
                    None => Err(Error::Transport(
                        "Single-request People executor was already consumed".into(),
                    )),
                }
            }
        },
    )
    .await;
}

pub(super) async fn load_continued_with<F, Fut>(
    state: Signal<ShellState>,
    request: SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
    scope: Option<super::recovery::Scope>,
    execute: F,
) where
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    run_with(
        state, request, transport, client, session, scope, true, execute,
    )
    .await;
}

async fn run_with<F, Fut>(
    mut state: Signal<ShellState>,
    request: SearchRequest,
    transport: &Arc<Transport>,
    client: &SlackClient,
    session: &WorkspaceSession,
    scope: Option<super::recovery::Scope>,
    continued: bool,
    execute: F,
) where
    F: FnMut(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    // Normal search also owns a cancellation lease; a successor's request slot
    // cannot be released by this exact request's drop.
    let _lease = super::recovery::Lease::new(state, &request);
    let owner = state
        .try_read()
        .ok()
        .and_then(|s| Identity::capture_account(&s))
        .filter(|i| i.matches_session(transport, session));
    let Some(owner) = owner else {
        return;
    };
    let current = |s: &ShellState| {
        request.kind == SearchKind::People
            && request.page == 1
            && owner.current(s)
            && super::messages::current_for(s, &request, transport, scope.as_ref())
    };
    if !state.try_read().ok().is_some_and(|s| current(&s)) {
        return;
    }
    let result = if continued {
        api::fetch_users_search_continued_with(
            client,
            session,
            request.query.clone(),
            execute,
            || state.try_read().ok().is_some_and(|s| current(&s)),
        )
        .await
    } else {
        api::fetch_users_search_with(client, session, request.query.clone(), execute)
            .await
            .map(Some)
    };
    let Ok(mut shell) = state.try_write() else {
        return;
    };
    if !current(&shell) {
        shell.search_pager.fail(&request);
        return;
    }
    match result {
        Ok(Some(users)) if shell.search_pager.complete_lookup(&request) => {
            let mut seen = HashSet::new();
            let users = users
                .into_iter()
                .filter(|u| !u.id.trim().is_empty() && seen.insert(u.id.clone()))
                .take(api::USER_SEARCH_LIMIT as usize)
                .map(Arc::new)
                .collect();
            let cursor = Cursor::capture(&shell);
            shell.search_people_results = Some(Results {
                owner,
                cursor,
                users,
            });
        }
        Ok(_) => {}
        Err(error) => {
            shell.search_pager.fail(&request);
            shell.report_failure(&error, "People search needs a connection", || {
                "Could not find people. Please try again.".into()
            });
        }
    }
}

#[derive(Clone)]
pub(crate) struct Source {
    owner: Identity,
    cursor: Cursor,
    user: Arc<User>,
}
enum Open {
    Known { id: String, visit: ProfileVisit },
    ReadOnly(crate::bootstrap::profile_search::Ticket),
}
impl Source {
    pub(crate) fn capture(shell: &ShellState, id: &str) -> Option<Self> {
        if shell.overlay != Some(Overlay::Search) {
            return None;
        }
        let user = rows(shell).iter().find(|u| u.id == id)?.clone();
        Some(Self {
            owner: Identity::capture(shell)?,
            cursor: Cursor::capture(shell),
            user,
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && self.cursor.current(shell)
            && rows(shell)
                .iter()
                .find(|u| u.id == self.user.id)
                .is_some_and(|u| Arc::ptr_eq(u, &self.user))
    }
    fn prepare(self, shell: &mut ShellState) -> Option<Open> {
        if !self.current(shell) {
            return None;
        }
        let known = shell
            .core
            .active_team
            .as_ref()
            .and_then(|t| shell.core.workspaces.get(t))?
            .users
            .contains_key(&self.user.id);
        shell.dismiss_overlay();
        if shell.overlay.is_some() {
            return None;
        }
        if known {
            super::super::discovery::show_profile(shell, self.user.id.clone());
            Some(Open::Known {
                id: self.user.id.clone(),
                visit: ProfileVisit::capture(shell)?,
            })
        } else {
            crate::bootstrap::profile_search::show(shell, self.user.as_ref().clone())
                .map(Open::ReadOnly)
        }
    }
}
pub(crate) fn open(mut state: Signal<ShellState>, source: Source) {
    let selected = state
        .try_write()
        .ok()
        .and_then(|mut s| source.prepare(&mut s));
    if let Some(selected) = selected {
        dioxus::core::spawn_forever(async move {
            match selected {
                Open::ReadOnly(ticket) => {
                    crate::bootstrap::profile_search::run(state, ticket).await
                }
                Open::Known { id, visit } => {
                    if state.try_read().ok().is_some_and(|s| visit.current(&s)) {
                        super::super::discovery::refresh_open_profile(state, id).await;
                    }
                }
            }
        });
    }
}

mod fixture;
pub(crate) use fixture::install;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod continuation_tests;

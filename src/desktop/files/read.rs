//! Captured Files page reads; successful primary data owns its own busy phase.
use super::{store, visible_for};
use crate::{
    resource_reads::{Context, Route, Ticket},
    state::ShellState,
};
use dioxus::prelude::*;
use std::{collections::BTreeSet, future::Future, rc::Rc, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    files::{FileSort, FilesPage, FilesQuery, FilesState},
    slack::{Error, SlackClient, Transport, api},
};

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    scope: Option<(u64, String)>,
    generation: u64,
    query: FilesQuery,
    page: u32,
    requested: Option<u32>,
    loading: bool,
    loaded: bool,
}
impl Stamp {
    fn capture(files: &FilesState) -> Self {
        Self {
            scope: files.scope.clone(),
            generation: files.generation,
            query: files.query.clone(),
            page: files.page,
            requested: files.requested_page,
            loading: files.loading,
            loaded: files.loaded,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Source {
    context: Rc<Context>,
    stamp: Stamp,
    pub(crate) query: FilesQuery,
}
impl Source {
    pub(crate) fn capture(shell: &ShellState, channel: Option<&str>) -> Option<Self> {
        let route = channel
            .map(|c| Route::ChannelFiles(c.to_owned()))
            .unwrap_or(Route::Files);
        Self::for_route(shell, route, None)
    }
    fn for_route(
        shell: &ShellState,
        route: Route,
        reconnect: Option<crate::bootstrap::reconnect::Visit>,
    ) -> Option<Self> {
        let context = Context::capture(shell, route, reconnect)?;
        let channel = context.route.channel();
        let files = if channel.is_some() {
            &shell.core.channel_files
        } else {
            &shell.core.files
        };
        let query = visible_for(shell, channel)
            .map(|f| f.query.clone())
            .unwrap_or_else(|| FilesQuery {
                channel: channel.map(str::to_owned),
                sort: if channel.is_some() {
                    FileSort::Newest
                } else {
                    FileSort::Viewed
                },
                ..Default::default()
            })
            .normalized();
        Some(Self {
            context,
            stamp: Stamp::capture(files),
            query,
        })
    }
    fn files<'a>(&self, shell: &'a ShellState) -> &'a FilesState {
        if self.context.route.channel().is_some() {
            &shell.core.channel_files
        } else {
            &shell.core.files
        }
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && self.stamp == Stamp::capture(self.files(shell))
            && self.query.channel.as_deref() == self.context.route.channel()
    }
    pub(crate) fn is_current(&self, state: Signal<ShellState>) -> bool {
        state.try_peek().is_ok_and(|shell| self.current(&shell))
    }
    fn dispatch(self, state: Signal<ShellState>, query: FilesQuery, page: u32) {
        if !self.is_current(state) || query.channel.as_deref() != self.context.route.channel() {
            return;
        }
        dioxus::core::spawn_forever(self.run_with(state, query, page, fetch, names));
    }
    pub(crate) fn submit(self, state: Signal<ShellState>, query: FilesQuery) {
        self.dispatch(state, query, 1);
    }
    fn visible_scope(&self) -> bool {
        self.stamp
            .scope
            .as_ref()
            .is_some_and(|(epoch, team)| *epoch == self.context.epoch && team == &self.context.team)
            && self.stamp.query.channel.as_deref() == self.context.route.channel()
    }
    pub(crate) fn resume_page(&self) -> u32 {
        if self.visible_scope() {
            self.stamp.requested.unwrap_or(self.stamp.page).max(1)
        } else {
            1
        }
    }
    pub(crate) fn refresh(self, state: Signal<ShellState>) {
        if self.visible_scope() && self.stamp.loading {
            return;
        }
        let page = if self.visible_scope() {
            self.stamp.page.max(1)
        } else {
            1
        };
        let query = self.query.clone();
        self.dispatch(state, query, page);
    }
    pub(crate) fn retry(self, state: Signal<ShellState>) {
        if self.visible_scope() && self.stamp.loading {
            return;
        }
        let page = self.resume_page();
        let query = self.query.clone();
        self.dispatch(state, query, page);
    }
    pub(crate) fn page(self, state: Signal<ShellState>, page: u32) {
        if !self.visible_scope() || self.stamp.loading || !self.stamp.loaded {
            return;
        }
        let query = self.query.clone();
        self.dispatch(state, query, page);
    }
    pub(crate) fn capture_reconnect(
        shell: &ShellState,
        channel: Option<&str>,
        visit: crate::bootstrap::reconnect::Visit,
    ) -> Option<Self> {
        let route = channel
            .map(|c| Route::ChannelFiles(c.to_owned()))
            .unwrap_or(Route::Files);
        Self::for_route(shell, route, Some(visit))
    }
    pub(crate) async fn run(self, state: Signal<ShellState>, query: FilesQuery, page: u32) {
        self.run_with(state, query, page, fetch, names).await;
    }
    fn published_current(&self, shell: &ShellState, generation: u64, query: &FilesQuery) -> bool {
        let files = self.files(shell);
        self.context.current(shell)
            && files.matches_scope(self.context.epoch, &self.context.team)
            && files.generation == generation
            && files.query == *query
            && files.loaded
            && !files.loading
            && files.requested_page.is_none()
    }
    async fn run_with<F, Fut, N, NFut>(
        self,
        mut state: Signal<ShellState>,
        query: FilesQuery,
        page: u32,
        fetch: F,
        names: N,
    ) where
        F: FnOnce(Request) -> Fut,
        Fut: Future<Output = Result<FilesPage, Error>>,
        N: FnOnce(NamesRequest) -> NFut,
        NFut: Future<Output = Result<Vec<super_platinum_core::slack::models::User>, Error>>,
    {
        let query = query.normalized();
        if query.channel.as_deref() != self.context.route.channel() {
            return;
        }
        let (generation, ticket) = {
            let Ok(mut shell) = state.try_write() else {
                return;
            };
            if !self.current(&shell) {
                return;
            }
            let files = store(&mut shell, query.channel.is_some());
            files.ensure_scope(self.context.epoch, &self.context.team);
            let generation = files.begin(query.clone(), page);
            let ticket = self.context.claim(&mut shell, generation);
            (generation, ticket)
        };
        let mut lease = Lease {
            state,
            context: self.context.clone(),
            ticket,
            armed: true,
        };
        let result = fetch(Request {
            transport: self.context.transport.clone(),
            client: self.context.client.clone(),
            session: self.context.session.clone(),
            query: query.clone(),
            page: page.max(1),
        })
        .await;
        let published = {
            let Ok(mut shell) = state.try_write() else {
                return;
            };
            if !lease.current(&shell) {
                lease.ticket.cancel(&mut shell);
                lease.armed = false;
                return;
            }
            let published = match result {
                Ok(page) => {
                    let deleted = shell
                        .core
                        .workspaces
                        .get(&self.context.team)
                        .map(|w| w.deleted_files.clone())
                        .unwrap_or_default();
                    let files = store(&mut shell, query.channel.is_some());
                    files.deleted.extend(deleted);
                    files.apply(generation, page)
                }
                Err(error) => {
                    store(&mut shell, query.channel.is_some()).fail(
                        generation,
                        "Files could not load. Try changing your search or refreshing.",
                    );
                    shell.report_failure(
                        &error,
                        "Files will be available when you reconnect.",
                        || "Could not load files. Please try again.".into(),
                    );
                    false
                }
            };
            lease.ticket.clear(&mut shell);
            lease.armed = false;
            published
        };
        if !published {
            return;
        }
        // Names are optional and can never keep the primary page busy.
        let (snapshots, ids) = {
            let Ok(shell) = state.try_peek() else {
                return;
            };
            if !self.published_current(&shell, generation, &query) {
                return;
            }
            let Some(workspace) = shell.core.workspaces.get(&self.context.team) else {
                return;
            };
            let ids = self
                .files(&shell)
                .items
                .iter()
                .filter_map(super_platinum_core::state::file_uploader_id)
                .filter(|id| crate::bootstrap::user_names::needs_name(workspace, id))
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if ids.is_empty() {
                return;
            }
            (crate::bootstrap::user_names::capture(workspace, &ids), ids)
        };
        let users = names(NamesRequest {
            transport: self.context.transport.clone(),
            client: self.context.client.clone(),
            session: self.context.session.clone(),
            ids,
        })
        .await;
        let Ok(users) = users else {
            return;
        };
        let Ok(mut shell) = state.try_write() else {
            return;
        };
        if !self.published_current(&shell, generation, &query) {
            return;
        }
        let eligible = self
            .files(&shell)
            .items
            .iter()
            .filter_map(super_platinum_core::state::file_uploader_id)
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        if let Some(workspace) = shell.core.workspaces.get_mut(&self.context.team) {
            crate::bootstrap::user_names::apply(workspace, &snapshots, &eligible, users);
        }
    }
}
struct Lease {
    state: Signal<ShellState>,
    context: Rc<Context>,
    ticket: Arc<Ticket>,
    armed: bool,
}
impl Lease {
    fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell) && self.ticket.active(shell)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if self.armed {
            if let Ok(mut shell) = self.state.try_write() {
                self.ticket.cancel(&mut shell);
            }
        }
    }
}
// Typed native inputs; never Debug/serialize/log credentials or query content.
struct Request {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    query: FilesQuery,
    page: u32,
}
struct NamesRequest {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    ids: Vec<String>,
}
async fn fetch(request: Request) -> Result<FilesPage, Error> {
    api::fetch_files(
        &request.transport,
        &request.client,
        &request.session,
        &request.query,
        request.page,
    )
    .await
}
async fn names(
    request: NamesRequest,
) -> Result<Vec<super_platinum_core::slack::models::User>, Error> {
    api::fetch_users_info(
        &request.transport,
        &request.client,
        &request.session,
        request.ids,
    )
    .await
}
pub(crate) fn load(
    state: Signal<ShellState>,
    query: FilesQuery,
    page: u32,
) -> impl Future<Output = ()> {
    let source = state
        .try_peek()
        .ok()
        .and_then(|shell| Source::capture(&shell, query.channel.as_deref()));
    async move {
        if let Some(source) = source {
            source.run_with(state, query, page, fetch, names).await;
        }
    }
}
pub(crate) fn load_reconnect(
    state: Signal<ShellState>,
    query: FilesQuery,
    page: u32,
    visit: crate::bootstrap::reconnect::Visit,
) -> impl Future<Output = ()> {
    let source = state.try_peek().ok().and_then(|shell| {
        let route = query
            .channel
            .as_ref()
            .map(|c| Route::ChannelFiles(c.clone()))
            .unwrap_or(Route::Files);
        Source::for_route(&shell, route, Some(visit))
    });
    async move {
        if let Some(source) = source {
            source.run_with(state, query, page, fetch, names).await;
        }
    }
}
pub(crate) fn load_picker(
    state: Signal<ShellState>,
    query: FilesQuery,
    page: u32,
    visit: crate::channel_tabs::Visit,
) -> impl Future<Output = ()> {
    let source = state.try_peek().ok().and_then(|shell| {
        if !visit.native_read_current(&shell) || query.channel.as_deref() != Some(visit.channel()) {
            return None;
        }
        let route = Route::Picker {
            channel: visit.channel().to_owned(),
            visit,
        };
        Source::for_route(&shell, route, None)
    });
    async move {
        if let Some(source) = source {
            source.run_with(state, query, page, fetch, names).await;
        }
    }
}

#[cfg(test)]
mod tests;

//! Channel Links reads retain successful pages and their original rendered source.
use super::visible_links;
use crate::{
    resource_reads::{Context, Route, Ticket},
    state::ShellState,
};
use dioxus::prelude::*;
use std::{future::Future, rc::Rc, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    links::{LinksPage, LinksState},
    slack::{Error, SlackClient, Transport, api},
};
#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    scope: Option<(u64, String, String)>,
    generation: u64,
    query: String,
    page: u32,
    requested: Option<u32>,
    loading: bool,
    loaded: bool,
}
impl Stamp {
    fn capture(links: &LinksState) -> Self {
        Self {
            scope: links.scope.clone(),
            generation: links.generation,
            query: links.query.clone(),
            page: links.page,
            requested: links.requested_page,
            loading: links.loading,
            loaded: links.loaded,
        }
    }
}
#[derive(Clone)]
pub(crate) struct Source {
    context: Rc<Context>,
    stamp: Stamp,
    pub(crate) query: String,
}
impl Source {
    pub(crate) fn capture(shell: &ShellState, channel: &str) -> Option<Self> {
        Self::for_visit(shell, channel, None)
    }
    fn for_visit(
        shell: &ShellState,
        channel: &str,
        visit: Option<crate::bootstrap::reconnect::Visit>,
    ) -> Option<Self> {
        let context = Context::capture(shell, Route::Links(channel.into()), visit)?;
        let query = visible_links(shell, channel)
            .map(|l| l.query.trim().to_owned())
            .unwrap_or_default();
        Some(Self {
            context,
            stamp: Stamp::capture(&shell.core.channel_links),
            query,
        })
    }
    pub(crate) fn capture_reconnect(
        shell: &ShellState,
        channel: &str,
        visit: crate::bootstrap::reconnect::Visit,
    ) -> Option<Self> {
        Self::for_visit(shell, channel, Some(visit))
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell) && self.stamp == Stamp::capture(&shell.core.channel_links)
    }
    pub(crate) fn is_current(&self, state: Signal<ShellState>) -> bool {
        state.try_peek().is_ok_and(|shell| self.current(&shell))
    }
    fn visible_scope(&self) -> bool {
        self.stamp
            .scope
            .as_ref()
            .is_some_and(|(epoch, team, channel)| {
                *epoch == self.context.epoch
                    && team == &self.context.team
                    && Some(channel.as_str()) == self.context.route.channel()
            })
    }
    pub(crate) fn resume_page(&self) -> u32 {
        if self.visible_scope() {
            self.stamp.requested.unwrap_or(self.stamp.page).max(1)
        } else {
            1
        }
    }
    fn dispatch(self, state: Signal<ShellState>, query: String, page: u32) {
        if !self.is_current(state) {
            return;
        }
        dioxus::core::spawn_forever(self.run(state, query, page));
    }
    pub(crate) fn submit(self, state: Signal<ShellState>, query: String) {
        self.dispatch(state, query, 1);
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
    pub(crate) async fn run(self, state: Signal<ShellState>, query: String, page: u32) {
        self.run_with(state, query, page, fetch).await;
    }
    async fn run_with<F, Fut>(
        self,
        mut state: Signal<ShellState>,
        query: String,
        page: u32,
        fetch: F,
    ) where
        F: FnOnce(Request) -> Fut,
        Fut: Future<Output = Result<LinksPage, Error>>,
    {
        let query = query.trim().to_owned();
        let Some(channel) = self.context.route.channel().map(str::to_owned) else {
            return;
        };
        let (generation, ticket) = {
            let Ok(mut shell) = state.try_write() else {
                return;
            };
            if !self.current(&shell) {
                return;
            }
            shell
                .core
                .channel_links
                .ensure_scope(self.context.epoch, &self.context.team, &channel);
            let generation = shell.core.channel_links.begin(query.clone(), page);
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
            channel,
            query,
            page: page.max(1),
        })
        .await;
        let Ok(mut shell) = state.try_write() else {
            return;
        };
        if !lease.current(&shell) {
            lease.ticket.cancel(&mut shell);
            lease.armed = false;
            return;
        }
        match result {
            Ok(page) => {
                shell.core.channel_links.apply(generation, page);
            }
            Err(error) => {
                shell
                    .core
                    .channel_links
                    .fail(generation, "Links could not load. Please try again.");
                shell.report_failure(
                    &error,
                    "Links will be available when you reconnect.",
                    || "Could not load channel links.".into(),
                );
            }
        }
        lease.ticket.clear(&mut shell);
        lease.armed = false;
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
struct Request {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    channel: String,
    query: String,
    page: u32,
}
async fn fetch(request: Request) -> Result<LinksPage, Error> {
    api::fetch_channel_links(
        &request.transport,
        &request.client,
        &request.session,
        &request.channel,
        &request.query,
        request.page,
    )
    .await
}
pub(crate) fn load_links(
    state: Signal<ShellState>,
    channel: String,
    query: String,
    page: u32,
) -> impl Future<Output = ()> {
    let source = state
        .try_peek()
        .ok()
        .and_then(|shell| Source::capture(&shell, &channel));
    async move {
        if let Some(source) = source {
            source.run(state, query, page).await;
        }
    }
}

pub(crate) fn load_reconnect(
    state: Signal<ShellState>,
    channel: String,
    query: String,
    page: u32,
    visit: crate::bootstrap::reconnect::Visit,
) -> impl Future<Output = ()> {
    let source = state
        .try_peek()
        .ok()
        .and_then(|shell| Source::capture_reconnect(&shell, &channel, visit));
    async move {
        if let Some(source) = source {
            source.run(state, query, page).await;
        }
    }
}

#[cfg(test)]
mod tests;

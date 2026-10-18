//! Profile-owned channel acquisition. Results remain unselectable until the
//! matching/ranking caller publishes IDs through the private query catalog.
use super::*;
use crate::app_home::{autocomplete, channel_fetches::search as ingress};
use std::time::Instant;
use super_platinum_core::{
    block_actions::{directory_endpoint::DirectoryEndpoint, shortcuts::grid},
    slack::{PreparedRequest, api::custom::texty::channels},
};
mod facts;

pub(crate) struct Search {
    lease: query::Lease,
    context: Arc<crate::scheduled::Context>,
    pub(super) ranking: autocomplete::Ranking,
}
pub(crate) struct Acquired {
    pub(super) pages: channels::Pages,
    pub(super) source: Arc<canonical::Canonical>,
}
impl Search {
    pub(crate) fn capture(state: Signal<ShellState>, lease: query::Lease) -> Result<Self, Error> {
        // Closed fixtures need an explicitly installed synthetic reader. Until
        // that integration exists, they cannot fall through to real transport.
        if crate::fixture::is_fixture() || !lease.can_dispatch(&state.peek()) {
            return Err(unavailable());
        }
        let context = lease.context(&state.peek()).ok_or_else(unavailable)?;
        let ranking = autocomplete::Ranking::capture(state, context.clone(), |s| lease.current(s))?;
        Ok(Self {
            lease,
            context,
            ranking,
        })
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.lease.current(shell) && self.context.current(shell) && self.ranking.current(shell)
    }
    fn can_dispatch(&self, shell: &ShellState) -> bool {
        !crate::fixture::is_fixture() && self.current(shell) && self.lease.can_dispatch(shell)
    }
    async fn request(
        &self,
        mut state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        if !self.can_dispatch(&state.peek()) {
            return Err(unavailable());
        }
        let result = tokio::select! {
            biased;
            _ = self.lease.cancelled() => Err(unavailable()),
            result = self.context.transport.execute(request) => result,
        };
        if let Err(error) = &result {
            self.lease.retain_failure(&mut state.write(), error);
        }
        if !self.current(&state.peek()) {
            return Err(unavailable());
        }
        result
    }
    /// Actual immutable shared history, read under Profile's lease. This does
    /// not adopt a Home query or create an independent history writer.
    pub(super) async fn navigation(
        &self,
        state: Signal<ShellState>,
    ) -> Result<autocomplete::Navigation, Error> {
        tokio::select! {
            biased;
            _ = self.lease.cancelled() => Err(unavailable()),
            value = autocomplete::Navigation::capture(state, self.lease.account(), self.lease.team(), |s| self.current(s)) => value,
        }
    }
    /// Remote branch only. The Texty caller chooses local/remote before calling
    /// this; an HTTP failure never becomes an invented successful local search.
    pub(super) async fn remote(
        &self,
        mut state: Signal<ShellState>,
        raw_query: &str,
    ) -> Result<Acquired, Error> {
        let result = self.remote_inner(state, raw_query).await;
        if let Err(error) = &result {
            self.lease.retain_failure(&mut state.write(), error);
        }
        result
    }
    async fn remote_inner(
        &self,
        state: Signal<ShellState>,
        raw_query: &str,
    ) -> Result<Acquired, Error> {
        let ctx = &self.context;
        let gantry = self
            .request(state, grid::discovery(&ctx.client, &ctx.session))
            .await?;
        let hints = facts::request_facts(&gantry, &ctx.session, None, self.ranking.offline())?;
        let query = channels::Query::capture(&ctx.session, raw_query, hints)?;
        let mut endpoint: Option<DirectoryEndpoint> = None;
        let mut pages = channels::Pages::default();
        loop {
            if !self.can_dispatch(&state.peek()) {
                return Err(unavailable());
            }
            if endpoint
                .as_ref()
                .is_none_or(|e| !e.current(&ctx.session, Instant::now()))
            {
                let raw = self
                    .request(
                        state,
                        DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?,
                    )
                    .await?;
                endpoint = Some(DirectoryEndpoint::decode(
                    &ctx.session,
                    &raw,
                    Instant::now(),
                )?);
            }
            let endpoint = endpoint.as_ref().ok_or_else(unavailable)?;
            let request = query.request(
                &ctx.client,
                &ctx.session,
                endpoint,
                Instant::now(),
                pages.marker(),
            )?;
            // Capture the original publication before each dispatched page.
            let receipt =
                ingress::Read::capture(state, self.lease.team(), &|s| self.can_dispatch(s))?;
            let response = tokio::select! {
                biased;
                _ = self.lease.cancelled() => Err(unavailable()),
                result = ctx.transport.execute_profile_channel_search_current(request, || {
                    let shell = state.peek();
                    self.can_dispatch(&shell) && receipt.current(&shell) && endpoint.current(&ctx.session, Instant::now())
                }) => result,
            }?;
            if !self.current(&state.peek()) || !endpoint.current(&ctx.session, Instant::now()) {
                return Err(unavailable());
            }
            let (raw, keys) = response.into_parts();
            let page = channels::Page::decode_ordered(raw, keys.as_deref())?;
            let rows = page.rows().to_vec();
            // Validate the accumulation before publishing this page. Preserve
            // all raw outcomes; none authorizes fabricated membership/counts.
            pages.append(page)?;
            let source = receipt.publish(state, rows, |s| self.current(s)).await?;
            if pages.complete() {
                if !self.current(&state.peek())
                    || !source.current_for(&state.peek(), self.lease.team())
                {
                    return Err(unavailable());
                }
                return Ok(Acquired { pages, source });
            }
        }
    }
}

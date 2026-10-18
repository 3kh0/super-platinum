//! Original-query member acquisition. The complete page set enters the existing
//! shared member store once; an HTTP page never becomes replacement Boot data.
use super::*;
use crate::directory_member_fetches::search as intake;
use super_platinum_core::slack::api::custom::texty::members;

pub(super) struct Acquired {
    pub(super) pages: members::Pages,
    pub(super) source: Arc<intake::Snapshot>,
}

pub(super) async fn remote(
    mut state: Signal<ShellState>,
    search: &Search,
    names: &member_names::Observation,
    raw_query: &str,
) -> Result<Acquired, Error> {
    let result = read(state, search, names, raw_query).await;
    if let Err(error) = &result {
        search.lease.retain_failure(&mut state.write(), error);
    }
    result
}
async fn read(
    state: Signal<ShellState>,
    search: &Search,
    names: &member_names::Observation,
    raw_query: &str,
) -> Result<Acquired, Error> {
    let ctx = &search.context;
    if !names.current(&state.peek(), search) || !search.can_dispatch(&state.peek()) {
        return Err(unavailable());
    }
    let query = members::Query::capture(
        &ctx.session,
        raw_query,
        members::RequestFacts {
            top_users: Some(serde_json::json!(
                search
                    .ranking
                    .offline()
                    .processed_top_users()
                    .map_err(|_| unavailable())?
            )),
            default_workspace: names.default_workspace.clone(),
        },
    )?;
    // Capture after name-policy Boot acquisition and before the first HTTP page.
    // Never recapture this source witness to make a late response look current.
    let receipt = intake::Read::capture(
        &state.peek(),
        search.lease.team(),
        &search.lease.member_generation(),
    )?;
    let current = |shell: &ShellState| {
        search.can_dispatch(shell) && names.current(shell, search) && receipt.current(shell)
    };
    let mut endpoint: Option<DirectoryEndpoint> = None;
    let mut pages = members::Pages::default();
    loop {
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        if endpoint
            .as_ref()
            .is_none_or(|e| !e.current(&ctx.session, Instant::now()))
        {
            let raw = search
                .request(
                    state,
                    DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?,
                )
                .await?;
            if !current(&state.peek()) {
                return Err(unavailable());
            }
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
        let response = if search.fixture {
            tokio::select! {
                biased;
                _ = search.lease.cancelled() => Err(unavailable()),
                result = fixture::completion::search(state, &request) => result,
            }
        } else {
            tokio::select! {
                biased;
                _ = search.lease.cancelled() => Err(unavailable()),
                result = ctx.transport.execute_profile_member_search_current(request, || current(&state.peek()) && endpoint.current(&ctx.session, Instant::now())) => result.map(|r|r.into_parts()),
            }
        }?;
        if !current(&state.peek()) || !endpoint.current(&ctx.session, Instant::now()) {
            return Err(unavailable());
        }
        pages.accept(response.0, response.1.as_deref())?;
        if pages.complete() {
            let source = receipt
                .publish(state, &pages, |shell| {
                    search.current(shell) && names.current(shell, search)
                })
                .await?;
            if !search.current(&state.peek())
                || !source.current(&state.peek())
                || !names.current(&state.peek(), search)
            {
                return Err(unavailable());
            }
            return Ok(Acquired {
                pages,
                source: Arc::new(source),
            });
        }
    }
}

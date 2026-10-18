//! Directory receipts only. Neither successful HTTP nor Local creates a row
//! selection permit; the caller next hydrates and captures fresh local stores.
use super::*;
use super_platinum_core::block_actions::{
    directory_endpoint::DirectoryEndpoint,
    workflow_management::activity_members::{Page as MemberPage, Pages},
};

pub(super) enum Acquired {
    Local,
    Remote(Pages),
}
pub(super) async fn request(
    mut state: Signal<ShellState>,
    q: &Query,
    request: super_platinum_core::slack::PreparedRequest,
) -> Result<Value, Error> {
    if !q.can_dispatch(&state.peek())
        || ((q.fixture || crate::fixture::is_fixture()) && !fixture::enabled(&state.peek()))
    {
        return Err(unavailable());
    }
    native::query(state, q).await?;
    let result = if q.fixture || crate::fixture::is_fixture() {
        fixture::request(&state.peek(), &request)
    } else {
        tokio::select! {
            result=q.owner.ctx.transport.execute(request)=>result,
            _=q.retired(state)=>Err(unavailable()),
        }
    };
    if let Err(Error::RateLimited { retry_after_secs }) = &result {
        navigation::retain_limit(&mut state.write(), &q.owner.ctx, *retry_after_secs);
    }
    let raw = result?;
    if !q.current(&state.peek()) {
        return Err(unavailable());
    }
    native::query(state, q).await?;
    Ok(raw)
}
pub(super) async fn acquire(mut state: Signal<ShellState>, q: &Query) -> Result<Acquired, Error> {
    let result = read(state, q).await;
    // Owner-bound cooldown survives UI retirement; it does not adopt rows into
    // the new query. Keep network failures distinct from an empty local search.
    if let Err(Error::RateLimited { retry_after_secs }) = &result {
        navigation::retain_limit(&mut state.write(), &q.owner.ctx, *retry_after_secs);
    }
    result
}
async fn read(state: Signal<ShellState>, q: &Query) -> Result<Acquired, Error> {
    if !q.current(&state.peek()) {
        return Err(unavailable());
    }
    if q.path() == SearchPath::Local {
        return Ok(Acquired::Local);
    }
    // Named fixtures never dispatch to a real server, even if an unrelated
    // transport later becomes available in the process.
    if (q.fixture || crate::fixture::is_fixture()) && !fixture::enabled(&state.peek()) {
        return Err(unavailable());
    }
    let ctx = &q.owner.ctx;
    let mut endpoint: Option<DirectoryEndpoint> = None;
    let mut pages = Pages::default();
    loop {
        if !q.can_dispatch(&state.peek()) {
            return Err(unavailable());
        }
        if !endpoint
            .as_ref()
            .is_some_and(|e| e.current(&ctx.session, Instant::now()))
        {
            let plan = DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?;
            if !q.can_dispatch(&state.peek()) {
                return Err(unavailable());
            }
            let raw = request(state, q, plan).await?;
            if !q.current(&state.peek()) {
                return Err(unavailable());
            }
            endpoint = Some(DirectoryEndpoint::decode(
                &ctx.session,
                &raw,
                Instant::now(),
            )?);
        }
        let endpoint = endpoint.as_ref().ok_or_else(unavailable)?;
        let request = q.member.request(
            &ctx.client,
            &ctx.session,
            endpoint,
            Instant::now(),
            pages.marker(),
        )?;
        if !q.can_dispatch(&state.peek()) {
            return Err(unavailable());
        }
        native::query(state, q).await?;
        let response = if q.fixture || crate::fixture::is_fixture() {
            tokio::select! {
                biased;
                result=fixture::search(state,&request)=>result,
                _=q.retired(state)=>Err(unavailable()),
            }
        } else {
            tokio::select! {
                result=ctx.transport.execute_activity_member_search_current(request,||q.can_dispatch(&state.peek()))=>result.map(|response|response.into_parts()),
                _=q.retired(state)=>Err(unavailable()),
            }
        }?;
        if !q.current(&state.peek()) || !endpoint.current(&ctx.session, Instant::now()) {
            return Err(unavailable());
        }
        let (raw, keys) = response;
        native::query(state, q).await?;
        pages.append(MemberPage::decode_ordered(raw, keys.as_deref())?)?;
        if pages.complete() {
            return Ok(Acquired::Remote(pages));
        }
        // The core accumulator bounds repeated markers/pages. No empty-success
        // or local fallback hides a failed, malformed or incomplete read.
    }
}

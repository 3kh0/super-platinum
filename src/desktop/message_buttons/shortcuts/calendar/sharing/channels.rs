//! One provider completes before ingress; parent retains partial-provider policy.
use super::*;
use crate::message_buttons::inputs::user::directory::read_scope::ReadScope;
use serde_json::{Value, json};
use std::future::Future;
use super_platinum_core::{
    block_actions::directory_endpoint::DirectoryEndpoint, slack::PreparedRequest,
};

fn body(query: &str, marker: Option<&str>) -> Value {
    let [_, (_, mut body), _] = queries(query);
    body["check_membership"] = json!(true);
    if let Some(marker) = marker {
        body["marker"] = json!(marker);
    }
    body
}
fn scope(
    state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
    endpoint: Option<Arc<DirectoryEndpoint>>,
) -> ReadScope {
    let owner = ctx.clone();
    let observed = ctx.clone();
    ReadScope::new(
        move || {
            state
                .try_read()
                .ok()
                .is_some_and(|s| owned_current(&s, id, generation, &owner))
        },
        move || {
            state.try_read().ok().is_some_and(|s| {
                owned_current(&s, id, generation, &ctx)
                    && !cooling(&s, &ctx)
                    && endpoint
                        .as_ref()
                        .is_none_or(|e| e.current(&ctx.session, Instant::now()))
            })
        },
        move |result| {
            let mut state = state;
            if let Ok(mut s) = state.try_write() {
                retain_limit(&mut s, &observed, result.as_ref().err());
            }
        },
    )
}
pub(super) async fn read<A>(
    state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
    query: &str,
    accepted: A,
) -> Result<Option<Vec<Channel>>, Error>
where
    A: FnOnce(&[Channel]),
{
    let original = ctx.clone();
    run_with(
        state,
        id,
        generation,
        &ctx,
        query,
        move |request, scope| {
            let ctx = original.clone();
            async move { scope.transport(&ctx, request).await }
        },
        accepted,
    )
    .await
}
async fn run_with<F, Fut, A>(
    mut state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: &Arc<Context>,
    query: &str,
    mut execute: F,
    accepted: A,
) -> Result<Option<Vec<Channel>>, Error>
where
    F: FnMut(PreparedRequest, ReadScope) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
    A: FnOnce(&[Channel]),
{
    let discovery = scope(state, id, generation, ctx.clone(), None);
    if query.len() > 4096 || !discovery.admitted() {
        return Ok(None);
    }
    let cached = {
        let s = state.peek();
        active(&s, id)
            .and_then(|c| c.directory_endpoint.as_ref())
            .filter(|e| e.current(&ctx.session, Instant::now()))
            .cloned()
    };
    let endpoint = match cached {
        Some(endpoint) => endpoint,
        None => {
            let received = execute(
                DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?,
                discovery.clone(),
            )
            .await;
            if !discovery.current() {
                return Ok(None);
            }
            let Some(raw) = received? else {
                return Ok(None);
            };
            let endpoint = Arc::new(DirectoryEndpoint::decode(
                &ctx.session,
                &raw,
                Instant::now(),
            )?);
            let mut s = state.write();
            if !owned_current(&s, id, generation, ctx) {
                return Ok(None);
            }
            active_mut(&mut s, id).unwrap().directory_endpoint = Some(endpoint.clone());
            endpoint
        }
    };
    let pages = scope(state, id, generation, ctx.clone(), Some(endpoint.clone()));
    let received = api::fetch_calendar_sharing_channel_rows_with(
        |marker| {
            if !pages.admitted() {
                return Ok(None);
            }
            let now = Instant::now();
            if !endpoint.current(&ctx.session, now) {
                return Ok(None);
            }
            endpoint
                .request(
                    &ctx.client,
                    &ctx.session,
                    now,
                    "channels/search",
                    body(query, marker),
                )
                .map(Some)
        },
        |request| execute(request, pages.clone()),
        || pages.current(),
    )
    .await;
    if !pages.current() {
        return Ok(None);
    }
    let Some(rows) = received? else {
        return Ok(None);
    };
    let channels: Vec<Channel> = rows
        .into_iter()
        .filter_map(|row| serde_json::from_value(row).ok())
        .collect();
    if !pages.current() {
        return Ok(None);
    }
    accepted(&channels);
    Ok(Some(channels))
}

#[cfg(test)]
mod tests;

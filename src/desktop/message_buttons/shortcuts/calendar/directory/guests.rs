//! Calendar's observed endpoint and shortcut budget, with a bounded raw24 read.
use super::*;
use crate::message_buttons::inputs::user::directory::read_scope::ReadScope;
use serde_json::{Value, json};
use std::future::Future;
use super_platinum_core::slack::{PreparedRequest, models::User};

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
                .is_some_and(|s| current(&s, id, Query::Guests(generation), &owner))
        },
        move || {
            state.try_read().ok().is_some_and(|s| {
                current(&s, id, Query::Guests(generation), &ctx)
                    && !cooling(&s, &ctx)
                    && endpoint
                        .as_ref()
                        .is_none_or(|e| e.current(&ctx.session, Instant::now()))
            })
        },
        move |result| {
            let mut state = state;
            if let Ok(mut s) = state.try_write() {
                // Preserve the original shortcut account-owned observation policy.
                retain_limit(&mut s, &observed, result.as_ref().err());
            }
        },
    )
}

pub(in crate::message_buttons::shortcuts::calendar) async fn read(
    state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
    query: &str,
) -> Result<Option<Vec<User>>, Error> {
    let original = ctx.clone();
    read_with(state, id, generation, &ctx, query, move |request, scope| {
        let ctx = original.clone();
        async move { scope.transport(&ctx, request).await }
    })
    .await
}

async fn read_with<F, Fut>(
    mut state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: &Arc<Context>,
    query: &str,
    mut execute: F,
) -> Result<Option<Vec<User>>, Error>
where
    F: FnMut(PreparedRequest, ReadScope) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
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
            let request = DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?;
            let received = execute(request, discovery.clone()).await;
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
            if !current(&s, id, Query::Guests(generation), ctx) {
                return Ok(None);
            }
            active_mut(&mut s, id).unwrap().directory_endpoint = Some(endpoint.clone());
            endpoint
        }
    };
    let pages = scope(state, id, generation, ctx.clone(), Some(endpoint.clone()));
    api::fetch_users_selector_rows_with(
        |marker| {
            if !pages.admitted() {
                return Ok(None);
            }
            let now = Instant::now();
            if !endpoint.current(&ctx.session, now) {
                return Ok(None);
            }
            let mut body = json!({"query":query,"count":24,"fuzz":1,"enable_workspace_ranking":true,
                "filter":"NOT deactivated","include_profile_only_users":true});
            if let Some(marker) = marker {
                body["marker"] = json!(marker);
            }
            endpoint
                .request(&ctx.client, &ctx.session, now, "users/search", body)
                .map(Some)
        },
        |request| {
            let read = execute(request, pages.clone());
            async move {
                let received = read.await?;
                if received
                    .as_ref()
                    .and_then(|v| v.get("results"))
                    .and_then(Value::as_array)
                    .is_some_and(|rows| rows.len() > 512)
                {
                    return Err(cal::invalid());
                }
                Ok(received)
            }
        },
        || pages.current(),
    )
    .await
    .map(|rows| {
        rows.map(|rows| {
            rows.into_iter()
                .filter_map(|raw| serde_json::from_value(raw).ok())
                .collect()
        })
    })
}

#[cfg(test)]
mod tests;

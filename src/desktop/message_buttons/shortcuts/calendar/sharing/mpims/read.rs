//! One actual first request has independent legacy and source-owned positive outcomes.
use super::*;
use crate::message_buttons::inputs::user::directory::read_scope::ReadScope;
use std::{future::Future, rc::Rc};
use super_platinum_core::{
    block_actions::directory_endpoint::DirectoryEndpoint, slack::PreparedRequest,
};
struct Proof {
    source: Arc<canonical::Canonical>,
    read: intake::Read,
    guarded: BTreeSet<String>,
}
impl Proof {
    fn current(&self, s: &ShellState, ctx: &Context) -> bool {
        self.source.current_for(s, &ctx.session.team_id) && self.read.current(s)
    }
}
fn failure(mut outcome: Outcome) -> Outcome {
    outcome.failed = true;
    outcome
}
fn scope(
    state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
    endpoint: Option<Arc<DirectoryEndpoint>>,
    proof: Option<Rc<Proof>>,
) -> ReadScope {
    let owner = ctx.clone();
    let observed = ctx.clone();
    ReadScope::new(
        move || {
            state.try_read().ok().is_some_and(|s| {
                owned_current(&s, id, generation, &owner)
                    && proof.as_ref().is_none_or(|p| p.current(&s, &owner))
            })
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
fn body(query: &str, marker: Option<&str>) -> Value {
    let [_, _, (_, mut body)] = queries(query);
    if let Some(marker) = marker {
        body["marker"] = Value::String(marker.into());
    }
    body
}
fn legacy(raw: &Value) -> (Vec<Channel>, bool) {
    let Some(rows) = raw
        .get("mpims")
        .or_else(|| raw.get("results"))
        .and_then(Value::as_array)
        .filter(|v| v.len() <= 512)
    else {
        return (Vec::new(), false);
    };
    (
        rows.iter()
            .take(100)
            .filter_map(crate::message_buttons::inputs::user::directory::decode_mpim)
            .collect(),
        true,
    )
}
pub(in crate::message_buttons::shortcuts::calendar::sharing) async fn read<A>(
    state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
    query: &str,
    accepted: A,
) -> Outcome
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
pub(super) async fn run_with<F, Fut, A>(
    mut state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: &Arc<Context>,
    query: &str,
    mut execute: F,
    accepted: A,
) -> Outcome
where
    F: FnMut(PreparedRequest, ReadScope) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
    A: FnOnce(&[Channel]),
{
    let outcome = Outcome {
        legacy: Vec::new(),
        acquired: None,
        failed: false,
    };
    if query.len() > 4096 || !owned_current(&state.peek(), id, generation, ctx) {
        return failure(outcome);
    }
    // No await between this original observation and Read capture. The Read's
    // own serial predicate rechecks the exact original canonical observation.
    let guarded = active(&state.peek(), id).unwrap().mpims.origins.ids();
    let original = if query.trim().is_empty() {
        None
    } else {
        canonical::observed(&state.peek(), &ctx.session.team_id).ok()
    };
    let proof = original.and_then(|source| {
        intake::Read::capture(state, &ctx.session.team_id, &|s| {
            owned_current(s, id, generation, ctx) && source.current_for(s, &ctx.session.team_id)
        })
        .ok()
        .map(|read| {
            Rc::new(Proof {
                source,
                read,
                guarded: guarded.clone(),
            })
        })
    });
    let discovery = scope(state, id, generation, ctx.clone(), None, None);
    if !discovery.admitted() {
        return failure(outcome);
    }
    let cached = active(&state.peek(), id)
        .and_then(|c| c.directory_endpoint.as_ref())
        .filter(|e| e.current(&ctx.session, Instant::now()))
        .cloned();
    let endpoint = match cached {
        Some(endpoint) => endpoint,
        None => {
            let request = match DirectoryEndpoint::discovery(&ctx.client, &ctx.session) {
                Ok(r) => r,
                Err(_) => return failure(outcome),
            };
            let received = execute(request, discovery.clone()).await;
            if !discovery.current() {
                return failure(outcome);
            }
            let raw = match received {
                Ok(Some(raw)) => raw,
                _ => return failure(outcome),
            };
            let endpoint = match DirectoryEndpoint::decode(&ctx.session, &raw, Instant::now()) {
                Ok(e) => Arc::new(e),
                Err(_) => return failure(outcome),
            };
            let mut s = state.write();
            if !owned_current(&s, id, generation, ctx) {
                return failure(outcome);
            }
            active_mut(&mut s, id).unwrap().directory_endpoint = Some(endpoint.clone());
            endpoint
        }
    };
    let first_scope = scope(
        state,
        id,
        generation,
        ctx.clone(),
        Some(endpoint.clone()),
        None,
    );
    if !first_scope.admitted() {
        return failure(outcome);
    }
    let request = match endpoint.request(
        &ctx.client,
        &ctx.session,
        Instant::now(),
        "mpims/search",
        body(query, None),
    ) {
        Ok(r) => r,
        Err(_) => return failure(outcome),
    };
    let received = execute(request, first_scope.clone()).await;
    if !first_scope.current() {
        return failure(outcome);
    }
    let first = match received {
        Ok(Some(raw)) => raw,
        _ => return failure(outcome),
    };
    let (decoded, valid) = legacy(&first);
    // Classify before dedup or guard denial. Do not manufacture a legacy
    // contribution from the positive results-then-mpims aggregate.
    let legacy_ids: BTreeSet<_> = decoded.iter().map(|v| v.id.clone()).collect();
    accepted(&decoded);
    if !owned_current(&state.peek(), id, generation, ctx) {
        return failure(outcome);
    }
    let mut outcome = Outcome {
        legacy: decoded
            .into_iter()
            .filter(|v| !guarded.contains(&v.id))
            .collect(),
        acquired: None,
        failed: !valid,
    };
    let Some(proof) = proof else {
        return outcome;
    };
    if !proof.current(&state.peek(), ctx) {
        return failure(outcome);
    }
    let pages = scope(
        state,
        id,
        generation,
        ctx.clone(),
        Some(endpoint.clone()),
        Some(proof.clone()),
    );
    let rows = api::fetch_calendar_sharing_mpim_rows_after_with(
        first,
        |marker| {
            if !pages.admitted() {
                return Ok(None);
            }
            endpoint
                .request(
                    &ctx.client,
                    &ctx.session,
                    Instant::now(),
                    "mpims/search",
                    body(query, marker),
                )
                .map(Some)
        },
        |request| execute(request, pages.clone()),
        || pages.current(),
    )
    .await;
    if !owned_current(&state.peek(), id, generation, ctx) {
        return failure(outcome);
    }
    let rows = match rows {
        Ok(Some(rows)) => rows,
        _ => return failure(outcome),
    };
    drop(pages);
    drop(first_scope);
    drop(discovery);
    let proof = match Rc::try_unwrap(proof) {
        Ok(p) => p,
        Err(_) => return failure(outcome),
    };
    let mut converted = Vec::new();
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    {
        let s = state.peek();
        if !owned_current(&s, id, generation, ctx) || !proof.current(&s, ctx) {
            return failure(outcome);
        }
        let source: HashMap<_, _> = proof
            .source
            .rows()
            .iter()
            .filter_map(|v| v["id"].as_str().map(|id| (id, v)))
            .collect();
        for raw in &rows {
            let Some(target) = raw
                .get("id")
                .and_then(Value::as_str)
                .filter(|v| valid_id(v))
            else {
                return failure(outcome);
            };
            // Original independent legacy rows stay independent. An already
            // guarded ID may refresh only through this new canonical path.
            if legacy_ids.contains(target) && !proof.guarded.contains(target) {
                continue;
            }
            let Some(cached) = source.get(target) else {
                continue;
            };
            let row = match api::convert_calendar_known_open_mpim(cached, raw) {
                Ok(Some(row)) => row,
                Ok(None) => continue,
                Err(_) => return failure(outcome),
            };
            converted.push(row);
            if seen.insert(target.to_owned()) {
                ids.push(target.to_owned());
            }
        }
    }
    if ids.is_empty() {
        return outcome;
    }
    {
        let mut s = state.write();
        if !owned_current(&s, id, generation, ctx) || !proof.current(&s, ctx) {
            return failure(outcome);
        }
        let registration = match active(&s, id).unwrap().mpims.origins.propose(&s, ctx, &ids) {
            Ok(r) => r,
            Err(_) => return failure(outcome),
        };
        active_mut(&mut s, id)
            .unwrap()
            .mpims
            .origins
            .commit(registration);
    }
    // The original consuming receipt guards acceptance. Its preimage must not
    // become the post-enqueue predicate; accepted shared work keeps its lifetime.
    let published = proof
        .read
        .publish(state, converted, |s| owned_current(s, id, generation, ctx))
        .await;
    let source = match published {
        Ok(source) => source,
        Err(_) => return failure(outcome),
    };
    if !owned_current(&state.peek(), id, generation, ctx)
        || !source.current_for(&state.peek(), &ctx.session.team_id)
    {
        return failure(outcome);
    }
    outcome.acquired = Some(Acquired { source, ids });
    outcome
}

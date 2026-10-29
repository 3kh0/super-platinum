//! Owned raw24 reads; broader Conversation fanout retains its independent path.
use super::read_scope::ReadScope;
use super::*;
use std::{future::Future, sync::Arc};
use super_platinum_core::slack::PreparedRequest;

fn scope(
    state: Signal<ShellState>,
    context: Arc<Context>,
    revision: u64,
    query_revision: u64,
) -> ReadScope {
    let c = context.clone();
    let a = context.clone();
    ReadScope::new(
        move || {
            state.try_read().ok().is_some_and(|s| {
                context_current(&s, &c) && request_current(&s, revision, query_revision)
            })
        },
        move || {
            let waiting = {
                let Ok(s) = state.try_read() else {
                    return false;
                };
                if !context_current(&s, &a) || !request_current(&s, revision, query_revision) {
                    return false;
                }
                cooling(&s, a.epoch, &a.session.team_id)
            };
            if !waiting {
                return true;
            }
            let mut state = state;
            let Ok(mut s) = state.try_write() else {
                return false;
            };
            admit_query(&mut s, &a, revision, query_revision)
        },
        move |result| {
            let Err(Error::RateLimited {
                retry_after_secs: Some(seconds),
            }) = result
            else {
                return;
            };
            let mut state = state;
            let Ok(mut s) = state.try_write() else {
                return;
            };
            if !owner_current(&s, &context) {
                return;
            }
            let now = std::time::Instant::now();
            let deadline = now
                .checked_add(std::time::Duration::from_secs((*seconds).max(1)))
                .unwrap_or(now + std::time::Duration::from_secs(86400));
            remember_cooldown(&mut s, context.epoch, &context.session.team_id, deadline);
        },
    )
}
pub(super) async fn run_with<F, Fut>(
    state: Signal<ShellState>,
    context: Arc<Context>,
    revision: u64,
    query_revision: u64,
    query: String,
    mut execute: F,
) where
    F: FnMut(PreparedRequest, ReadScope) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
{
    let scope = scope(state, context.clone(), revision, query_revision);
    let received = scope
        .lookup(
            |marker| prepare_users_page(&context, &query, marker),
            |request| execute(request, scope.clone()),
        )
        .await;
    if !scope.current() {
        return;
    }
    let result = match received {
        Ok(Some(users)) => Ok((users, Vec::new())),
        Err(error) => Err(error),
        Ok(None) => return,
    };
    finish_read(
        state,
        &context,
        revision,
        query_revision,
        &query,
        result,
        false,
    );
}

pub(super) async fn run_channels_with<F, Fut, A>(
    state: Signal<ShellState>,
    context: Arc<Context>,
    revision: u64,
    query_revision: u64,
    query: String,
    filter: super::super::conversations::Filter,
    mut execute: F,
    accepted_channels: A,
) where
    F: FnMut(PreparedRequest, ReadScope) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
    A: FnOnce(&[super_platinum_core::slack::models::Channel]),
{
    let scope = scope(state, context.clone(), revision, query_revision);
    let received = scope
        .lookup_channels(
            |marker| prepare_public_channels_page(&context, &query, &filter, marker),
            |request| execute(request, scope.clone()),
        )
        .await;
    if !scope.current() {
        return;
    }
    let result = match received {
        Ok(Some(channels)) => Ok((Vec::new(), channels)),
        Err(error) => Err(error),
        Ok(None) => return,
    };
    // Preserve the old accepted terminal-success ingress, including an empty batch.
    if let Ok((_, channels)) = &result {
        if !scope.current() {
            return;
        }
        accepted_channels(channels);
    }
    finish_read(
        state,
        &context,
        revision,
        query_revision,
        &query,
        result,
        false,
    );
}

#[cfg(test)]
mod channels_tests;

pub(super) async fn run_conversations_with<F, Fut, A>(
    state: Signal<ShellState>,
    context: Arc<Context>,
    revision: u64,
    query_revision: u64,
    query: String,
    filter: super::super::conversations::Filter,
    execute: F,
    accepted_channels: A,
) where
    F: Fn(fan_in::Provider, PreparedRequest, ReadScope, Option<fan_in::InitialGate>) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
    A: FnOnce(&[super_platinum_core::slack::models::Channel]),
{
    let scope = scope(state, context.clone(), revision, query_revision);
    let received = fan_in::read_with(&scope, &context, &query, &filter, execute).await;
    if !scope.current() {
        return;
    }
    let result = match received {
        Ok(Some(rows)) => Ok(rows),
        Err(error) => Err(error),
        Ok(None) => return,
    };
    if let Ok((_, channels)) = &result {
        if !scope.current() {
            return;
        }
        accepted_channels(channels);
    }
    finish_read(
        state,
        &context,
        revision,
        query_revision,
        &query,
        result,
        false,
    );
}
#[cfg(test)]
mod tests;

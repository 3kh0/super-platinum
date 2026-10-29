//! Owned raw24 completion; local selection and broader filtered discovery are separate.
use super::*;
use directory::read_scope::ReadScope;
use std::future::Future;
use super_platinum_core::slack::PreparedRequest;

fn scope(
    state: Signal<ShellState>,
    ticket: &Ticket,
    context: Arc<Context>,
    serial: u64,
    revision: u64,
) -> ReadScope {
    let c = context.clone();
    let a = context.clone();
    let admitted = ticket.clone();
    let observed = ticket.clone();
    ReadScope::new(
        move || {
            state.try_read().ok().is_some_and(|s| {
                request_current(&s, serial, revision) && directory::context_current(&s, &c)
            })
        },
        move || {
            let waiting = {
                let Ok(s) = state.try_read() else {
                    return false;
                };
                if !request_current(&s, serial, revision) || !directory::context_current(&s, &a) {
                    return false;
                }
                cooling(&s, &admitted)
            };
            if !waiting {
                return true;
            }
            let mut state = state;
            let Ok(mut s) = state.try_write() else {
                return false;
            };
            if !request_current(&s, serial, revision) || !directory::context_current(&s, &a) {
                return false;
            }
            if cooling(&s, &admitted) {
                let p = s.app_modal.as_mut().unwrap().users.as_mut().unwrap();
                p.loading = false;
                p.error = Some(
                    "Slack asked us to wait before searching again. Cached choices are shown."
                        .into(),
                );
                return false;
            }
            true
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
            if directory::owner_current(&s, &context) {
                cooldown(&mut s, &observed, *seconds);
            }
        },
    )
}
pub(super) async fn run_with<F, Fut>(
    mut state: Signal<ShellState>,
    ticket: Ticket,
    context: Arc<Context>,
    serial: u64,
    revision: u64,
    query: String,
    mut execute: F,
) where
    F: FnMut(PreparedRequest, ReadScope) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
{
    let scope = scope(state, &ticket, context.clone(), serial, revision);
    let received = scope
        .lookup(
            |marker| directory::prepare_users_page(&context, &query, marker),
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
    let Ok(mut s) = state.try_write() else {
        return;
    };
    if directory::owner_current(&s, &context) {
        complete_entities(&mut s, serial, revision, result);
    }
}

pub(super) async fn run_channels_with<F, Fut, A>(
    mut state: Signal<ShellState>,
    ticket: Ticket,
    context: Arc<Context>,
    serial: u64,
    revision: u64,
    query: String,
    filter: conversations::Filter,
    mut execute: F,
    accepted_channels: A,
) where
    F: FnMut(PreparedRequest, ReadScope) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
    A: FnOnce(&[Channel]),
{
    let scope = scope(state, &ticket, context.clone(), serial, revision);
    let received = scope
        .lookup_channels(
            |marker| directory::prepare_public_channels_page(&context, &query, &filter, marker),
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
    if let Ok((_, channels)) = &result {
        if !scope.current() {
            return;
        }
        accepted_channels(channels);
    }
    let Ok(mut s) = state.try_write() else {
        return;
    };
    if directory::owner_current(&s, &context) {
        complete_entities(&mut s, serial, revision, result);
    }
}

#[cfg(test)]
mod channels_tests;

pub(super) async fn run_conversations_with<F, Fut, A>(
    mut state: Signal<ShellState>,
    ticket: Ticket,
    context: Arc<Context>,
    serial: u64,
    revision: u64,
    query: String,
    filter: conversations::Filter,
    execute: F,
    accepted_channels: A,
) where
    F: Fn(
        directory::fan_in::Provider,
        PreparedRequest,
        ReadScope,
        Option<directory::fan_in::InitialGate>,
    ) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
    A: FnOnce(&[Channel]),
{
    let scope = scope(state, &ticket, context.clone(), serial, revision);
    let received = directory::fan_in::read_with(&scope, &context, &query, &filter, execute).await;
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
    let Ok(mut s) = state.try_write() else {
        return;
    };
    if directory::owner_current(&s, &context) {
        complete_entities(&mut s, serial, revision, result);
    }
}
#[cfg(test)]
mod tests;

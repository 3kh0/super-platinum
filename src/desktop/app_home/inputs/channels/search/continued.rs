//! Ordinary Home channels complete one raw24 lookup under the original query owner.
use super::*;
use directory::read_scope::ReadScope;
use std::future::Future;
use super_platinum_core::slack::PreparedRequest;

pub(super) fn scope(
    state: Signal<ShellState>,
    ticket: &Ticket,
    serial: u64,
    revision: u64,
) -> ReadScope {
    let current_ticket = ticket.clone();
    let admitted_ticket = ticket.clone();
    let observed_ticket = ticket.clone();
    ReadScope::new(
        move || {
            state.try_read().ok().is_some_and(|s| {
                request_current(&s, serial, revision)
                    && directory::context_current(&s, &current_ticket.owner.ctx)
            })
        },
        move || {
            let waiting = {
                let Ok(s) = state.try_read() else {
                    return false;
                };
                if !request_current(&s, serial, revision)
                    || !directory::context_current(&s, &admitted_ticket.owner.ctx)
                {
                    return false;
                }
                cooling(&s, &admitted_ticket)
            };
            if !waiting {
                return true;
            }
            let mut state = state;
            let Ok(mut s) = state.try_write() else {
                return false;
            };
            if !request_current(&s, serial, revision)
                || !directory::context_current(&s, &admitted_ticket.owner.ctx)
            {
                return false;
            }
            if cooling(&s, &admitted_ticket) {
                let p = s.app_home.channels.picker.as_mut().unwrap();
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
            // Channel reads retain their original query-gated cooldown policy.
            if directory::owner_current(&s, &observed_ticket.owner.ctx)
                && request_current(&s, serial, revision)
            {
                cooldown(&mut s, &observed_ticket, *seconds);
            }
        },
    )
}

pub(super) async fn run_with<F, Fut>(
    mut state: Signal<ShellState>,
    ticket: Ticket,
    serial: u64,
    revision: u64,
    query: String,
    mut execute: F,
) where
    F: FnMut(PreparedRequest, ReadScope) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
{
    let scope = scope(state, &ticket, serial, revision);
    let result = scope
        .lookup_channels(
            |marker| {
                directory::prepare_channels_page(&ticket.owner.ctx, &query, &ticket.policy, marker)
            },
            |request| execute(request, scope.clone()),
        )
        .await;
    if !scope.current() {
        return;
    }
    let result = match result {
        Ok(Some(channels)) => Ok(channels),
        Err(error) => Err(error),
        Ok(None) => return,
    };
    let Ok(mut s) = state.try_write() else {
        return;
    };
    if directory::owner_current(&s, &ticket.owner.ctx) {
        complete(&mut s, serial, revision, result);
    }
}

#[cfg(test)]
mod tests;

fn initial_requests(
    ticket: &Ticket,
    query: &str,
    filter: &conversations::Filter,
) -> Result<directory::aggregate::Requests, Error> {
    let channels = if filter.allows("public") || filter.allows("private") {
        Some(directory::prepare_conversation_page(
            &ticket.owner.ctx,
            query,
            filter,
            false,
            None,
        )?)
    } else {
        None
    };
    let users = if filter.allows("im") {
        Some(directory::prepare_conversation_page(
            &ticket.owner.ctx,
            query,
            filter,
            true,
            None,
        )?)
    } else {
        None
    };
    Ok(directory::aggregate::Requests {
        users: Ok(users),
        channels: Ok(channels),
        mpims: Ok(None),
    })
}
fn finish_conversations(
    mut state: Signal<ShellState>,
    ticket: &Ticket,
    scope: &ReadScope,
    serial: u64,
    revision: u64,
    received: Result<Option<directory::fan_in::Rows>, Error>,
) {
    if !scope.current() {
        return;
    }
    let result = match received {
        Ok(Some(rows)) => Ok(rows),
        Err(error) => Err(error),
        Ok(None) => return,
    };
    let Ok(mut s) = state.try_write() else {
        return;
    };
    if directory::owner_current(&s, &ticket.owner.ctx) {
        complete_entities(&mut s, serial, revision, result);
    }
}

pub(super) async fn run_conversations(
    state: Signal<ShellState>,
    ticket: Ticket,
    serial: u64,
    revision: u64,
    query: String,
) {
    use directory::fan_in::Provider;
    use fixture::exchange::Read;
    use std::rc::Rc;
    let scope = scope(state, &ticket, serial, revision);
    let Some(filter) = ticket
        .conversation
        .clone()
        .filter(directory::conversation_mask_matches)
    else {
        return;
    };
    if !scope.admitted() {
        return;
    }
    let bundle = if crate::fixture::is_fixture() {
        let requests = match initial_requests(&ticket, &query, &filter) {
            Ok(requests) => requests,
            Err(error) => {
                finish_conversations(state, &ticket, &scope, serial, revision, Err(error));
                return;
            }
        };
        match fixture::exchange::conversations(state, &ticket, &scope, &requests).await {
            Read::Registered(exchange) => Some(Rc::new(fixture::bundle::Bundle::new(
                exchange,
                scope.clone(),
            ))),
            Read::Refused => return,
            Read::Unavailable(error) => {
                scope.observe_attempt(&Err(error.clone()));
                finish_conversations(state, &ticket, &scope, serial, revision, Err(error));
                return;
            }
            Read::Live => return,
        }
    } else {
        None
    };
    let original = ticket.clone();
    let received = directory::fan_in::read_with(
        &scope,
        &ticket.owner.ctx,
        &query,
        &filter,
        move |provider, request, scope, initial| {
            let bundle = bundle.clone();
            let ticket = original.clone();
            async move {
                if let (Some(bundle), Some(gate)) = (bundle, initial) {
                    return bundle.family(provider, gate).await;
                }
                if !crate::fixture::is_fixture() {
                    return scope.transport(&ticket.owner.ctx, request).await;
                }
                let requests = directory::aggregate::Requests {
                    users: Ok(if provider == Provider::Users {
                        Some(request.clone())
                    } else {
                        None
                    }),
                    channels: Ok(if provider == Provider::Channels {
                        Some(request)
                    } else {
                        None
                    }),
                    mpims: Ok(None),
                };
                match fixture::exchange::conversations(state, &ticket, &scope, &requests).await {
                    Read::Registered(mut exchange) => {
                        scope
                            .finish_injected(async {
                                let envelope = exchange.receive().await?;
                                envelope
                                    .get(provider.key())
                                    .cloned()
                                    .ok_or_else(|| Error::Api("conversation_search_invalid".into()))
                            })
                            .await
                    }
                    Read::Refused => Ok(None),
                    Read::Unavailable(error) => scope.finish_injected(async { Err(error) }).await,
                    Read::Live => Err(Error::Offline("Home fixture read unavailable".into())),
                }
            }
        },
    )
    .await;
    finish_conversations(state, &ticket, &scope, serial, revision, received);
}

#[cfg(test)]
pub(super) async fn run_conversations_with<F, Fut>(
    state: Signal<ShellState>,
    ticket: Ticket,
    serial: u64,
    revision: u64,
    query: String,
    execute: F,
) where
    F: Fn(
        directory::fan_in::Provider,
        PreparedRequest,
        ReadScope,
        Option<directory::fan_in::InitialGate>,
    ) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
{
    let scope = scope(state, &ticket, serial, revision);
    let Some(filter) = ticket
        .conversation
        .clone()
        .filter(directory::conversation_mask_matches)
    else {
        return;
    };
    let received =
        directory::fan_in::read_with(&scope, &ticket.owner.ctx, &query, &filter, execute).await;
    finish_conversations(state, &ticket, &scope, serial, revision, received);
}

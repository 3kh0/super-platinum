//! One owned raw24 lookup; no selection or action authority is granted here.
use super::*;
use directory::read_scope::ReadScope;
use std::future::Future;
use super_platinum_core::slack::PreparedRequest;

fn scope(state: Signal<ShellState>, ticket: &Ticket, serial: u64, revision: u64) -> ReadScope {
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
                let p = s.app_home.users.picker.as_mut().unwrap();
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
            if directory::owner_current(&s, &observed_ticket.owner.ctx) {
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
        .lookup(
            |marker| directory::prepare_users_page(&ticket.owner.ctx, &query, marker),
            |request| execute(request, scope.clone()),
        )
        .await;
    if !scope.current() {
        return;
    }
    let result = match result {
        Ok(Some(users)) => Ok(users),
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

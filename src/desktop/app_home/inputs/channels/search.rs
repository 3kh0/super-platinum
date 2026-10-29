//! Channel search uses the existing shared request, decode, policy and cooldown.
use super::*;
use super_platinum_core::slack::models::Channel;

fn cooling(s: &ShellState, t: &Ticket) -> bool {
    directory::cooling(s, t.owner.ctx.epoch, &t.owner.ctx.session.team_id)
}
pub(crate) fn search(mut state: Signal<ShellState>, ticket: Ticket, query: String) {
    let request = {
        let mut s = state.write();
        if !open_local(&mut s, ticket.clone()) {
            return;
        }
        let choices = cached(&s, &ticket, &query);
        let wait = cooling(&s, &ticket);
        let p = s.app_home.channels.picker.as_mut().unwrap();
        if p.query == query && (p.loading || p.error.is_none()) {
            return;
        }
        p.cancel();
        p.query = query.clone();
        p.query_revision = next();
        p.candidates = choices;
        p.error = wait.then(|| {
            "Slack asked us to wait before searching again. Cached choices are shown.".into()
        });
        p.loading = !query.trim().is_empty() && !wait;
        if !p.loading {
            return;
        }
        let (sender, receiver) = tokio::sync::watch::channel(false);
        p.cancel = Some(sender);
        (p.serial, p.query_revision, receiver)
    };
    spawn(async move {
        let (serial, revision, mut cancellation) = request;
        tokio::select! {_=tokio::time::sleep(Duration::from_millis(250))=>{},_=cancellation.changed()=>return}
        if ticket.conversation.is_none() {
            let original = ticket.clone();
            let read = continued::run_with(
                state,
                ticket,
                serial,
                revision,
                query,
                move |prepared, scope| {
                    let mut state = state;
                    let ticket = original.clone();
                    async move {
                        if !scope.admitted() {
                            return Ok(None);
                        }
                        use fixture::exchange::Read;
                        match fixture::exchange::channel(state, &ticket, &scope, &prepared).await {
                            Read::Live => scope.transport(&ticket.owner.ctx, prepared).await,
                            Read::Refused => Ok(None),
                            Read::Unavailable(error) => {
                                scope.finish_injected(async { Err(error) }).await
                            }
                            Read::Registered(mut exchange) => {
                                scope.finish_injected(exchange.receive()).await
                            }
                        }
                    }
                },
            );
            until_cancelled(read, &mut cancellation).await;
            return;
        }
        if ticket
            .conversation
            .as_ref()
            .is_some_and(directory::conversation_mask_matches)
        {
            let read = continued::run_conversations(state, ticket, serial, revision, query);
            until_cancelled(read, &mut cancellation).await;
            return;
        }
        {
            let mut s = state.write();
            if !request_current(&s, serial, revision) {
                return;
            }
            if cooling(&s, &ticket) {
                let p = s.app_home.channels.picker.as_mut().unwrap();
                p.loading = false;
                p.error = Some(
                    "Slack asked us to wait before searching again. Cached choices are shown."
                        .into(),
                );
                return;
            }
        }
        let scope = continued::scope(state, &ticket, serial, revision);
        let result = tokio::select! {result=conversation::read(state,&ticket,&query,&scope)=>result,_=cancellation.changed()=>return};
        let result = match result {
            Ok(Some(rows)) => Ok(rows),
            Err(error) => Err(error),
            Ok(None) => return,
        };
        let mut s = state.write();
        if !directory::owner_current(&s, &ticket.owner.ctx)
            || !request_current(&s, serial, revision)
        {
            return;
        }
        if let Err(Error::RateLimited {
            retry_after_secs: Some(seconds),
            ..
        }) = &result
        {
            cooldown(&mut s, &ticket, *seconds);
        }
        complete_entities(&mut s, serial, revision, result);
    });
}
mod continued;

async fn until_cancelled(
    read: impl std::future::Future<Output = ()>,
    cancellation: &mut tokio::sync::watch::Receiver<bool>,
) {
    tokio::select! {_=read=>{},_=cancellation.changed()=>{}}
}

#[cfg(test)]
pub(in crate::app_home::inputs::channels) fn fixture_scope(
    state: Signal<ShellState>,
    ticket: &Ticket,
    serial: u64,
    revision: u64,
) -> directory::read_scope::ReadScope {
    continued::scope(state, ticket, serial, revision)
}

fn cooldown(s: &mut ShellState, t: &Ticket, seconds: u64) {
    let now = Instant::now();
    let deadline = now
        .checked_add(Duration::from_secs(seconds.max(1)))
        .unwrap_or(now + Duration::from_secs(86400));
    directory::remember_cooldown(s, t.owner.ctx.epoch, &t.owner.ctx.session.team_id, deadline);
}
pub(super) fn complete(
    s: &mut ShellState,
    serial: u64,
    revision: u64,
    result: Result<Vec<Channel>, Error>,
) -> bool {
    complete_entities(s, serial, revision, result.map(|v| (vec![], v)))
}
pub(super) fn complete_entities(
    s: &mut ShellState,
    serial: u64,
    revision: u64,
    result: Result<(Vec<super_platinum_core::slack::models::User>, Vec<Channel>), Error>,
) -> bool {
    if !request_current(s, serial, revision) {
        return false;
    }
    let p = s.app_home.channels.picker.as_ref().unwrap();
    let ticket = p.ticket.clone();
    let query = p.query.clone();
    match result {
        Ok((users, channels)) => {
            let choices = if ticket.conversation.is_some() {
                conversation::refresh(s, &ticket, &users, &channels);
                conversation::candidates(s, &ticket, &users, &channels, &query)
            } else {
                policy::refresh(s, &ticket, &channels);
                policy::candidates(s, &ticket, &channels, &query)
            };
            let p = s.app_home.channels.picker.as_mut().unwrap();
            p.candidates = choices;
            p.loading = false;
            p.error = None;
        }
        Err(error) => {
            let seconds = match error {
                Error::RateLimited {
                    retry_after_secs: Some(seconds),
                    ..
                } => seconds.max(1),
                _ => 5,
            };
            cooldown(s, &ticket, seconds);
            let p = s.app_home.channels.picker.as_mut().unwrap();
            p.loading = false;
            p.error = Some("Could not search Slack. Cached choices are shown.".into());
        }
    }
    true
}

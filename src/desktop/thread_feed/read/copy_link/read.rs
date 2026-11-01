//! Separately owned exact raw reads and original account Team URL data.
use super::*;
use crate::app_home::autocomplete::observations::{self, Store};
use super_platinum_core::slack::transport::{
    ThreadCopyRead as Kind, ThreadCopyReadFailure as Failure,
};

pub(super) struct Team {
    store: Store,
    raw: Value,
}
impl Team {
    fn capture(source: &Source, s: &ShellState, store: Store) -> Option<Self> {
        if !store.matches_reader(s, &source.client, &source.transport, &source.session) {
            return None;
        }
        // names::capture always begins its fresh vector with the original
        // Gantry discovery. Its receipt's method is HTTP POST, not an API name.
        let first = store.complete().names.reads.first()?;
        if first.get("method").and_then(Value::as_str) != Some("POST") {
            return None;
        }
        let raw = first.get("raw")?;
        if raw.get("ok") != Some(&Value::Bool(true))
            || raw.get("error").is_some_and(|e| !e.is_null())
        {
            return None;
        }
        let rows = raw.get("workspaces")?.as_array()?;
        let mut matches = rows
            .iter()
            .filter(|v| v.get("id").and_then(Value::as_str) == Some(source.team.as_str()));
        let team = matches.next()?.clone();
        if matches.next().is_some() {
            return None;
        }
        team.get("url")?.as_str()?;
        Some(Self { store, raw: team })
    }
    pub(super) fn current(&self, source: &Source, s: &ShellState) -> bool {
        self.store
            .matches_reader(s, &source.client, &source.transport, &source.session)
            && Store::capture(s).is_some_and(|v| v.same_publication(&self.store))
    }
}
fn unavailable() -> Error {
    Error::Api("thread_link_unavailable".into())
}
fn current(s: &ShellState, p: &Intent) -> bool {
    p.current(s)
        && p.watch.current()
        && !p.ledger.cooling(&p.actor)
        && p.action.scope.source.transport.health()
            != super_platinum_core::slack::transport::Health::Offline
}
fn backoff(p: &Intent, error: &Error) {
    let secs = match error {
        Error::RateLimited { retry_after_secs } => Some(retry_after_secs.unwrap_or(60)),
        Error::HttpStatus {
            retry_after_secs, ..
        } => *retry_after_secs,
        _ => None,
    };
    p.ledger.cooldown(&p.actor, secs);
}
async fn account(state: Signal<ShellState>, p: &Intent) -> Result<Team, Error> {
    let source = &p.action.scope.source;
    if let Some(store) = Store::capture(&state.peek()) {
        return Team::capture(source, &state.peek(), store).ok_or_else(unavailable);
    }
    let (context, authority, online) = {
        let s = state.peek();
        if !current(&s, p) {
            return Err(unavailable());
        }
        let context = Arc::new(crate::scheduled::Context::capture(&s).ok_or_else(unavailable)?);
        if !context.client.same_configuration(&source.client)
            || !Arc::ptr_eq(&context.transport, &source.transport)
            || !crate::emoji_preferences::same_session(&context.session, &source.session)
        {
            return Err(unavailable());
        }
        let authority =
            super_platinum_core::state::posting_metadata::authority::Authority::capture(
                &s.core,
                &source.team,
            )
            .map_err(|_| unavailable())?;
        let online = crate::connection::autocomplete::Observation::capture(
            &s,
            &source.team,
            &super_platinum_core::workflow_share::search::Query::new("")
                .map_err(|_| unavailable())?,
        )
        .ok_or_else(unavailable)?;
        if !online.online() {
            return Err(unavailable());
        }
        (context, authority, online)
    };
    let admitted = |s: &ShellState| current(s, p);
    let connected = |s: &ShellState| online.current(s) && online.online();
    #[cfg(test)]
    let io = state.peek().thread_feed_reads.copy_link.io.clone();
    #[cfg(test)]
    if let Some(io) = io {
        let request = |r| {
            let io = io.clone();
            async move { io.request(r).await }
        };
        let result =
            observations::acquire_with(state, context, authority, &admitted, &connected, &request)
                .await;
        return match result {
            Ok(a) => Team::capture(source, &state.peek(), a.store).ok_or_else(unavailable),
            Err(e) => {
                backoff(p, &e);
                Err(e)
            }
        };
    }
    let result = observations::acquire(state, context, authority, &admitted, &connected).await;
    match result {
        Ok(a) => Team::capture(source, &state.peek(), a.store).ok_or_else(unavailable),
        Err(e) => {
            backoff(p, &e);
            Err(e)
        }
    }
}
async fn read(state: Signal<ShellState>, p: &Intent, kind: Kind) -> Result<Value, Error> {
    if !current(&state.peek(), p) {
        return Err(unavailable());
    }
    let source = &p.action.scope.source;
    let request = match kind {
        Kind::History => api::conversations_history(
            &source.client,
            &source.session,
            api::HistoryArgs {
                channel: p.action.key.0.clone(),
                latest: Some(p.action.key.1.clone()),
                inclusive: true,
                limit: Some(1),
                ..Default::default()
            },
        ),
        Kind::Channel => {
            api::conversations_info(&source.client, &source.session, p.action.key.0.clone())
        }
    };
    #[cfg(test)]
    let io = state.peek().thread_feed_reads.copy_link.io.clone();
    #[cfg(test)]
    if let Some(io) = io {
        let result = io.request(request).await;
        if let Err(e) = &result {
            backoff(p, e)
        }
        if !current(&state.peek(), p) {
            return Err(unavailable());
        }
        return result;
    }
    if crate::fixture::is_fixture() {
        return Err(unavailable());
    }
    let result = source
        .transport
        .execute_thread_copy_read(request, kind, &p.action.key.0, &p.action.key.1, || {
            current(&state.peek(), p)
        })
        .await
        .map_err(|e| match e {
            Failure::NotRequested(e) | Failure::Failed(e) => e,
        });
    if let Err(e) = &result {
        backoff(p, e)
    }
    if !current(&state.peek(), p) {
        return Err(unavailable());
    }
    result
}
pub(super) async fn prepare(state: Signal<ShellState>, p: Arc<Intent>) {
    let result = async {
        if !current(&state.peek(), &p) {
            return Err(unavailable());
        }
        let team = account(state, &p).await?;
        if !current(&state.peek(), &p) || !team.current(&p.action.scope.source, &state.peek()) {
            return Err(unavailable());
        }
        let history = read(state, &p, Kind::History).await?;
        let info = read(state, &p, Kind::Channel).await?;
        let link = FreshRootLink::capture(
            &p.action.scope.source.team,
            &p.action.key.0,
            &p.action.key.1,
            &history,
            &info,
            &team.raw,
        )
        .map_err(|_| unavailable())?;
        if !current(&state.peek(), &p) || !team.current(&p.action.scope.source, &state.peek()) {
            return Err(unavailable());
        }
        Ok(Ready { link, team })
    }
    .await;
    if p.current(&state.peek()) {
        match result {
            Ok(ready) => {
                let published = p.watch.with_current(|| {
                    let Ok(mut r) = p.ready.lock() else {
                        return false;
                    };
                    *r = Some(ready);
                    p.phase.store(1, Ordering::Release);
                    p.notice("Link ready. Choose Copy link to update the clipboard.");
                    true
                });
                if published != Some(true) {
                    p.fail("This message changed. Prepare its link again.")
                }
            }
            Err(_) => p.fail(
                "The message link could not be verified. Try preparing it again when connected.",
            ),
        }
    }
    wake(state);
}

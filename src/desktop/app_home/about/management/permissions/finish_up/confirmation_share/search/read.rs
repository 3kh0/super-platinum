//! Share-owned ordered search acquisition; no selection, DM opening or send.
//! Families run serially so our accepted member publication precedes capture of
//! channel member-fact ownership. This scheduling is a native adaptation.
use super::super::{Query, native, owned_mut};
use crate::{
    app_home::channel_fetches::search as channels, connection::autocomplete::Observation,
    directory_member_fetches::search as members, state::ShellState,
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use super_platinum_core::{
    block_actions::directory_endpoint::DirectoryEndpoint,
    slack::{Error, PreparedRequest},
    workflow_share::search::{self, Kind},
};

#[cfg(test)]
mod tests;

pub(super) struct Results {
    pub query: String,
    pub member_ids: Vec<String>,
    pub channel_ids: Vec<String>,
    pub reads: Vec<Value>,
    pub failures: Vec<Value>,
    pub publications: Vec<Value>,
    pub route: Value,
    connection: Option<Arc<Observation>>,
    member_source: Option<Arc<members::Snapshot>>,
    channel_source: Option<Arc<crate::app_home::channel_fetches::canonical::Canonical>>,
}
impl Results {
    pub fn online(&self) -> bool {
        self.connection.as_ref().is_some_and(|o| o.online())
    }
    pub fn current(&self, s: &ShellState) -> bool {
        self.connection.as_ref().is_some_and(|o| o.current(s))
            && self
                .member_source
                .as_ref()
                .is_none_or(|source| source.current(s))
            && self
                .channel_source
                .as_ref()
                .is_none_or(|source| source.current(s))
    }
}

pub(super) async fn acquire(
    state: Signal<ShellState>,
    query: &Query,
    typed: &str,
    current: &impl Fn(&ShellState) -> bool,
) -> Results {
    let mut results = Results {
        query: typed.into(),
        member_ids: vec![],
        channel_ids: vec![],
        reads: vec![],
        failures: vec![],
        publications: vec![],
        route: json!({"route":"unavailable","online":false}),
        connection: None,
        member_source: None,
        channel_source: None,
    };
    if !query.current(&state.peek()) || !current(&state.peek()) {
        results
            .failures
            .push(json!({"kind":"query","phase":"capture","failure":{"kind":"retired"}}));
        return results;
    }
    let plan = match search::Query::new(typed) {
        Ok(plan) => plan,
        Err(error) => {
            results
                .failures
                .push(json!({"kind":"query","phase":"capture","failure":failure(&error)}));
            return results;
        }
    };
    let Some(connection) =
        Observation::capture(&state.peek(), &query.ctx().session.team_id, &plan).map(Arc::new)
    else {
        results
            .failures
            .push(json!({"kind":"query","phase":"connectivity","failure":{"kind":"unavailable"}}));
        return results;
    };
    results.route = connection.snapshot();
    results.connection = Some(connection.clone());
    // Local means no Flannel discovery, users/search or channels/search. The
    // caller still owns canonical local collection and actual ranking facts.
    if connection.route() == search::AcquisitionRoute::Local {
        return results;
    }
    let original_current = |s: &ShellState| current(s) && connection.current(s);
    let mut endpoint = None;
    for kind in plan.kinds() {
        if !query.current(&state.peek()) || !original_current(&state.peek()) {
            results
                .failures
                .push(json!({"kind":label(*kind),"phase":"capture","failure":{"kind":"retired"}}));
            break;
        }
        // Native custody adaptation: source can finish/upsert after its outer
        // fallback timeout; this owner drops the pending producer at two seconds.
        let result = match tokio::time::timeout(
            Duration::from_secs(2),
            family(
                state,
                query,
                &plan,
                *kind,
                &original_current,
                &mut endpoint,
                &mut results,
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err((
                "timeout",
                Error::Api("workflow_share_search_timeout".into()),
            )),
        };
        if let Err((phase, error)) = result {
            results
                .failures
                .push(json!({"kind":label(*kind),"phase":phase,"failure":failure(&error)}));
        }
    }
    results
}

enum Intake {
    Members(members::Read),
    Channels(channels::Read),
}
impl Intake {
    fn current(&self, s: &ShellState) -> bool {
        match self {
            Self::Members(read) => read.current(s),
            Self::Channels(read) => read.current(s),
        }
    }
}

async fn family(
    state: Signal<ShellState>,
    query: &Query,
    plan: &search::Query,
    kind: Kind,
    current: &impl Fn(&ShellState) -> bool,
    endpoint: &mut Option<DirectoryEndpoint>,
    results: &mut Results,
) -> Result<(), (&'static str, Error)> {
    let team = &query.ctx().session.team_id;
    // Capture once before any request for this family. Never recapture after a
    // marker response or disguise a newer source as the original read owner.
    let intake = match kind {
        Kind::Members => Intake::Members(
            members::Read::capture(&state.peek(), team, "workflow-share-existing-dm")
                .map_err(|e| ("capture", e))?,
        ),
        Kind::Channels => Intake::Channels(
            channels::Read::capture(state, team, &|s| query.current(s) && current(s))
                .map_err(|e| ("capture", e))?,
        ),
    };
    let admitted = |s: &ShellState| query.current(s) && current(s) && intake.current(s);
    let mut pages = search::Pages::default();
    loop {
        if !admitted(&state.peek()) {
            return Err(("read", retired()));
        }
        if endpoint
            .as_ref()
            .is_none_or(|e| !e.current(&query.ctx().session, Instant::now()))
        {
            let request = DirectoryEndpoint::discovery(&query.ctx().client, &query.ctx().session)
                .map_err(|e| ("discovery", e))?;
            let at = Instant::now();
            let response = dispatch(state, query, request, false, &admitted).await;
            record(
                &mut results.reads,
                kind,
                "api.getFlannelHttpUrl",
                None,
                &response,
            );
            let (raw, _) = response.map_err(|e| ("discovery", e))?;
            if !admitted(&state.peek())
                || !native::current(query, None).await
                || !admitted(&state.peek())
            {
                return Err(("discovery", retired()));
            }
            *endpoint = Some(
                DirectoryEndpoint::decode(&query.ctx().session, &raw, at)
                    .map_err(|e| ("discovery", e))?,
            );
        }
        let endpoint = endpoint.as_ref().ok_or_else(|| ("discovery", retired()))?;
        let read = search::Read::new(&query.ctx().session, kind, plan, pages.marker())
            .map_err(|e| ("request", e))?;
        let request = read
            .request(
                &query.ctx().client,
                &query.ctx().session,
                endpoint,
                Instant::now(),
            )
            .map_err(|e| ("request", e))?;
        let response = dispatch(state, query, request, true, &|s| {
            admitted(s) && endpoint.current(&query.ctx().session, Instant::now())
        })
        .await;
        record(
            &mut results.reads,
            kind,
            read.method(),
            Some(read.body()),
            &response,
        );
        let (raw, keys) = response.map_err(|e| ("read", e))?;
        if !admitted(&state.peek())
            || !native::current(query, None).await
            || !admitted(&state.peek())
        {
            return Err(("decode", retired()));
        }
        pages
            .accept(raw, keys.as_deref())
            .map_err(|e| ("decode", e))?;
        if pages.complete() {
            break;
        }
    }
    let rows = pages.rows().map_err(|e| ("aggregate", e))?.to_vec();
    let membership_ids = pages
        .outcomes()
        .map_err(|e| ("aggregate", e))?
        .membership_ids
        .clone();
    let ids = rows
        .iter()
        .map(|row| row["id"].as_str().map(str::to_owned).ok_or_else(retired))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ("aggregate", e))?;
    // Both shared APIs publish only this completed truncated aggregate, once.
    // Source euu1 supplies true for each returned member ID to ScqY; the raw
    // can_interact companion remains unmodified in the page read evidence.
    match intake {
        Intake::Members(read) => {
            let interactions = Value::Object(
                ids.iter()
                    .map(|id| (id.clone(), Value::Bool(true)))
                    .collect(),
            );
            let source = read
                .publish_rows(state, &rows, &interactions, |s| {
                    query.current(s) && current(s)
                })
                .await
                .map_err(|e| ("publish", e))?;
            if !query.current(&state.peek())
                || !current(&state.peek())
                || !source.current(&state.peek())
            {
                return Err(("publish", retired()));
            }
            results.member_ids = ids.clone();
            results.member_source = Some(Arc::new(source));
        }
        Intake::Channels(read) => {
            let source = read
                .publish_with_memberships(state, rows.clone(), &membership_ids, |s| {
                    query.current(s) && current(s)
                })
                .await
                .map_err(|e| ("publish", e))?;
            if !query.current(&state.peek())
                || !current(&state.peek())
                || !source.current(&state.peek())
            {
                return Err(("publish", retired()));
            }
            results.channel_ids = ids.clone();
            results.channel_source = Some(source);
        }
    }
    results.publications.push(json!({"kind":label(kind),"ids":ids,"rows":rows,"membershipIds":membership_ids,"pageCount":pages.pages().len(),"nextMarker":pages.marker(),"applied":true}));
    Ok(())
}

type Ordered = (Value, Option<Vec<String>>);
pub(super) async fn dispatch(
    mut state: Signal<ShellState>,
    query: &Query,
    request: PreparedRequest,
    ordered: bool,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<Ordered, Error> {
    let observed = state;
    let admitted = || query.current(&observed.peek()) && current(&observed.peek());
    if !admitted() || !query.authorized(state).await || !admitted() {
        return Err(retired());
    }
    if let Some(wait) = owned_mut(&mut state.write(), query)
        .and_then(|v| v.retry_after)
        .map(|until| until.saturating_duration_since(tokio::time::Instant::now()))
        .filter(|wait| !wait.is_zero())
    {
        return Err(Error::RateLimited {
            retry_after_secs: Some(wait.as_secs().saturating_add(1)),
        });
    }
    let result = tokio::select! {
        value = async {
            if crate::fixture::is_fixture() {
                crate::app_home::about::management::fixture::first_publish::confirmation::share::search::read(state,&request).await.unwrap_or_else(||Err(Error::Offline("closed Share search fixture has no response".into())))
            } else if ordered {
                query.ctx().transport.execute_workflow_share_search_current(request,admitted).await.map(|v|v.into_parts())
            } else { query.ctx().transport.execute(request).await.map(|v|(v,None)) }
        } => value,
        _ = async { while admitted() { tokio::time::sleep(Duration::from_millis(50)).await; } } => return Err(retired()),
    };
    if let Err(Error::RateLimited {
        retry_after_secs: Some(seconds),
    }) = &result
    {
        if let Some(v) = owned_mut(&mut state.write(), query) {
            v.retry_after = tokio::time::Instant::now().checked_add(Duration::from_secs(*seconds));
        }
    }
    // Return actual acquired bytes for evidence even if the native owner retired;
    // caller publication guards remain strict and cannot adopt these late rows.
    result
}
fn record(
    reads: &mut Vec<Value>,
    kind: Kind,
    method: &str,
    body: Option<&Value>,
    result: &Result<Ordered, Error>,
) {
    reads.push(match result {
        Ok((raw,keys)) => json!({"kind":label(kind),"method":method,"arguments":body,"raw":raw,"orderedKeys":keys}),
        Err(error) => json!({"kind":label(kind),"method":method,"arguments":body,"failure":failure(error)}),
    });
}
fn label(kind: Kind) -> &'static str {
    match kind {
        Kind::Members => "members",
        Kind::Channels => "channels",
    }
}
fn retired() -> Error {
    Error::Api("workflow_share_search_retired".into())
}
fn failure(error: &Error) -> Value {
    match error {
        Error::RateLimited { retry_after_secs } => {
            json!({"kind":"rate-limited","retryAfterSeconds":retry_after_secs})
        }
        Error::HttpStatus {
            status,
            retry_after_secs,
        } => json!({"kind":"http","status":status,"retryAfterSeconds":retry_after_secs}),
        Error::Offline(_) => json!({"kind":"offline"}),
        Error::Api(value) if value == "workflow_share_search_retired" => json!({"kind":"retired"}),
        Error::Api(value) if value == "workflow_share_search_timeout" => json!({"kind":"timeout"}),
        Error::Api(_) => json!({"kind":"api-or-invalid"}),
        Error::Transport(_) => json!({"kind":"transport"}),
        _ => json!({"kind":"unavailable"}),
    }
}

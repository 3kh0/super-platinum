//! SINGLE member100 acquisition and route-bearing new-choice admission.
use super::*;
use crate::directory_member_fetches::search as intake;
use crate::message_buttons::inputs::user::directory::read_scope::ReadScope;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    future::Future,
    rc::Rc,
};
use super_platinum_core::{
    block_actions::directory_endpoint::DirectoryEndpoint, slack::PreparedRequest,
};
mod origins;

#[derive(Clone, Default)]
pub(in crate::message_buttons::shortcuts::calendar) struct State {
    origins: origins::Origins,
    facts: Option<Arc<intake::Snapshot>>,
    generation: u64,
    hits: BTreeMap<String, bool>,
}
fn unavailable() -> Error {
    Error::Api("calendar_member_search_unavailable".into())
}
fn valid_id(id: &str) -> bool {
    id.len() <= 255 && domain::identifier(id)
}
fn member_generation(id: u64, generation: u64) -> String {
    format!("calendar-single-members-{id}-{generation}")
}
fn raw_profile_allowed(row: &Value) -> bool {
    matches!(
        row.get("is_profile_only_user"),
        None | Some(Value::Bool(false))
    )
}
fn body(query: &str, marker: Option<&str>) -> Value {
    let [(_, mut body), _, _] = queries(query);
    body["include_profile_only_users"] = json!(true);
    if let Some(marker) = marker {
        body["marker"] = json!(marker);
    }
    body
}
/// A new query may certify LOCAL cache facts; it acquires no server hit membership.
pub(super) fn start_query(s: &mut ShellState, id: u64, generation: u64, ctx: &Context) {
    let guarded = active(s, id).is_some_and(|c| !c.members.origins.is_empty());
    let facts = guarded
        .then(|| intake::snapshot(s, &ctx.session.team_id, &member_generation(id, generation)))
        .and_then(Result::ok)
        .map(Arc::new);
    if let Some(c) = active_mut(s, id) {
        c.members.generation = generation;
        c.members.hits.clear();
        c.members.facts = facts;
    }
}
struct Prepared<'a> {
    state: &'a State,
    current: bool,
    rows: HashMap<
        &'a str,
        &'a super_platinum_core::block_actions::workflow_management::member_hydration::Current,
    >,
}
impl Prepared<'_> {
    fn canonical(&self, user: &User) -> bool {
        self.rows
            .get(user.id.as_str())
            .is_some_and(|row| row.interaction != Some(false))
    }
    fn cached(&self, user: &User, name: &str, selected: &str) -> bool {
        if user.deleted || !domain::identifier(&user.id) {
            return false;
        }
        if user.id == selected {
            return true;
        }
        cal::sharing_profile_allowed(user)
            && (!self.state.origins.guarded(&user.id)
                || self.state.origins.independent(user, name)
                || self.canonical(user))
    }
    fn remote(&self, user: &User, generation: u64) -> bool {
        // Preserve independent pre-intake contributions. The sole production
        // member producer registers every ID before intake and append refuses
        // any unguarded ID, so an acquired member cannot use this exception.
        !self.state.origins.guarded(&user.id)
            || (self.state.generation == generation
                && self.state.hits.get(&user.id) == Some(&true)
                && self.canonical(user))
    }
}
fn prepare<'a>(s: &'a ShellState, id: u64) -> Option<Prepared<'a>> {
    let state = &active(s, id)?.members;
    // Once per serial operation, never once per candidate or across an await.
    let facts = state.facts.as_ref().filter(|facts| facts.current(s));
    let current = facts.is_some();
    let rows = facts
        .map(|facts| {
            facts
                .rows()
                .iter()
                .filter_map(|row| {
                    row.source
                        .get("id")
                        .and_then(Value::as_str)
                        .map(|id| (id, row))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Prepared {
        state,
        current,
        rows,
    })
}
pub(super) fn choices(
    s: &ShellState,
    id: u64,
    ctx: &Context,
    query: &str,
    selected: &str,
) -> Vec<cal::Choice> {
    let Some(prepared) = prepare(s, id) else {
        return Vec::new();
    };
    let Some(mpims) = super::mpims::prepare(s, id) else {
        return Vec::new();
    };
    load::share_choices_with_admission(
        s,
        ctx,
        query,
        selected,
        |user, name| prepared.cached(user, name, selected),
        |native, name| mpims.cached(s, ctx, native, name, selected),
    )
}
pub(super) fn publication(
    s: &ShellState,
    id: u64,
    ctx: &Context,
    query: &str,
    mut remote: Vec<Destination>,
) -> (Vec<cal::Choice>, Vec<Destination>, bool) {
    let Some(c) = active(s, id) else {
        return (Vec::new(), Vec::new(), true);
    };
    let Some(prepared) = prepare(s, id) else {
        return (Vec::new(), Vec::new(), true);
    };
    let selected = &c.share_channel;
    let Some(mpims) = super::mpims::prepare(s, id) else {
        return (Vec::new(), Vec::new(), true);
    };
    let choices = load::share_choices_with_admission(
        s,
        ctx,
        query,
        selected,
        |user, name| prepared.cached(user, name, selected),
        |native, name| mpims.cached(s, ctx, native, name, selected),
    );
    // Guarded IDs are reselected from current canonical native state, even if
    // an earlier provider contributed a same-ID stale payload to this vector.
    remote = remote
        .into_iter()
        .filter_map(|dest| match dest {
            Destination::Channel(raw) if c.mpims.guarded(&raw.id) => {
                let current = s
                    .core
                    .workspaces
                    .get(&ctx.session.team_id)?
                    .channels
                    .get(&raw.id)?;
                if !mpims.remote(current, c.share_generation) {
                    return None;
                }
                let dest = Destination::Channel(current.clone());
                dest.discovery_available(s, ctx, selected).then_some(dest)
            }
            other => (other.discovery_available(s, ctx, selected)
                && match &other {
                    Destination::User(user) => prepared.remote(user, c.share_generation),
                    Destination::Channel(_) => true,
                })
            .then_some(other),
        })
        .collect();
    let unavailable = (!prepared.current && !c.members.hits.is_empty()) || mpims.unavailable();
    (choices, remote, unavailable)
}
pub(super) fn selection(s: &ShellState, id: u64, ctx: &Context, value: &str) -> Option<Selection> {
    let c = active(s, id)?;
    if value == c.share_channel {
        return Some(Selection::Retain);
    }
    if !c.share_choices.iter().any(|choice| choice.id == value) {
        return None;
    }
    let w = s.core.workspaces.get(&ctx.session.team_id)?;
    let member = c.members.origins.guarded(value)
        || c.members.hits.contains_key(value)
        || w.users.contains_key(value)
        || c.share_remote
            .iter()
            .any(|d| matches!(d,Destination::User(u) if u.id==value));
    let guarded_mpim = c.mpims.guarded(value);
    if !member && !guarded_mpim {
        return Some(
            c.share_remote
                .iter()
                .find(|d| d.id() == value)
                .cloned()
                .map(Selection::Remote)
                .unwrap_or(Selection::Cached),
        );
    }
    let prepared = prepare(s, id)?;
    let mpims = super::mpims::prepare(s, id)?;
    let cached = load::share_choices_with_admission(
        s,
        ctx,
        &c.share_query,
        &c.share_channel,
        |u, n| prepared.cached(u, n, &c.share_channel),
        |native, name| mpims.cached(s, ctx, native, name, &c.share_channel),
    );
    if guarded_mpim {
        let current = w.channels.get(value)?;
        let dest = Destination::Channel(current.clone());
        if !dest.discovery_available(s, ctx, &c.share_channel) {
            return None;
        }
        let name = load::share_channel_name(s, ctx, current);
        if cached.iter().any(|choice| choice.id == value)
            && mpims.cached(s, ctx, current, &name, &c.share_channel)
        {
            return Some(Selection::Cached);
        }
        return mpims
            .remote(current, c.share_generation)
            .then_some(Selection::Remote(dest));
    }
    if cached.iter().any(|choice| choice.id == value) {
        return Some(Selection::Cached);
    }
    let user = w.users.get(value)?;
    if prepared.remote(user, c.share_generation)
        && Destination::User(user.clone()).discovery_available(s, ctx, &c.share_channel)
    {
        return Some(Selection::Remote(Destination::User(user.clone())));
    }
    None
}

pub(super) struct Acquired {
    source: Arc<intake::Snapshot>,
    ids: Vec<String>,
    profiles: BTreeMap<String, bool>,
}
pub(super) fn append(
    s: &mut ShellState,
    id: u64,
    generation: u64,
    ctx: &Context,
    acquired: Acquired,
    remote: &mut Vec<Destination>,
) -> bool {
    if !owned_current(s, id, generation, ctx) || !acquired.source.current(s) {
        return false;
    }
    let Some(w) = s.core.workspaces.get(&ctx.session.team_id) else {
        return false;
    };
    let ready: BTreeSet<_> = acquired
        .source
        .rows()
        .iter()
        .filter_map(|row| row.source.get("id").and_then(Value::as_str))
        .collect();
    let Some(c) = active(s, id) else {
        return false;
    };
    if acquired.ids.iter().any(|id| {
        !c.members.origins.guarded(id)
            || !ready.contains(id.as_str())
            || !w.users.get(id).is_some_and(|user| user.id == *id)
            || !acquired.profiles.contains_key(id)
    }) {
        return false;
    }
    let users: Vec<_> = acquired.ids.iter().map(|id| w.users[id].clone()).collect();
    let c = active_mut(s, id).unwrap();
    c.members.facts = Some(acquired.source.clone());
    c.members.generation = generation;
    c.members.hits = acquired.profiles;
    for user in users {
        if !remote.iter().any(|old| old.id() == user.id) {
            remote.push(Destination::User(user));
        }
    }
    true
}
fn scope(
    state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
    read: Rc<intake::Read>,
    endpoint: Option<Arc<DirectoryEndpoint>>,
) -> ReadScope {
    let owner = ctx.clone();
    let observed = ctx.clone();
    let current_read = read.clone();
    ReadScope::new(
        move || {
            state.try_read().ok().is_some_and(|s| {
                owned_current(&s, id, generation, &owner) && current_read.current(&s)
            })
        },
        move || {
            state.try_read().ok().is_some_and(|s| {
                owned_current(&s, id, generation, &ctx)
                    && read.current(&s)
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
pub(super) async fn read(
    state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: Arc<Context>,
    query: &str,
) -> Result<Option<Acquired>, Error> {
    let original = ctx.clone();
    run_with(state, id, generation, &ctx, query, move |request, scope| {
        let ctx = original.clone();
        async move { scope.transport(&ctx, request).await }
    })
    .await
}
async fn run_with<F, Fut>(
    mut state: Signal<ShellState>,
    id: u64,
    generation: u64,
    ctx: &Arc<Context>,
    query: &str,
    mut execute: F,
) -> Result<Option<Acquired>, Error>
where
    F: FnMut(PreparedRequest, ReadScope) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
{
    let receipt = {
        let s = state.peek();
        if query.len() > 4096 || !owned_current(&s, id, generation, ctx) {
            return Ok(None);
        }
        Rc::new(intake::Read::capture(
            &s,
            &ctx.session.team_id,
            &member_generation(id, generation),
        )?)
    };
    let discovery = scope(state, id, generation, ctx.clone(), receipt.clone(), None);
    if !discovery.admitted() {
        return Ok(None);
    }
    let cached = active(&state.peek(), id)
        .and_then(|c| c.directory_endpoint.as_ref())
        .filter(|e| e.current(&ctx.session, Instant::now()))
        .cloned();
    let endpoint = match cached {
        Some(endpoint) => endpoint,
        None => {
            let result = execute(
                DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?,
                discovery.clone(),
            )
            .await;
            if !discovery.current() {
                return Ok(None);
            }
            let Some(raw) = result? else {
                return Ok(None);
            };
            let endpoint = Arc::new(DirectoryEndpoint::decode(
                &ctx.session,
                &raw,
                Instant::now(),
            )?);
            let mut s = state.write();
            if !owned_current(&s, id, generation, ctx) || !receipt.current(&s) {
                return Ok(None);
            }
            active_mut(&mut s, id).unwrap().directory_endpoint = Some(endpoint.clone());
            endpoint
        }
    };
    let pages = scope(
        state,
        id,
        generation,
        ctx.clone(),
        receipt.clone(),
        Some(endpoint.clone()),
    );
    let rows = api::fetch_calendar_sharing_member_rows_with(
        |marker| {
            if !pages.admitted() {
                return Ok(None);
            }
            endpoint
                .request(
                    &ctx.client,
                    &ctx.session,
                    Instant::now(),
                    "users/search",
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
    let Some(rows) = rows? else {
        return Ok(None);
    };
    let mut ids = Vec::new();
    let mut profiles = BTreeMap::new();
    let mut interactions = serde_json::Map::new();
    for row in &rows {
        let member = row
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id))
            .ok_or_else(unavailable)?;
        if !profiles.contains_key(member) {
            ids.push(member.to_owned());
            profiles.insert(member.to_owned(), raw_profile_allowed(row));
        }
        interactions.insert(member.into(), Value::Bool(true));
    }
    {
        let mut s = state.write();
        if !owned_current(&s, id, generation, ctx) || !receipt.current(&s) {
            return Ok(None);
        }
        let c = active(&s, id).ok_or_else(unavailable)?;
        let w = s
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .ok_or_else(unavailable)?;
        let registration = c.members.origins.propose(&w.users, &ids)?;
        active_mut(&mut s, id)
            .unwrap()
            .members
            .origins
            .commit(registration);
    }
    let source = receipt
        .publish_rows(state, &rows, &Value::Object(interactions), |s| {
            owned_current(s, id, generation, ctx)
        })
        .await?;
    // Own installation retires Read; the returned Snapshot is the success witness.
    if !owned_current(&state.peek(), id, generation, ctx) || !source.current(&state.peek()) {
        return Ok(None);
    }
    Ok(Some(Acquired {
        source: Arc::new(source),
        ids,
        profiles,
    }))
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

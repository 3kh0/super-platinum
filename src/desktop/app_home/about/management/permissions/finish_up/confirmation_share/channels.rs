//! Share-owned destination observations and permission validation.
//! Canonical rows are acquisition hints, never posting permission grants.
use super::{Query, group, peer};
use crate::app_home::channel_fetches::canonical;
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::Value;
use std::sync::Arc;
use super_platinum_core::slack::{Error, api};
use super_platinum_core::{
    block_actions::posting_policy::{self, Flags},
    state::posting_metadata::{
        Unknown,
        authority::{Authority, BootRead, RawChannel},
    },
};
#[derive(Clone)]
pub(super) struct Choice {
    authority: Authority,
    raw: RawChannel,
    canonical: Arc<canonical::Canonical>,
    selected: Value,
    label: String,
    peer: Option<Arc<peer::Peer>>,
    group: Option<group::Witness>,
}
impl Choice {
    fn capture(
        s: &ShellState,
        authority: Authority,
        raw: RawChannel,
        team: &str,
        peer: Option<Arc<peer::Peer>>,
    ) -> Result<Self, Error> {
        Self::capture_group(s, authority, raw, team, peer, None)
    }
    fn capture_group(
        s: &ShellState,
        authority: Authority,
        raw: RawChannel,
        team: &str,
        peer: Option<Arc<peer::Peer>>,
        group: Option<group::Witness>,
    ) -> Result<Self, Error> {
        let canonical = canonical::observed(s, team)?;
        let id = raw.value()["id"].as_str().ok_or_else(unavailable)?;
        let source = canonical.row(id).ok_or_else(unavailable)?;
        let foreign = source
            .get("fromAnotherTeam")
            .filter(|v| v.is_boolean())
            .ok_or_else(unavailable)?;
        // Exactly one source-derived selector field; never a wire/default grant.
        let mut selected = raw.value().clone();
        selected["fromAnotherTeam"] = foreign.clone();
        let label = if let Some(group) = &group {
            group.label()
        } else if let Some(peer) = &peer {
            peer.label().to_owned()
        } else {
            raw.value()["name"]
                .as_str()
                .filter(|v| !v.is_empty())
                .ok_or_else(unavailable)?
                .to_owned()
        };
        Ok(Self {
            authority,
            raw,
            canonical,
            selected,
            label,
            peer,
            group,
        })
    }
    pub(super) fn id(&self) -> &str {
        self.raw.value()["id"].as_str().unwrap_or_default()
    }
    pub(super) fn label(&self) -> &str {
        &self.label
    }
    pub(super) fn raw(&self) -> &Value {
        &self.selected
    }
    pub(super) fn peer_evidence(&self) -> Value {
        self.peer
            .as_ref()
            .map(|p| p.evidence())
            .unwrap_or(Value::Null)
    }
    pub(super) fn group(&self) -> Option<&group::Witness> {
        self.group.as_ref()
    }
    pub(super) fn group_evidence(&self) -> Value {
        self.group
            .as_ref()
            .map(|g| g.evidence())
            .unwrap_or(Value::Null)
    }
    pub(super) fn flags(&self, s: &ShellState) -> Option<Flags> {
        if !self.authority.current(&s.core)
            || !self.raw.current(&s.core)
            || !self.canonical.current(s)
            || self.peer.as_ref().is_some_and(|p| !p.current(s))
        {
            return None;
        }
        let metadata = self.authority.metadata(&s.core).ok()?;
        let flags = metadata
            .effective(self.authority.configuration(), self.raw.value())
            .ok()?;
        let ctx = metadata
            .context(self.authority.configuration(), self.raw.value())
            .ok()?;
        if let Some(group) = &self.group {
            return group
                .eligible(s, self.raw(), flags, ctx.team_id)
                .then_some(flags);
        }
        if let Some(peer) = &self.peer {
            return super_platinum_core::workflow_share::existing_dm_eligible(
                self.raw(),
                peer.effective(),
                flags,
                ctx.team_id,
            )
            .then_some(flags);
        }
        super_platinum_core::workflow_share::joined_channel_eligible(
            self.raw(),
            flags,
            ctx.team_id,
            ctx.top_level_team_id,
        )
        .then_some(flags)
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.flags(s).is_some()
    }
}
#[derive(Clone)]
pub(super) struct Listing {
    authority: Authority,
    canonical: Arc<canonical::Canonical>,
    choices: Vec<Choice>,
    incomplete: bool,
}
impl Listing {
    /// Rebase only actual still-current permission witnesses after shared
    /// directory publication. Search hits alone never become permission grants.
    pub(super) fn merge_search(
        previous: Option<&Self>,
        s: &ShellState,
        team: &str,
        acquired: Vec<Choice>,
        selected: &str,
    ) -> Result<Option<Self>, Error> {
        let canonical = canonical::observed(s, team)?;
        let Some(authority) = acquired
            .first()
            .map(|c| c.authority.clone())
            .or_else(|| previous.map(|l| l.authority.clone()))
        else {
            return Ok(None);
        };
        if !authority.current(&s.core) {
            return Err(unavailable());
        }
        let mut choices = Vec::new();
        if let Some(previous) = previous {
            for old in previous
                .choices
                .iter()
                .filter(|c| c.id() == selected)
                .chain(
                    previous
                        .choices
                        .iter()
                        .filter(|c| c.id() != selected)
                        .take(50),
                )
            {
                if acquired.iter().any(|c| c.id() == old.id())
                    || !old.raw.current(&s.core)
                    || old.peer.as_ref().is_some_and(|p| !p.current(s))
                {
                    continue;
                }
                if let Ok(choice) = Choice::capture_group(
                    s,
                    authority.clone(),
                    old.raw.clone(),
                    team,
                    old.peer.clone(),
                    match old.group.as_ref().map(|g| g.rebase(s, canonical.clone())) {
                        Some(Some(g)) => Some(g),
                        Some(None) => continue,
                        None => None,
                    },
                ) {
                    if choice.current(s) {
                        choices.push(choice);
                    }
                }
            }
        }
        choices.extend(acquired.into_iter().filter(|c| c.current(s)));
        Ok(Some(Self {
            authority,
            canonical,
            choices,
            incomplete: true,
        }))
    }
    pub(super) fn adopt(
        previous: Option<&Self>,
        s: &ShellState,
        team: &str,
        choice: Choice,
    ) -> Result<Self, Error> {
        let canonical = canonical::observed(s, team)?;
        let authority = choice.authority.clone();
        let expected_count = 1 + previous.map_or(0, |p| {
            p.choices
                .iter()
                .filter(|old| old.id() != choice.id())
                .count()
        });
        let mut choices = Vec::new();
        if let Some(previous) = previous {
            for old in &previous.choices {
                if old.id() == choice.id()
                    || !old.raw.current(&s.core)
                    || old.peer.as_ref().is_some_and(|p| !p.current(s))
                {
                    continue;
                }
                if let Ok(rebased) = Choice::capture_group(
                    s,
                    authority.clone(),
                    old.raw.clone(),
                    team,
                    old.peer.clone(),
                    match old.group.as_ref().map(|g| g.rebase(s, canonical.clone())) {
                        Some(Some(g)) => Some(g),
                        Some(None) => continue,
                        None => None,
                    },
                ) {
                    if rebased.current(s) {
                        choices.push(rebased);
                    }
                }
            }
        }
        choices.push(choice);
        let incomplete = previous.is_none_or(|p| p.incomplete) || choices.len() < expected_count;
        Ok(Self {
            authority,
            canonical,
            choices,
            incomplete,
        })
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.authority.current(&s.core)
            && self.canonical.current(s)
            && self.choices.iter().all(|c| c.current(s))
    }
    pub(super) fn incomplete(&self) -> bool {
        self.incomplete
    }
    pub(super) fn rows(&self) -> &[Choice] {
        &self.choices
    }
    pub(super) fn find(&self, id: &str) -> Option<&Choice> {
        self.choices.iter().find(|c| c.id() == id)
    }
}
/// Partial known local collection, not full server search or frecency ranking.
/// Unknown permissions do not become grants or an exhaustive no-results claim.
pub(super) async fn listing(state: Signal<ShellState>, query: &Query) -> Result<Listing, Error> {
    let plan = super_platinum_core::workflow_share::search::Query::new(
        query.expected["query"].as_str().ok_or_else(unavailable)?,
    )?;
    let connection = crate::connection::autocomplete::Observation::capture(
        &state.peek(),
        &query.ctx().session.team_id,
        &plan,
    )
    .ok_or_else(unavailable)?;
    let authority = if connection.online() {
        authority(state, query).await?
    } else {
        Authority::capture(&state.peek().core, &query.ctx().session.team_id)
            .map_err(|_| unavailable())?
    };
    // Canonical identities are acquisition hints, not posting permission.
    // Canonical hydration can invalidate Boot's raw posting stamps; enumerating
    // only current posting witnesses would silently turn those into no results.
    let canonical = canonical::observed(&state.peek(), &query.ctx().session.team_id)?;
    let ids = canonical
        .rows()
        .iter()
        .filter(|row| row["is_mpim"] != true)
        .filter(|row| {
            row["is_im"] != true
                || peer::candidate(&state.peek(), &query.ctx().session.team_id, row)
        })
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let mut choices = Vec::new();
    let mut incomplete = ids.len() > 50;
    for id in ids.into_iter().take(50) {
        let observed = {
            let s = state.peek();
            if !query.current(&s) || !canonical.current(&s) || !connection.current(&s) {
                return Err(unavailable());
            }
            if canonical.row(&id).is_some_and(|row| row["is_im"] == true) {
                None
            } else {
                authority.raw_channel(&s.core, &id).ok()
            }
        };
        let raw = match observed {
            Some(raw) => raw,
            None if !connection.online() => {
                incomplete = true;
                continue;
            }
            None => match raw_channel(state, query, &authority, &id).await {
                Ok(raw) => raw,
                Err(_) => {
                    incomplete = true;
                    continue;
                }
            },
        };
        if !query.current(&state.peek())
            || !canonical.current(&state.peek())
            || !connection.current(&state.peek())
        {
            return Err(unavailable());
        }
        // Acquire missing policy dependencies instead of making unknown policy
        // permanently unselectable. Every dependency remains under this query.
        let choice = if connection.online() {
            resolve(state, query, &authority, raw, None).await
        } else {
            observed_choice(state, query, &authority, raw)
        };
        match choice {
            Ok(choice) => choices.push(choice),
            Err(_) => incomplete = true,
        }
    }
    let s = state.peek();
    if !query.current(&s)
        || !canonical.current(&s)
        || !authority.current(&s.core)
        || !connection.current(&s)
    {
        return Err(unavailable());
    }
    incomplete |= choices.iter().any(|c| !c.current(&s));
    choices.retain(|c| c.current(&s));
    Ok(Listing {
        authority,
        canonical,
        choices,
        incomplete,
    })
}
/// Offline suggestions may use an existing complete posting observation. They
/// never synthesize missing policy dependencies; Send still refreshes its own
/// destination independently in `validate`.
pub(super) fn known(state: Signal<ShellState>, query: &Query, id: &str) -> Result<Choice, Error> {
    let authority = Authority::capture(&state.peek().core, &query.ctx().session.team_id)
        .map_err(|_| unavailable())?;
    let raw = authority
        .raw_channel(&state.peek().core, id)
        .map_err(|_| unavailable())?;
    observed_choice(state, query, &authority, raw)
}
fn observed_choice(
    mut state: Signal<ShellState>,
    query: &Query,
    authority: &Authority,
    raw: RawChannel,
) -> Result<Choice, Error> {
    let mut s = state.write();
    if !query.current(&s)
        || !authority.current(&s.core)
        || !raw.current(&s.core)
        || raw.value()["is_im"] == true
        || raw.value()["is_mpim"] == true
    {
        return Err(unavailable());
    }
    raw.observe(&mut s.core).map_err(|_| unavailable())?;
    let choice = Choice::capture(
        &s,
        authority.clone(),
        raw,
        &query.ctx().session.team_id,
        None,
    )?;
    choice.current(&s).then_some(choice).ok_or_else(unavailable)
}
pub(super) async fn validate(
    state: Signal<ShellState>,
    query: &Query,
    id: &str,
) -> Result<Choice, Error> {
    let canonical = canonical::observed(&state.peek(), &query.ctx().session.team_id)?;
    if !query.current(&state.peek())
        || !canonical.current(&state.peek())
        || canonical.row(id).is_none()
    {
        return Err(unavailable());
    }
    let authority = authority(state, query).await?;
    let raw = raw_channel(state, query, &authority, id).await?;
    if !query.current(&state.peek()) || !canonical.current(&state.peek()) {
        return Err(unavailable());
    }
    resolve(state, query, &authority, raw, None).await
}
/// Consume the actual paired PostingRead publication; never reinterpret the
/// earlier conversations.open response as a fresh permission read.
pub(super) async fn opened_choice(
    state: Signal<ShellState>,
    query: &Query,
    id: &str,
    peer: Arc<peer::Peer>,
) -> Result<Choice, Error> {
    let authority = authority(state, query).await?;
    let raw = authority
        .raw_channel(&state.peek().core, id)
        .map_err(|_| unavailable())?;
    if raw.value()["user"] != peer.effective()["id"] || !peer.current(&state.peek()) {
        return Err(unavailable());
    }
    resolve(state, query, &authority, raw, Some(peer)).await
}
async fn authority(mut state: Signal<ShellState>, query: &Query) -> Result<Authority, Error> {
    let ctx = query.ctx();
    let (read, request) = {
        let mut s = state.write();
        if !query.current(&s) {
            return Err(unavailable());
        }
        if let Ok(authority) = Authority::capture(&s.core, &ctx.session.team_id) {
            return Ok(authority);
        }
        let read = BootRead::begin(&mut s.core, &ctx.transport, &ctx.session)
            .map_err(|_| unavailable())?;
        let request = match read.request(&s.core) {
            Ok(request) => request,
            Err(_) => {
                read.failed(&mut s.core);
                return Err(unavailable());
            }
        };
        (read, request)
    };
    let result = query.read(state, request).await;
    let mut s = state.write();
    if !query.current(&s) {
        read.failed(&mut s.core);
        return Err(unavailable());
    }
    let raw = match result {
        Ok(raw) => raw,
        Err(error) => {
            read.failed(&mut s.core);
            return Err(error);
        }
    };
    if !read.apply(&mut s.core, raw, false)? {
        return Err(unavailable());
    }
    Authority::capture(&s.core, &ctx.session.team_id).map_err(|_| unavailable())
}
async fn raw_channel(
    mut state: Signal<ShellState>,
    query: &Query,
    authority: &Authority,
    id: &str,
) -> Result<RawChannel, Error> {
    let ctx = query.ctx();
    let (read, request) = {
        let mut s = state.write();
        if !query.current(&s) || !authority.current(&s.core) {
            return Err(unavailable());
        }
        let read = authority
            .begin_channel_read(&mut s.core, id)
            .map_err(|_| unavailable())?;
        (
            read,
            api::conversations_info(&ctx.client, &ctx.session, id.to_owned()),
        )
    };
    let response = query.read(state, request).await?;
    let mut s = state.write();
    if !query.current(&s)
        || !read.current(&s.core)
        || response["ok"] != true
        || response.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(unavailable());
    }
    read.apply(
        &mut s.core,
        response.get("channel").cloned().ok_or_else(unavailable)?,
    )
    .map_err(|_| unavailable())
}
pub(super) async fn group_choice(
    state: Signal<ShellState>,
    query: &Query,
    id: &str,
    group: group::Witness,
) -> Result<Choice, Error> {
    let authority = authority(state, query).await?;
    let raw = authority
        .raw_channel(&state.peek().core, id)
        .map_err(|_| unavailable())?;
    resolve_group(state, query, &authority, raw, None, Some(group)).await
}
async fn resolve(
    state: Signal<ShellState>,
    query: &Query,
    authority: &Authority,
    raw: RawChannel,
    acquired_peer: Option<Arc<peer::Peer>>,
) -> Result<Choice, Error> {
    resolve_group(state, query, authority, raw, acquired_peer, None).await
}
async fn resolve_group(
    mut state: Signal<ShellState>,
    query: &Query,
    authority: &Authority,
    mut raw: RawChannel,
    acquired_peer: Option<Arc<peer::Peer>>,
    group: Option<group::Witness>,
) -> Result<Choice, Error> {
    let mut features_read = false;
    let mut roles_read = std::collections::BTreeSet::new();
    let mut changed_role_refresh = false;
    loop {
        let result = {
            let mut s = state.write();
            if !query.current(&s) || !authority.current(&s.core) || !raw.current(&s.core) {
                return Err(unavailable());
            }
            raw.observe(&mut s.core).and_then(|()| {
                authority
                    .metadata(&s.core)?
                    .effective(authority.configuration(), raw.value())
            })
        };
        match result {
            Ok(flags) => {
                let peer = if raw.value()["is_im"] == true {
                    match &acquired_peer {
                        Some(peer) if peer.current(&state.peek()) => Some(peer.clone()),
                        Some(_) => return Err(unavailable()),
                        None => Some(peer::acquire(state, query, authority, &raw, flags).await?),
                    }
                } else {
                    None
                };
                let choice = Choice::capture_group(
                    &state.peek(),
                    authority.clone(),
                    raw,
                    &query.ctx().session.team_id,
                    peer,
                    group,
                )?;
                return choice
                    .current(&state.peek())
                    .then_some(choice)
                    .ok_or_else(unavailable);
            }
            Err(Unknown::Policy(posting_policy::Unknown::FeatureAccess)) if !features_read => {
                features_read = true;
                let read = authority
                    .metadata_mut(&mut state.write().core)
                    .and_then(|m| m.begin_feature_read(authority.configuration()))
                    .map_err(|_| unavailable())?;
                dependency(state, query, authority, read).await?;
            }
            Err(Unknown::Policy(posting_policy::Unknown::ChannelAdmin)) => {
                let missing = authority
                    .metadata(&state.peek().core)
                    .and_then(|m| m.missing_workspace_roles(authority.configuration(), raw.value()))
                    .map_err(|_| unavailable())?;
                if missing.is_empty() || missing.len() > 100 {
                    return Err(unavailable());
                }
                for team in missing {
                    if !roles_read.insert(team.clone()) {
                        return Err(unavailable());
                    }
                    let read = {
                        let mut s = state.write();
                        if !query.current(&s) || !raw.current(&s.core) {
                            return Err(unavailable());
                        }
                        authority
                            .metadata_mut(&mut s.core)
                            .and_then(|m| {
                                m.begin_workspace_role_read(
                                    authority.configuration(),
                                    raw.value(),
                                    &team,
                                )
                            })
                            .map_err(|_| unavailable())?
                    };
                    dependency(state, query, authority, read).await?;
                    if !raw.current(&state.peek().core) {
                        if changed_role_refresh {
                            return Err(unavailable());
                        }
                        changed_role_refresh = true;
                        let id = raw.value()["id"].as_str().ok_or_else(unavailable)?;
                        raw = raw_channel(state, query, authority, id).await?;
                    }
                }
            }
            _ => return Err(unavailable()),
        }
    }
}
async fn dependency(
    mut state: Signal<ShellState>,
    query: &Query,
    authority: &Authority,
    read: super_platinum_core::state::posting_metadata::Read,
) -> Result<(), Error> {
    let ctx = query.ctx();
    let request = {
        let s = state.peek();
        if !query.current(&s) {
            return Err(unavailable());
        }
        read.request(
            authority.metadata(&s.core).map_err(|_| unavailable())?,
            authority.configuration(),
            &ctx.client,
            &ctx.session,
        )
        .map_err(|_| unavailable())?
    };
    let response = query.read(state, request).await?;
    let mut s = state.write();
    if !query.current(&s) {
        return Err(unavailable());
    }
    read.apply(
        authority
            .metadata_mut(&mut s.core)
            .map_err(|_| unavailable())?,
        authority.configuration(),
        &response,
    )
    .map_err(|_| unavailable())
}
fn unavailable() -> Error {
    Error::Api("workflow_share_channel_unavailable".into())
}

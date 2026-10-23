//! Confirmation-owned channel observations. Each selected channel gets a fresh
//! ordered posting read; the canonical producer owns fromAnotherTeam.
use super::*;
use crate::app_home::channel_fetches::canonical;
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
}
impl Choice {
    fn capture(
        s: &ShellState,
        authority: Authority,
        raw: RawChannel,
        team: &str,
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
        let label = raw.value()["name"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(unavailable)?
            .to_owned();
        Ok(Self {
            authority,
            raw,
            canonical,
            selected,
            label,
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
    pub(super) fn flags(&self, s: &ShellState) -> Option<Flags> {
        if !self.authority.current(&s.core)
            || !self.raw.current(&s.core)
            || !self.canonical.current(s)
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
        protocol::confirmation_add::channel_eligible(
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
    pub(super) fn feature_plan(
        &self,
        s: &ShellState,
        trigger: &Value,
        previews: &super_platinum_core::channel_workflows::Previews,
        manage: super_platinum_core::channel_workflows::feature_permission::Permission,
    ) -> Result<super_platinum_core::channel_workflows::feature::Add, Error> {
        let posting = self.flags(s).ok_or_else(unavailable)?;
        let metadata = self
            .authority
            .metadata(&s.core)
            .map_err(|_| unavailable())?;
        let ctx = metadata
            .context(self.authority.configuration(), self.raw.value())
            .map_err(|_| unavailable())?;
        super_platinum_core::channel_workflows::feature::Add::new(
            trigger,
            self.raw(),
            previews,
            super_platinum_core::channel_workflows::feature::Facts {
                team_id: ctx.team_id,
                top_level_team_id: ctx.top_level_team_id,
                self_member: ctx.member,
                posting,
                can_post: self.current(s),
                manage,
            },
        )
    }
    pub(super) fn scope(&self, s: &ShellState) -> Option<(String, String)> {
        if !self.current(s) {
            return None;
        }
        let ctx = self
            .authority
            .metadata(&s.core)
            .ok()?
            .context(self.authority.configuration(), self.raw.value())
            .ok()?;
        Some((ctx.team_id.into(), ctx.top_level_team_id.into()))
    }
}
#[derive(Clone)]
pub(super) struct Listing {
    authority: Authority,
    choices: Vec<Choice>,
    incomplete: bool,
}
impl Listing {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.authority.current(&s.core) && self.choices.iter().all(|c| c.current(s))
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
    let authority = authority(state, query).await?;
    // Canonical identities are acquisition hints, not posting permission.
    // Canonical hydration can invalidate Boot's raw posting stamps; enumerating
    // only current posting witnesses would silently turn those into no results.
    let canonical = canonical::observed(&state.peek(), &query.ctx().session.team_id)?;
    let ids = canonical
        .rows()
        .iter()
        .filter(|row| row["is_im"] != true && row["is_mpim"] != true)
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let mut choices = Vec::new();
    let mut incomplete = ids.len() > 50;
    for id in ids.into_iter().take(50) {
        let observed = {
            let s = state.peek();
            if !query.current(&s) || !canonical.current(&s) {
                return Err(unavailable());
            }
            authority.raw_channel(&s.core, &id)
        };
        let raw = match observed {
            Ok(raw) => raw,
            Err(_) => match raw_channel(state, query, &authority, &id).await {
                Ok(raw) => raw,
                Err(_) => {
                    incomplete = true;
                    continue;
                }
            },
        };
        let s = state.peek();
        if !query.current(&s) || !canonical.current(&s) {
            return Err(unavailable());
        }
        incomplete |= authority
            .metadata(&s.core)
            .and_then(|m| m.effective(authority.configuration(), raw.value()))
            .is_err();
        match Choice::capture(&s, authority.clone(), raw, &query.ctx().session.team_id) {
            Ok(choice) if choice.current(&s) => choices.push(choice),
            Ok(_) => {}
            Err(_) => incomplete = true,
        }
    }
    let s = state.peek();
    if !query.current(&s) || !canonical.current(&s) || !authority.current(&s.core) {
        return Err(unavailable());
    }
    Ok(Listing {
        authority,
        choices,
        incomplete,
    })
}
pub(super) async fn validate(
    state: Signal<ShellState>,
    query: &Query,
    id: &str,
) -> Result<Choice, Error> {
    let authority = authority(state, query).await?;
    let raw = raw_channel(state, query, &authority, id).await?;
    resolve(state, query, &authority, raw).await
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
async fn resolve(
    mut state: Signal<ShellState>,
    query: &Query,
    authority: &Authority,
    mut raw: RawChannel,
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
            Ok(_) => {
                let choice = Choice::capture(
                    &state.peek(),
                    authority.clone(),
                    raw,
                    &query.ctx().session.team_id,
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
    Error::Api("workflow_confirmation_channel_unavailable".into())
}

//! Fresh DM peer facts remain owned by the original Share query and source.
use super::{Query, unavailable};
use crate::{
    app_home::channel_fetches::{canonical, context::Context},
    state::ShellState,
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};
use super_platinum_core::{
    block_actions::{
        directory_endpoint::{
            DirectoryEndpoint,
            member_info::{self, Page, Read},
        },
        posting_policy::Flags,
    },
    slack::{Error, models::User},
    state::{
        directory_members::Subset,
        posting_metadata::authority::{Authority, RawChannel},
    },
};
mod person;
pub(super) use person::{
    Candidate, acquire_person, candidates, candidates_for, ranking_candidates,
};

const GENERATION: &str = "workflow-share-existing-dm";
struct Witness {
    ctx: Arc<Context>,
    subset: Subset,
    prior: BTreeMap<String, Option<bool>>,
    id: String,
}
impl Witness {
    fn capture(s: &ShellState, team: &str, id: &str) -> Result<Self, Error> {
        let ctx = Arc::new(Context::capture(s, team)?);
        let w = s.core.workspaces.get(team).ok_or_else(unavailable)?;
        let subset = w.directory_members.subset(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &BTreeSet::from([id.to_owned()]),
            &w.users,
        )?;
        let observed =
            w.directory_members
                .observations(&ctx.account, &ctx.session, ctx.epoch, GENERATION)?;
        let prior = BTreeMap::from([(
            id.to_owned(),
            *observed.interactions().get(id).ok_or_else(unavailable)?,
        )]);
        Ok(Self {
            ctx,
            subset,
            prior,
            id: id.into(),
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        let c = &self.ctx;
        c.current(s)
            && s.core.workspaces.get(&c.session.team_id).is_some_and(|w| {
                self.subset.current(
                    &w.directory_members,
                    &c.account,
                    &c.session,
                    c.epoch,
                    GENERATION,
                    &w.users,
                ) && self.subset.current_rows(
                    &w.directory_members,
                    &c.account,
                    &c.session,
                    c.epoch,
                    GENERATION,
                    &w.users,
                )
            })
    }
}
pub(super) struct Peer {
    witness: Arc<Witness>,
    raw: Value,
    effective: Value,
    label: String,
}
impl Peer {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.witness.current(s)
    }
    pub(super) fn effective(&self) -> &Value {
        &self.effective
    }
    pub(super) fn label(&self) -> &str {
        &self.label
    }
    pub(super) fn evidence(&self) -> Value {
        json!({"id":self.witness.id,"raw":self.raw,"effective":self.effective,
            "source":self.witness.subset.rows().first().map(|r|&r.source)})
    }
}
/// Acquisition shortlist only; no posting grant. Preserve unsupported bot IMs
/// outside this slice without spending requests for their permissions.
pub(super) fn candidate(s: &ShellState, team: &str, row: &Value) -> bool {
    if row["is_im"] != true || !row["id"].as_str().is_some_and(|id| id.starts_with('D')) {
        return false;
    }
    let Some(id) = row["user"].as_str() else {
        return false;
    };
    if matches!(
        id,
        "USLACK" | "USLACKBOT" | "USLACKSECURITY" | "USEARCH" | "UCHANNELEXPERT"
    ) {
        return false;
    }
    let Ok(witness) = Witness::capture(s, team, id) else {
        return false;
    };
    let [peer] = witness.subset.rows() else {
        return false;
    };
    [
        "is_bot",
        "is_app_user",
        "is_workflow_bot",
        "is_profile_only_user",
    ]
    .iter()
    .all(|key| {
        matches!(
            peer.source.get(*key),
            None | Some(Value::Null) | Some(Value::Bool(false))
        )
    }) && matches!(peer.source.get("bot_id"), None | Some(Value::Null))
}
pub(super) async fn acquire(
    state: Signal<ShellState>,
    query: &Query,
    authority: &Authority,
    raw: &RawChannel,
    flags: Flags,
) -> Result<Arc<Peer>, Error> {
    let team = &query.ctx().session.team_id;
    let (canonical, witness, selected) = {
        let s = state.peek();
        if !query.current(&s) || !authority.current(&s.core) || !raw.current(&s.core) {
            return Err(unavailable());
        }
        let canonical = canonical::observed(&s, team)?;
        let channel = raw.value();
        let id = channel["id"].as_str().ok_or_else(unavailable)?;
        let local = canonical.row(id).ok_or_else(unavailable)?;
        let peer = channel["user"].as_str().ok_or_else(unavailable)?;
        if local["is_im"] != true || local["user"].as_str() != Some(peer) {
            return Err(unavailable());
        }
        let witness = Witness::capture(&s, team, peer)?;
        let mut selected = channel.clone();
        selected["fromAnotherTeam"] = local
            .get("fromAnotherTeam")
            .filter(|v| v.is_boolean())
            .cloned()
            .ok_or_else(unavailable)?;
        let [source] = witness.subset.rows() else {
            return Err(unavailable());
        };
        // Existing ordinary-human source is an acquisition prerequisite only.
        // Fresh returned peer facts still determine final admission below.
        if !super_platinum_core::workflow_share::existing_dm_eligible(
            &selected,
            &source.source,
            flags,
            team,
        ) {
            return Err(unavailable());
        }
        (canonical, Arc::new(witness), selected)
    };
    let current = |s: &ShellState| {
        query.current(s)
            && authority.current(&s.core)
            && raw.current(&s.core)
            && canonical.current(s)
            && witness.current(s)
    };
    let fresh = acquire_fresh(state, query, &witness, &current).await?;
    if !super_platinum_core::workflow_share::existing_dm_eligible(
        &selected,
        &fresh.effective,
        flags,
        team,
    ) {
        return Err(unavailable());
    }
    Ok(Arc::new(Peer {
        witness,
        raw: fresh.raw,
        effective: fresh.effective,
        label: fresh.label,
    }))
}

struct Fresh {
    raw: Value,
    effective: Value,
    label: String,
}
/// Exact shared read mechanics only. Each caller supplies its original source
/// fence and separately applies its own DM or person admission policy.
async fn acquire_fresh(
    state: Signal<ShellState>,
    query: &Query,
    witness: &Witness,
    owner_current: &impl Fn(&ShellState) -> bool,
) -> Result<Fresh, Error> {
    let current = |s: &ShellState| query.current(s) && witness.current(s) && owner_current(s);
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    let ctx = query.ctx();
    let (discovery, _) = query
        .read_guarded(
            state,
            DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?,
            false,
            &current,
        )
        .await?;
    let endpoint = DirectoryEndpoint::decode(&ctx.session, &discovery, Instant::now())?;
    let read = Read::capture(vec![witness.id.clone()])?;
    let request = endpoint.member_info(&ctx.client, &ctx.session, Instant::now(), &read)?;
    let (response, keys) = query.read_guarded(state, request, true, &current).await?;
    if !current(&state.peek()) || !endpoint.current(&ctx.session, Instant::now()) {
        return Err(unavailable());
    }
    let page = Page::decode(&read, response, keys)?;
    if !page.failed_ids().is_empty()
        || !page.invalid_ids().is_empty()
        || !page.pending_ids().is_empty()
        || !page.disconnected_ids().is_empty()
        || page.next_marker().is_some()
    {
        return Err(unavailable());
    }
    let [actual] = page.rows() else {
        return Err(unavailable());
    };
    if actual["id"].as_str() != Some(witness.id.as_str()) {
        return Err(unavailable());
    }
    let projected =
        member_info::project_interaction(page.rows(), page.interactions(), &witness.prior)?;
    let [effective] = projected.as_slice() else {
        return Err(unavailable());
    };
    let user: User = serde_json::from_value(effective.clone()).map_err(|_| unavailable())?;
    let label = super_platinum_core::state::display_name(Some(&user), &witness.id);
    Ok(Fresh {
        raw: actual.clone(),
        effective: effective.clone(),
        label,
    })
}

#[cfg(test)]
mod tests;

//! Current native selector facts for Activity's member-only ranker. Remote
//! scores/facts cannot supply native conversation identity or navigation data.
use super::*;
use crate::app_home::workflows::{navigation_history, ranking_cache};
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::{
    block_actions::home::workflow::{
        directory_local, directory_observations::Entity, directory_selectors as selectors,
    },
    composer_emoji::history::History,
};
#[cfg(test)]
mod tests;

pub(super) struct Input {
    pub(super) selectors: Value,
    pub(super) history: History,
    pub(super) cache: ranking_cache::Snapshot,
    pub(super) now_ms: f64,
    names: names::Observation,
    navigation: navigation_history::Witness,
    native: Value,
    rows: Vec<Value>,
    drafts: drafts::Receipt,
}
impl Input {
    pub(super) fn current(&self, s: &ShellState, q: &Query, rows: &[Value]) -> bool {
        rows == self.rows
            && q.current(s)
            && self.names.current(s, q)
            && self.navigation.current(s)
            && self.cache.current(s)
            && self.drafts.current(s, q)
            && native(s, q, rows).is_ok_and(|raw| raw == self.native)
    }
    /// Only lazy draft bits may change this retained ranking seed. All names,
    /// models, assignments, navigation and non-draft native facts stay exact.
    pub(super) fn refresh_drafts(
        &mut self,
        s: &ShellState,
        q: &Query,
        rows: &[Value],
    ) -> Result<(), Error> {
        if rows != self.rows
            || !q.current(s)
            || !self.names.current(s, q)
            || !self.navigation.current(s)
            || !self.cache.current(s)
        {
            return Err(unavailable());
        }
        let current = native(s, q, rows)?;
        if without_drafts(&current) != without_drafts(&self.native) {
            return Err(unavailable());
        }
        self.selectors["metadata"] = current["metadata"].clone();
        self.native = current;
        self.drafts = drafts::snapshot(s, q)?;
        Ok(())
    }
}

fn without_drafts(raw: &Value) -> Value {
    let mut copy = raw.clone();
    if let Some(metadata) = copy["metadata"].as_object_mut() {
        for row in metadata.values_mut() {
            if let Some(row) = row.as_object_mut() {
                row.remove("draft");
            }
        }
    }
    copy
}

pub(super) async fn capture(
    state: Signal<ShellState>,
    q: &Query,
    names: &names::Observation,
    rows: &[Value],
) -> Result<Input, Error> {
    if !names.current(&state.peek(), q) {
        return Err(unavailable());
    }
    let drafts = drafts::snapshot(&state.peek(), q)?;
    // ed8e shared entry is an explicit pending composition/review prerequisite;
    // original Home cache paths are not copied into this parent controller.
    let cache = ranking_cache::capture_for(state, q.owner.ctx.clone(), |s| q.current(s))?;
    let account = state
        .peek()
        .core
        .active_account
        .clone()
        .ok_or_else(unavailable)?;
    let (navigation, navigation_owner) =
        navigation_history::capture_for(state, &account, &q.owner.ctx.session.team_id, |s| {
            q.current(s) && names.current(s, q)
        })
        .await?;
    let ctx = &q.owner.ctx;
    let assignments = read::request(
        state,
        q,
        super_platinum_core::slack::api::list_experiments(&ctx.client, &ctx.session),
    )
    .await?;
    let mut experiments = serde_json::Map::new();
    for key in [
        "autocomplete_affinity_score",
        "autocomplete_affinity_ml_model",
    ] {
        experiments.insert(
            key.into(),
            serde_json::json!(assignment(&assignments, key)?),
        );
    }
    let source = native(&state.peek(), q, rows)?;
    let mut members = BTreeMap::new();
    // Only completed source hydration rows and the actually acquired self are
    // used for names. Typed native cache rows remain comparison witnesses.
    for row in rows {
        let id = row["id"].as_str().ok_or_else(unavailable)?.to_owned();
        if members.insert(id, row.clone()).is_some() {
            return Err(unavailable());
        }
    }
    members.insert(ctx.session.user_id.clone(), names.gantry["self"].clone());
    let fields = members
        .values()
        .flat_map(|raw| {
            ["real_name", "display_name", "email"]
                .map(|key| raw["profile"][key].as_str().unwrap_or("").to_owned())
        })
        .collect();
    let normalized = render::normalize(state, q, fields).await?;
    let mut observed_members = BTreeMap::new();
    for (id, raw) in &members {
        let mut raw = raw.clone();
        raw["is_self"] = (id == &ctx.session.user_id).into();
        let comparison = names
            .policy
            .member_name(&raw, "Name hidden")
            .map_err(|_| unavailable())?;
        for key in ["real_name", "display_name", "email"] {
            if normalized
                .name(raw["profile"][key].as_str().unwrap_or(""))
                .is_none()
            {
                return Err(unavailable());
            }
        }
        let raw = directory_local::derive_member_names(&raw, |value| {
            normalized
                .name(value)
                .expect("all captured source fields checked")
                .to_owned()
        })
        .ok_or_else(unavailable)?;
        observed_members.insert(
            id.clone(),
            selectors::item(
                &raw,
                selectors::Names {
                    comparison: &comparison,
                    mpim: None,
                    custom: None,
                    custom_allowed: false,
                },
            )
            .ok_or_else(unavailable)?,
        );
    }
    let mut ims = serde_json::Map::new();
    for (user, raw) in source["ims"].as_object().ok_or_else(unavailable)? {
        let comparison = observed_members
            .get(user)
            .and_then(|row| row["facts"]["comparisonName"].as_str())
            .ok_or_else(unavailable)?;
        ims.insert(
            user.clone(),
            selectors::item(
                raw,
                selectors::Names {
                    comparison,
                    mpim: None,
                    custom: None,
                    custom_allowed: false,
                },
            )
            .ok_or_else(unavailable)?,
        );
    }
    if names.enterprise.is_some()
        && names.default_workspace.is_none()
        && observed_members
            .values()
            .any(|raw| raw["enterprise_id"].as_str() == names.enterprise.as_deref())
    {
        return Err(unavailable());
    }
    let mut selectors = serde_json::json!({"selfId":ctx.session.user_id,"self":observed_members.get(&ctx.session.user_id),
        "teamId":ctx.session.team_id,"enterpriseId":names.enterprise,"enterprise":names.enterprise.is_some(),
        "searcherTeams":names.policy.workspace_ids().collect::<Vec<_>>(),"defaultTeam":names.default_workspace,
        "members":observed_members,"ims":ims,"metadata":source["metadata"],"navigationHistory":navigation,
        "experiments":experiments,"offline":{"channel":{},"mpim":{}},"entityCtr":{},"remoteDm":{},"remoteMpim":{},"affinity":{},"channelPriority":{}});
    for kind in [Entity::Member, Entity::Channel, Entity::Mpim, Entity::Im] {
        selectors["entityCtr"][kind.key()] = serde_json::json!(cache.offline.entity_ctr(kind));
    }
    for id in members.keys() {
        for kind in [Entity::Channel, Entity::Mpim] {
            if let Some(f) = cache.offline.features(id, kind) {
                selectors["offline"][kind.key()][id] = serde_json::json!({"team_ctr_90dd":f.team_ctr_90dd,"user_ctr_30dd":f.user_ctr_30dd});
            }
        }
        if let Some(v) = cache.offline.dm(id) {
            selectors["remoteDm"][id] = v.clone();
        }
        if let Some(v) = cache.offline.mpdm(id) {
            selectors["remoteMpim"][id] = v.clone();
        }
        selectors["affinity"][id] = serde_json::json!(cache.offline.affinity(id));
        selectors["channelPriority"][id] = serde_json::json!(cache.offline.priority(id));
    }
    let result = Input {
        selectors,
        history: names.history.clone(),
        cache,
        now_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| unavailable())?
            .as_secs_f64()
            * 1000.0,
        names: names.clone(),
        navigation: navigation_owner,
        native: source,
        rows: rows.to_vec(),
        drafts,
    };
    if !result.current(&state.peek(), q, rows) {
        return Err(unavailable());
    }
    Ok(result)
}

fn assignment<'a>(raw: &'a Value, key: &str) -> Result<Option<&'a str>, Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|e| !e.is_null()) {
        return Err(unavailable());
    }
    let assignments = raw["assignments"].as_object().ok_or_else(unavailable)?;
    assignments
        .get(key)
        .map(|v| v["group"].as_str().ok_or_else(unavailable))
        .transpose()
}

fn native(s: &ShellState, q: &Query, rows: &[Value]) -> Result<Value, Error> {
    if !q.current(s) {
        return Err(unavailable());
    }
    let team = &q.owner.ctx.session.team_id;
    let workspace = s.core.workspaces.get(team).ok_or_else(unavailable)?;
    let mut needed = BTreeSet::from([q.owner.ctx.session.user_id.clone()]);
    for row in rows {
        needed.insert(row["id"].as_str().ok_or_else(unavailable)?.to_owned());
    }
    let mut ims = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    for channel in workspace.channels.values().filter(|c| c.is_im) {
        let Some(peer) = channel.user.as_ref().filter(|id| needed.contains(*id)) else {
            continue;
        };
        if ims
            .insert(
                peer.clone(),
                serde_json::to_value(channel).map_err(|_| unavailable())?,
            )
            .is_some()
        {
            return Err(unavailable());
        }
        let messages = workspace.messages.get(&channel.id);
        let local_draft =
            s.core
                .drafts
                .items
                .contains_key(&super_platinum_core::drafts::DraftKey {
                    team: team.clone(),
                    channel: channel.id.clone(),
                    thread: None,
                });
        metadata.insert(channel.id.clone(),serde_json::json!({
            "lastRead":messages.and_then(|m| m.last_read.as_ref()).or(channel.last_read.as_ref()),
            "latest":messages.and_then(|m| m.latest.as_ref()),"starred":channel.is_starred,
            "unread":messages.map(|m| m.unread_count>0).unwrap_or_else(|| channel.unread_count.is_some_and(|n|n>0)),
            "draft":local_draft || workspace.draft_presence.has_channel(&channel.id),"localDraft":local_draft,"priority":workspace.priority_scores.get(&channel.id)
        }));
    }
    let native_users = needed
        .into_iter()
        .map(|id| {
            let raw = workspace
                .users
                .get(&id)
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| unavailable())?;
            Ok((id, raw))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    Ok(serde_json::json!({"ims":ims,"metadata":metadata,"users":native_users}))
}

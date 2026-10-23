//! Profile member ranking facts from original source, native DM metadata and
//! existing account caches. No Activity owner, policy or second cache is used.
use super::*;
use crate::directory_member_fetches::search::Snapshot;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::{
    block_actions::home::workflow::{
        directory_local, directory_observations::Entity, directory_selectors as selectors,
    },
    composer_emoji::history::History,
    slack::api::custom::texty::ranking as rank,
};
mod save;
#[cfg(test)]
mod tests;
pub(crate) use save::SaveMemberRank;

#[derive(Clone)]
pub(super) struct Input {
    pub(super) selectors: Value,
    pub(super) history: History,
    pub(super) cache: autocomplete::Ranking,
    pub(super) now_ms: f64,
    pub(super) passthrough: bool,
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    names: Arc<member_names::Observation>,
    source: Arc<Snapshot>,
    preferences: Arc<ranking::Preferences>,
    navigation: autocomplete::Navigation,
    native: Value,
    ids: BTreeSet<String>,
    draft_revision: u64,
    files: Option<file_rank_input::Input>,
}
impl Input {
    pub(super) fn file_ignore_fuzzy(&self) -> Result<bool, Error> {
        self.files
            .as_ref()
            .map(|files| files.ignore_fuzzy())
            .ok_or_else(unavailable)
    }

    fn base_current(&self, s: &ShellState, search: &Search<impl Host>) -> bool {
        (self.owner)(s)
            && search.current(s)
            && self.names.current(s, search)
            && self.source.current(s)
            && self.preferences.current(s)
            && self.cache.current(s)
            && self.navigation.current(s)
            && self
                .files
                .as_ref()
                .is_none_or(|files| files.current(s, search))
    }
    pub(super) fn current(&self, s: &ShellState, search: &Search<impl Host>) -> bool {
        self.base_current(s, search)
            && revision(s, search.lease.team()) == Some(self.draft_revision)
            && native(s, search.lease.team(), &self.ids).is_ok_and(|v| v == self.native)
    }
    /// Only shared server-draft presence may refresh this same query. A local
    /// composer, source, name, navigation or non-draft native change retires it.
    pub(super) fn refresh_drafts(
        &mut self,
        s: &ShellState,
        search: &Search<impl Host>,
    ) -> Result<(), Error> {
        if !self.base_current(s, search) {
            return Err(unavailable());
        }
        let next = native(s, search.lease.team(), &self.ids)?;
        if without_drafts(&next) != without_drafts(&self.native) {
            return Err(unavailable());
        }
        self.selectors["metadata"] = next["metadata"].clone();
        self.native = next;
        self.draft_revision = revision(s, search.lease.team()).ok_or_else(unavailable)?;
        Ok(())
    }
}
/// `rows` are original Snapshot source objects, not renderer-derived additions.
/// `lower` is the renderer's already observed ordinary toLowerCase map, including
/// gantry.self. Missing map entries refuse; no Rust lowercase approximation.
pub(super) async fn capture(
    state: Signal<ShellState>,
    search: &Search<impl Host>,
    names: Arc<member_names::Observation>,
    source: Arc<Snapshot>,
    preferences: Arc<ranking::Preferences>,
    rows: &[Value],
    lower: impl Fn(&str) -> Result<String, rank::Error>,
) -> Result<Input, Error> {
    capture_with_files(state, search, names, source, preferences, rows, lower, None).await
}

pub(super) async fn capture_with_files(
    state: Signal<ShellState>,
    search: &Search<impl Host>,
    names: Arc<member_names::Observation>,
    source: Arc<Snapshot>,
    preferences: Arc<ranking::Preferences>,
    rows: &[Value],
    lower: impl Fn(&str) -> Result<String, rank::Error>,
    files: Option<Arc<file_source::Snapshot>>,
) -> Result<Input, Error> {
    let current = |s: &ShellState| {
        search.current(s)
            && names.current(s, search)
            && source.current(s)
            && preferences.current(s)
            && files.as_ref().is_none_or(|files| files.current(s))
    };
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    let originals: Vec<_> = source.rows().iter().map(|r| r.source.clone()).collect();
    let mut members = admitted(&originals, rows)?;
    let ctx = &search.context;
    let self_id = &ctx.session.user_id;
    if names.gantry["self"]["id"].as_str() != Some(self_id.as_str()) {
        return Err(unavailable());
    }
    members.insert(self_id.clone(), names.gantry["self"].clone());
    let ids = members.keys().cloned().collect();
    let navigation = search.navigation(state).await?;
    if !current(&state.peek()) || !navigation.current(&state.peek()) {
        return Err(unavailable());
    }
    let (native, draft_revision) = {
        let s = state.peek();
        (
            native(&s, search.lease.team(), &ids)?,
            revision(&s, search.lease.team()).ok_or_else(unavailable)?,
        )
    };
    let mut observed = BTreeMap::new();
    for (id, row) in &members {
        let mut row = row.clone();
        row["is_self"] = json!(id == self_id);
        let comparison = names
            .policy
            .member_name(&row, "Name hidden")
            .map_err(|_| unavailable())?;
        let row = derive(&row, &lower)?;
        observed.insert(id.clone(), selector(&row, &comparison)?);
    }
    let mut ims = serde_json::Map::new();
    for (id, row) in native["ims"].as_object().ok_or_else(unavailable)? {
        let comparison = observed
            .get(id)
            .and_then(|r| r["facts"]["comparisonName"].as_str())
            .ok_or_else(unavailable)?;
        ims.insert(id.clone(), selector(row, comparison)?);
    }
    if names.enterprise.is_some()
        && names.default_workspace.is_none()
        && observed
            .values()
            .any(|row| row["enterprise_id"].as_str() == names.enterprise.as_deref())
    {
        return Err(unavailable());
    }
    let cache = search.ranking.clone();
    let mut selectors = json!({"selfId":self_id,"self":observed.get(self_id),"teamId":ctx.session.team_id,
        "enterpriseId":names.enterprise,"enterprise":names.enterprise.is_some(),
        "searcherTeams":names.policy.workspace_ids().collect::<Vec<_>>(),"defaultTeam":names.default_workspace,
        "members":observed,"ims":ims,"metadata":native["metadata"],"navigationHistory":navigation.value(),
        "experiments":preferences.experiments(),"offline":{"channel":{},"mpim":{}},"entityCtr":{},
        "remoteDm":{},"remoteMpim":{},"affinity":{},"channelPriority":{}});
    for kind in [Entity::Member, Entity::Channel, Entity::Mpim, Entity::Im] {
        selectors["entityCtr"][kind.key()] = json!(cache.offline().entity_ctr(kind));
    }
    selectors["entityCtr"]["userGroup"] = json!(cache.offline().other_ctr());
    for id in members.keys() {
        for kind in [Entity::Channel, Entity::Mpim] {
            if let Some(f) = cache.offline().features(id, kind) {
                selectors["offline"][kind.key()][id] =
                    json!({"team_ctr_90dd":f.team_ctr_90dd,"user_ctr_30dd":f.user_ctr_30dd});
            }
        }
        if let Some(v) = cache.offline().dm(id) {
            selectors["remoteDm"][id] = v.clone();
        }
        if let Some(v) = cache.offline().mpdm(id) {
            selectors["remoteMpim"][id] = v.clone();
        }
        selectors["affinity"][id] = json!(cache.offline().affinity(id));
        selectors["channelPriority"][id] = json!(cache.offline().priority(id));
    }
    let files = files
        .map(|source| {
            file_rank_input::capture(
                &state.peek(),
                search,
                source,
                &cache,
                preferences.assignments(),
                &mut selectors,
            )
        })
        .transpose()?;
    let result = Input {
        selectors,
        history: preferences.history().clone(),
        cache,
        now_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| unavailable())?
            .as_secs_f64()
            * 1000.0,
        passthrough: preferences.passthrough(),
        owner: {
            let lease = search.lease.clone();
            Arc::new(move |shell| lease.current(shell))
        },
        names,
        source,
        preferences,
        navigation,
        native,
        ids,
        draft_revision,
        files,
    };
    if !result.current(&state.peek(), search) {
        return Err(unavailable());
    }
    Ok(result)
}
fn admitted(source: &[Value], rows: &[Value]) -> Result<BTreeMap<String, Value>, Error> {
    let source: BTreeMap<_, _> = source
        .iter()
        .filter_map(|r| r["id"].as_str().map(|id| (id, r)))
        .collect();
    let mut selected = BTreeMap::new();
    for row in rows {
        let id = row["id"].as_str().ok_or_else(unavailable)?;
        if source.get(id).copied() != Some(row)
            || selected.insert(id.to_owned(), row.clone()).is_some()
        {
            return Err(unavailable());
        }
    }
    Ok(selected)
}
fn derive(
    row: &Value,
    lower: &impl Fn(&str) -> Result<String, rank::Error>,
) -> Result<Value, Error> {
    let mut fields = BTreeMap::new();
    for key in ["real_name", "display_name", "email"] {
        let value = row["profile"][key].as_str().unwrap_or("");
        fields.insert(value, lower(value).map_err(|_| unavailable())?);
    }
    directory_local::derive_member_names(row, |value| fields[value].clone()).ok_or_else(unavailable)
}
fn selector(row: &Value, comparison: &str) -> Result<Value, Error> {
    selectors::item(
        row,
        selectors::Names {
            comparison,
            mpim: None,
            custom: None,
            custom_allowed: false,
        },
    )
    .ok_or_else(unavailable)
}
fn revision(s: &ShellState, team: &str) -> Option<u64> {
    Some(s.core.workspaces.get(team)?.draft_presence.revision())
}
fn without_drafts(raw: &Value) -> Value {
    let mut raw = raw.clone();
    if let Some(rows) = raw["metadata"].as_object_mut() {
        for row in rows.values_mut() {
            if let Some(row) = row.as_object_mut() {
                row.remove("draft");
            }
        }
    }
    raw
}
fn native(s: &ShellState, team: &str, needed: &BTreeSet<String>) -> Result<Value, Error> {
    let workspace = s.core.workspaces.get(team).ok_or_else(unavailable)?;
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
        let local = s
            .core
            .drafts
            .items
            .contains_key(&super_platinum_core::drafts::DraftKey {
                team: team.into(),
                channel: channel.id.clone(),
                thread: None,
            });
        metadata.insert(channel.id.clone(),json!({
            "lastRead":messages.and_then(|m|m.last_read.as_ref()).or(channel.last_read.as_ref()),
            "latest":messages.and_then(|m|m.latest.as_ref()),"starred":channel.is_starred,
            "unread":messages.map(|m|m.unread_count>0).unwrap_or_else(||channel.unread_count.is_some_and(|n|n>0)),
            "draft":local||workspace.draft_presence.has_channel(&channel.id),"localDraft":local,
            "priority":workspace.priority_scores.get(&channel.id)}));
    }
    let users = needed
        .iter()
        .map(|id| {
            let raw = workspace
                .users
                .get(id)
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| unavailable())?;
            Ok((id.clone(), raw))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    Ok(json!({"ims":ims,"metadata":metadata,"users":users}))
}

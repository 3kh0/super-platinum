//! Share-owned observations for matching/ranking, never destination authority.
//! All asynchronous acquisition remains tied to the original canonical/native
//! inputs. A later read cannot bless a changed candidate or companion bag.
use super::*;
use crate::app_home::{autocomplete, channel_fetches::canonical::Canonical};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use super_platinum_core::{
    block_actions::home::workflow::{
        directory_local::derive_member_names,
        directory_observations::Entity,
        directory_selectors::{self, Names as SelectorNames},
    },
    composer_emoji::history::History,
    state::posting_metadata::authority::Authority,
};
mod account;
mod names;
mod source;

pub(super) struct Input {
    pub(super) cache: autocomplete::Ranking,
    pub(super) history: History,
    pub(super) selectors: Value,
    pub(super) members: Vec<Value>,
    pub(super) channels: Vec<Value>,
    pub(super) member_names: Vec<String>,
    pub(super) channel_names: Vec<String>,
    pub(super) normalized: super::render::Normalized,
    pub(super) reads: Vec<Value>,
    pub(super) observation_source: &'static str,
    pub(super) observation_reads: Vec<Value>,
    pub(super) now_ms: f64,
    pub(super) passthrough: bool,
    source: source::Source,
    authority: Authority,
    preferences_revision: u64,
    navigation: autocomplete::Navigation,
    complete: autocomplete::observations::Store,
    routing: crate::connection::autocomplete::Observation,
}
impl Input {
    pub(super) fn current(
        &self,
        s: &ShellState,
        q: &Query,
        guard: &impl Fn(&ShellState) -> bool,
    ) -> bool {
        q.current(s)
            && guard(s)
            && self.cache.current(s)
            && self.authority.current(&s.core)
            && self.navigation.current(s)
            && self.source.current(s, q)
            && self.complete.current(s)
            && self.routing.current(s)
            && self.preferences_revision == autocomplete::preferences_revision(s)
    }
}

fn item(raw: &Value, name: &str) -> Result<Value, Error> {
    directory_selectors::item(
        raw,
        SelectorNames {
            comparison: name,
            mpim: None,
            custom: None,
            custom_allowed: false,
        },
    )
    .ok_or_else(unavailable)
}

pub(super) async fn capture(
    state: Signal<ShellState>,
    q: &Query,
    canonical: Arc<Canonical>,
    members: &[peer::Candidate],
    channels: &[Value],
    guard: &impl Fn(&ShellState) -> bool,
) -> Result<Input, Error> {
    let plan = super_platinum_core::workflow_share::search::Query::new(
        q.expected["query"].as_str().ok_or_else(unavailable)?,
    )?;
    let routing = crate::connection::autocomplete::Observation::capture(
        &state.peek(),
        &q.ctx().session.team_id,
        &plan,
    )
    .ok_or_else(unavailable)?;
    let (source, authority, preferences_revision, account) = {
        let s = state.peek();
        if !q.current(&s) || !guard(&s) {
            return Err(unavailable());
        }
        (
            source::Source::capture(&s, q, canonical, members, channels)
                .map_err(|e| super::at("observations.source", e))?,
            Authority::capture(&s.core, &q.ctx().session.team_id)
                .map_err(|_| super::at("observations.authority", unavailable()))?,
            autocomplete::preferences_revision(&s),
            s.core.active_account.clone().ok_or_else(unavailable)?,
        )
    };
    let current = |s: &ShellState| {
        q.current(s)
            && guard(s)
            && routing.current(s)
            && source.current(s, q)
            && authority.current(&s.core)
            && preferences_revision == autocomplete::preferences_revision(s)
    };
    let acquired = account::capture(state, q, &authority, routing.online(), &current)
        .await
        .map_err(|e| super::at("observations.names", e))?;
    let complete = acquired.store;
    let current = |s: &ShellState| current(s) && complete.current(s);
    // The shared producer chooses cold versus warm/stale snapshots. Missing
    // offline names already refused above; no fabricated fallback is created.
    let cache = if routing.online() {
        autocomplete::Ranking::capture(state, q.original.owner.ctx.clone(), &current)
    } else {
        autocomplete::Ranking::capture_without_reads(state, q.original.owner.ctx.clone(), &current)
    }
    .map_err(|e| super::at("observations.cache", e))?;
    let names = &complete.complete().names;
    let assignments = &complete.complete().assignments;
    let reads = acquired.reads;
    let passthrough =
        names::assignment(&assignments, "autocomplete_flannel_fuzzy_match_passthrough")?
            == Some("treatment");
    let experiments = json!({
        "autocomplete_affinity_score":names::assignment(&assignments,"autocomplete_affinity_score")?,
        "autocomplete_affinity_ml_model":names::assignment(&assignments,"autocomplete_affinity_ml_model")?
    });
    let navigation =
        autocomplete::Navigation::capture(state, &account, &q.ctx().session.team_id, &current)
            .await
            .map_err(|e| super::at("observations.navigation", e))?;
    let mut raws = source.value()["members"]
        .as_object()
        .ok_or_else(unavailable)?
        .clone();
    if names.self_user["id"] != q.ctx().session.user_id {
        return Err(unavailable());
    }
    // Gantry is the explicit current self producer; ordinary peers remain exact
    // original member-source rows, rather than typed-user reserialization.
    raws.insert(q.ctx().session.user_id.clone(), names.self_user.clone());
    let mut fields = BTreeSet::new();
    let mut comparisons = BTreeMap::new();
    for (id, raw) in &raws {
        fields.extend(
            ["real_name", "display_name", "email"]
                .map(|k| raw["profile"][k].as_str().unwrap_or("").to_owned()),
        );
        let label = names
            .policy
            .member_name(raw, "Name hidden")
            .map_err(|_| super::at("observations.member-name", unavailable()))?;
        fields.insert(label.clone());
        comparisons.insert(id.clone(), label);
    }
    let channel_names = channels
        .iter()
        .map(|raw| {
            raw["name"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(unavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    fields.extend(channel_names.iter().cloned());
    let query = super_platinum_core::workflow_share::search::Query::new(
        q.expected["query"].as_str().ok_or_else(unavailable)?,
    )?;
    let normalized = super::render::normalize(state, q, query.wire(), fields, &current)
        .await
        .map_err(|e| super::at("observations.normalize", e))?;
    let mut observed = BTreeMap::new();
    for (id, raw) in &raws {
        let mut raw = raw.clone();
        raw["is_self"] = (id == &q.ctx().session.user_id).into();
        for key in ["real_name", "display_name", "email"] {
            if normalized
                .name(raw["profile"][key].as_str().unwrap_or(""))
                .is_none()
            {
                return Err(super::at("observations.normalized-field", unavailable()));
            }
        }
        let raw = derive_member_names(&raw, |value| {
            normalized
                .name(value)
                .expect("captured profile fields")
                .to_owned()
        })
        .ok_or_else(unavailable)?;
        observed.insert(
            id.clone(),
            item(&raw, &comparisons[id])
                .map_err(|e| super::at("observations.member-selectors", e))?,
        );
    }
    if observed[&q.ctx().session.user_id]["facts"]["team"]
        .as_str()
        .is_none_or(str::is_empty)
    {
        return Err(super::at("observations.self-team", unavailable()));
    }
    if names.enterprise.is_some()
        && names.default_workspace.is_none()
        && observed
            .values()
            .any(|v| v["enterprise_id"].as_str() == names.enterprise.as_deref())
    {
        return Err(unavailable());
    }
    let member_names = members
        .iter()
        .map(|m| comparisons.get(m.id()).cloned().ok_or_else(unavailable))
        .collect::<Result<Vec<_>, _>>()?;
    let member_items = members
        .iter()
        .map(|m| observed.get(m.id()).cloned().ok_or_else(unavailable))
        .collect::<Result<Vec<_>, _>>()?;
    let channel_items = channels
        .iter()
        .zip(&channel_names)
        .map(|(raw, name)| item(raw, name))
        .collect::<Result<Vec<_>, _>>()?;
    let mut ims = serde_json::Map::new();
    for (id, raw) in source.value()["ims"].as_object().ok_or_else(unavailable)? {
        ims.insert(
            id.clone(),
            if raw.is_null() {
                Value::Null
            } else {
                item(raw, &comparisons[id])?
            },
        );
    }
    let history = navigation_for(navigation.value(), &source.value()["metadata"])
        .map_err(|e| super::at("observations.navigation-projection", e))?;
    let mut selectors = json!({"selfId":q.ctx().session.user_id,"self":observed.get(&q.ctx().session.user_id),
        "teamId":q.ctx().session.team_id,"enterpriseId":names.enterprise,"enterprise":names.enterprise.is_some(),
        "searcherTeams":names.policy.workspace_ids().collect::<Vec<_>>(),"defaultTeam":names.default_workspace,
        "members":observed,"ims":ims,"metadata":source.value()["metadata"],"navigationHistory":history,
        "experiments":experiments,"offline":{"channel":{}},"entityCtr":{},"remoteDm":{},"remoteMpim":{},"affinity":{},"channelPriority":{}});
    for kind in [Entity::Member, Entity::Channel] {
        selectors["entityCtr"][kind.key()] = json!(cache.offline().entity_ctr(kind));
    }
    for m in members {
        let id = m.id();
        selectors["remoteDm"][id] = cache.offline().dm(id).cloned().unwrap_or(Value::Null);
        selectors["remoteMpim"][id] = cache.offline().mpdm(id).cloned().unwrap_or(Value::Null);
        selectors["affinity"][id] = json!(cache.offline().affinity(id));
    }
    for raw in channels {
        let id = raw["id"].as_str().ok_or_else(unavailable)?;
        selectors["offline"]["channel"][id] = cache
            .offline()
            .features(id, Entity::Channel)
            .map(|f| json!({"team_ctr_90dd":f.team_ctr_90dd,"user_ctr_30dd":f.user_ctr_30dd}))
            .unwrap_or(Value::Null);
        selectors["channelPriority"][id] = json!(cache.offline().priority(id));
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| unavailable())?
        .as_secs_f64()
        * 1000.0;
    let result = Input {
        cache,
        history: names.history.clone(),
        selectors,
        members: member_items,
        channels: channel_items,
        member_names,
        channel_names,
        normalized,
        reads,
        observation_source: acquired.source,
        observation_reads: complete.complete().reads.clone(),
        now_ms,
        passthrough,
        source,
        authority,
        preferences_revision,
        navigation,
        complete,
        routing,
    };
    if !result.current(&state.peek(), q, guard) {
        return Err(super::at("observations.final-current", unavailable()));
    }
    Ok(result)
}

fn navigation_for(observed: &Value, metadata: &Value) -> Result<Value, Error> {
    // A complete acquired projection establishes absence only for IDs read from
    // the original metadata witness. Missing/unacquired projection refuses.
    let mut rows = observed.as_object().ok_or_else(unavailable)?.clone();
    for id in metadata.as_object().ok_or_else(unavailable)?.keys() {
        rows.entry(id.clone()).or_insert(Value::Null);
    }
    Ok(Value::Object(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_preserves_observed_values_and_marks_only_acquired_absence() {
        let raw = json!({"C00000001":{"ts":42,"position":2}});
        let result = navigation_for(&raw, &json!({"C00000001":{},"D00000002":{}})).unwrap();
        assert_eq!(result["C00000001"], raw["C00000001"]);
        assert_eq!(result.get("D00000002"), Some(&Value::Null));
        assert!(result.get("COTHER001").is_none());
        assert!(navigation_for(&Value::Null, &json!({})).is_err());
        assert!(navigation_for(&raw, &Value::Null).is_err());
    }

    #[test]
    fn remote_rank_facts_cannot_replace_canonical_type_or_observed_name() {
        let raw = json!({"id":"C00000001","name":"raw name","is_channel":true,"is_member":true,
            "facts":{"member":true,"comparisonName":"forged"}});
        let projected = item(&raw, "Observed name").unwrap();
        assert_eq!(projected["facts"]["member"], false);
        assert_eq!(projected["facts"]["comparisonName"], "Observed name");
        assert_eq!(projected["name"], "raw name");
        assert!(item(&Value::Null, "missing").is_err());
    }
}

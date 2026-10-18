//! One current native selector snapshot for the retained workflow ranker.
//! Metadata affects ordering only; Choice still owns every admission proof.
use super::*;
use std::collections::BTreeMap;
use super_platinum_core::block_actions::home::workflow::{
    directory_local::{derive_member_names, visible_mpim_custom_name},
    directory_observations::Entity as OfflineEntity,
    directory_selectors::{self as selectors, Names},
};

pub(super) struct Witness {
    names: entity_names::Observation,
    navigation: navigation_history::Witness,
    native: Value,
}
impl Witness {
    pub(super) fn current(
        &self,
        s: &ShellState,
        q: &entity_search::Query,
        rows: &[Choice],
    ) -> bool {
        self.names.current(s)
            && self.navigation.current(s)
            && native(s, q, rows).is_ok_and(|v| v == self.native)
    }
}

// Read actual native stores, without touching them or deriving latest from a
// currently rendered transcript. Missing values remain missing observations.
fn native(s: &ShellState, q: &entity_search::Query, rows: &[Choice]) -> Result<Value, Error> {
    if !q.current(s) {
        return Err(unavailable());
    }
    let team = &q.owner().owner.ctx.session.team_id;
    let w = s.core.workspaces.get(team).ok_or_else(unavailable)?;
    let mut needed = BTreeSet::from([q.owner().owner.ctx.session.user_id.clone()]);
    let mut ids = BTreeSet::new();
    for row in rows {
        match &row.entity {
            Entity::User { user, .. } => {
                needed.insert(user.user().id.clone());
            }
            Entity::Conversation { raw, .. } => {
                ids.insert(raw["id"].as_str().ok_or_else(unavailable)?.to_owned());
                if let Some(id) = raw["user"].as_str() {
                    needed.insert(id.into());
                }
                if let Some(members) = raw["members"].as_array() {
                    needed.extend(members.iter().filter_map(Value::as_str).map(str::to_owned));
                }
            }
        }
    }
    let mut ims = BTreeMap::new();
    for channel in w.channels.values().filter(|c| c.is_im) {
        if let Some(peer) = channel.user.as_ref().filter(|id| needed.contains(*id)) {
            // Do not choose an arbitrary D identity if the native index is
            // ambiguous; a peer ID never constructs a replacement conversation.
            if ims
                .insert(
                    peer.clone(),
                    serde_json::to_value(channel).map_err(|_| unavailable())?,
                )
                .is_some()
            {
                return Err(unavailable());
            }
            ids.insert(channel.id.clone());
        }
    }
    let users = needed
        .into_iter()
        .map(|id| {
            let raw = w
                .users
                .get(&id)
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| unavailable())?;
            Ok((id, raw.unwrap_or(Value::Null)))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let mut metadata = BTreeMap::new();
    let mut channels = BTreeMap::new();
    for id in ids {
        let channel = w.channels.get(&id);
        let messages = w.messages.get(&id);
        let key = super_platinum_core::drafts::DraftKey {
            team: team.clone(),
            channel: id.clone(),
            thread: None,
        };
        metadata.insert(id.clone(), serde_json::json!({
            "lastRead": messages.and_then(|m| m.last_read.as_ref()).or_else(|| channel.and_then(|c| c.last_read.as_ref())),
            "latest": messages.and_then(|m| m.latest.as_ref()),
            "starred": channel.is_some_and(|c| c.is_starred),
            "unread": messages.map(|m| m.unread_count > 0).unwrap_or_else(|| channel.and_then(|c| c.unread_count).is_some_and(|n| n > 0)),
            "draft": s.core.drafts.items.contains_key(&key),
            "priority": w.priority_scores.get(&id)
        }));
        channels.insert(
            id,
            channel
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| unavailable())?,
        );
    }
    Ok(serde_json::json!({"users":users,"ims":ims,"channels":channels,"metadata":metadata}))
}

fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|n| n != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
// Exact 7tpZ.Oi + Xt53.Ux: this is an internal model name, deliberately distinct
// from the user-facing preference-controlled MPIM label.
fn mpim_name(
    raw: &Value,
    users: &BTreeMap<String, Value>,
    self_id: &str,
    team: &str,
) -> Result<Option<String>, Error> {
    let peers = raw["members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| *id != self_id)
        .filter_map(|id| users.get(id))
        .collect::<Vec<_>>();
    if peers.len() < 2 {
        return match raw.get("_internal_name") {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(v)) => Ok(Some(v.clone())),
            _ => Err(unavailable()),
        };
    }
    let names = peers
        .into_iter()
        .map(|member| {
            let name = member["name"].as_str().ok_or_else(unavailable)?;
            if !truth(&member["isExternal"]) {
                return Ok(name.to_owned());
            }
            let same = member["team_id"] == team
                || member["enterprise_user"]["teams"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id == team));
            let slackbot = truth(&member["is_slackbot"]) || member["id"] == "USLACK";
            let suffix = if same || slackbot {
                team
            } else {
                member["team_id"].as_str().unwrap_or("")
            };
            Ok(format!("{name}_{suffix}"))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Some(format!("@{}", names.join(","))))
}

pub(super) async fn capture(
    state: Signal<ShellState>,
    query: &entity_search::Query,
    rows: &[Choice],
    names: &entity_names::Observation,
) -> Result<(ranking::Input, Witness), Error> {
    if !names.current(&state.peek()) {
        return Err(unavailable());
    }
    let cache = ranking_cache::capture(state, query)?;
    let ctx = &query.owner().owner.ctx;
    let assignments = query
        .read(state, api::list_experiments(&ctx.client, &ctx.session))
        .await?;
    let mut experiments = serde_json::Map::new();
    for key in [
        "autocomplete_affinity_score",
        "autocomplete_affinity_ml_model",
    ] {
        experiments.insert(
            key.into(),
            serde_json::json!(entity_members::assignment(&assignments, key)?),
        );
    }
    let naming = entity_members::assignment(&assignments, "mpdm_naming")? == Some("on");
    let external_naming = entity_members::assignment(&assignments, "sc_mpdm_naming")? == Some("on");
    let (navigation, navigation_owner) = navigation_history::capture(state, query).await?;
    let source = native(&state.peek(), query, rows)?;
    let mut members = source["users"]
        .as_object()
        .ok_or_else(unavailable)?
        .iter()
        .filter(|(_, v)| v.is_object())
        .map(|(id, v)| (id.clone(), v.clone()))
        .collect::<BTreeMap<_, _>>();
    members.insert(ctx.session.user_id.clone(), names.gantry.self_user.clone());
    for row in rows {
        if let Entity::User { user, .. } = &row.entity {
            members.insert(
                user.user().id.clone(),
                row.matching
                    .as_ref()
                    .map(|m| m.item.clone())
                    .unwrap_or_else(|| user.raw().clone()),
            );
        }
    }
    let fields = members
        .values()
        .flat_map(|m| {
            ["real_name", "display_name", "email"]
                .map(|key| m["profile"][key].as_str().unwrap_or("").to_owned())
        })
        .collect();
    let normalized = entity_normalization::capture(state, query, fields).await?;
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
        let raw = derive_member_names(&raw, |s| {
            normalized
                .name(s)
                .expect("captured member field")
                .to_owned()
        })
        .ok_or_else(unavailable)?;
        observed_members.insert(
            id.clone(),
            selectors::item(
                &raw,
                Names {
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
            .and_then(|m| m["facts"]["comparisonName"].as_str())
            .unwrap_or("");
        ims.insert(
            user.clone(),
            selectors::item(
                raw,
                Names {
                    comparison,
                    mpim: None,
                    custom: None,
                    custom_allowed: false,
                },
            )
            .ok_or_else(unavailable)?,
        );
    }
    let mut items = Vec::new();
    for row in rows {
        let item = match &row.entity {
            Entity::User { user, .. } => observed_members
                .get(&user.user().id)
                .cloned()
                .ok_or_else(unavailable)?,
            Entity::Conversation { raw, .. } => {
                let name = if raw["is_mpim"] == true {
                    mpim_name(raw, &members, &ctx.session.user_id, &ctx.session.team_id)?
                } else {
                    None
                };
                let allowed = visible_mpim_custom_name(
                    raw,
                    naming,
                    raw["is_ext_shared"] == true,
                    external_naming,
                );
                selectors::item(
                    raw,
                    Names {
                        comparison: &row.label,
                        mpim: name.as_deref(),
                        custom: raw["name"].as_str(),
                        custom_allowed: allowed,
                    },
                )
                .ok_or_else(unavailable)?
            }
        };
        items.push(item);
    }
    if names.enterprise.is_some()
        && names.gantry.default_workspace.is_none()
        && observed_members
            .values()
            .any(|m| m["enterprise_id"].as_str() == names.enterprise.as_deref())
    {
        return Err(unavailable());
    }
    let mut selectors = serde_json::json!({"selfId":ctx.session.user_id,"self":observed_members.get(&ctx.session.user_id),
        "teamId":ctx.session.team_id,"enterpriseId":names.enterprise,"enterprise":names.enterprise.is_some(),
        "searcherTeams":names.policy.workspace_ids().collect::<Vec<_>>(),"defaultTeam":names.gantry.default_workspace,
        "members":observed_members,"ims":ims,"metadata":source["metadata"],"navigationHistory":navigation,
        "experiments":experiments,"offline":{"channel":{},"mpim":{}},"entityCtr":{},"remoteDm":{},"remoteMpim":{},"affinity":{},"channelPriority":{}});
    for kind in [
        OfflineEntity::Member,
        OfflineEntity::Channel,
        OfflineEntity::Mpim,
        OfflineEntity::Im,
    ] {
        selectors["entityCtr"][kind.key()] = serde_json::json!(cache.offline.entity_ctr(kind));
    }
    let all_ids = members
        .keys()
        .cloned()
        .chain(
            items
                .iter()
                .filter_map(|v| v["id"].as_str())
                .map(str::to_owned),
        )
        .collect::<BTreeSet<_>>();
    for id in all_ids {
        for kind in [OfflineEntity::Channel, OfflineEntity::Mpim] {
            if let Some(f) = cache.offline.features(&id, kind) {
                selectors["offline"][kind.key()][&id] = serde_json::json!({"team_ctr_90dd":f.team_ctr_90dd,"user_ctr_30dd":f.user_ctr_30dd});
            }
        }
        if let Some(v) = cache.offline.dm(&id) {
            selectors["remoteDm"][&id] = v.clone();
        }
        if let Some(v) = cache.offline.mpdm(&id) {
            selectors["remoteMpim"][&id] = v.clone();
        }
        selectors["affinity"][&id] = serde_json::json!(cache.offline.affinity(&id));
        selectors["channelPriority"][&id] = serde_json::json!(cache.offline.priority(&id));
    }
    let witness = Witness {
        names: names.clone(),
        navigation: navigation_owner,
        native: source,
    };
    if !witness.current(&state.peek(), query, rows) || !cache.current(&state.peek()) {
        return Err(unavailable());
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| unavailable())?
        .as_secs_f64()
        * 1000.0;
    Ok((
        ranking::Input {
            cache,
            items,
            selectors,
            history: names.history.clone(),
            now_ms,
        },
        witness,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn internal_mpim_names_preserve_raw_peer_names_and_exact_external_suffix() {
        let peers = BTreeMap::from([
            (
                "U1".into(),
                json!({"id":"U1","name":"raw_one","isExternal":true,"team_id":"T_OTHER","profile":{"display_name":"Unrelated"}}),
            ),
            (
                "U2".into(),
                json!({"id":"U2","name":"raw_two","isExternal":true,"team_id":"T_OTHER","enterprise_user":{"teams":["T_CURRENT"]}}),
            ),
        ]);
        assert_eq!(
            mpim_name(
                &json!({"members":["SELF","U1","U2"]}),
                &peers,
                "SELF",
                "T_CURRENT"
            )
            .unwrap()
            .as_deref(),
            Some("@raw_one_T_OTHER,raw_two_T_CURRENT")
        );
        assert_eq!(
            mpim_name(
                &json!({"members":["SELF","U1"],"_internal_name":"retained"}),
                &peers,
                "SELF",
                "T_CURRENT"
            )
            .unwrap()
            .as_deref(),
            Some("retained")
        );
        assert!(
            mpim_name(&json!({"members":[]}), &peers, "SELF", "T_CURRENT")
                .unwrap()
                .is_none()
        );
    }
}

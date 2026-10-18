//! Actual local D/G identities, matched through the retained hHTo port. Peer
//! records supply names, not member-picker permissions or synthetic DM IDs.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::block_actions::home::workflow::directory_local::{
    MemberSubqueryScore, Options, Scored, derive_member_names, filter_ims, filter_mpims,
    mpim_subquery_inputs, visible_mpim_custom_name,
};

#[derive(Clone)]
pub(super) struct Witness {
    origin: Origin,
    channel: Value,
    peers: BTreeMap<String, Option<Value>>,
}
impl Witness {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.origin.current(s)
            && s.core
                .workspaces
                .get(&self.origin.owner.ctx.session.team_id)
                .is_some_and(|workspace| {
                    let id = self.channel["id"].as_str().unwrap_or("");
                    workspace
                        .channels
                        .get(id)
                        .and_then(|c| serde_json::to_value(c).ok())
                        .as_ref()
                        == Some(&self.channel)
                        && self.peers.iter().all(|(id, raw)| {
                            workspace
                                .users
                                .get(id)
                                .and_then(|u| serde_json::to_value(u).ok())
                                == *raw
                        })
                })
    }
}
pub(super) struct Candidate {
    pub(super) raw: Value,
    pub(super) label: String,
    pub(super) matching: entity_members::MatchInfo,
    pub(super) mpim: Option<MpimMetadata>,
    pub(super) witness: Witness,
}
#[derive(Clone)]
pub(super) struct MpimMetadata {
    pub(super) score_details_by_member: Option<BTreeMap<String, BTreeMap<&'static str, u32>>>,
    pub(super) subquery_scores_by_member: Option<Vec<Vec<MemberSubqueryScore>>>,
}

pub(super) async fn load(
    state: Signal<ShellState>,
    query: &entity_search::Query,
    scope: &entity_external::Scope,
    names: &entity_names::Observation,
) -> Result<Vec<Candidate>, Error> {
    if query.members() {
        return Ok(Vec::new());
    }
    let ctx = &query.owner().owner.ctx;
    let (channels, originals) = {
        let s = state.peek();
        if !query.current(&s) || !scope.current(&s) || !names.current(&s) {
            return Err(unavailable());
        }
        let workspace = s
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .ok_or_else(unavailable)?;
        let channels = workspace
            .channels
            .values()
            .filter(|c| c.is_im || c.is_mpim)
            .map(|c| serde_json::to_value(c).map_err(|_| unavailable()))
            .collect::<Result<Vec<_>, _>>()?;
        let needed: BTreeSet<_> = channels
            .iter()
            .flat_map(|raw| {
                raw.get("user").and_then(Value::as_str).into_iter().chain(
                    raw.get("members")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str),
                )
            })
            .collect();
        let peers = workspace
            .users
            .iter()
            .filter(|(id, _)| needed.contains(id.as_str()))
            .map(|(id, u)| {
                serde_json::to_value(u)
                    .map(|raw| (id.clone(), raw))
                    .map_err(|_| unavailable())
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        (channels, peers)
    };
    if channels.is_empty() {
        return Ok(Vec::new());
    }
    // EmXR acquires all raw IM/MPIM records, including closed conversations.
    // hXqI does not request onlyVisibleIms/onlyVisibleMpims.
    let effective: BTreeMap<_, _> = originals
        .iter()
        .map(|(id, raw)| {
            (
                id.clone(),
                entity_members::effective(raw, &ctx.session.user_id, false, None),
            )
        })
        .collect();
    let name_fields = effective
        .values()
        .flat_map(|raw| {
            ["real_name", "display_name", "email"]
                .map(|k| raw["profile"][k].as_str().unwrap_or("").to_owned())
        })
        .collect();
    let casing = entity_normalization::capture(state, query, name_fields).await?;
    let peers = effective
        .iter()
        .map(|(id, raw)| {
            for key in ["real_name", "display_name", "email"] {
                if casing
                    .name(raw["profile"][key].as_str().unwrap_or(""))
                    .is_none()
                {
                    return Err(unavailable());
                }
            }
            derive_member_names(raw, |name| {
                casing.name(name).expect("captured names").to_owned()
            })
            .map(|raw| (id.clone(), raw))
            .ok_or_else(unavailable)
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut options = Options::workflow(
        &query.text,
        &casing.query.normalized,
        &casing.query.matcher_normalized,
        matches!(casing.query.normalized.as_str(), "me" | "you"),
    );
    options.allow_empty_query = true;
    options.email_forward_only = true;
    // No remote users request occurs for this hXqI channel-only picker, hence
    // the actual remote MEMBER passthrough set is empty even for IM peer U IDs.
    let remote = BTreeSet::new();
    let ims: Vec<_> = channels
        .iter()
        .filter(|raw| raw["is_im"] == true)
        .cloned()
        .collect();
    let groups: Vec<_> = channels
        .iter()
        .filter(|raw| raw["is_mpim"] == true)
        .cloned()
        .collect();
    let mut result = Vec::new();
    for matched in filter_ims(&ims, &peers, &options, &remote, |raw| scope.allows(raw)) {
        let peer = matched.item["user"].as_str().and_then(|id| peers.get(id));
        let label = peer
            .map(|peer| names.policy.member_name(peer, "Name hidden"))
            .transpose()
            .map_err(|_| unavailable())?
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| matched.id.to_owned());
        result.push(candidate(query, matched, label, None, &originals));
    }
    if groups.is_empty() {
        let s = state.peek();
        if !query.current(&s) || !scope.current(&s) || !names.current(&s) {
            return Err(unavailable());
        }
        result.retain(|candidate| candidate.witness.current(&s));
        return Ok(result);
    }
    let (naming, external_naming) = if groups
        .iter()
        .any(|raw| raw.pointer("/properties/has_custom_mpdm_name") == Some(&Value::Bool(true)))
    {
        let assignments = query
            .read(state, api::list_experiments(&ctx.client, &ctx.session))
            .await?;
        (
            entity_members::assignment(&assignments, "mpdm_naming")? == Some("on"),
            entity_members::assignment(&assignments, "sc_mpdm_naming")? == Some("on"),
        )
    } else {
        (false, false)
    };
    let pieces = mpim_subquery_inputs(&casing.query.normalized);
    let mut fields: BTreeSet<_> = pieces.iter().cloned().collect();
    for raw in &groups {
        if visible_mpim_custom_name(raw, naming, raw["is_ext_shared"] == true, external_naming) {
            fields.insert(raw["name"].as_str().unwrap_or("").to_owned());
        }
    }
    let fields = entity_normalization::fields(state, query, &casing, fields).await?;
    let subqueries = pieces
        .iter()
        .map(|p| fields.get(p).cloned().ok_or_else(unavailable))
        .collect::<Result<Vec<_>, _>>()?;
    let named = |raw: &Value| {
        visible_mpim_custom_name(raw, naming, raw["is_ext_shared"] == true, external_naming)
            .then(|| fields.get(raw["name"].as_str().unwrap_or("")).cloned())
            .flatten()
    };
    for matched in filter_mpims(
        &groups,
        &peers,
        &options,
        &subqueries,
        &remote,
        |raw| scope.allows(raw),
        named,
    ) {
        let raw = matched.result.item;
        let label =
            if visible_mpim_custom_name(raw, naming, raw["is_ext_shared"] == true, external_naming)
            {
                raw["name"].as_str().unwrap_or("").to_owned()
            } else {
                raw["members"]
                    .as_array()
                    .map(|ids| {
                        ids.iter()
                            .filter_map(Value::as_str)
                            .filter(|id| *id != ctx.session.user_id)
                            .filter_map(|id| peers.get(id))
                            .map(|member| names.policy.member_name(member, "Name hidden"))
                            .collect::<Result<Vec<_>, _>>()
                            .map(|values| values.join(", "))
                    })
                    .transpose()
                    .map_err(|_| unavailable())?
                    .unwrap_or_default()
            };
        let metadata = MpimMetadata {
            score_details_by_member: matched.score_details_by_member,
            subquery_scores_by_member: matched.subquery_scores_by_member,
        };
        result.push(candidate(
            query,
            matched.result,
            label,
            Some(metadata),
            &originals,
        ));
    }
    let s = state.peek();
    if !query.current(&s) || !scope.current(&s) || !names.current(&s) {
        return Err(unavailable());
    }
    result.retain(|candidate| candidate.witness.current(&s));
    Ok(result)
}

fn candidate(
    query: &entity_search::Query,
    value: Scored<'_>,
    label: String,
    mpim: Option<MpimMetadata>,
    users: &BTreeMap<String, Value>,
) -> Candidate {
    let mut ids = Vec::new();
    if let Some(id) = value.item["user"].as_str() {
        ids.push(id);
    }
    if let Some(members) = value.item["members"].as_array() {
        ids.extend(members.iter().filter_map(Value::as_str));
    }
    let witness = Witness {
        origin: query.owner().clone(),
        channel: value.item.clone(),
        peers: ids
            .into_iter()
            .filter_map(|id| Some((id.to_owned(), users.get(id).cloned())))
            .collect(),
    };
    Candidate {
        raw: value.item.clone(),
        label,
        mpim,
        witness,
        matching: entity_members::MatchInfo {
            item: value.item.clone(),
            score: Some(value.score),
            score_details: value.score_details,
            is_exact_match: value.is_exact_match,
            exact_match_details: value.exact_match_details,
            is_self: value.is_self,
            is_flannel_fuzzy_passthrough: value.is_flannel_fuzzy_passthrough,
            remote: false,
        },
    }
}
fn display_name(raw: &Value) -> String {
    [
        raw.pointer("/profile/display_name"),
        raw.pointer("/profile/real_name"),
        raw.get("name"),
        raw.get("id"),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .find(|s| !s.is_empty())
    .unwrap_or("")
    .to_owned()
}
fn unavailable() -> Error {
    Error::Api("home_workflow_local_conversations_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (ShellState, Origin) {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        crate::app_home::fixture::install(&mut s);
        let h = s.app_home.surface.as_mut().unwrap();
        let home = h.home.clone().unwrap();
        let mut raw = h.view.as_ref().unwrap().raw().clone();
        raw["blocks"] = json!([{"type":"actions","block_id":"wf","elements":[{
            "type":"workflow_button","action_id":"run","text":{"type":"plain_text","text":"Run"},
            "workflow":{"trigger":{"url":"https://slack.com/shortcuts/Ft12345678"}}
        }]}]);
        h.view = Some(View::decode(&home, raw).unwrap());
        inputs::hydrate(&mut s);
        let h = s.app_home.surface.as_ref().unwrap();
        let origin = Origin {
            owner: h.owner.clone(),
            key: Key {
                visit: h.id,
                revision: h.revision,
            },
            workflow: Workflow::capture(
                h.view.as_ref().unwrap(),
                "wf",
                "run",
                &h.owner.ctx.session.user_id,
            )
            .unwrap(),
            input_revision: inputs::revision(&s),
        };
        assert!(origin.current(&s));
        (s, origin)
    }
    #[test]
    fn previously_missing_mpim_peer_arrival_retires_original_match_witness() {
        let (mut s, origin) = fixture();
        let team = origin.owner.ctx.session.team_id.clone();
        let channel: super_platinum_core::slack::models::Channel =
            serde_json::from_value(json!({"id":"GLOCAL","is_mpim":true,"members":["UNEW"]}))
                .unwrap();
        let raw = serde_json::to_value(&channel).unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .insert(channel.id.clone(), channel);
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .users
            .remove("UNEW");
        let witness = Witness {
            origin,
            channel: raw,
            peers: BTreeMap::from([("UNEW".into(), None)]),
        };
        assert!(witness.current(&s));
        let peer =
            serde_json::from_value(json!({"id":"UNEW","profile":{"display_name":"New peer"}}))
                .unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .users
            .insert("UNEW".into(), peer);
        assert!(!witness.current(&s));
        assert_eq!(witness.channel["id"], "GLOCAL");
    }
    #[test]
    fn dm_peer_replacement_retires_d_identity_but_unrelated_user_does_not() {
        let (mut s, origin) = fixture();
        let team = origin.owner.ctx.session.team_id.clone();
        let channel: super_platinum_core::slack::models::Channel =
            serde_json::from_value(json!({"id":"DLOCAL","is_im":true,"user":"UPEER"})).unwrap();
        let raw = serde_json::to_value(&channel).unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .insert(channel.id.clone(), channel);
        let peer = serde_json::from_value(json!({"id":"UPEER","profile":{"display_name":"Peer"}}))
            .unwrap();
        let peer_raw = serde_json::to_value(&peer).unwrap();
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .users
            .insert("UPEER".into(), peer);
        let witness = Witness {
            origin,
            channel: raw,
            peers: BTreeMap::from([("UPEER".into(), Some(peer_raw))]),
        };
        assert!(witness.current(&s));
        s.core.workspaces.get_mut(&team).unwrap().users.insert(
            "UOTHER".into(),
            serde_json::from_value(json!({"id":"UOTHER"})).unwrap(),
        );
        assert!(witness.current(&s));
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .channels
            .get_mut("DLOCAL")
            .unwrap()
            .user = Some("UREPLACEMENT".into());
        assert!(!witness.current(&s));
        assert_eq!(witness.channel["id"], "DLOCAL");
    }
}

//! Local and returned channel matching before posting admission and ranking.
//! Keep actual display/event observations and prior-name match evidence.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::block_actions::home::workflow::{
    ConversationKind, Directory, DirectoryConversation, DirectoryResults,
    directory_local::{ChannelNameInput, Options, channel_name_input, filter_channels},
};
#[derive(Clone)]
pub(super) struct Witness {
    origin: Origin,
    id: String,
    stamp: Option<Value>,
    meeting: Option<meeting_names::Witness>,
}
impl Witness {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.origin.current(s)
            && stamp(s, &self.origin, &self.id) == self.stamp
            && self.meeting.as_ref().is_none_or(|m| m.current(s))
    }
}
#[derive(Clone)]
pub(super) struct Previous {
    pub(super) scores: Option<Vec<(String, Option<u32>)>>,
    pub(super) best: Option<String>,
}
pub(super) struct Candidate {
    pub(super) raw: Value,
    pub(super) label: String,
    pub(super) matching: entity_members::MatchInfo,
    pub(super) previous: Previous,
    pub(super) witness: Witness,
}
fn stamp(s: &ShellState, origin: &Origin, id: &str) -> Option<Value> {
    s.core
        .workspaces
        .get(&origin.owner.ctx.session.team_id)
        .and_then(|w| w.channels.get(id))
        .and_then(|c| serde_json::to_value(c).ok())
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn private(v: &Value) -> bool {
    (truth(&v["is_private"]) || truth(&v["is_group"])) && !truth(&v["is_mpim"])
}
fn public(v: &Value) -> bool {
    truth(&v["is_channel"]) && !private(v) && !truth(&v["is_mpim"])
}
fn local(v: &Value) -> bool {
    if truth(&v["fromAnotherTeam"]) {
        return false;
    }
    let record = v
        .pointer("/properties/record_channel/record_id")
        .and_then(Value::as_str);
    let salesforce = record.is_some_and(|id| {
        let parts: Vec<_> = id.split(':').collect();
        parts.len() == 3 && parts[0] == "S"
    });
    public(v) || (private(v) && (!salesforce || truth(&v["is_member"])))
}
pub(super) async fn load(
    state: Signal<ShellState>,
    query: &entity_search::Query,
    pages: &[DirectoryResults],
) -> Result<Vec<Candidate>, Error> {
    if query.members() {
        return Ok(Vec::new());
    }
    let (mut raw, stamps) = {
        let s = state.peek();
        if !query.current(&s) {
            return Err(unavailable());
        }
        let w = s
            .core
            .workspaces
            .get(&query.owner().owner.ctx.session.team_id)
            .ok_or_else(unavailable)?;
        let all = w
            .channels
            .values()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| unavailable())?;
        let ordered = all
            .iter()
            .filter(|v| public(v) && local(v))
            .chain(all.iter().filter(|v| private(v) && local(v)))
            .cloned()
            .collect::<Vec<_>>();
        let stamps = all
            .into_iter()
            .filter_map(|v| v["id"].as_str().map(str::to_owned).map(|id| (id, v)))
            .collect::<BTreeMap<_, _>>();
        (ordered, stamps)
    };
    let mut remote = BTreeSet::new();
    for page in pages.iter().filter(|p| p.directory() != Directory::Users) {
        for index in 0..page.rows().len() {
            let Ok(value) = page.conversation(index, None) else {
                continue;
            };
            let Ok(channel) = DirectoryConversation::decode(value.clone()) else {
                continue;
            };
            if channel.kind() != ConversationKind::Channel {
                continue;
            }
            remote.insert(channel.id().to_owned());
            if let Some(old) = raw.iter_mut().find(|r| r["id"] == value["id"]) {
                *old = value;
            } else {
                raw.push(value);
            }
        }
    }
    raw.retain(|r| channel_name_input(r).is_some());
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let ctx = &query.owner().owner.ctx;
    let assignments = query
        .read(state, api::list_experiments(&ctx.client, &ctx.session))
        .await?;
    let passthrough =
        entity_members::assignment(&assignments, "autocomplete_flannel_fuzzy_match_passthrough")?
            == Some("treatment");
    let links = raw
        .iter()
        .filter_map(|r| match channel_name_input(r) {
            Some(ChannelNameInput::Meeting {
                event_link_id: Some(link),
                ..
            }) => Some(link.to_owned()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let (events, meeting) = if links.is_empty() {
        (BTreeMap::new(), None)
    } else {
        let (events, witness) = meeting_names::resolve(state, query, links).await?;
        let formatted = meeting_names::format(state, query, &events, &witness).await?;
        (formatted, Some(witness))
    };
    let names = names(state, query, &raw, &events).await?;
    let casing = entity_normalization::capture(state, query, BTreeSet::new()).await?;
    let mut fields = names.values().cloned().collect::<BTreeSet<_>>();
    for r in &raw {
        fields.extend(
            r["previous_names"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
    }
    let fields = entity_normalization::fields(state, query, &casing, fields).await?;
    let mut options = Options::workflow(
        &query.text,
        &casing.query.normalized,
        &casing.query.matcher_normalized,
        false,
    );
    options.allow_empty_query = true;
    options.passthrough_remote_fuzzy_filtering = passthrough;
    let mut result = Vec::new();
    for m in filter_channels(
        &raw,
        &options,
        true,
        &remote,
        |_| true,
        |r| fields[&names[r["id"].as_str().unwrap()]].clone(),
        |name| fields[name].clone(),
    ) {
        let witness = Witness {
            origin: query.owner().clone(),
            id: m.id.into(),
            stamp: stamps.get(m.id).cloned(),
            meeting: matches!(
                channel_name_input(m.item),
                Some(ChannelNameInput::Meeting { .. })
            )
            .then(|| meeting.clone())
            .flatten(),
        };
        result.push(Candidate {
            raw: m.item.clone(),
            label: names[m.id].clone(),
            matching: entity_members::MatchInfo {
                item: m.item.clone(),
                score: m.score,
                score_details: None,
                is_exact_match: m.is_exact_match,
                exact_match_details: None,
                is_self: false,
                is_flannel_fuzzy_passthrough: m.is_flannel_fuzzy_passthrough,
                remote: remote.contains(m.id),
            },
            previous: Previous {
                scores: m
                    .previous_name_scores
                    .map(|v| v.into_iter().map(|p| (p.name.into(), p.score)).collect()),
                best: m.best_previous_name_match.map(str::to_owned),
            },
            witness,
        });
    }
    let s = state.peek();
    if !query.current(&s) || result.iter().any(|r| !r.witness.current(&s)) {
        return Err(unavailable());
    }
    Ok(result)
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    generation: String,
    names: BTreeMap<String, String>,
}
async fn names(
    state: Signal<ShellState>,
    query: &entity_search::Query,
    raw: &[Value],
    events: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, Error> {
    let generation = query.generation.to_string();
    let inputs=raw.iter().map(|r|{
        let id=r["id"].as_str().ok_or_else(unavailable)?;
        let input=match channel_name_input(r).ok_or_else(unavailable)? {
            ChannelNameInput::Ordinary{raw_name}=>serde_json::json!({"kind":"ordinary","raw":raw_name}),
            ChannelNameInput::Record{raw_name_or_ellipsis}=>serde_json::json!({"kind":"record","raw":raw_name_or_ellipsis}),
            ChannelNameInput::Meeting{raw_name,event_link_id}=>serde_json::json!({"kind":"meeting","raw":raw_name,"known":event_link_id.and_then(|id|events.get(id))}),
        };Ok((id.to_owned(),input))
    }).collect::<Result<BTreeMap<_,_>,Error>>()?;
    let request = serde_json::json!({"generation":generation,"inputs":inputs});
    let script = format!(
        "{}\ndioxus.send(workflowChannelNames({}));await dioxus.recv();",
        include_str!("entity_channels.js"),
        request
    );
    if !query.current(&state.peek()) {
        return Err(unavailable());
    }
    let mut eval = dioxus::document::eval(&script);
    let response = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Option<Reply>>()).await;
    let _ = eval.send(());
    if !query.current(&state.peek()) {
        return Err(unavailable());
    }
    let response = response
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
    if response.generation != generation || response.names.keys().ne(inputs.keys()) {
        return Err(unavailable());
    }
    Ok(response.names)
}
fn unavailable() -> Error {
    Error::Api("home_workflow_channel_names_unavailable".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn store_filters_use_source_raw_other_team_and_exact_salesforce_shape() {
        assert!(local(
            &serde_json::json!({"is_channel":true,"team_id":"OTHER"})
        ));
        assert!(!local(
            &serde_json::json!({"is_channel":true,"fromAnotherTeam":true})
        ));
        assert!(!local(
            &serde_json::json!({"is_private":true,"properties":{"record_channel":{"record_id":"S:x:y"}}})
        ));
        assert!(local(
            &serde_json::json!({"is_private":true,"properties":{"record_channel":{"record_id":"AC:x:y"}}})
        ));
        assert!(!local(&serde_json::json!({"is_mpim":true,"is_group":true})));
    }
}

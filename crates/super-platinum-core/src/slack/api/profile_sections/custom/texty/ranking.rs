//! Channel-only Texty policies over shared 4ucY/ukdy channel math.
//! Ejs0 and installed l24D channel features agree except the explicit missing
//! offline-channel fallback below. No member/file/MPIM feature graph is imported.
use super::matching::{Matched, ordinary_list_channel, ordinary_local};
use crate::{
    block_actions::home::workflow::{directory_ranking as shared, directory_selectors},
    composer_emoji::{history::History, model::Model, ranking::SimilarityCache},
};
use serde_json::{Value, json};
pub use shared::{Error, Normalize, Ranked};
use std::cmp::Ordering;
#[cfg(test)]
mod tests;
/// Complete caller-owned observations, never wire-provided ranking facts.
/// metadata contains one explicit record per candidate, with latest/lastRead/
/// draft/starred/unread/priority keys (null records source absence). navigation
/// and experiment maps represent complete observed collections; absent entries
/// within those maps retain source defaults. The caller owns freshness/custody.
pub struct Context<'a> {
    pub state: &'a Value,
    pub history: &'a History,
    pub model: &'a Model,
    pub now_ms: f64,
    pub normalize: &'a Normalize<'a>,
}
pub struct Observation {
    pub rows: Vec<Ranked>,
    pub latest_ids: Vec<String>,
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn observed_state(
    context: &Context<'_>,
    matched: &[Matched<'_>],
    eligible: fn(&Value) -> bool,
) -> Result<Value, Error> {
    let state = context.state;
    if !context.now_ms.is_finite() {
        return Err(Error::InvalidClock);
    }
    if state["selfId"].as_str().is_none_or(str::is_empty)
        || !state["metadata"].is_object()
        || !state["navigationHistory"].is_object()
        || !state["experiments"].is_object()
        || state.get("enterprise").is_none()
        || state.get("teamId").is_none()
        || state.get("offline").is_none()
    {
        return Err(Error::MissingObservation(
            "Profile channel selector state".into(),
        ));
    }
    let mut result = state.clone();
    let offline = truth(&state["teamId"]) && truth(&state["offline"]);
    if offline
        && (!state["offline"]["channel"].is_object()
            || state["entityCtr"]["channel"]
                .as_f64()
                .is_none_or(|v| !v.is_finite())
            || !state["channelPriority"].is_object())
    {
        return Err(Error::MissingObservation(
            "Profile channel offline observations".into(),
        ));
    }
    for row in matched {
        if !eligible(row.raw()) {
            return Err(Error::InvalidInput);
        }
        let id = row.id();
        let md = &state["metadata"][id];
        if !md.is_object()
            || [
                "latest", "lastRead", "draft", "starred", "unread", "priority",
            ]
            .iter()
            .any(|key| md.get(*key).is_none())
        {
            return Err(Error::MissingObservation(format!("channel metadata {id}")));
        }
        if offline && state["offline"]["channel"].get(id).is_none() {
            // Exact Ejs0 fallback; do not substitute Offline::default_features,
            // whose team CTR default is intentionally different.
            result["offline"]["channel"][id] =
                json!({"user_ctr_30dd":state["entityCtr"]["channel"],"team_ctr_90dd":0});
        }
    }
    Ok(result)
}
/// Exact Texty initial75/model/final25 path. Returned original_index refers to
/// `matched`, not a DOM position; caller must keep the private choice binding.
pub fn rank_channels(
    query: &str,
    matched: &[Matched<'_>],
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    collate: impl Fn(&str, &str) -> Result<Ordering, Error>,
    demote_passthrough_observed: bool,
) -> Result<Observation, Error> {
    rank_channels_for(
        query,
        matched,
        context,
        cache,
        collate,
        demote_passthrough_observed,
        ordinary_local,
    )
}
/// Same Am1o Texty 75/model/25 policy for interactive ordinary List channels,
/// including canonically observed another-team rows. This does not admit
/// special channel names or weaken required ranking/renderer observations.
pub fn rank_list_channels(
    query: &str,
    matched: &[Matched<'_>],
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    collate: impl Fn(&str, &str) -> Result<Ordering, Error>,
    demote_passthrough_observed: bool,
) -> Result<Observation, Error> {
    rank_channels_for(
        query,
        matched,
        context,
        cache,
        collate,
        demote_passthrough_observed,
        ordinary_list_channel,
    )
}
fn rank_channels_for(
    query: &str,
    matched: &[Matched<'_>],
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    collate: impl Fn(&str, &str) -> Result<Ordering, Error>,
    demote_passthrough_observed: bool,
    eligible: fn(&Value) -> bool,
) -> Result<Observation, Error> {
    let state = observed_state(context, matched, eligible)?;
    let mut rows = Vec::new();
    for matched in matched {
        let scored = &matched.scored;
        let item = directory_selectors::item(
            scored.item,
            directory_selectors::Names {
                comparison: &matched.comparison,
                mpim: None,
                custom: None,
                custom_allowed: false,
            },
        )
        .ok_or(Error::InvalidInput)?;
        // Selection authority remains in the desktop catalog. Raw wire facts
        // are discarded by the shared selector before any bonus/model access.
        let mut row = json!({"id":scored.id,"item":item,"score":scored.score});
        if let Some(exact) = scored.is_exact_match {
            row["isExactMatch"] = json!(exact);
        }
        if let Some(previous) = &scored.previous_name_scores {
            row["previousNameScores"] = json!(
                previous
                    .iter()
                    .map(|p| json!({"name":p.name,"score":p.score}))
                    .collect::<Vec<_>>()
            );
        }
        if let Some(previous) = scored.best_previous_name_match {
            row["bestPreviousNameMatch"] = json!(previous);
        }
        if scored.is_flannel_fuzzy_passthrough {
            row["isFlannelFuzzyPassthrough"] = json!(true);
        }
        rows.push(row);
    }
    let options = shared::Options {
        limit: 25,
        source: "texty-autocomplete".into(),
        demote_passthrough: demote_passthrough_observed,
        ..Default::default()
    };
    let shared_context = shared::Context {
        state: &state,
        history: context.history,
        model: context.model,
        now_ms: context.now_ms,
        normalize: context.normalize,
    };
    let failure = std::cell::RefCell::new(None);
    let result = shared::rank_observed(
        query,
        &json!({"channels":rows}),
        &options,
        &shared_context,
        cache,
        |a, b| {
            collate(a, b).unwrap_or_else(|e| {
                *failure.borrow_mut() = Some(e);
                Ordering::Equal
            })
        },
    )?;
    if let Some(error) = failure.into_inner() {
        return Err(error);
    }
    Ok(Observation {
        rows: result.rows,
        latest_ids: result.channel_latest_ids,
    })
}

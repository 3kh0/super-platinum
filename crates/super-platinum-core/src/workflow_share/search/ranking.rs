//! Share's ordinary-member/joined-channel matcher and mixed ordering adapter.
//! Ranking never grants destination eligibility or replaces original custody.
use super::{Kind, LIMIT};
use crate::{
    block_actions::home::workflow::{
        directory_local as local, directory_ranking as shared, directory_selectors as selectors,
    },
    composer_emoji::{history::History, model::Model, ranking::SimilarityCache},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
mod observations;
mod state;
#[cfg(test)]
mod tests;
pub use observations::{ObservationPlan, RankObservations, RendererReply};
pub use shared::Error;
pub const SOURCE: &str = "share_dialog_channel_picker";
pub const INITIAL_LIMIT: usize = LIMIT * 3;

/// Actual query-local experiment/name observations; no Profile defaults.
pub struct MatchFacts {
    pub query_is_maybe_self: bool,
    pub can_match_beyond_word_boundaries: bool,
    pub passthrough_remote_fuzzy_filtering: bool,
}
fn options<'a>(
    query: &'a str,
    prepared: &'a local::PreparedQuery,
    facts: &MatchFacts,
) -> local::Options<'a> {
    let mut o = local::Options::workflow(
        query,
        &prepared.normalized,
        &prepared.matcher_normalized,
        facts.query_is_maybe_self,
    );
    o.allow_empty_query = true;
    o.match_email = true;
    o.email_forward_only = true;
    o.passthrough_remote_fuzzy_filtering = facts.passthrough_remote_fuzzy_filtering;
    o.fuzzy.can_match_beyond_word_boundaries = facts.can_match_beyond_word_boundaries;
    o
}
pub fn match_members<'a>(
    query: &str,
    prepared: &local::PreparedQuery,
    members: &'a [Value],
    remote: &BTreeSet<String>,
    facts: &MatchFacts,
    eligible: impl Fn(&Value) -> bool,
) -> Vec<local::Scored<'a>> {
    local::filter_members(members, &options(query, prepared, facts), remote, eligible)
}
pub fn match_channels<'a>(
    query: &str,
    prepared: &local::PreparedQuery,
    channels: &'a [Value],
    remote: &BTreeSet<String>,
    facts: &MatchFacts,
    eligible: impl Fn(&Value) -> bool,
    display_name: impl Fn(&Value) -> String,
    previous_name: impl Fn(&str) -> String,
) -> Vec<local::ChannelScored<'a>> {
    local::filter_channels(
        channels,
        &options(query, prepared, facts),
        false,
        remote,
        eligible,
        display_name,
        previous_name,
    )
}
/// Explicit immutable selector/model/history observations from the original owner.
/// An observed absent selector is null; a missing required key is not an absence.
pub struct RankInputs<'a> {
    pub state: &'a Value,
    pub history: &'a History,
    pub model: &'a Model,
    pub now_ms: f64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub kind: Kind,
    /// Position in the original matched family, before any mixed sorting/cutoff.
    pub index: usize,
    pub id: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Rows {
    groups: Value,
    identities: Vec<Identity>,
}
impl Rows {
    pub fn new(
        members: &[local::Scored<'_>],
        member_names: &[String],
        channels: &[local::ChannelScored<'_>],
        channel_names: &[String],
    ) -> Result<Self, Error> {
        if members.len() != member_names.len() || channels.len() != channel_names.len() {
            return Err(Error::InvalidInput);
        }
        let mut identities = Vec::new();
        let mut seen = BTreeSet::new();
        let mut member_rows = Vec::new();
        let mut channel_rows = Vec::new();
        for (index, (m, name)) in members.iter().zip(member_names).enumerate() {
            let item = project(m.item, name)?;
            if item["facts"]["member"] != true
                || !item["facts"]["channelType"].is_null()
                || item["id"] != m.id
                || !seen.insert(m.id.to_owned())
            {
                return Err(Error::InvalidInput);
            }
            let mut row = json!({"id":m.id,"item":item,"score":m.score,"isSelf":m.is_self,"isFlannelFuzzyPassthrough":m.is_flannel_fuzzy_passthrough});
            if let Some(v) = &m.score_details {
                row["scoreDetails"] = json!(v)
            }
            if let Some(v) = m.is_exact_match {
                row["isExactMatch"] = json!(v)
            }
            if let Some(v) = &m.exact_match_details {
                row["exactMatchDetails"] = json!(v)
            }
            member_rows.push(row);
            identities.push(Identity {
                kind: Kind::Members,
                index,
                id: m.id.into(),
            });
        }
        for (index, (c, name)) in channels.iter().zip(channel_names).enumerate() {
            let item = project(c.item, name)?;
            if !matches!(
                item["facts"]["channelType"].as_str(),
                Some("channel" | "private")
            ) || item["facts"]["channelMember"] != true
                || item["id"] != c.id
                || !seen.insert(c.id.to_owned())
            {
                return Err(Error::InvalidInput);
            }
            let mut row = json!({"id":c.id,"item":item,"score":c.score,"isFlannelFuzzyPassthrough":c.is_flannel_fuzzy_passthrough});
            if let Some(v) = c.is_exact_match {
                row["isExactMatch"] = json!(v)
            }
            if let Some(v) = &c.previous_name_scores {
                row["previousNameScores"] = json!(
                    v.iter()
                        .map(|p| json!({"name":p.name,"score":p.score}))
                        .collect::<Vec<_>>()
                )
            }
            if let Some(v) = c.best_previous_name_match {
                row["bestPreviousNameMatch"] = json!(v)
            }
            channel_rows.push(row);
            identities.push(Identity {
                kind: Kind::Channels,
                index,
                id: c.id.into(),
            });
        }
        Ok(Self {
            groups: json!({"members":member_rows,"channels":channel_rows}),
            identities,
        })
    }
    pub fn groups(&self) -> &Value {
        &self.groups
    }
    pub fn identities(&self) -> &[Identity] {
        &self.identities
    }
    fn values(&self) -> impl Iterator<Item = &Value> {
        self.groups["members"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(self.groups["channels"].as_array().into_iter().flatten())
    }
}
fn project(raw: &Value, name: &str) -> Result<Value, Error> {
    selectors::item(
        raw,
        selectors::Names {
            comparison: name,
            mpim: None,
            custom: None,
            custom_allowed: false,
        },
    )
    .ok_or(Error::InvalidInput)
}
pub struct Ranked {
    pub identity: Identity,
    pub row: Value,
    pub features: Vec<(&'static str, f64)>,
    pub model_score: f64,
}
pub struct ResultSet {
    pub rows: Vec<Ranked>,
    pub channel_latest_ids: Vec<String>,
}
pub fn rank(
    query: &str,
    rows: &Rows,
    inputs: &RankInputs<'_>,
    observations: &RankObservations,
    cache: &mut SimilarityCache,
) -> Result<ResultSet, Error> {
    observations.validate(query, rows)?;
    state::validate(inputs.state, rows)?;
    let normalize = |s: &str| observations.normalize(s);
    let context = shared::Context {
        state: inputs.state,
        history: inputs.history,
        model: inputs.model,
        now_ms: inputs.now_ms,
        normalize: &normalize,
    };
    let options = shared::Options {
        limit: LIMIT,
        source: SOURCE.into(),
        demote_passthrough: observations.demote_passthrough(),
        ..Default::default()
    };
    let missing = std::cell::Cell::new(false);
    let result = shared::rank_observed(query, &rows.groups, &options, &context, cache, |a, b| {
        observations.compare(a, b).unwrap_or_else(|| {
            missing.set(true);
            std::cmp::Ordering::Equal
        })
    })?;
    if missing.get() {
        return Err(Error::MissingObservation("Share locale comparison".into()));
    }
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for r in result.rows {
        let identity = rows
            .identities
            .get(r.original_index)
            .ok_or(Error::InvalidInput)?;
        if r.row["id"] != identity.id || !seen.insert(r.original_index) {
            return Err(Error::InvalidInput);
        }
        ordered.push(Ranked {
            identity: identity.clone(),
            row: r.row,
            features: r.features,
            model_score: r.model_score,
        });
    }
    Ok(ResultSet {
        rows: ordered,
        channel_latest_ids: result.channel_latest_ids,
    })
}

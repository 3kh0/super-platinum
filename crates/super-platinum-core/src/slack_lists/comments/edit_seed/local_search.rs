//! Channel-only rNCs Save lookup over original local canonical rows.
//! This is not interactive Texty: private/previous-name/empty-query matching
//! are disabled. Callers retain source, policy, normalization and ranking owners.
use super::prepare::{MentionKind, MentionLookup, MentionSearch};
use crate::block_actions::home::workflow::{
    directory_local as local, directory_ranking as ranking, directory_selectors,
};
use crate::composer_emoji::ranking::SimilarityCache;
pub use ranking::{Context, Error};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
pub mod member;
mod rank;
#[cfg(test)]
mod tests;

pub struct Plan {
    lookup: MentionLookup,
    formatted: String,
    beyond: bool,
}
/// Exact hNDQ(dfPN.SB) and JS-lowercased comparison-name observations.
pub struct Name {
    pub normalized: String,
    pub comparison_lowercase: String,
}
pub struct Query<'a> {
    pub formatted: &'a str,
    pub prepared: &'a local::PreparedQuery,
    /// Reached jytj/Intl.Segmenter word-boundary eligibility; not byte length.
    pub beyond_boundary_eligible: Option<bool>,
}
pub struct Matched<'a> {
    scored: local::ChannelScored<'a>,
    comparison: String,
    query: String,
}
impl Matched<'_> {
    pub fn raw(&self) -> &Value {
        self.scored.item
    }
    pub fn id(&self) -> &str {
        self.scored.id
    }
}
pub struct Observation {
    /// Original raw rows after final25 and rNCs' case-sensitive name filter.
    pub search: MentionSearch,
    /// Actual model shortlist latest-selector reads, before exact-name filter.
    pub latest_ids: Vec<String>,
    /// Ordered final25 IDs before rNCs' exact-name filter; not selection authority.
    pub ranked_ids: Vec<String>,
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
/// EmXR.Wr(includeFromAnotherTeam:true) -> zdO/.rW, in original store order.
fn public(row: &Value) -> bool {
    truth(row)
        && truth(&row["is_channel"])
        && !truth(&row["is_private"])
        && !truth(&row["is_group"])
        && !truth(&row["is_mpim"])
}
impl Plan {
    /// `beyond` is the positively observed LuF9.Ai result. Empty queries return
    /// before reading it; missing observations never imply an empty result.
    pub fn capture(lookup: &MentionLookup, beyond: Option<bool>) -> Result<Option<Self>, Error> {
        if lookup.kind != MentionKind::Channel {
            return Err(Error::UnsupportedFeature("Save member lookup".into()));
        }
        if lookup.query.is_empty() {
            return Ok(None);
        }
        let beyond =
            beyond.ok_or_else(|| Error::MissingObservation("matchBeyondWordBoundaries".into()))?;
        let query = lookup.query.as_str();
        // Ojx4.O({shouldTrimEmoji:false}): one trigger, no trim or case folding.
        let formatted = match query.chars().next() {
            Some(
                c @ ('@' | '\u{ff20}' | '#' | '\u{ff03}' | '/' | '\u{ff0f}' | '+' | '\u{ff0b}'),
            ) => &query[c.len_utf8()..],
            _ => query,
        };
        Ok(Some(Self {
            lookup: lookup.clone(),
            formatted: formatted.into(),
            beyond,
        }))
    }
    pub fn formatted_query(&self) -> &str {
        &self.formatted
    }
    /// Exact requested options, useful for a retained caller-owned receipt.
    pub fn requested(&self) -> Value {
        json!({"query":self.lookup.query,"options":{"entities":{"channels":{
            "includePrivate":false,"includeFromAnotherTeam":true}},"limit":25,
            "sort":{"source":"texty-autocomplete"}}})
    }
    /// Exact supported source subset for the caller's name observations. This
    /// does not sort, clone, or treat unsupported name producers as negatives.
    pub fn name_rows<'a>(&self, rows: &'a [Value]) -> Result<Vec<&'a Value>, Error> {
        let mut ids = BTreeSet::new();
        let mut result = Vec::new();
        for row in rows.iter().filter(|row| public(row)) {
            let Some(input) = local::channel_name_input(row) else {
                continue;
            };
            if !matches!(input, local::ChannelNameInput::Ordinary { raw_name: Some(name) } if !name.is_empty())
            {
                return Err(Error::UnsupportedFeature(
                    "Save channel name producer".into(),
                ));
            }
            let id = row["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or(Error::InvalidInput)?;
            if !row["fromAnotherTeam"].is_boolean() || !ids.insert(id) {
                return Err(Error::InvalidInput);
            }
            result.push(row);
        }
        Ok(result)
    }
    /// Only ordinary channel names are supported by this bounded adapter.
    /// Reached record/meeting names require their real name producer, not omission.
    pub fn matching<'a>(
        &self,
        query: &Query<'_>,
        rows: &'a [Value],
        name: impl Fn(&Value) -> Result<Name, Error>,
    ) -> Result<Vec<Matched<'a>>, Error> {
        if query.formatted != self.formatted {
            return Err(Error::InvalidInput);
        }
        let mut options = local::Options::workflow(
            &self.formatted,
            &query.prepared.normalized,
            &query.prepared.matcher_normalized,
            false,
        );
        options.fuzzy.can_match_beyond_word_boundaries = self.beyond
            && query
                .beyond_boundary_eligible
                .ok_or_else(|| Error::MissingObservation("word-boundary eligibility".into()))?;
        let mut names = HashMap::new();
        for row in self.name_rows(rows)? {
            // hHTo excludes unknown/file/huddle rows before name acquisition.
            names.insert(row as *const Value, name(row)?);
        }
        // fRHe's archived/required/external/member/canPost defaults make the
        // channel-specific predicate true. No global filter is supplied by rNCs.
        let scores = local::filter_channels(
            rows,
            &options,
            false,
            &BTreeSet::new(),
            |row| names.contains_key(&(row as *const Value)),
            |row| names[&(row as *const Value)].normalized.clone(),
            |_| unreachable!("previous-name search is disabled"),
        );
        Ok(scores
            .into_iter()
            .map(|scored| Matched {
                comparison: names[&(scored.item as *const Value)]
                    .comparison_lowercase
                    .clone(),
                scored,
                query: self.lookup.query.clone(),
            })
            .collect())
    }
    /// Retained 4ucY -> Ejs0 initial75/model/final25, then exact raw name filter.
    /// `Context.state` must contain actual complete selector observations; see
    /// the existing Texty ranker contract. No wire `facts` are trusted.
    pub fn rank(
        &self,
        matched: &[Matched<'_>],
        context: &Context<'_>,
        cache: &mut SimilarityCache,
        collate: impl Fn(&str, &str) -> Result<std::cmp::Ordering, Error>,
    ) -> Result<Observation, Error> {
        rank::run(self, matched, context, cache, collate)
    }
}

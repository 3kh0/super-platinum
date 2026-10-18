use super::*;
use crate::{
    block_actions::home::workflow::{
        directory_local as local, directory_ranking as ranking, directory_selectors as selectors,
    },
    composer_emoji::ranking::SimilarityCache,
};
use std::cmp::Ordering;
/// Captured query-local renderer/experiment observations, never wire fields.
pub struct MatchFacts {
    pub query_is_maybe_self: bool,
    pub can_match_beyond_word_boundaries: bool,
    pub passthrough_remote_fuzzy_filtering: bool,
}
pub fn match_members<'a>(
    query: &Query,
    path: SearchPath,
    prepared: &local::PreparedQuery,
    members: &'a [Value],
    remote: &BTreeSet<String>,
    facts: &MatchFacts,
    eligible: impl Fn(&Value) -> bool,
) -> Vec<local::Scored<'a>> {
    let mut options = local::Options::workflow(
        query.text(path),
        &prepared.normalized,
        &prepared.matcher_normalized,
        facts.query_is_maybe_self,
    );
    options.allow_empty_query = true;
    options.email_forward_only = true;
    options.passthrough_remote_fuzzy_filtering = facts.passthrough_remote_fuzzy_filtering;
    options.fuzzy.can_match_beyond_word_boundaries = facts.can_match_beyond_word_boundaries;
    local::filter_members(members, &options, remote, eligible)
}
/// Comparison names are captured VRXO policy/locale observations. Raw remote
/// `facts` are discarded by selectors::item. Return original matcher indices;
/// a sorted row never becomes a replacement selection/custody object.
pub fn rank_members(
    query: &Query,
    path: SearchPath,
    matched: &[local::Scored<'_>],
    comparison_names: &[String],
    inputs: &RankInputs<'_>,
    observations: &RankObservations,
    cache: &mut SimilarityCache,
) -> Result<Vec<usize>, ranking::Error> {
    let rows = rows(matched, comparison_names)?;
    observations.validate(query.text(path), &rows)?;
    let normalize = |s: &str| observations.normalize(s);
    let context = ranking::Context {
        state: inputs.state,
        history: inputs.history,
        model: inputs.model,
        now_ms: inputs.now_ms,
        normalize: &normalize,
    };
    let options = ranking::Options {
        limit: LIMIT,
        demote_passthrough: observations.demote_passthrough(),
        ..Default::default()
    };
    let missing = std::cell::Cell::new(false);
    let ranked = ranking::rank(
        query.text(path),
        &json!({"members":rows}),
        &options,
        &context,
        cache,
        |a, b| {
            observations.compare(a, b).unwrap_or_else(|| {
                missing.set(true);
                Ordering::Equal
            })
        },
    )?;
    if missing.get() {
        return Err(ranking::Error::MissingObservation(
            "activity comparison name".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    ranked
        .into_iter()
        .map(|r| {
            let i = r.original_index;
            if i >= matched.len() || !seen.insert(i) || r.row["id"].as_str() != Some(matched[i].id)
            {
                return Err(ranking::Error::InvalidInput);
            }
            Ok(i)
        })
        .collect()
}

pub(super) fn rows(
    matched: &[local::Scored<'_>],
    comparison_names: &[String],
) -> Result<Vec<Value>, ranking::Error> {
    if matched.len() != comparison_names.len() {
        return Err(ranking::Error::InvalidInput);
    }
    let mut rows = Vec::new();
    for (scored, name) in matched.iter().zip(comparison_names) {
        let item = selectors::item(
            scored.item,
            selectors::Names {
                comparison: name,
                mpim: None,
                custom: None,
                custom_allowed: false,
            },
        )
        .ok_or(ranking::Error::InvalidInput)?;
        if item["facts"]["member"] != true || item["id"].as_str() != Some(scored.id) {
            return Err(ranking::Error::InvalidInput);
        }
        let mut row = json!({"id":scored.id,"item":item,"score":scored.score,"isSelf":scored.is_self,"isFlannelFuzzyPassthrough":scored.is_flannel_fuzzy_passthrough});
        if let Some(details) = &scored.score_details {
            row["scoreDetails"] = json!(details);
        }
        if let Some(exact) = scored.is_exact_match {
            row["isExactMatch"] = json!(exact);
        }
        if let Some(details) = &scored.exact_match_details {
            row["exactMatchDetails"] = json!(details);
        }
        rows.push(row);
    }
    Ok(rows)
}

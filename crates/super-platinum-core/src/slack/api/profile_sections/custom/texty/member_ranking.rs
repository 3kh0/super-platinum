//! Mixed 4ucY/Ejs0 Texty adapter: one initial 75 and one final 25.
//! Locale/name/policy/cache inputs belong to one captured owner. Ranking grants
//! no selection authority and never manufactures missing cache observations.
use super::groups::GroupScored;
use super::member_matching::{truth, validate};
use crate::block_actions::home::workflow::{
    directory_local as local, directory_ranking as shared, directory_selectors,
};
use crate::composer_emoji::ranking::SimilarityCache;
use serde_json::{Value, json};
pub use shared::{Context, Error, Ranked};
use std::cmp::Ordering;

fn missing(label: &str) -> Error {
    Error::MissingObservation(format!("Profile member {label}"))
}
fn observed(context: &Context<'_>, matched: &[local::Scored<'_>]) -> Result<(), Error> {
    let s = context.state;
    if !context.now_ms.is_finite() {
        return Err(Error::InvalidClock);
    }
    if s["selfId"].as_str().is_none_or(str::is_empty)
        || s["self"]["facts"]["team"]
            .as_str()
            .is_none_or(str::is_empty)
        || ["ims", "metadata", "navigationHistory", "experiments"]
            .iter()
            .any(|key| !s[*key].is_object())
        || ["enterprise", "enterpriseId", "teamId", "offline"]
            .iter()
            .any(|key| s.get(*key).is_none())
        || [
            "autocomplete_affinity_score",
            "autocomplete_affinity_ml_model",
        ]
        .iter()
        .any(|key| s["experiments"].get(*key).is_none())
    {
        return Err(missing("selector state"));
    }
    if truth(&s["teamId"])
        && truth(&s["offline"])
        && (["remoteDm", "remoteMpim", "affinity"]
            .iter()
            .any(|key| !s[*key].is_object())
            || s["entityCtr"]["member"]
                .as_f64()
                .is_none_or(|n| !n.is_finite()))
    {
        return Err(missing("offline observations"));
    }
    for m in matched {
        validate(m.item)?;
        if m.item["id"].as_str() != Some(m.id) || m.is_self || m.is_exact_match.is_some() {
            return Err(Error::InvalidInput);
        }
        // A complete observed IM map may have no entry for this member. That
        // absence is source behavior; inventing a U -> D mapping changes scores.
        let im = &s["ims"][m.id];
        if !im.is_null() {
            let id = im["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| missing("IM identity"))?;
            if !s["metadata"][id].is_object()
                || [
                    "latest", "lastRead", "draft", "starred", "unread", "priority",
                ]
                .iter()
                .any(|key| s["metadata"][id].get(*key).is_none())
            {
                return Err(missing("IM metadata"));
            }
        }
        if truth(&s["enterpriseId"])
            && s["enterpriseId"] == m.item["enterprise_id"]
            && (!s["searcherTeams"].is_array() || s.get("defaultTeam").is_none())
        {
            return Err(missing("enterprise workspace observations"));
        }
    }
    Ok(())
}
/// `original_index` is the position in `matched`, including duplicate IDs. The
/// comparison callback supplies the captured VRXO name, not a raw-wire `facts`
/// override. Both normalization and collation must be observed by the caller.
pub fn rank_members(
    query: &str,
    matched: &[local::Scored<'_>],
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    comparison: impl Fn(&Value) -> Result<String, Error>,
    collate: impl Fn(&str, &str) -> Result<Ordering, Error>,
    demote_passthrough_observed: bool,
) -> Result<Vec<Ranked>, Error> {
    rank_mixed(
        query,
        matched,
        &[],
        context,
        cache,
        comparison,
        collate,
        demote_passthrough_observed,
    )
}
/// Members precede group occurrences in the shared `original_index` space.
/// Group items are the captured effective source records, not incoming partials.
/// The shared sorter owns both limits; neither category is capped independently.
pub fn rank_mixed(
    query: &str,
    matched: &[local::Scored<'_>],
    groups: &[GroupScored<'_>],
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    comparison: impl Fn(&Value) -> Result<String, Error>,
    collate: impl Fn(&str, &str) -> Result<Ordering, Error>,
    demote_passthrough_observed: bool,
) -> Result<Vec<Ranked>, Error> {
    rank_mixed_files(
        query,
        matched,
        groups,
        &[],
        context,
        cache,
        comparison,
        collate,
        demote_passthrough_observed,
    )
}

/// Member, group, then file occurrences share one `original_index` space.
/// File rows require the independently observed effective file collection;
/// the shared sorter applies its shortlist before any category is published.
pub fn rank_mixed_files(
    query: &str,
    matched: &[local::Scored<'_>],
    groups: &[GroupScored<'_>],
    files: &[super::file_matching::FileScored<'_>],
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    comparison: impl Fn(&Value) -> Result<String, Error>,
    collate: impl Fn(&str, &str) -> Result<Ordering, Error>,
    demote_passthrough_observed: bool,
) -> Result<Vec<Ranked>, Error> {
    observed(context, matched)?;
    let mut rows = Vec::with_capacity(matched.len());
    for m in matched {
        let name = comparison(m.item)?;
        let item = directory_selectors::item(
            m.item,
            directory_selectors::Names {
                comparison: &name,
                mpim: None,
                custom: None,
                custom_allowed: false,
            },
        )
        .ok_or(Error::InvalidInput)?;
        let mut row = json!({"id":m.id,"item":item,"score":m.score});
        if let Some(details) = &m.score_details {
            row["scoreDetails"] = json!(details);
        }
        if let Some(details) = &m.exact_match_details {
            row["exactMatchDetails"] = json!(details);
        }
        if m.is_flannel_fuzzy_passthrough {
            row["isFlannelFuzzyPassthrough"] = json!(true);
        }
        rows.push(row);
    }
    let mut group_rows = Vec::with_capacity(groups.len());
    for group in groups {
        if group.id != group.snapshot.id() {
            return Err(Error::InvalidInput);
        }
        let item = directory_selectors::group(group.item()).ok_or(Error::InvalidInput)?;
        let mut row = json!({"id":group.id,"item":item,"score":group.score});
        if let Some(exact) = group.is_exact_match {
            row["isExactMatch"] = json!(exact);
        }
        group_rows.push(row);
    }
    let file_rows = super::file_ranking::rows(files, context)?;
    let failure = std::cell::RefCell::new(None);
    let result = shared::rank(
        query,
        &json!({"members":rows,"userGroups":group_rows,"files":file_rows}),
        &shared::Options {
            limit: 25,
            source: "texty-autocomplete".into(),
            demote_passthrough: demote_passthrough_observed,
            ..Default::default()
        },
        context,
        cache,
        |a, b| {
            collate(a, b).unwrap_or_else(|error| {
                *failure.borrow_mut() = Some(error);
                Ordering::Equal
            })
        },
    )?;
    if let Some(error) = failure.into_inner() {
        return Err(error);
    }
    Ok(result)
}

#[cfg(test)]
mod file_tests;
#[cfg(test)]
mod mixed_tests;
#[cfg(test)]
mod tests;

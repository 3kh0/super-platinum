//! Retained hHTo member/IM filtering. This is the scoring stage, not frecency/model sorting.
//! The caller owns normalized-query and user-store producers, 8Bbq eligibility,
//! current account fencing and actual remote-member IDs. No U-to-D synthesis.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
mod channel;
mod fuzzy;
mod mpim;
mod normalization;
pub use channel::{
    ChannelNameInput, ChannelScored, PreviousNameScore, channel_name_input, filter_channels,
};
pub use fuzzy::{FuzzyMatcher, FuzzyOptions};
pub use mpim::{
    MemberSubqueryScore, MpimScored, filter_mpims, mpim_subquery_inputs, visible_mpim_custom_name,
};
pub use normalization::{
    PreparedQuery, derive_member_names, normalize_lowered_query, normalize_name_source,
    normalize_query_with_lowercase, prepare_query,
};

#[derive(Clone, Debug)]
pub struct Options<'a> {
    pub raw_query: &'a str,
    /// Exact hNDQ output, not a display-name substring or palette query.
    pub normalized_query: &'a str,
    /// jytj constructor applies hNDQ once more to hHTo.normalized_query.
    pub matcher_query: &'a str,
    /// Exact KhGM observation, including current localized "me" / "you".
    pub query_is_maybe_self: bool,
    pub allow_empty_query: bool,
    pub prefer_exact_match: bool,
    pub avoid_exact_match_for_members: bool,
    pub self_first: bool,
    pub match_email: bool,
    pub email_forward_only: bool,
    pub passthrough_remote_fuzzy_filtering: bool,
    pub fuzzy: FuzzyOptions,
}
impl<'a> Options<'a> {
    /// fRHe defaults. Query normalization/self-language remain explicit inputs.
    pub fn workflow(
        raw_query: &'a str,
        normalized_query: &'a str,
        matcher_query: &'a str,
        query_is_maybe_self: bool,
    ) -> Self {
        Self {
            raw_query,
            normalized_query,
            matcher_query,
            query_is_maybe_self,
            allow_empty_query: false,
            prefer_exact_match: true,
            avoid_exact_match_for_members: true,
            self_first: false,
            match_email: false,
            email_forward_only: false,
            passthrough_remote_fuzzy_filtering: false,
            fuzzy: FuzzyOptions::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Scored<'a> {
    pub item: &'a Value,
    pub id: &'a str,
    pub score: u32,
    /// None only for empty-query/raw-D-ID paths; Some(empty) matches hHTo {}.
    pub score_details: Option<BTreeMap<&'static str, u32>>,
    pub is_exact_match: Option<bool>,
    pub exact_match_details: Option<BTreeMap<&'static str, bool>>,
    pub is_self: bool,
    pub is_flannel_fuzzy_passthrough: bool,
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
fn known(v: &Value) -> bool {
    !v.is_null() && !truthy(&v["isUnknown"]) && !truthy(&v["isNonExistent"])
}
fn stored<'a>(v: &'a Value, field: &str) -> &'a str {
    v[field].as_str().unwrap_or("")
}
fn plain<'a>(item: &'a Value, id: &'a str) -> Scored<'a> {
    Scored {
        item,
        id,
        score: 0,
        score_details: None,
        is_exact_match: None,
        exact_match_details: None,
        is_self: false,
        is_flannel_fuzzy_passthrough: false,
    }
}
fn score_member(
    member: &Value,
    options: &Options<'_>,
    matcher: &FuzzyMatcher,
) -> (Option<u32>, BTreeMap<&'static str, u32>) {
    let mut details = BTreeMap::new();
    if options.query_is_maybe_self && truthy(&member["is_self"]) {
        details.insert("self_score", 0);
    } else {
        for (key, field) in [
            ("real_name", "_real_name_normalized_lc"),
            ("display_name", "_display_name_normalized_lc"),
        ] {
            if let Some(score) = matcher.score(stored(member, field)) {
                details.insert(key, score);
            }
        }
    }
    if options.match_email {
        let fuzzy = FuzzyOptions {
            forward_only: options.email_forward_only,
            can_match_beyond_word_boundaries: false,
            ..options.fuzzy
        };
        if let Some(score) = FuzzyMatcher::new(options.matcher_query, fuzzy)
            .score(stored(member, "_email_normalized_lc"))
        {
            details.insert("email", score);
        }
    }
    (details.values().min().copied(), details)
}
fn exact(member: &Value, options: &Options<'_>) -> BTreeMap<&'static str, bool> {
    let mut details = BTreeMap::from([
        (
            "real_name",
            stored(member, "_real_name_normalized_lc") == options.normalized_query,
        ),
        (
            "display_name",
            stored(member, "_display_name_normalized_lc") == options.normalized_query,
        ),
    ]);
    if options.match_email {
        details.insert(
            "email",
            stored(member, "_email_normalized_lc") == options.normalized_query,
        );
    }
    details
}
fn ranked<'a>(
    item: &'a Value,
    id: &'a str,
    member: &Value,
    options: &Options<'_>,
    matcher: &FuzzyMatcher,
    remote: &BTreeSet<String>,
    member_id: &str,
) -> Option<Scored<'a>> {
    let (score, details) = score_member(member, options, matcher);
    let accepted = score.is_some_and(|n| n <= options.fuzzy.limit);
    let passthrough =
        !accepted && options.passthrough_remote_fuzzy_filtering && remote.contains(member_id);
    if !accepted && !passthrough {
        return None;
    }
    let mut result = plain(item, id);
    result.score = if passthrough { 2 } else { score.unwrap() };
    result.score_details = Some(details);
    result.is_flannel_fuzzy_passthrough = passthrough;
    if !options.avoid_exact_match_for_members && options.prefer_exact_match {
        let matches = exact(member, options);
        result.is_exact_match =
            Some(!options.normalized_query.is_empty() && matches.values().any(|b| *b));
        result.exact_match_details = Some(matches);
    }
    Some(result)
}
/// hHTo.filterIms: preserve source order and original IM object/id. `eligible`
/// must be 8Bbq.getImFilterFunction plus the global filter, not member eligibility.
pub fn filter_ims<'a>(
    ims: &'a [Value],
    peers: &BTreeMap<String, Value>,
    options: &Options<'_>,
    remote_members: &BTreeSet<String>,
    eligible: impl Fn(&Value) -> bool,
) -> Vec<Scored<'a>> {
    let fuzzy = FuzzyOptions {
        can_match_beyond_word_boundaries: false,
        ..options.fuzzy
    };
    let matcher = FuzzyMatcher::new(options.matcher_query, fuzzy);
    ims.iter()
        .filter_map(|im| {
            if !eligible(im) || !truthy(&im["is_im"]) {
                return None;
            }
            let peer_id = im["user"].as_str().filter(|s| !s.is_empty())?;
            let peer = peers.get(peer_id).filter(|p| known(p))?;
            let id = im["id"].as_str()?;
            if options.normalized_query.is_empty() && options.allow_empty_query {
                return Some(plain(im, id));
            }
            ranked(im, id, peer, options, &matcher, remote_members, peer_id)
                .or_else(|| (options.raw_query == id).then(|| plain(im, id)))
        })
        .collect()
}
/// hHTo.filterMembers, before downstream model/frecency ordering. The supplied
/// predicate owns workflow-specific member eligibility and the global filter.
pub fn filter_members<'a>(
    members: &'a [Value],
    options: &Options<'_>,
    remote_members: &BTreeSet<String>,
    eligible: impl Fn(&Value) -> bool,
) -> Vec<Scored<'a>> {
    let matcher = FuzzyMatcher::new(options.matcher_query, options.fuzzy);
    members
        .iter()
        .filter_map(|member| {
            if !known(member) || !eligible(member) {
                return None;
            }
            let id = member["id"].as_str()?;
            let mut result = if options.normalized_query.is_empty() && options.allow_empty_query {
                plain(member, id)
            } else {
                ranked(member, id, member, options, &matcher, remote_members, id)?
            };
            result.is_self = options.self_first && truthy(&member["is_self"]);
            Some(result)
        })
        .collect()
}

#[cfg(test)]
mod tests;

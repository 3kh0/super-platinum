//! hHTo.filterMpims over actual local group conversations. Remote MPIM IDs are
//! generic matcher input only: hXqI's channel picker supplies an empty set.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct MemberSubqueryScore {
    pub id: String,
    /// None is JavaScript Infinity, not zero and not an omitted member.
    pub score: Option<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MpimScored<'a> {
    pub result: Scored<'a>,
    /// Values are the actual per-member `score_details` maps. None for named,
    /// empty-query and raw-ID fallback branches; Some(empty) is meaningful.
    pub score_details_by_member: Option<BTreeMap<String, BTreeMap<&'static str, u32>>>,
    pub subquery_scores_by_member: Option<Vec<Vec<MemberSubqueryScore>>>,
}
fn plain_mpim<'a>(item: &'a Value, id: &'a str) -> MpimScored<'a> {
    MpimScored {
        result: plain(item, id),
        score_details_by_member: None,
        subquery_scores_by_member: None,
    }
}
/// Exact LG6y.JM policy. `externally_shared` is the caller's actual drRZ.ju
/// observation; experiment flags are true only for observed assignment "on".
pub fn visible_mpim_custom_name(
    item: &Value,
    naming_on: bool,
    externally_shared: bool,
    external_naming_on: bool,
) -> bool {
    truthy(&item["is_mpim"])
        && truthy(&item["properties"]["has_custom_mpdm_name"])
        && naming_on
        && (!externally_shared || external_naming_on)
}
/// hHTo.getSubQueryMatchers inputs, in order. Split first, discard empty pieces,
/// then remove exactly one leading @. An @-only piece remains an empty query.
/// Caller normalizes EACH returned piece once using actual hNDQ (including
/// renderer locale lowercase); this is independent of the full matcher query.
pub fn mpim_subquery_inputs(normalized_query: &str) -> Vec<String> {
    normalized_query
        .split([',', '\u{ff0c}', '\u{3001}', '\u{ff64}', '|', ' '])
        .filter(|s| !s.is_empty())
        .map(|s| s.strip_prefix('@').unwrap_or(s).to_owned())
        .collect()
}
/// `subqueries` is one actual hNDQ-normalized string per mpim_subquery_inputs,
/// including empty @-only queries. `named` returns the hNDQ-normalized raw name
/// only after actual LG6y.JM allows it. `eligible` is the caller's 8Bbq MPIM and
/// global predicates. Peers are resolved by actual raw members IDs; unknown and
/// actual is_self peers are removed, not ordinary workflow-ineligible members.
/// No sorting, new remote route, U/G construction or conversation writes occur.
pub fn filter_mpims<'a>(
    items: &'a [Value],
    peers: &BTreeMap<String, Value>,
    options: &Options<'_>,
    subqueries: &[String],
    remote_mpims: &BTreeSet<String>,
    eligible: impl Fn(&Value) -> bool,
    named: impl Fn(&Value) -> Option<String>,
) -> Vec<MpimScored<'a>> {
    let fuzzy = FuzzyOptions {
        forward_only: false,
        substrings_only: false,
        can_match_beyond_word_boundaries: false,
        ..options.fuzzy
    };
    let matcher = FuzzyMatcher::new(options.matcher_query, fuzzy);
    let submatchers: Vec<_> = subqueries
        .iter()
        .map(|q| FuzzyMatcher::new(q, fuzzy))
        .collect();
    items
        .iter()
        .filter_map(|item| {
            if !eligible(item) {
                return None;
            }
            let id = item["id"].as_str()?;
            if options.normalized_query.is_empty() && options.allow_empty_query {
                return Some(plain_mpim(item, id));
            }
            if !truthy(&item["isUnknown"]) && item["name"].as_str().is_some_and(|n| !n.is_empty()) {
                if let Some(name) = named(item) {
                    if let Some(score) = matcher.score(&name).filter(|n| *n <= options.fuzzy.limit)
                    {
                        let mut result = plain_mpim(item, id);
                        result.result.score = score;
                        if options.prefer_exact_match {
                            result.result.is_exact_match = Some(
                                !options.normalized_query.is_empty()
                                    && options.normalized_query == name,
                            );
                        }
                        return Some(result);
                    }
                }
            }
            // 7tpZ.Ug returns undefined for non-MPIM; an absent list refuses before
            // raw-ID fallback, while an actual empty list still enters scoring.
            if !truthy(&item["is_mpim"]) {
                return None;
            }
            let ids = item["members"].as_array()?;
            let ids: Vec<_> = ids.iter().map(Value::as_str).collect::<Option<_>>()?;
            let members: Vec<_> = ids
                .iter()
                .filter_map(|id| peers.get(*id))
                .filter(|m| known(m) && !truthy(&m["is_self"]))
                .collect();
            let mut best = BTreeMap::<String, u32>::new();
            let mut details = BTreeMap::new();
            let mut rows = Vec::new();
            let mut every_subquery_matches = true;
            for matcher in &submatchers {
                let mut row = Vec::new();
                let mut query_best: Option<u32> = None;
                for member in &members {
                    let member_id = member["id"].as_str()?;
                    let (score, score_details) = score_member(member, options, matcher);
                    if let Some(score) = score {
                        if best.get(member_id).is_none_or(|old| score < *old) {
                            best.insert(member_id.to_owned(), score);
                            details.insert(member_id.to_owned(), score_details);
                        }
                        query_best = Some(query_best.map_or(score, |old| old.min(score)));
                    }
                    row.push(MemberSubqueryScore {
                        id: member_id.to_owned(),
                        score,
                    });
                }
                every_subquery_matches &= query_best.is_some();
                rows.push(row);
            }
            // Kf1L/68A sums minimum finite scores across ALL matched members, not
            // just one best member per query and not the sum of query minima.
            let score =
                every_subquery_matches.then(|| best.values().map(|n| u64::from(*n)).sum::<u64>());
            let accepted = score.is_some_and(|n| n <= u64::from(options.fuzzy.limit));
            let passthrough = !accepted
                && options.passthrough_remote_fuzzy_filtering
                && remote_mpims.contains(id);
            if !accepted && !passthrough {
                return (options.raw_query == id).then(|| plain_mpim(item, id));
            }
            let mut result = plain_mpim(item, id);
            result.result.score = if passthrough {
                2
            } else {
                score.unwrap() as u32
            };
            result.result.is_flannel_fuzzy_passthrough = passthrough;
            result.score_details_by_member = Some(details);
            result.subquery_scores_by_member = Some(rows);
            Some(result)
        })
        .collect()
}

#[cfg(test)]
mod tests;

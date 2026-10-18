//! Exact yL3i file + query tail for profile file-navigation rows.
use super::features::{number, parse_float, text, truth};
use super::{Error, Normalize};
use serde_json::Value;
#[path = "file_words.rs"]
mod words;
pub(super) const KEYS: &[&str] = &[
    "TYPE_NAVIGATION_FILE",
    "FILE_SUGGESTION_SCORE",
    "FILE_AGE_DECAY_7D",
    "FILE_AGE_DECAY_28D",
    "FILE_AGE_DECAY_90D",
    "FILE_FROM_CURRENT_USER",
    "FILE_LOG_PUBLIC_SHARES",
    "FILE_LOG_PRIVATE_SHARES",
    "FILE_LOG_ACTIONS",
    "FILE_LATEST_ACTION_AGE_DECAY_2H",
    "FILE_LATEST_ACTION_AGE_DECAY_7D",
    "FILE_LATEST_ACTION_AGE_DECAY_90D",
    "FILE_LATEST_SHARE_AGE_DECAY_2H",
    "FILE_LATEST_SHARE_AGE_DECAY_7D",
    "FILE_LATEST_SHARE_AGE_DECAY_90D",
    "FILE_CHANNEL_CTR",
    "FILE_SUGGESTIONS_SEARCHER",
    "FILE_USER_CTR",
    "FILE_NAME_CONTAINS_QUERY_NORMALIZED",
    "FILE_IS_CANVAS",
    "FILE_HAS_DMS_WITH_CREATOR",
    "FILE_HAS_DMS_WITH_CREATOR_V2",
    "FILE_LATEST_VIEW_BY_USER_OLDER_90D",
    "FILE_IS_SFDC_LIST",
    "FILE_IS_TOP_ENGAGED",
    "TYPE_QUERY",
    "TYPE_USER_QUERY",
    "TYPE_QUERY_HISTORY",
    "ACTIVE_TOKEN_STRING",
    "ACTIVE_TOKEN_MEMBER",
    "ACTIVE_TOKEN_PHRASE",
    "ACTIVE_TOKEN_QUERY_SUGGESTION",
    "ACTIVE_TOKEN_FILTER_DURING",
    "ACTIVE_TOKEN_FILTER_FROM",
    "ACTIVE_TOKEN_FILTER_IN",
    "ACTIVE_TOKEN_FILTER_TO",
    "ACTIVE_TOKEN_FILTER_BEFORE",
    "ACTIVE_TOKEN_FILTER_HAS",
    "ACTIVE_TOKEN_FILTER_IS",
    "ACTIVE_TOKEN_FILTER_ON",
    "ACTIVE_TOKEN_FILTER_AFTER",
    "ACTIVE_TOKEN_FILTER_WITH",
    "QUERY_SUGGESTION_SCORE_V2",
    "QUERY_FRECENCY_RAW",
    "QUERY_FRECENCY_WEIGHT_HIT_VISITS",
    "QUERY_FRECENCY_HIT_COUNT",
    "QUERY_FRECENCY_WEIGHT_ITEM_VISITS",
    "QUERY_FRECENCY_ITEM_COUNT",
    "QUERY_FUZZY_SCORE",
    "QUERY_FRECENCY_BONUS",
    "QUERY_HISTORY_AGE_DECAY_30D",
    "QUERY_HISTORY_LOG_INDEX",
    "IS_PREFIX_OF_PAST_USER_QUERY",
    "CHAR_OVERLAP_RATIO_USER_HISTORY",
    "FILTER_USER_IS_ACTIVE",
    "FILTER_USER_IS_CURRENT_USER",
    "FILTER_USER_DM_IS_STARRED",
    "FILTER_USER_DM_IS_UNREAD",
    "FILTER_USER_DM_READ_AGE_DECAY_7D",
    "FILTER_USER_DM_READ_AGE_DECAY_2H",
    "FILTER_USER_DM_LATEST_MSG_AGE_DECAY_7D",
    "FILTER_USER_DM_LATEST_MSG_AGE_DECAY_2H",
    "FILTER_USER_FIRST_NAME_NORMAL_MATCH",
    "FILTER_USER_FIRST_NAME_NORMAL_PREFIX_MATCH",
    "FILTER_USER_LAST_NAME_NORMAL_MATCH",
    "FILTER_USER_LAST_NAME_NORMAL_PREFIX_MATCH",
    "FILTER_USER_PRIORITY",
    "FILTER_CHANNEL_IS_ARCHIVED",
    "FILTER_CHANNEL_IS_MEMBER",
    "FILTER_CHANNEL_IS_STARRED",
    "FILTER_CHANNEL_IS_UNREAD",
    "FILTER_CHANNEL_READ_AGE_DECAY_7D",
    "FILTER_CHANNEL_READ_AGE_DECAY_2H",
    "FILTER_CHANNEL_PRIORITY",
    "STRING_EXACT_MATCH",
    "STRING_EXACT_PREFIX_MATCH",
    "STRING_NORMAL_MATCH",
    "STRING_NORMAL_PREFIX_MATCH",
    "STRING_SAME_NUMBER_WORDS",
    "EXCESS_STRING_LENGTH",
    "SUGGESTION_TEXT_LENGTH",
    "QUERY_LENGTH",
    "QUERY_RATIO_LENGTH_STRING",
    "QUERY_TOKEN_NORMAL_MATCH",
    "QUERY_TOKEN_NORMAL_PREFIX_MATCH",
    "QUERY_LOG_HIT_COUNT",
];
fn yes(v: bool) -> f64 {
    u8::from(v) as f64
}
fn numeric(n: f64) -> f64 {
    if n.is_finite() { n } else { 0.0 }
}
fn valid(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
}
fn decay(now: f64, ts: f64, half: f64) -> f64 {
    if valid(ts) {
        ((0.5_f64.ln() / half) * (now - ts).max(0.0)).exp().min(1.0)
    } else {
        0.0
    }
}
fn log(n: usize) -> f64 {
    (n as f64).ln_1p()
}
fn normalized(s: &str, normalize: &Normalize<'_>) -> Result<String, Error> {
    let value = normalize(s)?;
    Ok(value.strip_prefix(['@', '#']).unwrap_or(&value).to_owned())
}
pub(super) fn value(
    key: &str,
    e: &Value,
    now: f64,
    normalize: &Normalize<'_>,
) -> Result<f64, Error> {
    if !KEYS.contains(&key) {
        return Err(Error::UnsupportedFeature(key.to_owned()));
    }
    let item = &e["item"];
    let name = text(&e["name"]);
    let query = text(&e["query"]);
    let nq = text(&e["normalizedQuery"]);
    let field = |k: &str| number(&e[k]);
    let bit = |k: &str| yes(truth(&e[k]));
    let ratio = || {
        if name.is_empty() {
            0.0
        } else {
            query.encode_utf16().count() as f64 / name.encode_utf16().count() as f64
        }
    };
    let half = if key.contains("2H") {
        7200.0
    } else if key.contains("28D") {
        2419200.0
    } else if key.contains("90D") {
        7776000.0
    } else {
        604800.0
    };
    let out = match key {
        "TYPE_NAVIGATION_FILE" | "FILE_SUGGESTIONS_SEARCHER" => 1.0,
        "FILE_SUGGESTION_SCORE" => field("rawScore"),
        "FILE_AGE_DECAY_7D" | "FILE_AGE_DECAY_28D" | "FILE_AGE_DECAY_90D" => {
            if item
                .get("created")
                .is_some_and(|v| !v.is_null() && !v.is_number())
            {
                return Err(Error::InvalidInput);
            }
            decay(now, number(&item["created"]), half)
        }
        "FILE_FROM_CURRENT_USER" => bit("fromCurrentUser"),
        "FILE_LOG_PUBLIC_SHARES" => item["shares"]["public"]
            .as_object()
            .map_or(0.0, |m| log(m.len())),
        "FILE_LOG_PRIVATE_SHARES" => item["shares"]["private"]
            .as_object()
            .map_or(0.0, |m| log(m.len())),
        "FILE_LOG_ACTIONS" => item["actions"].as_array().map_or(0.0, |m| log(m.len())),
        "FILE_LATEST_ACTION_AGE_DECAY_2H"
        | "FILE_LATEST_ACTION_AGE_DECAY_7D"
        | "FILE_LATEST_ACTION_AGE_DECAY_90D" => decay(now, field("latestActionTs"), half),
        "FILE_LATEST_SHARE_AGE_DECAY_2H"
        | "FILE_LATEST_SHARE_AGE_DECAY_7D"
        | "FILE_LATEST_SHARE_AGE_DECAY_90D" => decay(now, field("latestShareTs"), half),
        "FILE_CHANNEL_CTR" => field("fileChannelCtr"),
        "FILE_USER_CTR" => field("fileUserCtr"),
        "FILE_NAME_CONTAINS_QUERY_NORMALIZED" => {
            let value = normalized(name, normalize)?;
            yes(!name.is_empty() && !value.is_empty() && !nq.is_empty() && value.contains(nq))
        }
        "FILE_IS_CANVAS" => yes(e["fileType"] == "quip"),
        "FILE_HAS_DMS_WITH_CREATOR" => bit("hasNonNullDmsWithCreator"),
        "FILE_HAS_DMS_WITH_CREATOR_V2" => bit("hasNonNullLocalOrRemoteDmsWithCreator"),
        "FILE_LATEST_VIEW_BY_USER_OLDER_90D" => {
            let ts = parse_float(&e["lastReadTs"]) / 1000.0;
            let age = (now - ts).max(0.0);
            yes(!valid(ts) || age == 0.0 || age > 7776000.0)
        }
        "FILE_IS_SFDC_LIST" => yes(e["type"] == "sidebarEntities"),
        "FILE_IS_TOP_ENGAGED" => bit("fileIsTopEngaged"),
        "STRING_EXACT_MATCH" => yes(!name.is_empty() && name == query),
        "STRING_EXACT_PREFIX_MATCH" => {
            yes(!name.is_empty() && !query.is_empty() && name.starts_with(query))
        }
        "FILTER_USER_FIRST_NAME_NORMAL_MATCH"
        | "FILTER_USER_LAST_NAME_NORMAL_MATCH"
        | "STRING_NORMAL_MATCH" => yes(!name.is_empty() && normalized(name, normalize)? == nq),
        "FILTER_USER_FIRST_NAME_NORMAL_PREFIX_MATCH"
        | "FILTER_USER_LAST_NAME_NORMAL_PREFIX_MATCH"
        | "STRING_NORMAL_PREFIX_MATCH" => {
            let value = normalized(name, normalize)?;
            yes(!name.is_empty() && !value.is_empty() && !nq.is_empty() && value.starts_with(nq))
        }
        "STRING_SAME_NUMBER_WORDS" => yes(!name.is_empty()
            && words::count(name) + usize::from(name.ends_with(' ')) == words::count(query)),
        "EXCESS_STRING_LENGTH" => {
            if name.is_empty() {
                0.0
            } else {
                name.encode_utf16()
                    .count()
                    .saturating_sub(query.encode_utf16().count()) as f64
            }
        }
        "SUGGESTION_TEXT_LENGTH" => name.encode_utf16().count() as f64,
        "QUERY_LENGTH" => {
            if name.is_empty() {
                0.0
            } else {
                nq.encode_utf16().count() as f64
            }
        }
        "QUERY_RATIO_LENGTH_STRING" => ratio(),
        // aD6p's file matcher creates no search-query tokens/scoring context.
        // rank() refuses token-bearing rows, so the remaining query-only fields
        // are the actual source's null/false/zero numeric result for this kind.
        // The first/last-name branches above deliberately use the default title:
        // undefined text invokes makeTextFeature's default parameter.
        _ => 0.0,
    };
    Ok(numeric(out))
}

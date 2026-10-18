//! Exact numeric jhqG v8 member/channel/MPIM features, including source quirks.
use super::lists::*;
use super::{Error, Normalize};
use serde_json::Value;

pub(super) fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(x) => *x,
        Value::Number(x) => x.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Value::String(x) => !x.is_empty(),
        _ => true,
    }
}
pub(super) fn number(v: &Value) -> f64 {
    match v {
        Value::Number(x) => x.as_f64().unwrap_or(f64::NAN),
        Value::Bool(x) => u8::from(*x) as f64,
        _ => 0.0,
    }
}
pub(super) fn numeric(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}
pub(super) fn yes(v: bool) -> f64 {
    u8::from(v) as f64
}
pub(super) fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
pub(super) fn parse_float(v: &Value) -> f64 {
    if v.is_number() {
        return number(v);
    }
    let s = text(v).trim_start_matches(super::js_space);
    if s.starts_with("Infinity") || s.starts_with("+Infinity") {
        return f64::INFINITY;
    }
    if s.starts_with("-Infinity") {
        return f64::NEG_INFINITY;
    }
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-') {
            end = i + c.len_utf8()
        } else {
            break;
        }
    }
    while end > 0 {
        if let Ok(v) = s[..end].parse::<f64>() {
            return v;
        }
        end -= 1;
    }
    f64::NAN
}
fn valid(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}
fn decay(now: f64, stamp: f64, half: f64) -> f64 {
    if valid(stamp) {
        ((0.5_f64.ln() / half) * (now - stamp).max(0.0))
            .exp()
            .min(1.0)
    } else {
        0.0
    }
}
fn older(now: f64, stamp: f64, span: f64) -> bool {
    !valid(stamp) || (now - stamp).max(0.0) == 0.0 || (now - stamp).max(0.0) > span
}
fn log(v: f64) -> f64 {
    if v > 0.0 { v.ln_1p() } else { 0.0 }
}
fn norm(s: &str, n: &Normalize<'_>) -> Result<String, Error> {
    let v = n(s)?;
    Ok(v.strip_prefix(['@', '#']).unwrap_or(&v).to_owned())
}
fn normal_match(s: &str, q: &str, n: &Normalize<'_>, prefix: bool) -> Result<bool, Error> {
    let s = norm(s, n)?;
    Ok(if prefix {
        !s.is_empty() && !q.is_empty() && s.starts_with(q)
    } else {
        s == q
    })
}
fn source_member_read(e: &Value) -> f64 {
    if truth(&e["im"]) && truth(&e["lastReadImTs"]) {
        parse_float(&e["lastReadImTs"])
    } else if truth(&e["remoteImMetadata"]) && truth(&e["lastReadRemoteImMetadataTs"]) {
        number(&e["lastReadRemoteImMetadataTs"]) / 1e6
    } else {
        f64::NAN
    }
}
fn source_mpim_read(e: &Value) -> f64 {
    if truth(&e["mpimFeatures"]) && truth(&e["mpimFeatures"]["lastReadTs"]) {
        parse_float(&e["mpimFeatures"]["lastReadTs"])
    } else if truth(&e["remoteMpimMetadata"]) && truth(&e["lastReadRemoteMpimMetadataTs"]) {
        number(&e["lastReadRemoteMpimMetadataTs"]) / 1e6
    } else {
        f64::NAN
    }
}
fn source_membership(e: &Value) -> f64 {
    if truth(&e["remoteMpimMetadata"]) && truth(&e["remoteMpimMembershipCount"]) {
        number(&e["remoteMpimMembershipCount"])
    }
    // Actual source returns lastReadTs here, not membershipCount.
    else if truth(&e["mpimFeatures"]) && truth(&e["mpimFeatures"]["membershipCount"]) {
        number(&e["mpimFeatures"]["lastReadTs"])
    } else {
        0.0
    }
}

pub(super) fn vector(
    e: &Value,
    now: f64,
    source: &str,
    n: &Normalize<'_>,
) -> Result<Vec<(&'static str, f64)>, Error> {
    let tail = match text(&e["suggestionType"]) {
        "user-navigation" => USER,
        "channel-navigation" => CHANNEL,
        "mpim-navigation" => MPIM,
        "file-navigation" => super::file_features::KEYS,
        _ => &[],
    };
    GLOBAL
        .iter()
        .chain(tail)
        .map(|&key| value(key, e, now, source, n).map(|v| (key, numeric(v))))
        .collect()
}
fn value(key: &str, e: &Value, now: f64, source: &str, n: &Normalize<'_>) -> Result<f64, Error> {
    let item = &e["item"];
    let f = &item["facts"];
    let flag = |k: &str| truth(&f[k]);
    let field = |k: &str| number(&e[k]);
    let bit = |k: &str| yes(truth(&e[k]));
    let member = flag("member");
    let channel = flag("channel") || flag("privateChannel");
    let mpim = flag("mpim");
    let im = truth(&e["im"]);
    let kind = text(&e["suggestionType"]);
    let name = text(&e["name"]);
    let query = text(&e["query"]);
    let nq = text(&e["normalizedQuery"]);
    let score = e["score"].as_f64();
    let nav = truth(&e["isNavigational"]) || truth(&e["isAction"]) || truth(&e["isApp"]);
    let nav_name = if nav { name } else { "" };
    let no_user = if kind == "user-navigation" { "" } else { name };
    let normal = |s: &str, prefix: bool| normal_match(s, nq, n, prefix);
    let prefix = |s: &str| !s.is_empty() && !query.is_empty() && s.starts_with(query);
    let ratio = |s: &str| {
        if s.is_empty() {
            0.0
        } else {
            query.encode_utf16().count() as f64 / s.encode_utf16().count() as f64
        }
    };
    let read = source_member_read(e);
    let mread = source_mpim_read(e);
    let membership = source_membership(e);
    let half = if key.contains("2H") {
        7200.0
    } else if key.contains("30D") {
        2592000.0
    } else if key.contains("90D") {
        7776000.0
    } else {
        604800.0
    };
    let res = match key {
        "ID" => 0.0,
        "FRECENCY" => {
            if matches!(
                kind,
                "user-navigation" | "channel-navigation" | "mpim-navigation" | "file-navigation"
            ) {
                field("frecencyScore")
            } else {
                0.0
            }
        }
        "RAW_FRECENCY" => field("rawFrecencyScore"),
        "FRECENCY_BONUS" => super::bonus_sum(&e["bonusPointMap"]),
        "FRECENCY_LOG_HIT_COUNT" => field("hitCount").ln_1p(),
        "FRECENCY_LOG_ITEM_COUNT" => field("itemCount").ln_1p(),
        "FRECENCY_LOG_HIT_COUNT_SCALED" => field("logHitCountScaled"),
        "FRECENCY_LOG_ITEM_COUNT_SCALED" => field("logItemCountScaled"),
        "FRECENCY_HIT_COUNT" => field("hitCount"),
        "FRECENCY_ITEM_COUNT" => field("itemCount"),
        "FRECENCY_WEIGHT_HIT_VISITS" => field("weightHitVisits"),
        "FRECENCY_WEIGHT_ITEM_VISITS" => field("weightItemVisits"),
        "FRECENCY_HIT_VISITS_4H" => field("hitVisits4h"),
        "FRECENCY_HIT_VISITS_1D" => field("hitVisits1d"),
        "FRECENCY_HIT_VISITS_3D" => field("hitVisits3d"),
        "FRECENCY_HIT_VISITS_1W" => field("hitVisits1w"),
        "FRECENCY_HIT_VISITS_1M" => field("hitVisits1m"),
        "FRECENCY_HIT_VISITS_90D" => field("hitVisits90d"),
        "FRECENCY_ITEM_VISITS_4H" => field("itemVisits4h"),
        "FRECENCY_ITEM_VISITS_1D" => field("itemVisits1d"),
        "FRECENCY_ITEM_VISITS_3D" => field("itemVisits3d"),
        "FRECENCY_ITEM_VISITS_1W" => field("itemVisits1w"),
        "FRECENCY_ITEM_VISITS_1M" => field("itemVisits1m"),
        "FRECENCY_ITEM_VISITS_90D" => field("itemVisits90d"),
        "FUZZY_SCORE" => score.unwrap_or(0.0),
        "FUZZY_SCORE_50" => yes(e["bonusPointMap"]["fuzzyScoreBonus"].as_f64() == Some(50.0)),
        "FUZZY_SCORE_25" => yes(e["bonusPointMap"]["fuzzyScoreBonus"].as_f64() == Some(25.0)),
        "FUZZY_SCORE_ALL" => yes(truth(&e["bonusPointMap"]["fuzzyScoreBonus"])
            && number(&e["bonusPointMap"]["fuzzyScoreBonus"]) < 25.0),
        "FUZZY_SCORE_SCALED_EXP" => score
            .filter(|v| v.is_finite() && v.fract() == 0.0)
            .map_or(0.0, |v| 0.5_f64.powf(v.min(10.0))),
        "FUZZY_SCORE_SCALED_FLIPPED" => score
            .filter(|v| v.is_finite() && v.fract() == 0.0)
            .map_or(0.0, |v| 10.0 - v.min(10.0)),
        "FUZZY_SCORE_SCALED_SUBTRACT" => score
            .filter(|v| v.is_finite() && v.fract() == 0.0)
            .map_or(0.0, |v| v.min(10.0) - 1.0),
        "NAV_EXACT_MATCH" => yes(!nav_name.is_empty() && nav_name == query),
        "NAV_EXACT_PREFIX_MATCH" => yes(prefix(nav_name)),
        "NAV_NORMAL_MATCH" => yes(!nav_name.is_empty() && normal(nav_name, false)?),
        "NAV_NORMAL_PREFIX_MATCH" => yes(!nav_name.is_empty() && normal(nav_name, true)?),
        "NAV_NORMAL_MATCH_NO_USER" => yes(!no_user.is_empty() && normal(no_user, false)?),
        "NAV_NORMAL_PREFIX_MATCH_NO_USER" => yes(!no_user.is_empty() && normal(no_user, true)?),
        "NAV_CONTAINS_QUERY_NORMALIZED" => {
            let mut hit = false;
            if matches!(kind, "channel-navigation" | "user-navigation") {
                for s in [name, text(&item["name"])] {
                    for line in s.split('\n') {
                        let x = norm(line, n)?;
                        hit |= !x.is_empty() && !nq.is_empty() && x.contains(nq);
                    }
                }
            }
            yes(hit)
        }
        "IS_FLANNEL_FUZZY_PASSTHROUGH" => bit("isFlannelFuzzyPassthrough"),
        "NAV_QUERY_LENGTH" => {
            if nav_name.is_empty() {
                0.0
            } else {
                nq.encode_utf16().count() as f64
            }
        }
        "NAV_TEXT_LENGTH" => nav_name.encode_utf16().count() as f64,
        "NAV_QUERY_RATIO_LENGTH_STRING" => ratio(nav_name),
        "EXACT_MATCH_LOG_RESULTS" => bit("isExactMatch") * log(field("numResults")),
        "USER_ENTITY_TYPE_CTR_30DD" => field("entityTypeCtr"),
        "NUM_RESULTS" => field("numResults"),
        "TYPE_NAVIGATION_USER" => yes(member),
        "TYPE_NAVIGATION_CHANNEL" => yes(channel),
        "TYPE_NAVIGATION_MPIM" => yes(mpim),
        "USER_PRIORITY" => {
            if member && im {
                field("priority")
            } else {
                0.0
            }
        }
        "USER_IS_ACTIVE" => yes(member && flag("active")),
        "USER_IS_EXTERNAL" => yes(member && flag("external")),
        "USER_IS_CURRENT_USER" => yes(member && flag("self")),
        "MEMBER_OF_CURRENT_CHANNEL" => bit("isMemberOfCurrentChannel"),
        "NOT_MEMBER_OF_CURRENT_CHANNEL" => yes(e["isMemberOfCurrentChannel"] == Value::Bool(false)),
        "CURRENT_USER_TEXTY" => yes(member && flag("self") && source == "texty-autocomplete"),
        "USER_SAME_TEAM" => yes(member && truth(&e["sameTeam"])),
        "USER_IS_BOT" => yes(member && flag("bot")),
        "IS_SLACKBOT" => yes(member && flag("slackbot")),
        "IS_USLACK" => yes(member && flag("uslack")),
        "IS_NON_SLACKBOT_BOT" => yes(member && flag("bot") && !flag("slackbot")),
        "IS_NON_USLACK_BOT" => yes(member && flag("bot") && !flag("uslack")),
        "IS_SLACKBOT_DM_AGE_DECAY_2H"
        | "IS_SLACKBOT_DM_AGE_DECAY_7D"
        | "IS_SLACKBOT_DM_AGE_DECAY_30D" => {
            if member && flag("slackbot") {
                decay(now, read, half)
            } else {
                0.0
            }
        }
        "IS_USLACK_DM_AGE_DECAY_2H"
        | "IS_USLACK_DM_AGE_DECAY_7D"
        | "IS_USLACK_DM_AGE_DECAY_30D" => {
            if member && flag("uslack") {
                decay(now, read, half)
            } else {
                0.0
            }
        }
        "USER_IS_DEACTIVATED" => yes(member && !flag("active")),
        "USER_IS_IN_CURRENT_THREAD" => yes(member && truth(&e["isMemberOfCurrentThread"])),
        // Workflow directory matches never have query-completion filter tokens.
        "MEMBER_FILTER_USER_IN_CHANNEL" | "FROM_AND_CHANNEL_FILTERS_USER_IN_CHANNEL" => 0.0,
        "USER_DM_IS_UNREAD" => yes(member && im && truth(&e["imIsUnread"])),
        "USER_DM_IS_STARRED" => yes(member && im && truth(&e["imIsStarred"])),
        "USER_DM_READ_AGE_DECAY_7D" | "USER_DM_READ_AGE_DECAY_2H" => {
            if member && im {
                decay(now, parse_float(&e["lastReadTs"]), half)
            } else {
                0.0
            }
        }
        "USER_DM_READ_AGE_DECAY_7D_V2" | "USER_DM_READ_AGE_DECAY_2H_V2" => {
            if member && im {
                decay(now, parse_float(&e["lastReadImTs"]), half)
            } else {
                0.0
            }
        }
        "USER_DM_LATEST_MSG_AGE_DECAY_7D" | "USER_DM_LATEST_MSG_AGE_DECAY_2H" => {
            if member && im {
                decay(now, parse_float(&e["latestTs"]), half)
            } else {
                0.0
            }
        }
        "USER_DM_LATEST_MSG_AGE_DECAY_7D_V2" | "USER_DM_LATEST_MSG_AGE_DECAY_2H_V2" => {
            if member && im {
                decay(now, parse_float(&e["latestImTs"]), half)
            } else {
                0.0
            }
        }
        "USER_DM_IS_FROZEN" => yes(member && im && truth(&e["im"]["facts"]["frozen"])),
        "USER_DM_HAS_DRAFT" => bit("imHasDraft"),
        "USER_DM_NULL" => yes(member && !im),
        "USER_DM_EMPTY" => yes(member && im && !valid(parse_float(&e["latestTs"]))),
        "USER_DM_EMPTY_V2" => yes(member && im && !valid(parse_float(&e["latestImTs"]))),
        "USER_DM_NOT_OPEN" => yes(member && im && !truth(&e["im"]["facts"]["open"])),
        "USER_DM_OPEN" => yes(member && im && truth(&e["im"]["facts"]["open"])),
        "USER_DM_LATEST_OLDER_30D" | "USER_DM_LATEST_OLDER_90D" => {
            yes(member && older(now, parse_float(&e["latestTs"]), half))
        }
        "USER_DM_LATEST_OLDER_30D_V2" | "USER_DM_LATEST_OLDER_90D_V2" => {
            yes(member && older(now, parse_float(&e["latestImTs"]), half))
        }
        "USER_HAS_DMED" => yes(member && valid(read)),
        "USER_DM_READ_AGE_DECAY_2H_V3" | "USER_DM_READ_AGE_DECAY_7D_V3" => {
            if member {
                decay(now, read, half)
            } else {
                0.0
            }
        }
        "USER_DM_READ_OLDER_7D" | "USER_DM_READ_OLDER_30D" | "USER_DM_READ_OLDER_90D" => {
            yes(member && older(now, read, half))
        }
        "USER_QUERY_RATIO_LENGTH_STRING"
        | "CHANNEL_QUERY_RATIO_LENGTH_STRING"
        | "MPIM_QUERY_RATIO_LENGTH_STRING" => ratio(name),
        "NAV_NORMAL_PREFIX_MATCH_USER" => {
            yes(!name.is_empty() && normal(name, true)? && !normal(text(&f["firstName"]), true)?)
        }
        "NAV_NORMAL_MATCH_USER" => {
            yes(!name.is_empty() && normal(name, false)? && !normal(text(&f["firstName"]), true)?)
        }
        "USER_FIRST_NAME_NORMAL_MATCH" | "USER_LAST_NAME_NORMAL_MATCH" => {
            let s = text(
                &f[if key.contains("FIRST") {
                    "firstName"
                } else {
                    "lastName"
                }],
            );
            yes(!s.is_empty() && normal(s, false)?)
        }
        "USER_FIRST_NAME_NORMAL_PREFIX_MATCH" | "USER_LAST_NAME_NORMAL_PREFIX_MATCH" => {
            let s = text(
                &f[if key.contains("FIRST") {
                    "firstName"
                } else {
                    "lastName"
                }],
            );
            yes(!s.is_empty() && normal(s, true)?)
        }
        "USER_FIRST_NAME_EXACT_MATCH" | "USER_LAST_NAME_EXACT_MATCH" => {
            let s = text(
                &f[if key.contains("FIRST") {
                    "firstName"
                } else {
                    "lastName"
                }],
            );
            yes(!s.is_empty() && s == query)
        }
        "USER_FIRST_NAME_EXACT_PREFIX_MATCH" | "USER_LAST_NAME_EXACT_PREFIX_MATCH" => {
            yes(prefix(text(
                &f[if key.contains("FIRST") {
                    "firstName"
                } else {
                    "lastName"
                }],
            )))
        }
        "USER_EMAIL_MATCH" => yes(e["scoreDetails"]["email"]
            .as_f64()
            .is_some_and(|v| v >= 0.0)),
        "USER_EMAIL_ONLY_MATCH" => yes(e["scoreDetails"]["email"]
            .as_f64()
            .is_some_and(|v| v >= 0.0)
            && e["scoreDetails"].as_object().is_some_and(|o| o.len() == 1)),
        "QUERY_IS_SELF" => yes(e["scoreDetails"]["self_score"].as_f64() == Some(0.0)),
        "HISTORY_AGE_DECAY_2H" | "HISTORY_AGE_DECAY_7D" => decay(now, field("historyTs"), half),
        "HISTORY_POSITION" => field("historyPosition"),
        "USER_MPIM_READ_AGE_DECAY_7D" | "USER_MPIM_READ_AGE_DECAY_2H" => {
            if member {
                decay(now, number(&e["mpimFeatures"]["lastReadTs"]), half)
            } else {
                0.0
            }
        }
        "USER_MPIM_LATEST_MSG_AGE_DECAY_7D" | "USER_MPIM_LATEST_MSG_AGE_DECAY_2H" => {
            if member {
                decay(now, number(&e["mpimFeatures"]["latestMsgTs"]), half)
            } else {
                0.0
            }
        }
        "USER_MPIM_FRECENCY_LOG_ITEM_COUNT" => {
            if member {
                number(&e["mpimFeatures"]["itemCount"]).ln_1p()
            } else {
                0.0
            }
        }
        "USER_MPIM_FRECENCY_LOG_HIT_COUNT" => {
            if member {
                number(&e["mpimFeatures"]["hitCount"]).ln_1p()
            } else {
                0.0
            }
        }
        "USER_HAS_MPDMED" => yes(member && valid(mread)),
        "USER_MPIM_LOG1P_MEMBERSHIP_COUNT" => {
            if member {
                log(membership)
            } else {
                0.0
            }
        }
        "USER_MPIM_GTE5_MEMBERSHIP_COUNT" => yes(member && membership >= 5.0),
        "USER_MPIM_GTE25_MEMBERSHIP_COUNT" => yes(member && membership >= 25.0),
        "USER_MPIM_READ_AGE_DECAY_7D_V2" | "USER_MPIM_READ_AGE_DECAY_2H_V2" => {
            if member && truth(&e["mpimFeatures"]) {
                decay(now, mread, half)
            } else {
                0.0
            }
        }
        "MEMBER_OF_SEARCHER_DEFAULT_TEAM" => bit("isMemberOfSearcherDefaultTeam"),
        "HAS_WORKSPACE_INTERSECTION_WITH_SEARCHER" => {
            yes(field("numWorkspacesIntersectingWithSearcher") > 0.0)
        }
        "NUM_WORKSPACES_INTERSECTING_WITH_SEARCHER" => {
            field("numWorkspacesIntersectingWithSearcher")
        }
        "USER_AFFINITY_SCORE" => {
            if member {
                field("userAffinityScore")
            } else {
                0.0
            }
        }
        "CHANNEL_IS_MEMBER" => yes(channel && flag("channelMember")),
        "CHANNEL_PRIORITY" => {
            if channel {
                field("priority")
            } else {
                0.0
            }
        }
        "CHANNEL_IS_STARRED" => yes(channel && truth(&e["isStarred"])),
        "CHANNEL_IS_ARCHIVED" => yes(channel && flag("archived")),
        "CHANNEL_IS_OTHER_TEAM" => yes(channel && flag("otherTeam")),
        "CHANNEL_IS_FROZEN" => yes(channel && flag("frozen")),
        "CHANNEL_HAS_DRAFT" => bit("hasDraft"),
        "QUERY_LENGTH_DECAY_NORMAL_MATCH_CHANNEL" => {
            if !name.is_empty() && normal(name, false)? && nq.encode_utf16().count() > 1 {
                2.0_f64.powf(-(nq.encode_utf16().count() as f64))
            } else {
                0.0
            }
        }
        "NAV_NORMAL_MATCH_CHANNEL_LOG_RESULTS" => {
            yes(!name.is_empty() && normal(name, false)?) * log(field("numResults"))
        }
        "CHANNEL_IS_UNREAD" => yes(channel && truth(&e["channelIsUnread"])),
        "CHANNEL_READ_AGE_DECAY_2H"
        | "CHANNEL_READ_AGE_DECAY_7D"
        | "MPIM_READ_AGE_DECAY_2H"
        | "MPIM_READ_AGE_DECAY_7D" => {
            if (key.starts_with("CHANNEL") && channel) || (key.starts_with("MPIM") && mpim) {
                decay(now, parse_float(&e["lastReadTs"]), half)
            } else {
                0.0
            }
        }
        "CHANNEL_LATEST_MSG_AGE_DECAY_2H"
        | "CHANNEL_LATEST_MSG_AGE_DECAY_7D"
        | "MPIM_LATEST_MSG_AGE_DECAY_2H"
        | "MPIM_LATEST_MSG_AGE_DECAY_7D" => {
            if (key.starts_with("CHANNEL") && channel) || (key.starts_with("MPIM") && mpim) {
                decay(now, parse_float(&e["latestTs"]), half)
            } else {
                0.0
            }
        }
        "CHANNEL_IS_PREVIOUS_NAME_EXACT_MATCH" | "CHANNEL_IS_PREVIOUS_NAME_PREFIX_MATCH" => {
            let joined = item["previous_names"]
                .as_array()
                .map(|a| a.iter().map(text).collect::<Vec<_>>().join(","))
                .unwrap_or_default();
            yes(channel
                && !joined.is_empty()
                && joined.split(',').any(|s| {
                    if key.contains("PREFIX") {
                        prefix(s)
                    } else {
                        s == query
                    }
                }))
        }
        "ONLY_MATCHES_PREVIOUS_NAME" => bit("bestPreviousNameMatch"),
        "CHANNEL_IS_NOT_MEMBER" => yes(channel && !flag("channelMember")),
        "CHANNEL_PRIORITY_90D_SCORE" => {
            if channel {
                field("channelPriority90dScore")
            } else {
                0.0
            }
        }
        "CHANNEL_TEAM_CTR_90DD" | "MPIM_TEAM_CTR_90DD" => {
            number(&e["offlineFeatures"]["team_ctr_90dd"])
        }
        "CHANNEL_USER_CTR_30DD" | "MPIM_USER_CTR_30DD" => {
            number(&e["offlineFeatures"]["user_ctr_30dd"])
        }
        "MPIM_IS_STARRED" => yes(mpim && truth(&e["isStarred"])),
        "MPIM_IS_UNREAD" => yes(mpim && truth(&e["channelIsUnread"])),
        "MPIM_WITH_DEACTIVATED_USER" => yes(mpim && truth(&e["isMpimWithDeactivatedMember"])),
        "MPIM_PRIORITY" => {
            if mpim {
                field("priority")
            } else {
                0.0
            }
        }
        "MPIM_MEMBERS" => item["members"].as_array().map_or(0.0, |a| a.len() as f64),
        "MPIM_MEMBER_NAME_MATCHES" => e["scoreDetailsByMember"].as_object().map_or(0.0, |a| {
            a.values()
                .filter(|v| {
                    ["real_name", "display_name"]
                        .iter()
                        .any(|k| v["score_details"][k].as_f64().is_some_and(|v| v >= 0.0))
                })
                .count() as f64
        }),
        "MPIM_MEMBER_ALL_MATCHES" => e["scoreDetailsByMember"]
            .as_object()
            .map_or(0.0, |a| a.len() as f64),
        "MPIM_MATCHER_SCORE" => field("score"),
        "MPIM_MATCHER_SCORE_0" => yes(score == Some(0.0)),
        "MPIM_MATCHER_SCORE_1" => yes(score == Some(1.0)),
        "MPIM_MATCHER_SCORE_2" => yes(score == Some(2.0)),
        "MPIM_MATCHER_SCORE_3" => yes(score == Some(3.0)),
        "MPIM_USERS_MAX_LATEST_MSG_AGE_DECAY_2H" | "MPIM_USERS_MAX_LATEST_MSG_AGE_DECAY_7D" => {
            decay(now, field("maxLatestMsgTsOfAllMembers"), half)
        }
        "MPIM_USERS_AVG_LATEST_MSG_AGE_DECAY_2H" | "MPIM_USERS_AVG_LATEST_MSG_AGE_DECAY_7D" => {
            decay(now, field("avgLatestMsgTsOfAllMembers"), half)
        }
        "MPIM_USERS_MAX_MSG_READ_AGE_DECAY_2H" | "MPIM_USERS_MAX_MSG_READ_AGE_DECAY_7D" => {
            decay(now, field("maxLastReadTsOfAllMembers"), half)
        }
        "MPIM_USERS_AVG_MSG_READ_AGE_DECAY_2H" | "MPIM_USERS_AVG_MSG_READ_AGE_DECAY_7D" => {
            decay(now, field("avgLastReadTsOfAllMembers"), half)
        }
        "MPIM_USERS_PCT_NON_EMPTY_DMS" => field("pctNonEmptyDmsForAllMembers"),
        "MPIM_USERS_PCT_NON_NULL_DMS" => field("pctNonNullDmsForAllMembers"),
        "MPIM_USERS_ALL_INTERNAL" => bit("isMpimWithOnlyInternalMembers"),
        "MPIM_NUM_OTHER_MEMBERS_EQUALS_NUM_QUERY_WORDS" => {
            bit("mpimNumOtherMembersEqualsNumQueryWords")
        }
        "MPIM_ALL_OTHER_MEMBERS_NAMES_HAVE_PREFIX_MATCH" => {
            bit("allOtherMemberNamesHavePrefixMatch")
        }
        _ if kind == "file-navigation" => return super::file_features::value(key, e, now, n),
        _ => return Err(Error::UnsupportedFeature(key.to_owned())),
    };
    Ok(res)
}

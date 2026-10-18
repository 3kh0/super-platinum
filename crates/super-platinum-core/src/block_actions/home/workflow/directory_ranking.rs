//! Workflow directory ranking: 8Bbq -> 4ucY/tyx1 -> l24D/jhqG/ukdy.
//! Pure observations only. These rows never authorize entity selection or Start.
//! `facts` are app-owned selector observations, not fields trusted from Slack wire.
use crate::composer_emoji::{history::History, model::Model, ranking::SimilarityCache};
use serde_json::{Value, json};
use std::cmp::Ordering;
mod features;
mod lists;
mod prepare;
#[cfg(test)]
mod tests;
use features::{number, text, truth};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    InvalidClock,
    MissingObservation(String),
    UnsupportedFeature(String),
    NonFiniteScore,
}
pub type Normalize<'a> = dyn Fn(&str) -> Result<String, Error> + 'a;
/// Model/history/current selectors are a single account/query snapshot owned by
/// the caller. `state` uses the explicit selector observation contract in README.
pub struct Context<'a> {
    pub state: &'a Value,
    pub history: &'a History,
    pub model: &'a Model,
    pub now_ms: f64,
    pub normalize: &'a Normalize<'a>,
}
#[derive(Clone, Debug)]
pub struct Options {
    pub limit: usize,
    pub source: String,
    pub self_first: bool,
    pub flat: bool,
    pub prefer_exact: bool,
    pub prefer_channels_user_belongs_to: bool,
    pub prefer_active_mpims: bool,
    pub demote_passthrough: bool,
    pub current_channel: Option<Value>,
    pub current_thread_users: Option<Vec<String>>,
    pub prefer_current_channel_members: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            limit: 24,
            source: String::new(),
            self_first: false,
            flat: false,
            prefer_exact: true,
            prefer_channels_user_belongs_to: true,
            prefer_active_mpims: true,
            demote_passthrough: false,
            current_channel: None,
            current_thread_users: None,
            prefer_current_channel_members: false,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Ranked {
    pub original_index: usize,
    pub row: Value,
    pub features: Vec<(&'static str, f64)>,
    pub model_score: f64,
}
pub(super) fn js_space(c: char) -> bool {
    matches!(c as u32,9..=13|32|160|5760|8192..=8202|8232|8233|8239|8287|12288|65279)
}
pub(super) fn flag(item: &Value, key: &str) -> bool {
    truth(&item["facts"][key])
}
pub(super) fn string_or<'a>(v: &'a Value, fallback: &'a Value) -> &'a str {
    if truth(v) { text(v) } else { text(fallback) }
}
pub(super) fn format_query(query: &str) -> &str {
    match query.chars().next() {
        Some(
            c @ (':' | '@' | '\u{ff20}' | '#' | '\u{ff03}' | '/' | '\u{ff0f}' | '+' | '\u{ff0b}'),
        ) => &query[c.len_utf8()..],
        _ => query,
    }
}
fn partition_first<T>(items: Vec<T>, predicate: impl Fn(&T) -> bool) -> Vec<T> {
    let (mut a, b): (Vec<_>, Vec<_>) = items.into_iter().partition(predicate);
    a.extend(b);
    a
}
fn comparison_name(row: &Value) -> String {
    let i = &row["item"];
    if flag(i, "mpim") {
        text(&i["name_normalized"]).to_owned()
    } else if truth(&i["facts"]["channelType"]) {
        string_or(&i["facts"]["comparisonName"], &i["name"]).to_lowercase()
    } else if flag(i, "member") {
        string_or(&i["facts"]["comparisonName"], &i["name"]).to_owned()
    } else {
        text(&i["name"]).to_owned()
    }
}
pub(super) fn match_score(row: &Value) -> Option<f64> {
    row.get("score").and_then(|v| {
        if v.is_null() {
            Some(f64::INFINITY)
        } else {
            v.as_f64()
        }
    })
}
fn initial_cmp(a: &Value, b: &Value, collate: &impl Fn(&str, &str) -> Ordering) -> Ordering {
    let am = flag(&a["item"], "mpim");
    let bm = flag(&b["item"], "mpim");
    if am != bm {
        return if am {
            Ordering::Greater
        } else {
            Ordering::Less
        };
    }
    let ai = flag(&a["item"], "im");
    let bi = flag(&b["item"], "im");
    if ai != bi {
        return if ai {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let (Some(ascore), Some(bscore)) = (match_score(a), match_score(b)) else {
        return Ordering::Equal;
    };
    let difference = ascore - bscore;
    if difference != 0.0 {
        return difference.partial_cmp(&0.0).unwrap_or(Ordering::Equal);
    }
    if am && bm {
        let c = (number(&a["item"]["facts"]["memberCount"])
            - number(&b["item"]["facts"]["memberCount"]))
        .partial_cmp(&0.0)
        .unwrap_or(Ordering::Equal);
        if c != Ordering::Equal {
            return c;
        }
    }
    collate(text(&a["name"]), text(&b["name"]))
}
pub(super) fn bonus_sum(v: &Value) -> f64 {
    [
        "isMpim",
        "isMpimWithDeactivatedMember",
        "isExactMatch",
        "fuzzyScoreBonus",
        "onlyMatchesPreviousName",
        "isMemberOfThisChannel",
        "isStarred",
        "isSelf",
        "isArchived",
        "isNotChannelMember",
        "isChannelFromAnotherTeam",
        "isDeactivatedMember",
        "userAffinity",
        "channelPriority90D",
    ]
    .iter()
    .map(|k| number(&v[*k]))
    .sum()
}
fn bonus(
    query: &str,
    row: &Value,
    context: &Context<'_>,
    options: &Options,
    cache: &mut SimilarityCache,
) -> Value {
    let item = &row["item"];
    let s = context.state;
    let mut map = serde_json::Map::new();
    let mut set = |k: &str, v: f64| {
        map.insert(k.to_owned(), json!(v));
    };
    if flag(item, "mpim") {
        set("isMpim", 0.0);
        if options.prefer_active_mpims
            && item["members"].as_array().is_some_and(|a| {
                a.iter().any(|id| {
                    let m = &s["members"][text(id)];
                    flag(m, "deleted") || flag(m, "deactivated")
                })
            })
        {
            set("isMpimWithDeactivatedMember", -50.0)
        }
        return Value::Object(map);
    }
    if truth(&row["isExactMatch"]) {
        set("isExactMatch", 100.0)
    } else if !flag(item, "archived") && !flag(item, "deleted") && !flag(item, "deactivated") {
        let base = text(&item["name"]);
        let fuzzy = if flag(item, "member") {
            let real = text(&item["_real_name_normalized_lc"]);
            let display = text(&item["_display_name_normalized_lc"]);
            let display = if display.trim_matches(js_space).is_empty() {
                base
            } else {
                display
            };
            let real = if real.trim_matches(js_space).is_empty() {
                base
            } else {
                real
            };
            let a = cache.similarity(query, real);
            if real == display {
                a
            } else {
                a.max(cache.similarity(query, display))
            }
        } else {
            cache.similarity(query, base)
        };
        set("fuzzyScoreBonus", fuzzy * 50.0);
    }
    if truth(&row["bestPreviousNameMatch"]) {
        set("onlyMatchesPreviousName", -50.0)
    }
    if options.prefer_current_channel_members
        && truth(&item["presence"])
        && options.current_channel.is_some()
        && flag(item, "currentChannelMember")
    {
        set("isMemberOfThisChannel", 25.0);
    }
    if flag(item, "conversation") && truth(&s["metadata"][text(&item["id"])]["starred"]) {
        set("isStarred", 10.0)
    }
    if flag(item, "member") {
        let im = &s["ims"][text(&item["id"])];
        if truth(im) && truth(&s["metadata"][text(&im["id"])]["starred"]) {
            set("isStarred", 10.0)
        }
    }
    if options.self_first && flag(item, "self") {
        set("isSelf", 250.0)
    }
    if flag(item, "channel") || flag(item, "privateChannel") {
        if flag(item, "archived") {
            set("isArchived", -50.0)
        }
        if flag(item, "channel")
            && options.prefer_channels_user_belongs_to
            && !flag(item, "archived")
            && !flag(item, "channelMember")
        {
            set("isNotChannelMember", -25.0)
        }
        if truth(&s["enterprise"]) && flag(item, "otherTeam") {
            set("isChannelFromAnotherTeam", -15.0)
        }
    }
    if flag(item, "member") && (flag(item, "deleted") || flag(item, "deactivated")) {
        set("isDeactivatedMember", -50.0)
    }
    if s["experiments"]["autocomplete_affinity_score"] == "on"
        && truth(&s["teamId"])
        && truth(&s["offline"])
    {
        if flag(item, "member") && number(&s["affinity"][text(&row["id"])]) > 0.0 {
            set("userAffinity", 50.0)
        }
        if flag(item, "conversation") && number(&s["channelPriority"][text(&row["id"])]) > 0.0 {
            set("channelPriority90D", 50.0)
        }
    }
    Value::Object(map)
}
/// Rows carry exact matcher metadata plus an `item` with explicitly captured
/// `facts`. Groups are in original matching order. No eligibility/filtering here.
/// Numeric collation must be captured from the active renderer locale.
pub fn rank(
    query: &str,
    groups: &Value,
    options: &Options,
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    collate: impl Fn(&str, &str) -> Ordering,
) -> Result<Vec<Ranked>, Error> {
    Ok(rank_observed(query, groups, options, context, cache, collate)?.rows)
}
/// Preserve the exact channel latest-selector reads made on the model shortlist.
/// This is an observation for the owning consumer, never selection authority.
pub struct Observation {
    pub rows: Vec<Ranked>,
    pub channel_latest_ids: Vec<String>,
}
pub fn rank_observed(
    query: &str,
    groups: &Value,
    options: &Options,
    context: &Context<'_>,
    cache: &mut SimilarityCache,
    collate: impl Fn(&str, &str) -> Ordering,
) -> Result<Observation, Error> {
    if !context.now_ms.is_finite() {
        return Err(Error::InvalidClock);
    }
    if context.state["selfId"].as_str().is_none_or(str::is_empty) {
        return Err(Error::InvalidInput);
    }
    if groups.as_object().is_none_or(|groups| {
        groups
            .keys()
            .any(|k| !matches!(k.as_str(), "members" | "channels" | "mpims" | "ims"))
    }) {
        return Err(Error::InvalidInput);
    }
    let mut rows = Vec::new();
    let mut buckets = [Vec::new(), Vec::new(), Vec::new()];
    for (group, kind) in [
        ("members", "member"),
        ("channels", "channel"),
        ("mpims", "mpim"),
        ("ims", "ims"),
    ] {
        if let Some(values) = groups.get(group) {
            let values = values.as_array().ok_or(Error::InvalidInput)?;
            for row in values {
                if !row.is_object()
                    || !row["item"].is_object()
                    || !row["item"]["facts"].is_object()
                    || row["item"]["id"].as_str().is_none()
                    || row["id"].as_str().is_none()
                    || row.get("lastSuggestionToken").is_some()
                    || row
                        .get("isMemberFilterAndHasPreviousChannelFilter")
                        .is_some()
                {
                    return Err(Error::InvalidInput);
                }
                if row
                    .get("score")
                    .is_some_and(|x| !x.is_number() && !x.is_null())
                {
                    return Err(Error::InvalidInput);
                }
                let mut r = row.clone();
                r["type"] = json!(kind);
                r["_originalIndex"] = json!(rows.len());
                let b = if group == "channels" {
                    if flag(&r["item"], "archived") {
                        2
                    } else if flag(&r["item"], "channelMember") {
                        0
                    } else {
                        1
                    }
                } else {
                    0
                };
                buckets[b].push(r.clone());
                rows.push(r);
            }
        }
    }
    let shortlist = options.limit.saturating_mul(3).min(500);
    let model_only = rows.len() < shortlist;
    for bucket in &mut buckets {
        if !options.flat {
            for r in &mut *bucket {
                r["name"] = json!(comparison_name(r));
            }
            bucket.sort_by(|a, b| initial_cmp(a, b, &collate));
        }
    }
    let mut ordered: Vec<_> = buckets.into_iter().flatten().collect();
    // flat=true deliberately skips precomputed names, exactly as 4ucY does.
    if options.flat {
        ordered.sort_by(|a, b| initial_cmp(a, b, &collate));
    }
    if options.prefer_exact {
        ordered = partition_first(ordered, |r| truth(&r["isExactMatch"]));
    }
    let normalized = (context.normalize)(format_query(query))?;
    let ids: Vec<_> = ordered.iter().map(|r| text(&r["item"]["id"])).collect();
    let stats = context
        .history
        .score_normalized(&normalized, &ids, context.now_ms);
    for (r, h) in ordered.iter_mut().zip(stats) {
        r["rawFrecencyScore"] = json!(h.raw);
        r["hitCount"] = json!(h.hit_count);
        r["itemCount"] = json!(h.item_count);
        r["weightHitVisits"] = json!(h.hit_weight);
        r["weightItemVisits"] = json!(h.item_weight);
        r["logHitCountScaled"] = json!(h.log_hit_scaled);
        r["logItemCountScaled"] = json!(h.log_item_scaled);
        r["latestItemVisit"] = json!(h.latest_item);
        r["latestHitVisit"] = json!(h.latest_hit);
        for (i, suffix) in ["4h", "1d", "3d", "1w", "1m", "90d"].iter().enumerate() {
            r[format!("hitVisits{suffix}")] = json!(h.hit_buckets[i]);
            r[format!("itemVisits{suffix}")] = json!(h.item_buckets[i]);
        }
        let bonuses = bonus(query, r, context, options, cache);
        let sum = bonus_sum(&bonuses);
        let score = h.raw + sum;
        if !score.is_finite() {
            return Err(Error::NonFiniteScore);
        }
        r["bonusPointMap"] = bonuses;
        r["frecencyScore"] = json!(score);
    }
    if !model_only {
        ordered.sort_by(|a, b| number(&b["frecencyScore"]).total_cmp(&number(&a["frecencyScore"])));
    }
    if options.self_first && !ordered.first().is_some_and(|r| truth(&r["isSelf"])) {
        ordered = partition_first(ordered, |r| truth(&r["isSelf"]));
    }
    ordered.truncate(shortlist);
    let prepared = prepare::suggestions(query, &normalized, ordered, context, options)?;
    let channel_latest_ids = prepared
        .iter()
        .filter(|row| row["suggestionType"] == "channel-navigation")
        .map(|row| text(&row["id"]).to_owned())
        .collect();
    let mut feature_vectors = std::collections::BTreeMap::new();
    for row in &prepared {
        feature_vectors.insert(
            text(&row["id"]).to_owned(),
            features::vector(
                row,
                context.now_ms / 1000.0,
                &options.source,
                context.normalize,
            )?,
        );
    }
    let mut results = Vec::new();
    for row in prepared {
        // l24D featureVectors is keyed by id; duplicate rows use the last vector.
        let features = feature_vectors
            .get(text(&row["id"]))
            .ok_or(Error::InvalidInput)?
            .clone();
        // Current ukdy unconditionally adds -1 to USER_IS_EXTERNAL; the shared
        // emoji model has no such feature and remains unchanged.
        let score = context
            .model
            .score_with_adjustment(&features, &options.source, |name| {
                if name == "USER_IS_EXTERNAL" {
                    -1.0
                } else {
                    0.0
                }
            });
        if !score.is_finite() {
            return Err(Error::NonFiniteScore);
        }
        results.push(Ranked {
            original_index: row["_originalIndex"].as_u64().ok_or(Error::InvalidInput)? as usize,
            row,
            features,
            model_score: score,
        });
    }
    results.sort_by(|a, b| b.model_score.total_cmp(&a.model_score));
    let tail = results.split_off(results.len().min(2));
    results.extend(partition_first(tail, |r| {
        truth(&r.row["isExactMatch"]) || truth(&r.row["bestNameIsExactMatch"])
    }));
    if options.demote_passthrough
        && results
            .iter()
            .any(|r| truth(&r.row["isFlannelFuzzyPassthrough"]))
    {
        let normalized = (context.normalize)(query)?;
        let q = normalized.strip_prefix(['@', '#']).unwrap_or(&normalized);
        if !q.is_empty() {
            let mut last = None;
            for (i, r) in results.iter().enumerate() {
                if truth(&r.row["isFlannelFuzzyPassthrough"]) {
                    continue;
                }
                for s in [text(&r.row["name"]), text(&r.row["item"]["name"])] {
                    if !s.is_empty() {
                        let n = (context.normalize)(s)?;
                        if n.strip_prefix(['@', '#']).unwrap_or(&n).contains(q) {
                            last = Some(i);
                        }
                    }
                }
            }
            if let Some(last) = last {
                let mut head = Vec::new();
                let mut tail = Vec::new();
                for (i, r) in results.into_iter().enumerate() {
                    if i <= last && !truth(&r.row["isFlannelFuzzyPassthrough"]) {
                        head.push(r)
                    } else {
                        tail.push(r)
                    }
                }
                head.extend(tail);
                results = head;
            }
        }
    }
    if options.self_first && !results.first().is_some_and(|r| truth(&r.row["isSelf"])) {
        results = partition_first(results, |r| truth(&r.row["isSelf"]));
    }
    results.truncate(options.limit);
    Ok(Observation {
        rows: results,
        channel_latest_ids,
    })
}

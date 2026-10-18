//! l24D metadata projection over the selected model shortlist, not the full set.
use super::features::{number, parse_float, text, truth};
use super::{Context, Error, Options, flag, string_or};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn at<'a>(state: &'a Value, item: &Value) -> &'a Value {
    &state["metadata"][text(&item["id"])]
}
fn finite(v: f64) -> Value {
    serde_json::Number::from_f64(v).map_or(Value::Null, Value::Number)
}
fn members(item: &Value) -> Vec<&str> {
    item["members"]
        .as_array()
        .map_or_else(Vec::new, |a| a.iter().map(text).collect())
}
fn copy(e: &mut Value, key: &str, value: &Value) {
    if !value.is_null() {
        e[key] = value.clone();
    }
}
fn nonzero(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}
fn sum_field(e: &mut Value, key: &str, n: f64) {
    e[key] = finite(number(&e[key]) + n);
}
fn max_field(e: &mut Value, key: &str, n: f64) {
    e[key] = finite(if truth(&e[key]) {
        number(&e[key]).max(n)
    } else {
        n
    });
}

pub(super) fn suggestions(
    query: &str,
    normalized: &str,
    rows: Vec<Value>,
    context: &Context<'_>,
    options: &Options,
) -> Result<Vec<Value>, Error> {
    let s = context.state;
    let self_id = string_or(&s["selfId"], &Value::Null).to_owned();
    let self_id = if self_id.is_empty() {
        "U_SELF"
    } else {
        &self_id
    };
    let offline = truth(&s["teamId"]) && truth(&s["offline"]);
    let affinity = s["experiments"]["autocomplete_affinity_ml_model"] == "on";
    let count = rows.len();
    let mut cross: BTreeMap<String, Value> = BTreeMap::new();
    for row in &rows {
        let item = &row["item"];
        if row["type"] == "mpim" || flag(item, "mpim") {
            for id in members(item).into_iter().filter(|id| *id != self_id) {
                let e = cross
                    .entry(id.to_owned())
                    .or_insert_with(|| json!({"itemCount":0,"hitCount":0,"membershipCount":0}));
                let md = at(s, item);
                let read = parse_float(&md["lastRead"]);
                let latest = parse_float(&md["latest"]);
                if nonzero(read) {
                    max_field(e, "lastReadTs", read)
                }
                if nonzero(latest) {
                    max_field(e, "latestMsgTs", latest)
                }
                sum_field(e, "hitCount", number(&row["hitCount"]));
                sum_field(e, "itemCount", number(&row["itemCount"]));
                sum_field(e, "membershipCount", 1.0);
            }
        }
    }
    let mut result = Vec::new();
    for mut e in rows {
        let item = e["item"].clone();
        let id = text(&item["id"]);
        let md = at(s, &item);
        e["id"] = json!(id);
        e["query"] = json!(query);
        e["normalizedQuery"] = json!(normalized);
        e["suggestionType"] = json!("unknown");
        e["numResults"] = json!(count);
        if e["type"] == "member" || flag(&item, "member") {
            let im = &s["ims"][id];
            let imd = at(s, im);
            let remote = if offline {
                &s["remoteDm"][id]
            } else {
                &Value::Null
            };
            let rmpim = if offline {
                &s["remoteMpim"][id]
            } else {
                &Value::Null
            };
            copy(&mut e, "remoteImMetadata", remote);
            copy(&mut e, "remoteMpimMetadata", rmpim);
            e["im"] = im.clone();
            copy(&mut e, "latestImTs", &imd["latest"]);
            copy(&mut e, "imHasDraft", &imd["draft"]);
            copy(&mut e, "imIsStarred", &imd["starred"]);
            e["imIsUnread"] = json!(truth(im) && truth(&imd["unread"]));
            e["name"] = json!(string_or(&item["facts"]["bestName"], &item["name"]));
            copy(&mut e, "lastReadImTs", &imd["lastRead"]);
            copy(
                &mut e,
                "lastReadRemoteImMetadataTs",
                &remote["last_read_abs"],
            );
            copy(
                &mut e,
                "lastReadRemoteMpimMetadataTs",
                &rmpim["last_read_abs"],
            );
            copy(
                &mut e,
                "remoteMpimMembershipCount",
                &rmpim["membership_count"],
            );
            if truth(&s["enterpriseId"]) && s["enterpriseId"] == item["enterprise_id"] {
                let teams = item["enterprise_user"]["teams"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let searcher = s["searcherTeams"].as_array().cloned().unwrap_or_default();
                let common: Vec<_> = searcher.into_iter().filter(|v| teams.contains(v)).collect();
                e["isMemberOfSearcherDefaultTeam"] = json!(common.contains(&s["defaultTeam"]));
                e["numWorkspacesIntersectingWithSearcher"] = json!(common.len());
            }
            if item["facts"]["team"].as_str().unwrap_or("")
                == s["self"]["facts"]["team"].as_str().unwrap_or("")
            {
                e["sameTeam"] = json!(true)
            }
            let channel = options.current_channel.as_ref().unwrap_or(&Value::Null);
            if truth(channel) {
                if flag(channel, "mpim") {
                    e["isMemberOfCurrentChannel"] = json!(members(channel).contains(&id))
                } else if !flag(channel, "im") {
                    e["isMemberOfCurrentChannel"] = json!(flag(&item, "currentChannelMember"))
                }
            }
            if let Some(users) = &options.current_thread_users {
                e["isMemberOfCurrentThread"] = json!(users.iter().any(|x| x == id))
            }
            if affinity && offline {
                copy(&mut e, "userAffinityScore", &s["affinity"][id])
            }
            copy(&mut e, "priority", &imd["priority"]);
            e["suggestionType"] = json!("user-navigation");
            e["isNavigational"] = json!(true);
            copy(
                &mut e,
                "historyTs",
                &s["navigationHistory"][text(&im["id"])]["ts"],
            );
            copy(
                &mut e,
                "historyPosition",
                &s["navigationHistory"][text(&im["id"])]["position"],
            );
            if let Some(v) = cross.get(id) {
                e["mpimFeatures"] = v.clone();
            }
        } else {
            e["name"] = json!(string_or(&item["name"], &item["string"]));
        }
        if e["type"] == "channel" || flag(&item, "conversation") {
            e["suggestionType"] = json!("channel-navigation");
            e["isNavigational"] = json!(true);
            e["isChannelMember"] = json!(
                flag(&item, "channel") && !flag(&item, "archived") && flag(&item, "channelMember")
            );
            for (key, source) in [
                ("hasDraft", "draft"),
                ("latestTs", "latest"),
                ("lastReadTs", "lastRead"),
                ("priority", "priority"),
                ("isStarred", "starred"),
            ] {
                copy(&mut e, key, &md[source]);
            }
            if offline {
                copy(&mut e, "offlineFeatures", &s["offline"]["channel"][id]);
            }
            for key in ["ts", "position"] {
                copy(
                    &mut e,
                    if key == "ts" {
                        "historyTs"
                    } else {
                        "historyPosition"
                    },
                    &s["navigationHistory"][id][key],
                );
            }
            if affinity && offline {
                copy(&mut e, "channelPriority90dScore", &s["channelPriority"][id]);
            }
            e["channelIsUnread"] = json!(truth(&md["unread"]));
        }
        if e["type"] == "mpim" || flag(&item, "mpim") {
            e["suggestionType"] = json!("mpim-navigation");
            e["isNavigational"] = json!(true);
            let ids = members(&item);
            let people: Vec<_> = ids.iter().map(|id| &s["members"][id]).collect();
            e["mpimNumOtherMembersEqualsNumQueryWords"] =
                json!(normalized.split(' ').count() as isize == ids.len() as isize - 1);
            if !normalized.is_empty() {
                let words: Vec<_> = normalized
                    .split(' ')
                    .map(|x| x.trim_matches(super::js_space))
                    .filter(|x| !x.is_empty())
                    .map(str::to_owned)
                    .collect();
                let pairs: Vec<_> = words.chunks(2).map(|p| p.join(" ")).collect();
                let comma: Vec<_> = normalized
                    .split(',')
                    .map(|x| x.trim_matches(super::js_space))
                    .filter(|x| !x.is_empty())
                    .map(str::to_owned)
                    .collect();
                let all = |parts: &[String]| {
                    people
                        .iter()
                        .filter(|p| {
                            !flag(p, "self")
                                && !text(&p["real_name"]).is_empty()
                                && parts.iter().any(|q| {
                                    q.encode_utf16().count() > 1
                                        && text(&p["real_name"])
                                            .to_lowercase()
                                            .starts_with(&q.to_lowercase())
                                })
                        })
                        .count() as isize
                        == ids.len() as isize - 1
                };
                e["allOtherMemberNamesHavePrefixMatch"] =
                    json!(all(&words) || all(&pairs) || all(&comma));
            }
            e["isMpimWithDeactivatedMember"] = json!(
                people
                    .iter()
                    .any(|p| flag(p, "deleted") || flag(p, "deactivated"))
            );
            e["isMpimWithOnlyInternalMembers"] = json!(people.iter().all(|p| !flag(p, "external")));
            copy(&mut e, "hasDraft", &md["draft"]);
            e["name"] = json!(if flag(&item, "customNameAllowed")
                && truth(&item["facts"]["customName"])
            {
                text(&item["facts"]["customName"])
            } else {
                string_or(&item["facts"]["mpimName"], &item["name"])
            });
            if offline {
                copy(&mut e, "offlineFeatures", &s["offline"]["mpim"][id]);
            }
            for key in ["ts", "position"] {
                copy(
                    &mut e,
                    if key == "ts" {
                        "historyTs"
                    } else {
                        "historyPosition"
                    },
                    &s["navigationHistory"][id][key],
                );
            }
            copy(&mut e, "priority", &md["priority"]);
            e["channelIsUnread"] = json!(truth(&md["unread"]));
            e["maxLastReadTsOfAllMembers"] = Value::Null;
            e["maxLatestMsgTsOfAllMembers"] = Value::Null;
            let (mut reads, mut latests, mut nr, mut nl, mut nonnull) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for p in people.iter().filter(|p| text(&p["id"]) != self_id) {
                let pid = text(&p["id"]);
                let im = &s["ims"][pid];
                let imd = at(s, im);
                let remote = if offline {
                    &s["remoteDm"][pid]
                } else {
                    &Value::Null
                };
                let read = if truth(&imd["lastRead"]) {
                    parse_float(&imd["lastRead"])
                } else {
                    number(&remote["last_read_abs"]) / 1e6
                };
                if truth(im) || truth(remote) {
                    nonnull += 1.0;
                    if nonzero(read) {
                        nr += 1.0;
                        reads += read;
                        max_field(&mut e, "maxLastReadTsOfAllMembers", read);
                    }
                    let latest = if truth(im) {
                        parse_float(&imd["latest"])
                    } else {
                        f64::NAN
                    };
                    if nonzero(latest) {
                        nl += 1.0;
                        latests += latest;
                        max_field(&mut e, "maxLatestMsgTsOfAllMembers", latest);
                    }
                }
            }
            e["avgLastReadTsOfAllMembers"] = if reads == 0.0 {
                Value::Null
            } else {
                finite(reads / nr)
            };
            e["avgLatestMsgTsOfAllMembers"] = if latests == 0.0 {
                Value::Null
            } else {
                finite(latests / nl)
            };
            let denom = people.len() as f64 - 1.0;
            e["pctNonEmptyDmsForAllMembers"] = finite(nr / denom * 100.0);
            e["pctNonNullDmsForAllMembers"] = finite(nonnull / denom * 100.0);
            e["membersForEscapeHatchQuery"] = json!(people);
        }
        if e["type"] == "file" {
            super::file_prepare::apply(&mut e, s, offline, affinity, self_id)?;
        }
        if offline {
            let kind = text(&e["type"]).to_owned();
            copy(&mut e, "entityTypeCtr", &s["entityCtr"][kind]);
        }
        result.push(e);
    }
    Ok(result)
}

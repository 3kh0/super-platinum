//! rNCs' atomic local member + user-group lookup. No popup/ListContext authority.
//! Source inventories, selector facts, JS strings and ranking state belong to
//! the original Save. Neither a missing observation nor a failed category is an
//! empty result. Resolver names deliberately differ from VRXO ranking names.
use super::super::prepare::{MentionKind, MentionLookup, MentionNames, MentionSearch};
use crate::block_actions::home::workflow::directory_local as local;
use crate::composer_emoji::ranking::SimilarityCache;
use crate::slack::api::custom::texty::{groups, member_ranking};
pub use member_ranking::{Context, Error};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
mod eligibility;
pub use eligibility::{GroupFacts, MemberFacts};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Default)]
pub struct Flags {
    /// Actual registered client metadata, not enterprise/team inference.
    pub unified_user: Option<bool>,
    /// APWq.v: slackforce_seamless_block_app_membership === "on".
    pub salesforce_blocked: Option<bool>,
    /// Original LuF9/p8lb renderer toggle, not an assignment fallback.
    pub beyond: Option<bool>,
}
pub struct Plan {
    lookup: MentionLookup,
    formatted: String,
    unified_user: bool,
    salesforce_blocked: bool,
    beyond: bool,
}
pub struct Query<'a> {
    pub formatted: &'a str,
    pub prepared: &'a local::PreparedQuery,
    pub maybe_self: Option<bool>,
    pub beyond_boundary_eligible: Option<bool>,
}
/// Original canonical rows and renderer XbX7 derived-name observations. No
/// fields besides the three name projections may differ, including order.
pub struct Members<'a> {
    source: &'a [Value],
    derived: &'a [Value],
}
impl<'a> Members<'a> {
    pub fn capture(source: &'a [Value], derived: &'a [Value]) -> Result<Self, Error> {
        if source.len() != derived.len() {
            return Err(Error::InvalidInput);
        }
        for (raw, row) in source.iter().zip(derived) {
            let mut copy = row.as_object().ok_or(Error::InvalidInput)?.clone();
            if !raw.is_object() || raw["id"].as_str().is_none_or(str::is_empty) {
                return Err(Error::InvalidInput);
            }
            for key in [
                "_real_name_normalized_lc",
                "_display_name_normalized_lc",
                "_email_normalized_lc",
            ] {
                if !row[key].is_string() {
                    return Err(Error::MissingObservation(format!("member name {key}")));
                }
                match raw.get(key) {
                    Some(value) => {
                        copy.insert(key.into(), value.clone());
                    }
                    None => {
                        copy.remove(key);
                    }
                }
            }
            if Value::Object(copy) != *raw {
                return Err(Error::InvalidInput);
            }
        }
        Ok(Self { source, derived })
    }
}
pub struct Matched<'a> {
    lookup: MentionLookup,
    members: Vec<local::Scored<'a>>,
    originals: Vec<&'a Value>,
    groups: Vec<groups::GroupScored<'a>>,
}
impl Matched<'_> {
    pub fn members(&self) -> impl Iterator<Item = &Value> {
        self.originals.iter().copied()
    }
    pub fn groups(&self) -> impl Iterator<Item = &groups::Snapshot> {
        self.groups.iter().map(|row| row.snapshot)
    }
}
pub struct Observation {
    pub search: MentionSearch,
    pub ranked_ids: Vec<String>,
}
fn observed(value: Option<bool>, label: &str) -> Result<bool, Error> {
    value.ok_or_else(|| Error::MissingObservation(label.into()))
}
fn truth(v: &Value) -> bool {
    super::truth(v)
}
impl Plan {
    pub fn capture(lookup: &MentionLookup, flags: Flags) -> Result<Option<Self>, Error> {
        if lookup.kind != MentionKind::Member {
            return Err(Error::InvalidInput);
        }
        if lookup.query.is_empty() {
            return Ok(None);
        }
        let raw = lookup.query.as_str();
        let formatted = match raw.chars().next() {
            Some(
                c @ ('@' | '\u{ff20}' | '#' | '\u{ff03}' | '/' | '\u{ff0f}' | '+' | '\u{ff0b}'),
            ) => &raw[c.len_utf8()..],
            _ => raw,
        };
        Ok(Some(Self {
            lookup: lookup.clone(),
            formatted: formatted.into(),
            unified_user: observed(flags.unified_user, "registered unified-user mode")?,
            salesforce_blocked: observed(flags.salesforce_blocked, "Salesforce member exclusion")?,
            beyond: observed(flags.beyond, "matchBeyondWordBoundaries")?,
        }))
    }
    pub fn formatted_query(&self) -> &str {
        &self.formatted
    }
    pub fn requested(&self) -> Value {
        json!({"query":self.lookup.query,"options":{"entities":{
            "userGroups":{"includeFromAnotherTeam":self.unified_user},
            "members":{"includeSelf":true,"includeSlackbot":false,"excludeSalesforceApp":self.salesforce_blocked}},
            "limit":25,"sort":{"source":"texty-autocomplete"}}})
    }
    pub fn matching<'a>(
        &self,
        query: &Query<'_>,
        rows: &Members<'a>,
        group_rows: &'a [groups::Snapshot],
        member_facts: impl Fn(&Value) -> Result<MemberFacts, Error>,
        group_facts: impl Fn(&groups::Snapshot) -> Result<GroupFacts, Error>,
        group_names: impl Fn(&groups::Snapshot) -> Result<groups::Names, crate::slack::Error>,
    ) -> Result<Matched<'a>, Error> {
        if query.formatted != self.formatted {
            return Err(Error::InvalidInput);
        }
        if query.prepared.normalized.is_empty() {
            // Existing Profile group scorer enables empty-query admission. Do
            // not borrow that behavior for rNCs' nonempty raw query edge case.
            return Err(Error::UnsupportedFeature(
                "Save empty-normalized member query".into(),
            ));
        }
        let mut options = local::Options::workflow(
            &self.formatted,
            &query.prepared.normalized,
            &query.prepared.matcher_normalized,
            observed(query.maybe_self, "localized maybe-self")?,
        );
        options.fuzzy.can_match_beyond_word_boundaries =
            self.beyond && observed(query.beyond_boundary_eligible, "word-boundary eligibility")?;
        let mut eligible = BTreeSet::new();
        let mut sources = HashMap::new();
        for (raw, row) in rows.source.iter().zip(rows.derived) {
            if truth(&row["isUnknown"]) || truth(&row["isNonExistent"]) {
                continue;
            }
            if member_facts(raw)?.allows(self.salesforce_blocked)? {
                eligible.insert(row as *const Value);
                sources.insert(row as *const Value, raw);
            }
        }
        let members = local::filter_members(rows.derived, &options, &BTreeSet::new(), |row| {
            eligible.contains(&(row as *const Value))
        });
        let originals = members
            .iter()
            .map(|row| sources[&(row.item as *const Value)])
            .collect();
        // rNCs has no guest/self category restriction. A rejected group receipt
        // rejects this whole lookup; it never publishes just the member half.
        let mut allowed_groups = HashMap::new();
        for group in group_rows {
            if truth(&group.effective()["isUnknown"]) || truth(&group.effective()["isNonExistent"])
            {
                continue;
            }
            allowed_groups.insert(
                group as *const groups::Snapshot,
                group_facts(group)?.allows(self.unified_user)?,
            );
        }
        let groups = groups::matches(
            query.prepared,
            group_rows,
            |row| {
                Ok(allowed_groups
                    .get(&(row as *const groups::Snapshot))
                    .copied()
                    .unwrap_or(false))
            },
            group_names,
        )
        .map_err(|_| Error::MissingObservation("Save group names".into()))?;
        Ok(Matched {
            lookup: self.lookup.clone(),
            members,
            originals,
            groups,
        })
    }
    /// One original 75/model/25 mixed pass. No exact username postfilter and no
    /// ListContext channel membership, current thread users or category caps.
    pub fn rank(
        &self,
        matched: &Matched<'_>,
        context: &Context<'_>,
        cache: &mut SimilarityCache,
        comparison: impl Fn(&Value) -> Result<String, Error>,
        collate: impl Fn(&str, &str) -> Result<std::cmp::Ordering, Error>,
        demote_passthrough: bool,
    ) -> Result<Observation, Error> {
        if matched.lookup != self.lookup {
            return Err(Error::InvalidInput);
        }
        let ranked = member_ranking::rank_mixed(
            &self.formatted,
            &matched.members,
            &matched.groups,
            context,
            cache,
            comparison,
            collate,
            demote_passthrough,
        )?;
        let mut rows = Vec::new();
        let mut ranked_ids = Vec::new();
        for row in ranked {
            let raw = if row.original_index < matched.originals.len() {
                matched.originals[row.original_index]
            } else {
                matched
                    .groups
                    .get(row.original_index - matched.originals.len())
                    .ok_or(Error::InvalidInput)?
                    .item()
            };
            ranked_ids.push(raw["id"].as_str().ok_or(Error::InvalidInput)?.to_owned());
            rows.push(raw.clone());
        }
        Ok(Observation {
            search: MentionSearch {
                lookup: self.lookup.clone(),
                rows,
            },
            ranked_ids,
        })
    }
}
fn name_string(v: &Value) -> Result<&str, Error> {
    if !truth(v) {
        Ok("")
    } else {
        v.as_str().ok_or(Error::InvalidInput)
    }
}
/// 5cyv.KX/KP -> hWCs, intentionally neither display policy nor ranking name.
pub fn names(raw: &Value) -> Result<MentionNames, Error> {
    if !raw.is_object() || truth(&raw["is_broadcast_keyword"]) {
        return Err(Error::InvalidInput);
    }
    let (primary, secondary) = if truth(&raw["is_usergroup"]) {
        let handle = raw["handle"]
            .as_str()
            .ok_or_else(|| Error::UnsupportedFeature("Save group handle interpolation".into()))?;
        (format!("@{handle}"), String::new())
    } else {
        let display = name_string(&raw["profile"]["display_name"])?;
        let real = name_string(&raw["profile"]["real_name"])?;
        let real = if real.is_empty() {
            name_string(&raw["real_name"])?
        } else {
            real
        };
        (
            format!("@{}", if display.is_empty() { real } else { display }),
            if !display.is_empty() && !real.is_empty() {
                format!("@{real}")
            } else {
                String::new()
            },
        )
    };
    Ok(MentionNames {
        raw: raw.clone(),
        primary,
        secondary,
    })
}

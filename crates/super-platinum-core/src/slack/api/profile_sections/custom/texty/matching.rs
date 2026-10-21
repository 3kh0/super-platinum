//! Ordinary Am1o/fRHe matching over checked canonical rows. Profile and List
//! retain their separate locality boundaries. Special channel-name categories
//! are unsupported; this never adds a server filter or claims full inventory.
pub use super::ranking::Error;
use crate::block_actions::home::workflow::directory_local as local;
use serde_json::Value;
use std::{cell::Cell, collections::BTreeSet};

/// Renderer-owned dfPN comparison (already JS-lowercased for initial sorting)
/// and hNDQ normalized matching name. Neither is inferred from editable text.
pub struct ChannelName {
    pub comparison_lowercase: String,
    pub normalized: String,
}
pub struct Matched<'a> {
    pub(super) scored: local::ChannelScored<'a>,
    pub(super) comparison: String,
}
impl Matched<'_> {
    pub fn raw(&self) -> &Value {
        self.scored.item
    }
    pub fn id(&self) -> &str {
        self.scored.id
    }
    pub fn score(&self) -> Option<u32> {
        self.scored.score
    }
    pub fn exact(&self) -> Option<bool> {
        self.scored.is_exact_match
    }
    pub fn previous_match(&self) -> Option<&str> {
        self.scored.best_previous_name_match
    }
    pub fn passthrough(&self) -> bool {
        self.scored.is_flannel_fuzzy_passthrough
    }
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn private(raw: &Value) -> bool {
    truth(&raw["is_private"]) || truth(&raw["is_group"])
}
/// Explicit supported subset. False is not nonexistence or permission denial.
/// fromAnotherTeam is a canonical derived field, not an inferred credential fact.
pub fn ordinary_local(raw: &Value) -> bool {
    raw["fromAnotherTeam"] == false && ordinary_list_channel(raw)
}
/// Ordinary named List Texty candidates admit another-team rows when locality
/// was actually projected by the canonical store. Missing/ill-typed locality
/// remains unavailable; credentials or wire team IDs are not a substitute.
/// This is a supported category boundary, not an exhaustive Am1o inventory.
pub fn ordinary_list_channel(raw: &Value) -> bool {
    let id = raw["id"].as_str().unwrap_or("");
    raw.is_object()
        && id.len() >= 9
        && matches!(id.as_bytes().first(), Some(b'C' | b'G'))
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && raw["fromAnotherTeam"].is_boolean()
        && (truth(&raw["is_channel"]) || private(raw))
        && [
            "is_im",
            "is_mpim",
            "is_file",
            "is_usergroup",
            "isUnknown",
            "isNonExistent",
        ]
        .iter()
        .all(|key| !truth(&raw[*key]))
        && matches!(local::channel_name_input(raw),Some(local::ChannelNameInput::Ordinary {raw_name:Some(name)}) if !name.is_empty())
        && raw
            .get("properties")
            .is_none_or(|v| v.is_null() || v.is_object())
}
/// Name observations are required even for a currently unmatched row. The
/// caller filters the explicit supported subset before calling this adapter.
/// Matching preserves public-then-private source inventory order and raw rows.
pub fn channels<'a>(
    raw_query: &str,
    query: &local::PreparedQuery,
    rows: &'a [Value],
    remote_ids: &BTreeSet<String>,
    passthrough_observed: bool,
    name: impl Fn(&Value) -> Result<ChannelName, Error>,
    previous: impl Fn(&str) -> Result<String, Error>,
) -> Result<Vec<Matched<'a>>, Error> {
    channels_for(
        raw_query,
        query,
        rows,
        remote_ids,
        passthrough_observed,
        name,
        previous,
        ordinary_local,
    )
}
/// Ordinary interactive List message-input: includePrivate=true,
/// includeFromAnotherTeam=true, searchPreviousNames=true, allowEmptyQuery=true.
/// No Canvas AI context is supplied, hence no AI-exclusion filter is inferred.
pub fn list_channels<'a>(
    raw_query: &str,
    query: &local::PreparedQuery,
    rows: &'a [Value],
    remote_ids: &BTreeSet<String>,
    passthrough_observed: bool,
    name: impl Fn(&Value) -> Result<ChannelName, Error>,
    previous: impl Fn(&str) -> Result<String, Error>,
) -> Result<Vec<Matched<'a>>, Error> {
    channels_for(
        raw_query,
        query,
        rows,
        remote_ids,
        passthrough_observed,
        name,
        previous,
        ordinary_list_channel,
    )
}
fn channels_for<'a>(
    raw_query: &str,
    query: &local::PreparedQuery,
    rows: &'a [Value],
    remote_ids: &BTreeSet<String>,
    passthrough_observed: bool,
    name: impl Fn(&Value) -> Result<ChannelName, Error>,
    previous: impl Fn(&str) -> Result<String, Error>,
    eligible: fn(&Value) -> bool,
) -> Result<Vec<Matched<'a>>, Error> {
    if rows.iter().any(|row| !eligible(row)) {
        return Err(Error::InvalidInput);
    }
    let names = rows.iter().map(&name).collect::<Result<Vec<_>, _>>()?;
    // Pointer identity keeps duplicate IDs distinct without rescanning the catalog.
    let indexes: std::collections::HashMap<*const Value, usize> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row as *const Value, index))
        .collect();
    let mut normalized_previous = std::collections::BTreeMap::new();
    for row in rows {
        if let Some(v) = row.get("previous_names").filter(|v| !v.is_null()) {
            for value in v.as_array().ok_or(Error::InvalidInput)? {
                let value = value.as_str().ok_or(Error::InvalidInput)?;
                if !value.starts_with("mpdm-") {
                    normalized_previous.insert(value.to_owned(), previous(value)?);
                }
            }
        }
    }
    let options = local::Options {
        raw_query,
        normalized_query: &query.normalized,
        matcher_query: &query.matcher_normalized,
        query_is_maybe_self: false,
        allow_empty_query: true,
        prefer_exact_match: true,
        avoid_exact_match_for_members: true,
        self_first: false,
        match_email: false,
        email_forward_only: false,
        passthrough_remote_fuzzy_filtering: passthrough_observed,
        fuzzy: Default::default(),
    };
    let missing = Cell::new(false);
    let mut result = Vec::new();
    for want_private in [false, true] {
        let matched = local::filter_channels(
            rows,
            &options,
            true,
            remote_ids,
            |row| private(row) == want_private,
            |row| {
                indexes
                    .get(&(row as *const Value))
                    .map(|&i| names[i].normalized.clone())
                    .unwrap_or_else(|| {
                        missing.set(true);
                        String::new()
                    })
            },
            |value| {
                normalized_previous.get(value).cloned().unwrap_or_else(|| {
                    missing.set(true);
                    String::new()
                })
            },
        );
        for scored in matched {
            let index = *indexes
                .get(&(scored.item as *const Value))
                .ok_or(Error::InvalidInput)?;
            result.push(Matched {
                scored,
                comparison: names[index].comparison_lowercase.clone(),
            });
        }
    }
    if missing.get() {
        return Err(Error::MissingObservation(
            "channel name normalization".into(),
        ));
    }
    Ok(result)
}

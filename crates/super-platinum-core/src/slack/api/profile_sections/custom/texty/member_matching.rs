//! aD6p -> fRHe/hHTo Profile member matching, with the caller's observed names.
//! Eligibility is separate (member_policy); no Activity self/deleted filtering.
use crate::block_actions::home::workflow::directory_local as local;
pub use crate::block_actions::home::workflow::directory_ranking::Error;
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
pub(super) fn validate(row: &Value) -> Result<(), Error> {
    if !row.is_object()
        || !row["id"]
            .as_str()
            .is_some_and(super::member_policy::member_id)
        || [
            "is_im",
            "is_mpim",
            "is_channel",
            "is_group",
            "is_private",
            "is_file",
            "is_usergroup",
            "isUnknown",
            "isNonExistent",
        ]
        .iter()
        .any(|key| truth(&row[*key]))
    {
        return Err(Error::InvalidInput);
    }
    for key in ["_real_name_normalized_lc", "_display_name_normalized_lc"] {
        if !row[key].is_string() {
            return Err(Error::MissingObservation(format!("member name {key}")));
        }
    }
    Ok(())
}
/// `rows` retain canonical raw identity and normalized names captured under the
/// current renderer/name-policy owner. The predicate is the observed Profile
/// eligibility decision, not a remote result or matching-score permission.
/// Unknown/nonexistent rows are excluded before requiring name observations.
pub fn members<'a>(
    raw_query: &str,
    query: &local::PreparedQuery,
    rows: &'a [Value],
    remote_ids: &BTreeSet<String>,
    passthrough_observed: bool,
    query_is_maybe_self: bool,
    eligible: impl Fn(&Value) -> bool,
) -> Result<Vec<local::Scored<'a>>, Error> {
    let mut admitted = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        if row.is_null()
            || truth(&row["isUnknown"])
            || truth(&row["isNonExistent"])
            || !eligible(row)
        {
            continue;
        }
        validate(row)?;
        admitted.insert(index);
    }
    let indexes: std::collections::HashMap<*const Value, usize> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row as *const Value, index))
        .collect();
    Ok(local::filter_members(
        rows,
        &local::Options {
            raw_query,
            normalized_query: &query.normalized,
            matcher_query: &query.matcher_normalized,
            query_is_maybe_self,
            allow_empty_query: true,
            prefer_exact_match: true,
            avoid_exact_match_for_members: true,
            self_first: false,
            match_email: false,
            email_forward_only: false,
            passthrough_remote_fuzzy_filtering: passthrough_observed,
            fuzzy: Default::default(),
        },
        remote_ids,
        |row| {
            indexes
                .get(&(row as *const Value))
                .is_some_and(|i| admitted.contains(i))
        },
    ))
}

#[cfg(test)]
mod tests;

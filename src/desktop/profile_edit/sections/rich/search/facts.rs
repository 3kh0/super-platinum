//! Pure Profile request facts from actual gantry and shared offline observations.
//! The Search caller owns transport, query, cache identity and currentness.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use super_platinum_core::{
    block_actions::{home::workflow::directory_observations::Offline, shortcuts::grid},
    config::WorkspaceSession,
    slack::{Error, api::custom::texty::channels::RequestFacts},
};

#[cfg(test)]
mod tests;

fn unavailable() -> Error {
    Error::Api("profile_channel_request_facts_unavailable".into())
}

/// An absent/null default is meaningful only inside this complete, correctly
/// identified response. Session credentials never substitute for membership.
/// `expected_internal` is optional independently observed membership, not the
/// active workspace or a list manufactured from available session credentials.
pub(super) fn validate_gantry(
    raw: &Value,
    owner: &WorkspaceSession,
    expected_internal: Option<&[String]>,
) -> Result<Option<String>, Error> {
    if raw.get("error").is_some_and(|value| !value.is_null()) {
        return Err(unavailable());
    }
    grid::Membership::decode(raw.clone(), owner)?;
    if let Some(expected) = expected_internal {
        let actual = raw["workspaces"]
            .as_array()
            .ok_or_else(unavailable)?
            .iter()
            .filter(|row| row["allow_external_users"] != true)
            .map(|row| row["id"].as_str().ok_or_else(unavailable))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if actual != expected.iter().map(String::as_str).collect() {
            return Err(unavailable());
        }
    }
    // Same narrow complete-response contract as the installed gantry name
    // observers. Do not fill the active team or silently accept another shape.
    match raw.get("default_workspace") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(id)) if id.starts_with('T') && !id.is_empty() => Ok(Some(id.clone())),
        _ => Err(unavailable()),
    }
}

/// `offline` must come from the caller's actual current shared cache observation.
/// Its source-derived cold [] is preserved. Missing original property order is
/// an error, never an omitted hint or a newly sorted map of loaded channels.
pub(super) fn request_facts(
    raw: &Value,
    owner: &WorkspaceSession,
    expected_internal: Option<&[String]>,
    offline: &Offline,
) -> Result<RequestFacts, Error> {
    let default_workspace = validate_gantry(raw, owner, expected_internal)?;
    let top = offline
        .processed_top_channels()
        .map_err(|_| unavailable())?;
    Ok(RequestFacts {
        top_channels: Some(json!(top)),
        default_workspace,
    })
}

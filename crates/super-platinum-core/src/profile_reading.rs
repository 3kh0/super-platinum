//! Bounded same-team human reading projection, never a canonical member or grant.
//! Source: current-global-threads-profile-admission/flannel-info, 2026-10-31.
//! Only the original native runner can supply request/owner/occurrence custody.
use crate::slack::models::User;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

const UNAVAILABLE: &str = "This member is not available for this read-only profile.";

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 255 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

#[derive(Clone)]
pub struct Projection {
    user: User,
}
impl Projection {
    /// This is classification of a genuinely original raw member occurrence.
    /// Calling it on a DTO does not create native lineage or SourceReady.
    pub fn from_raw(raw: &Value, id: &str, team: &str) -> Result<Self, &'static str> {
        if !valid_id(id)
            || !valid_id(team)
            || !raw.is_object()
            || raw.get("id").and_then(Value::as_str) != Some(id)
            || raw.get("team_id").and_then(Value::as_str) != Some(team)
            || raw.get("enterprise_user") != Some(&Value::Null)
            || raw
                .get("name")
                .and_then(Value::as_str)
                .is_none_or(|name| name.is_empty() || name == id)
        {
            return Err(UNAVAILABLE);
        }
        for key in ["is_bot", "is_app_user", "deleted"] {
            if raw.get(key) != Some(&json!(false)) {
                return Err(UNAVAILABLE);
            }
        }
        // A stricter native refusal, rather than erasing an incoming sentinel.
        for key in ["isUnknown", "isNonExistent"] {
            if raw
                .get(key)
                .is_some_and(|v| !v.is_null() && v != &json!(false))
            {
                return Err(UNAVAILABLE);
            }
        }
        // p6lZ's explicit boolean branch; never default its stateful fallback.
        let can_interact = raw
            .get("can_interact")
            .and_then(Value::as_bool)
            .ok_or(UNAVAILABLE)?;
        let mut user: User = serde_json::from_value(raw.clone()).map_err(|_| UNAVAILABLE)?;
        user.is_stranger = !can_interact;
        Ok(Self { user })
    }
    pub fn user(&self) -> &User {
        &self.user
    }
}

/// Successful original HTTP/transport ownership stays outside this parser.
/// Missing ids remain missing. No predecessor, empty cache or zero revision is
/// used as knownness. Ambiguous/unrequested rows and pending envelopes refuse.
pub fn from_reply(
    raw: &Value,
    requested: &[String],
    team: &str,
) -> Result<Vec<Projection>, &'static str> {
    let ids = requested.iter().map(String::as_str).collect::<HashSet<_>>();
    if !raw.is_object()
        || requested.is_empty()
        || requested.len() > 100
        || ids.len() != requested.len()
        || ids.iter().any(|id| !valid_id(id))
        || !valid_id(team)
        || raw.get("ok").is_some_and(|v| v != &json!(true))
        || raw.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(UNAVAILABLE);
    }
    for key in ["pending", "next_marker"] {
        if raw.get(key).is_some_and(|v| match v {
            Value::Null => false,
            Value::Bool(false) => false,
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
            Value::String(s) => !s.is_empty(),
            _ => true,
        }) {
            return Err(UNAVAILABLE);
        }
    }
    let failed = match raw.get("failed_ids") {
        None | Some(Value::Null) => HashSet::new(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().ok_or(UNAVAILABLE))
            .collect::<Result<HashSet<_>, _>>()?,
        _ => return Err(UNAVAILABLE),
    };
    if failed.iter().any(|id| !ids.contains(id)) {
        return Err(UNAVAILABLE);
    }
    let rows = match raw.get("results") {
        Some(Value::Array(a)) => a.iter().collect::<Vec<_>>(),
        Some(Value::Object(o)) => o
            .values()
            .flat_map(|v| match v {
                Value::Array(a) => a.iter().collect(),
                _ => vec![v],
            })
            .collect(),
        _ => return Err(UNAVAILABLE),
    };
    let mut counts = HashMap::new();
    for row in &rows {
        let id = row.get("id").and_then(Value::as_str).ok_or(UNAVAILABLE)?;
        if !ids.contains(id) {
            return Err(UNAVAILABLE);
        }
        *counts.entry(id).or_insert(0usize) += 1;
    }
    if counts.values().any(|n| *n != 1) {
        return Err(UNAVAILABLE);
    }
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let id = row.get("id")?.as_str()?;
            (!failed.contains(id))
                .then(|| Projection::from_raw(row, id, team).ok())
                .flatten()
        })
        .collect())
}

#[cfg(test)]
mod tests;

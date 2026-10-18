//! Source-backed 2gUC/xqiG member reacquisition.
//! Transport, pagination scheduling and store adoption remain separate owners.
use super::*;
use crate::block_actions::workflow_management::member_hydration::IncomingInteractions;
use std::collections::{BTreeMap, BTreeSet};

mod conditional;
#[cfg(test)]
mod tests;
pub mod unknown;

fn unavailable() -> Error {
    Error::Api("directory_member_info_unavailable".into())
}
fn member(id: &str) -> bool {
    IncomingInteractions::decode(&json!({(id):true})).is_ok()
}
fn marker(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}

/// One closed request of at most the source's 100-ID chunk size.
/// No Debug/serialization: this is not a credential or account authority.
#[derive(Clone, PartialEq, Eq)]
pub struct Read {
    ids: Vec<String>,
    marker: Option<String>,
    absent: bool,
    conditional: Option<conditional::Conditional>,
}
impl Read {
    pub fn capture(ids: Vec<String>) -> Result<Self, Error> {
        let unique: BTreeSet<_> = ids.iter().collect();
        if ids.is_empty()
            || ids.len() > 100
            || unique.len() != ids.len()
            || !ids.iter().all(|id| member(id))
        {
            return Err(unavailable());
        }
        Ok(Self {
            ids,
            marker: None,
            absent: false,
            conditional: None,
        })
    }
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    pub fn next_page(&self, next: &str) -> Result<Self, Error> {
        if !marker(next) || self.marker.as_deref() == Some(next) {
            return Err(unavailable());
        }
        Ok(Self {
            ids: self.ids.clone(),
            marker: Some(next.into()),
            absent: self.absent,
            conditional: self.conditional.clone(),
        })
    }
    fn absent(ids: Vec<String>) -> Result<Self, Error> {
        let mut read = Self::capture(ids)?;
        read.absent = true;
        Ok(read)
    }
    fn body(&self) -> Value {
        let mut body =
            json!({"ids":self.ids,"check_interaction":true,"include_profile_only_users":true});
        if self.absent {
            body.as_object_mut().expect("object").remove("ids");
            body["updated_ids"] = self
                .ids
                .iter()
                .map(|id| (id.clone(), json!(0)))
                .collect::<serde_json::Map<_, _>>()
                .into();
        }
        if let Some(conditional) = &self.conditional {
            conditional.apply(&mut body);
        }
        if let Some(marker) = &self.marker {
            body["marker"] = json!(marker);
        }
        body
    }
}

impl DirectoryEndpoint {
    pub fn member_info(
        &self,
        client: &SlackClient,
        owner: &WorkspaceSession,
        now: Instant,
        read: &Read,
    ) -> Result<PreparedRequest, Error> {
        if !self.current(owner, now) {
            return Err(unavailable());
        }
        self.observed_request(client, owner, "users/info", read.body())
    }
}

/// Closed transport admission, in addition to the producer's endpoint witness.
pub(crate) fn valid_wire_body(body: &Value) -> bool {
    let Some(object) = body.as_object() else {
        return false;
    };
    if !object.keys().all(|key| {
        matches!(
            key.as_str(),
            "token"
                | "enterprise_token"
                | "ids"
                | "updated_ids"
                | "cannot_interact"
                | "disconnected"
                | "check_interaction"
                | "include_profile_only_users"
                | "marker"
        )
    }) || !body["token"].as_str().is_some_and(|v| !v.is_empty())
        || !body
            .get("enterprise_token")
            .is_none_or(|v| v.as_str().is_some_and(|v| !v.is_empty()))
        || body["check_interaction"] != true
        || body["include_profile_only_users"] != true
        || !body
            .get("marker")
            .is_none_or(|v| v.as_str().is_some_and(marker))
    {
        return false;
    }
    if let Some(updated) = body.get("updated_ids") {
        let Some(updated) = updated.as_object() else {
            return false;
        };
        return !object.contains_key("ids")
            && updated.values().all(conditional::valid_version)
            && Read::capture(updated.keys().cloned().collect()).is_ok()
            && conditional::valid_ids(body.get("cannot_interact"), updated)
            && conditional::valid_ids(body.get("disconnected"), updated);
    }
    if object.contains_key("cannot_interact") || object.contains_key("disconnected") {
        return false;
    }
    let Some(ids) = body["ids"].as_array() else {
        return false;
    };
    let Some(ids) = ids
        .iter()
        .map(|v| v.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    Read::capture(ids).is_ok()
}

/// Same-byte raw response and actual Object.keys order, bound to its request.
/// Incomplete or failed IDs are retained and never become successful coverage.
pub struct Page {
    read: Read,
    raw: Value,
    rows: Vec<Value>,
    failed: Vec<String>,
    invalid: Vec<String>,
    pending: Vec<String>,
    disconnected: Vec<String>,
    interactions: BTreeMap<String, bool>,
    next: Option<String>,
}
impl Page {
    pub fn decode(read: &Read, raw: Value, keys: Option<Vec<String>>) -> Result<Self, Error> {
        if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
            return Err(unavailable());
        }
        let mut rows = Vec::new();
        match raw.get("results") {
            None => {
                if keys.is_some() {
                    return Err(unavailable());
                }
            }
            Some(Value::Array(values)) => {
                if keys.is_some() {
                    return Err(unavailable());
                }
                rows.extend(values.iter().cloned());
            }
            Some(Value::Object(values)) => {
                let keys = keys.ok_or_else(unavailable)?;
                let supplied: BTreeSet<_> = keys.iter().collect();
                if supplied.len() != keys.len() || supplied != values.keys().collect() {
                    return Err(unavailable());
                }
                for key in keys {
                    let value = values.get(&key).ok_or_else(unavailable)?;
                    if let Some(group) = value.as_array() {
                        rows.extend(group.iter().cloned());
                    } else {
                        rows.push(value.clone());
                    }
                }
            }
            _ => return Err(unavailable()),
        }
        let requested: BTreeSet<_> = read.ids.iter().map(String::as_str).collect();
        if rows.iter().any(|row| {
            !row.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| requested.contains(id))
        }) {
            return Err(unavailable());
        }
        let ids = |key: &str| -> Result<Vec<String>, Error> {
            let Some(value) = raw.get(key) else {
                return Ok(Vec::new());
            };
            value
                .as_array()
                .ok_or_else(unavailable)?
                .iter()
                .map(|v| {
                    let id = v
                        .as_str()
                        .filter(|id| requested.contains(id))
                        .ok_or_else(unavailable)?;
                    Ok(id.to_owned())
                })
                .collect()
        };
        let failed = ids("failed_ids")?;
        let invalid = ids("invalid_ids")?;
        let pending = ids("pending_ids")?;
        let disconnected = ids("disconnected_ids")?;
        let mut interactions = BTreeMap::new();
        if let Some(value) = raw.get("can_interact") {
            for (id, value) in value.as_object().ok_or_else(unavailable)? {
                if !requested.contains(id.as_str()) {
                    return Err(unavailable());
                }
                interactions.insert(id.clone(), value.as_bool().ok_or_else(unavailable)?);
            }
        }
        let next = match raw.get("next_marker") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) if marker(s) => Some(s.clone()),
            _ => return Err(unavailable()),
        };
        Ok(Self {
            read: read.clone(),
            raw,
            rows,
            failed,
            invalid,
            pending,
            disconnected,
            interactions,
            next,
        })
    }
    pub fn request(&self) -> &Read {
        &self.read
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn failed_ids(&self) -> &[String] {
        &self.failed
    }
    pub fn invalid_ids(&self) -> &[String] {
        &self.invalid
    }
    pub fn pending_ids(&self) -> &[String] {
        &self.pending
    }
    pub fn disconnected_ids(&self) -> &[String] {
        &self.disconnected
    }
    pub fn interactions(&self) -> &BTreeMap<String, bool> {
        &self.interactions
    }
    pub fn next_marker(&self) -> Option<&str> {
        self.next.as_deref()
    }
}

/// Actual xqiG mapping after all pages/chunks/pending rounds are combined.
/// Caller supplies a captured prior interaction entry for every missing result
/// flag: None means observed absence; an absent map key is unavailable evidence.
pub fn project_interaction(
    rows: &[Value],
    final_interactions: &BTreeMap<String, bool>,
    observed_prior: &BTreeMap<String, Option<bool>>,
) -> Result<Vec<Value>, Error> {
    rows.iter()
        .map(|raw| {
            let id = raw
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| member(id))
                .ok_or_else(unavailable)?;
            let interact = if let Some(value) = final_interactions.get(id) {
                *value
            } else {
                observed_prior.get(id).ok_or_else(unavailable)? != &Some(false)
            };
            let mut projected = raw.clone();
            projected["is_stranger"] = json!(!interact);
            Ok(projected)
        })
        .collect()
}

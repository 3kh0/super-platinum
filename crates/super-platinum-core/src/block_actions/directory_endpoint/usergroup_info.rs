//! D8Wa/2gUC direct group reads. Scheduling and publication belong to the caller.
//! Raw outcomes are retained; no UnknownFetchManager sentinel is synthesized.
use super::*;
use std::collections::BTreeSet;
mod fallback;
pub use fallback::{Fallback, FallbackPage};
#[cfg(test)]
mod tests;

fn unavailable() -> Error {
    Error::Api("directory_usergroup_info_unavailable".into())
}
fn group(id: &str) -> bool {
    id.len() >= 9
        && id.len() <= 255
        && id.starts_with('S')
        && id.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn marker(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn valid_ids(ids: &[String], limit: usize) -> bool {
    !ids.is_empty()
        && ids.len() <= limit
        && ids.iter().all(|id| group(id))
        && ids.iter().collect::<BTreeSet<_>>().len() == ids.len()
}
fn positive(raw: &Value) -> Result<(), Error> {
    if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(unavailable());
    }
    Ok(())
}
fn requested_rows(rows: &[Value], ids: &[String]) -> Result<(), Error> {
    if rows.iter().any(|row| {
        !row.get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| ids.iter().any(|requested| requested == id))
    }) {
        return Err(unavailable());
    }
    Ok(())
}

/// One unconditional, unique source chunk. No credential or publication proof.
#[derive(Clone, PartialEq, Eq)]
pub struct Read {
    ids: Vec<String>,
    marker: Option<String>,
}
impl Read {
    pub fn capture(ids: Vec<String>) -> Result<Self, Error> {
        if !valid_ids(&ids, 100) {
            return Err(unavailable());
        }
        Ok(Self { ids, marker: None })
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
        })
    }
    fn body(&self) -> Value {
        let mut body = json!({"ids":self.ids});
        if let Some(marker) = &self.marker {
            body["marker"] = json!(marker);
        }
        body
    }
}
impl DirectoryEndpoint {
    pub fn usergroup_info(
        &self,
        client: &SlackClient,
        owner: &WorkspaceSession,
        now: Instant,
        read: &Read,
    ) -> Result<PreparedRequest, Error> {
        if !self.current(owner, now) {
            return Err(unavailable());
        }
        self.observed_request(client, owner, "usergroups/info", read.body())
    }
}

pub(crate) fn valid_wire_body(body: &Value) -> bool {
    let Some(object) = body.as_object() else {
        return false;
    };
    if !object.keys().all(|key| {
        matches!(
            key.as_str(),
            "token" | "enterprise_token" | "ids" | "marker"
        )
    }) || !body["token"].as_str().is_some_and(|s| !s.is_empty())
        || !body
            .get("enterprise_token")
            .is_none_or(|v| v.as_str().is_some_and(|s| !s.is_empty()))
        || !body
            .get("marker")
            .is_none_or(|v| v.as_str().is_some_and(marker))
    {
        return false;
    }
    let Some(ids) = body["ids"].as_array().and_then(|ids| {
        ids.iter()
            .map(|v| v.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
    }) else {
        return false;
    };
    valid_ids(&ids, 100)
}
pub(crate) use fallback::valid_fallback_fields;

/// Positive response bound to the exact chunk/page, including unresolved IDs.
/// Object results require keys from the SAME response bytes, never sorted keys.
pub struct Page {
    read: Read,
    raw: Value,
    rows: Vec<Value>,
    failed: Vec<String>,
    invalid: Vec<String>,
    pending: Vec<String>,
    next: Option<String>,
}
impl Page {
    pub fn decode(read: &Read, raw: Value, keys: Option<Vec<String>>) -> Result<Self, Error> {
        positive(&raw)?;
        let mut rows = Vec::new();
        match raw.get("results") {
            None if keys.is_none() => {}
            Some(Value::Array(values)) if keys.is_none() => rows.extend(values.iter().cloned()),
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
        requested_rows(&rows, &read.ids)?;
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
                        .filter(|id| read.ids.iter().any(|r| r.as_str() == *id))
                        .ok_or_else(unavailable)?;
                    Ok(id.to_owned())
                })
                .collect()
        };
        let failed = ids("failed_ids")?;
        let pending = ids("pending_ids")?;
        let invalid = ids("invalid_ids")?;
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
            pending,
            invalid,
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
    pub fn pending_ids(&self) -> &[String] {
        &self.pending
    }
    pub fn invalid_ids(&self) -> &[String] {
        &self.invalid
    }
    pub fn next_marker(&self) -> Option<&str> {
        self.next.as_deref()
    }
}

//! 2gUC channel-info acquisition. Query ownership, chunk concurrency, fallback
//! scheduling and canonical channel projection belong to the caller.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
mod tests;
fn unavailable() -> Error {
    Error::Api("directory_channel_info_unavailable".into())
}
fn channel(id: &str) -> bool {
    id.len() >= 9
        && id.len() <= 255
        && matches!(id.as_bytes()[0], b'C' | b'G' | b'D' | b'Z')
        && id
            .bytes()
            .skip(1)
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn marker(s: &str) -> bool {
    !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
}
/// Ordered request IDs are independent of the returned object-key order.
/// Conditional versions must come from actual canonical source records; a
/// missing source version is zero, never a typed Channel default projection.
#[derive(Clone, PartialEq, Eq)]
pub struct Read {
    ids: Vec<String>,
    updated: Option<BTreeMap<String, u64>>,
    marker: Option<String>,
}
impl Read {
    pub fn capture(
        ids: Vec<String>,
        updated: Option<BTreeMap<String, u64>>,
    ) -> Result<Self, Error> {
        if ids.is_empty()
            || ids.len() > 100
            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
            || !ids.iter().all(|id| channel(id))
            || updated
                .as_ref()
                .is_some_and(|m| m.keys().collect::<BTreeSet<_>>() != ids.iter().collect())
        {
            return Err(unavailable());
        }
        Ok(Self {
            ids,
            updated,
            marker: None,
        })
    }
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    pub fn next_page(&self, next: &str) -> Result<Self, Error> {
        if !marker(next) || self.marker.as_deref() == Some(next) {
            return Err(unavailable());
        }
        let mut next_read = self.clone();
        next_read.marker = Some(next.into());
        Ok(next_read)
    }
    fn body(&self) -> Value {
        let mut body = json!({"check_membership":true});
        if let Some(updated) = &self.updated {
            body["updated_ids"] = json!(updated);
        } else {
            body["ids"] = json!(self.ids);
        }
        if let Some(marker) = &self.marker {
            body["marker"] = json!(marker);
        }
        body
    }
}
impl DirectoryEndpoint {
    pub fn channel_info(
        &self,
        client: &SlackClient,
        owner: &WorkspaceSession,
        now: Instant,
        read: &Read,
    ) -> Result<PreparedRequest, Error> {
        if !self.current(owner, now) {
            return Err(unavailable());
        }
        self.observed_request(client, owner, "channels/info", read.body())
    }
}
pub(crate) fn valid_wire_body(body: &Value) -> bool {
    let Some(o) = body.as_object() else {
        return false;
    };
    if !o.keys().all(|k| {
        matches!(
            k.as_str(),
            "token" | "enterprise_token" | "ids" | "updated_ids" | "check_membership" | "marker"
        )
    }) || !body["token"].as_str().is_some_and(|s| !s.is_empty())
        || !body
            .get("enterprise_token")
            .is_none_or(|v| v.as_str().is_some_and(|s| !s.is_empty()))
        || body["check_membership"] != true
        || !body
            .get("marker")
            .is_none_or(|v| v.as_str().is_some_and(marker))
        || body.get("ids").is_some() == body.get("updated_ids").is_some()
    {
        return false;
    }
    if let Some(ids) = body.get("ids") {
        let Some(ids) = ids.as_array().and_then(|v| {
            v.iter()
                .map(|x| x.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        }) else {
            return false;
        };
        Read::capture(ids, None).is_ok()
    } else {
        let Some(updated) = body["updated_ids"].as_object().and_then(|o| {
            o.iter()
                .map(|(k, v)| v.as_u64().map(|v| (k.clone(), v)))
                .collect::<Option<BTreeMap<_, _>>>()
        }) else {
            return false;
        };
        Read::capture(updated.keys().cloned().collect(), Some(updated)).is_ok()
    }
}
/// Same-byte Object.keys sidecar is required for result objects. Errors and
/// pending/invalid IDs are retained, not promoted to verified nonexistence.
pub struct Page {
    read: Read,
    raw: Value,
    rows: Vec<Value>,
    failed: Vec<String>,
    invalid: Vec<String>,
    pending: Vec<String>,
    membership: Vec<String>,
    next: Option<String>,
}
impl Page {
    pub fn decode(read: &Read, raw: Value, keys: Option<Vec<String>>) -> Result<Self, Error> {
        if !raw.is_object()
            || raw.get("ok").is_some_and(|v| v != &Value::Bool(true))
            || raw.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(unavailable());
        }
        let mut rows = Vec::new();
        match raw.get("results") {
            None if keys.is_none() => {}
            Some(Value::Array(a)) if keys.is_none() => rows.extend(a.iter().cloned()),
            Some(Value::Object(o)) => {
                let keys = keys.ok_or_else(unavailable)?;
                let unique: BTreeSet<_> = keys.iter().collect();
                if unique.len() != keys.len() || unique != o.keys().collect() {
                    return Err(unavailable());
                }
                for key in keys {
                    let v = &o[&key];
                    if let Some(a) = v.as_array() {
                        rows.extend(a.iter().cloned());
                    } else {
                        rows.push(v.clone());
                    }
                }
            }
            _ => return Err(unavailable()),
        }
        let requested: BTreeSet<_> = read.ids.iter().map(String::as_str).collect();
        if rows
            .iter()
            .any(|r| !r["id"].as_str().is_some_and(|id| requested.contains(id)))
        {
            return Err(unavailable());
        }
        let ids = |key: &str| -> Result<Vec<String>, Error> {
            raw.get(key)
                .map(|v| {
                    v.as_array()
                        .ok_or_else(unavailable)?
                        .iter()
                        .map(|v| {
                            v.as_str()
                                .filter(|id| requested.contains(id))
                                .map(str::to_owned)
                                .ok_or_else(unavailable)
                        })
                        .collect()
                })
                .unwrap_or_else(|| Ok(Vec::new()))
        };
        let failed = ids("failed_ids")?;
        let invalid = ids("invalid_ids")?;
        let pending = ids("pending_ids")?;
        let membership = ids("member_channels")?;
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
            membership,
            next,
        })
    }
    pub fn read(&self) -> &Read {
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
    pub fn membership_ids(&self) -> &[String] {
        &self.membership
    }
    pub fn next_marker(&self) -> Option<&str> {
        self.next.as_deref()
    }
}

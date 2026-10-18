//! Channel tab inventory and exact intents. Unknown siblings are never rewritten.
//! The first authoring slice supports ordinary Canvas tabs, not knowledge canvases.
mod intent;
pub use intent::{Access, Action, Intent, Observation, can_share_request};
mod permission;
pub use permission::{Permission, permission};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    Shape,
    Limit,
    Missing,
    Unsupported,
    Changed,
    Permission,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inventory {
    channel: String,
    properties: Value,
    rows: Vec<Value>,
}
impl Inventory {
    pub fn from_channel(raw: &Value) -> Result<Self, Fault> {
        let channel = raw
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| channel_id(s))
            .ok_or(Fault::Shape)?;
        let properties = match raw.get("properties") {
            None | Some(Value::Null) => serde_json::json!({}),
            Some(v) if v.is_object() => v.clone(),
            _ => return Err(Fault::Shape),
        };
        let rows = match properties.get("tabz") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(rows)) => rows.clone(),
            _ => return Err(Fault::Shape),
        };
        if rows.len() > 256
            || serde_json::to_vec(&properties)
                .map_err(|_| Fault::Shape)?
                .len()
                > 256 * 1024
        {
            return Err(Fault::Limit);
        }
        // Retain unknown kinds, fields, and order verbatim; duplicate identities are
        // unsafe anchors for any operation, even when the chosen row is supported.
        let mut ids = std::collections::BTreeSet::new();
        for row in &rows {
            let id = id(row).ok_or(Fault::Shape)?;
            if !row.is_object() || !ids.insert(id) {
                return Err(Fault::Shape);
            }
        }
        Ok(Self {
            channel: channel.into(),
            properties,
            rows,
        })
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn properties(&self) -> &Value {
        &self.properties
    }
    pub fn row(&self, key: &str) -> Option<&Value> {
        self.rows.iter().find(|r| id(r) == Some(key))
    }
    pub fn canvas(&self, key: &str) -> Result<&Value, Fault> {
        let row = self.row(key).ok_or(Fault::Missing)?;
        if row["type"] != "canvas" || !tab_id(key) || file_id(row).is_none() {
            return Err(Fault::Unsupported);
        }
        // Knowledge removal also disables a separate service. It cannot use this
        // ordinary removeTab path. Missing/malformed enabled metadata is not false.
        if self
            .properties
            .get("channel_knowledge_canvas")
            .is_some_and(|k| k["file_id"].as_str() == file_id(row) && k["is_enabled"] != false)
        {
            return Err(Fault::Unsupported);
        }
        Ok(row)
    }
    pub fn same_basis(&self, other: &Self) -> bool {
        self == other
    }
}
pub fn id(row: &Value) -> Option<&str> {
    row.get("id")
        .filter(|v| !v.is_null())
        .or_else(|| row.get("type"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 128)
}
pub fn file_id(row: &Value) -> Option<&str> {
    row.pointer("/data/file_id")
        .and_then(Value::as_str)
        .filter(|s| resource_id(s))
}
pub fn label(row: &Value) -> &str {
    row.get("label").and_then(Value::as_str).unwrap_or("")
}
fn channel_id(s: &str) -> bool {
    identifier(s, b"CG")
}
fn resource_id(s: &str) -> bool {
    identifier(s, b"F")
}
fn identifier(s: &str, prefix: &[u8]) -> bool {
    (9..=64).contains(&s.len())
        && prefix.contains(&s.as_bytes()[0])
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn tab_id(s: &str) -> bool {
    s.strip_prefix("Ct").is_some_and(|x| {
        (8..=64).contains(&x.len())
            && x.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests;

//! The jlH+ companion of an actual 46wa opened-conversation upsert.
//! No count defaults, message-body insertion, or canonical channel merge.
use super::{Error, Result};
use crate::state::ChannelMessages;
use serde_json::Value;
use std::collections::HashMap;

pub struct Companions {
    id: String,
    latest: Option<String>,
    last_read: Option<String>,
}
impl Companions {
    /// Source uses truthy latest (string or latest.ts) and truthy last_read.
    /// Native timestamps must be strings; malformed truthy timestamps refuse
    /// before any source queue mutation rather than invent a native cursor.
    pub fn capture(raw: &Value) -> Result<Self> {
        let id = raw["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(Error::Shape)?;
        let latest = match raw.get("latest") {
            Some(v @ Value::String(_)) => Some(v),
            Some(Value::Object(v)) => v.get("ts"),
            _ => None,
        };
        Ok(Self {
            id: id.into(),
            latest: timestamp(latest)?,
            last_read: timestamp(raw.get("last_read"))?,
        })
    }
    pub fn apply(self, messages: &mut HashMap<String, ChannelMessages>) {
        if self.latest.is_none() && self.last_read.is_none() {
            return;
        }
        let bag = messages.entry(self.id).or_default();
        if let Some(latest) = self.latest {
            if bag.latest.as_ref() != Some(&latest) {
                bag.latest = Some(latest);
            }
        }
        if let Some(last_read) = self.last_read {
            if bag.last_read.as_ref() != Some(&last_read) {
                bag.last_read = Some(last_read);
                bag.read_revision = bag.read_revision.wrapping_add(1);
            }
        }
    }
}
fn timestamp(value: Option<&Value>) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => Ok(None),
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(Error::Shape),
    }
}

#[cfg(test)]
mod tests;

//! Bounded ordinary D8Sp field projection, not a generic socket merge policy.
use super::{Message, Page, Target, timestamp};
use crate::slack::events::{MessageUpdate, MessageUpdateKind};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

mod fields;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyEditError {
    /// Foreign/stale identity, unowned row, or a parent notification.
    Refused,
    /// A known reply requires preprocessing outside this ordinary-body port.
    Unsupported,
}

/// Selected projected writes, including writes equal to the current value.
///
/// `None` removes a processed field (source `undefined`); `Some(null)` writes
/// JSON null. Applying to a typed Message maps both through that field's existing
/// serde representation. No original HTTP/POST receipt is changed or created.
#[derive(Clone, Debug)]
pub struct ReplyEdit {
    channel: String,
    root: String,
    ts: String,
    writes: BTreeMap<String, Option<Value>>,
}

impl ReplyEdit {
    pub fn project(
        target: &Target,
        known: &Message,
        update: &MessageUpdate,
    ) -> Result<Self, ReplyEditError> {
        let refused = ReplyEditError::Refused;
        let root = target.root_ts().ok_or(refused)?;
        let ts = known.ts.as_deref().ok_or(refused)?;
        let raw = update.raw().as_object().ok_or(refused)?;
        if target.validate().is_err()
            || update.kind() != MessageUpdateKind::Changed
            || update.outer_channel() != target.channel_id()
            || update.typed_channel() != Some(target.channel_id())
            || update.typed_ts() != Some(ts)
            || raw.get("ts").and_then(Value::as_str) != Some(ts)
            || raw.get("thread_ts").and_then(Value::as_str) != Some(root)
            || raw
                .get("channel")
                .is_some_and(|v| v.as_str() != Some(target.channel_id()))
            || !timestamp(root)
            || !timestamp(ts)
            || ts == root
            || known.thread_ts.as_deref() != Some(root)
            || known
                .channel
                .as_deref()
                .is_some_and(|c| c != target.channel_id())
            || hidden(&known.extra)
            || matches!(
                known.subtype.as_deref(),
                Some("tombstone" | "message_deleted")
            )
        {
            return Err(refused);
        }
        if !ordinary_subtype(known.subtype.as_deref())
            || !ordinary_subtype(raw.get("subtype").and_then(Value::as_str))
            || fields::unsupported(raw)
        {
            return Err(ReplyEditError::Unsupported);
        }
        let mut writes = fields::project(raw);
        writes.insert(
            "channel".into(),
            Some(Value::String(target.channel_id().into())),
        );
        let edit = Self {
            channel: target.channel_id().into(),
            root: root.into(),
            ts: ts.into(),
            writes,
        };
        // A malformed projected typed shape must not partially modify a row.
        edit.apply(known).ok_or(ReplyEditError::Unsupported)?;
        Ok(edit)
    }

    pub fn channel_id(&self) -> &str {
        &self.channel
    }
    pub fn root_ts(&self) -> &str {
        &self.root
    }
    pub fn ts(&self) -> &str {
        &self.ts
    }
    pub fn writes_reactions(&self) -> bool {
        self.writes.contains_key("reactions")
    }

    /// Apply only this patch's projected fields, preserving unrelated ACK fields.
    /// The caller retains owner/visit, deletion and actual projection authority.
    pub fn apply(&self, known: &Message) -> Option<Message> {
        if known.ts.as_deref() != Some(self.ts())
            || known.thread_ts.as_deref() != Some(self.root_ts())
            || known
                .channel
                .as_deref()
                .is_some_and(|c| c != self.channel_id())
            || hidden(&known.extra)
            || !ordinary_subtype(known.subtype.as_deref())
        {
            return None;
        }
        let Value::Object(mut next) = serde_json::to_value(known).ok()? else {
            return None;
        };
        for (field, value) in &self.writes {
            match value {
                Some(value) => {
                    next.insert(field.clone(), value.clone());
                }
                None => {
                    next.remove(field);
                }
            }
        }
        serde_json::from_value(Value::Object(next)).ok()
    }

    /// Newer projected writes win; other earlier writes remain. Storage is
    /// bounded by the fixed projection field set, not by the number of events.
    pub fn union(&self, newer: &Self) -> Option<Self> {
        if (self.channel_id(), self.root_ts(), self.ts())
            != (newer.channel_id(), newer.root_ts(), newer.ts())
        {
            return None;
        }
        let mut combined = self.clone();
        combined.writes.extend(newer.writes.clone());
        Some(combined)
    }
}

impl Page {
    /// Update a known typed reply only. `false` also includes equal-current
    /// writes; callers must retain the ReplyEdit separately for ACK precedence.
    pub fn apply_reply_edit(&mut self, edit: &ReplyEdit) -> bool {
        if self.target.channel_id() != edit.channel_id()
            || self.target.root_ts() != Some(edit.root_ts())
            || self.raw["deleted"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|v| v.as_str() == Some(edit.ts())))
        {
            return false;
        }
        let Some(index) = self
            .messages
            .iter()
            .position(|m| m.ts.as_deref() == Some(edit.ts()))
        else {
            return false;
        };
        let Some(next) = edit.apply(&self.messages[index]) else {
            return false;
        };
        if serde_json::to_value(&next).ok() == serde_json::to_value(&self.messages[index]).ok() {
            return false;
        }
        self.messages[index] = next;
        true
    }
}

fn ordinary_subtype(value: Option<&str>) -> bool {
    matches!(value, None | Some(""))
}
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}
fn hidden(values: &BTreeMap<String, Value>) -> bool {
    HIDDEN.iter().any(|key| truthy(values.get(*key)))
}
const HIDDEN: &[&str] = &[
    "hidden",
    "no_display",
    "_hidden_stub",
    "isNonExistent",
    "is_deleted",
    "is_tombstoned",
];

#[cfg(test)]
mod tests;

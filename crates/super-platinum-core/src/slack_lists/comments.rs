//! Item comments use the List's file conversation and an acquired Slack root.
//! Retained cYdQ -> gOq3 -> rGEL; IIdT maps F identifiers to C identifiers.
use super::{Document, ListError, Record};
use crate::slack::models::Message;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

mod deletion;
mod edits;
pub use edits::{ReplyEdit, ReplyEditError};
pub mod journal;
mod live;
mod own_delete;
pub use own_delete::{
    OwnDeleteAccepted, OwnDeleteIntent, OwnDeletePermission, OwnDeletePermissionQuery,
    OwnDeletePolicy, OwnDeleteRecord,
};
mod own_edit;
pub use own_edit::{
    OwnEditAccepted, OwnEditAttempt, OwnEditIntent, OwnEditPhase, OwnEditPolicy, OwnEditRecord,
    OwnEditSeed,
};
pub mod projection;
mod reaction_action;
mod reactions;
pub use reaction_action::{
    ReactionAccepted, ReactionFamilyIntent, ReactionFamilyRecord, ReactionIntent, ReactionRecord,
    ReactionResponse, own_reaction_family_names, reaction_family_base,
};
mod reply;
pub use reply::{ReplyAccepted, ReplyAttempt, ReplyIntent, ReplyPhase, ReplyPolicy, ReplyRecord};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    document: Value,
    record: Value,
    channel: String,
    root: Option<String>,
}

impl Target {
    pub fn capture(document: &Document, record: &Record) -> Result<Self, ListError> {
        if Document::parse(document.raw.clone())? != *document
            || Record::parse(record.raw.clone())? != *record
            || record
                .raw
                .get("list_id")
                .is_some_and(|v| v.as_str() != Some(&document.id))
            || !identity(&record.id)
            || flag(&record.raw, "is_deleted")?
        {
            return Err(ListError("List comment item identity is unavailable."));
        }
        flag(&record.raw, "archived")?;
        let root = match record.raw.get("thread_ts") {
            None | Some(Value::Null) => None,
            Some(Value::String(ts)) if ts.is_empty() => None,
            Some(Value::String(ts)) if timestamp(ts) => Some(ts.clone()),
            _ => return Err(ListError("List comment thread identity is malformed.")),
        };
        Ok(Self {
            document: document.raw.clone(),
            record: record.raw.clone(),
            channel: channel(&document.id)?,
            root,
        })
    }
    pub fn list_id(&self) -> &str {
        self.document["id"].as_str().unwrap()
    }
    pub fn record_id(&self) -> &str {
        self.record["id"].as_str().unwrap()
    }
    pub fn channel_id(&self) -> &str {
        &self.channel
    }
    pub fn root_ts(&self) -> Option<&str> {
        self.root.as_deref()
    }
    pub fn request_ts(&self) -> String {
        self.root
            .clone()
            .unwrap_or_else(|| format!("{}-{}", self.list_id(), self.record_id()))
    }
    pub fn document_raw(&self) -> &Value {
        &self.document
    }
    pub fn record_raw(&self) -> &Value {
        &self.record
    }
    pub fn revalidate(&self, document: &Document, record: &Record) -> Result<(), ListError> {
        let current = Self::capture(document, record)?;
        if self.document != current.document
            || self.record != current.record
            || self.channel != current.channel
            || current
                .root
                .as_ref()
                .is_some_and(|root| Some(root) != self.root.as_ref())
        {
            return Err(ListError("List comment owner changed."));
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), ListError> {
        let doc = Document::parse(self.document.clone())?;
        let row = Record::parse(self.record.clone())?;
        self.revalidate(&doc, &row)?;
        if self.root.as_deref().is_some_and(|ts| !timestamp(ts)) {
            return Err(ListError("List comment root is malformed."));
        }
        Ok(())
    }
    /// A fresh file/record response may update the root on an explicit refresh.
    /// A continuation must retain the exact acquired root; an absent root field
    /// may reuse it only when both original raw owners remain byte-value equal.
    pub fn refresh(&self, file: &Value, record: &Value, paging: bool) -> Result<Self, ListError> {
        self.validate()?;
        if file["ok"] != true
            || record["ok"] != true
            || [file, record]
                .iter()
                .any(|v| v.get("error").is_some_and(|e| !e.is_null()))
        {
            return Err(ListError("List comment acquisition was not confirmed."));
        }
        let document = Document::parse(file["file"].clone())?;
        let row = Record::parse(record["record"].clone())?;
        if document.id != self.list_id()
            || row.id != self.record_id()
            || record
                .get("list_id")
                .is_some_and(|v| v.as_str() != Some(self.list_id()))
            || ["date_created", "created_by"]
                .iter()
                .any(|key| row.raw.get(*key) != self.record.get(*key))
        {
            return Err(ListError("List comment item identity changed."));
        }
        let fresh = Self::capture(&document, &row)?;
        if !paging {
            return Ok(fresh);
        }
        let root = self
            .root_ts()
            .ok_or(ListError("List comment continuation has no root."))?;
        match fresh.root_ts() {
            Some(next) if next == root => Ok(fresh),
            Some(_) => Err(ListError("List comment continuation root changed.")),
            None => {
                self.revalidate(&document, &row)?;
                Ok(self.clone())
            }
        }
    }
}

pub fn channel(file: &str) -> Result<String, ListError> {
    let suffix = file
        .strip_prefix('F')
        .ok_or(ListError("List file identity is malformed."))?;
    if !(8..=14).contains(&suffix.len())
        || !suffix
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err(ListError("List file identity is malformed."));
    }
    Ok(format!("C{suffix}"))
}
pub fn timestamp(value: &str) -> bool {
    crate::canvas::comments::timestamp(value)
}
pub(crate) fn identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
}
pub(super) fn flag(raw: &Value, key: &str) -> Result<bool, ListError> {
    match raw.get(key) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => Ok(false),
        Some(Value::Bool(true)) => Ok(true),
        _ => Err(ListError("List comment availability is malformed.")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tombstones {
    Include,
    Exclude,
}
impl Tombstones {
    /// rGEL: !socket.isConnected || channelTopic.desynced || userTopic.desynced.
    pub fn from_realtime(connected: bool, channel_desynced: bool, user_desynced: bool) -> Self {
        if !connected || channel_desynced || user_desynced {
            Self::Include
        } else {
            Self::Exclude
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct ReplyQuery {
    pub cursor: Option<String>,
    pub oldest: Option<String>,
    pub latest: Option<String>,
}
impl ReplyQuery {
    pub fn validate(&self, target: &Target) -> Result<(), ListError> {
        target.validate()?;
        if self
            .cursor
            .as_deref()
            .is_some_and(|s| s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control))
            || self.oldest.as_deref().is_some_and(|s| !timestamp(s))
            || self.latest.as_deref().is_some_and(|s| !timestamp(s))
            || (self.cursor.is_some() && (self.oldest.is_some() || self.latest.is_some()))
            // gOq3 dispatches two reads for distinct simultaneous bounds. This
            // single-page API deliberately refuses that separate composition.
            || (self.oldest.is_some() && self.latest.is_some()
                && !(self.oldest == self.latest && self.oldest.as_deref() == target.root_ts()))
            || (target.root.is_none()
                && (self.cursor.is_some() || self.oldest.is_some() || self.latest.is_some()))
        {
            return Err(ListError("List comment continuation is malformed."));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Page {
    target: Target,
    raw: Value,
    messages: Vec<Message>,
    root_index: Option<usize>,
    cursor: Option<String>,
    has_more: bool,
}
impl Page {
    pub fn parse(target: &Target, raw: Value) -> Result<Self, ListError> {
        target.validate()?;
        if raw["ok"] != true
            || raw.get("error").is_some_and(|v| !v.is_null())
            || raw
                .get("channel")
                .is_some_and(|v| v.as_str() != Some(target.channel_id()))
        {
            return Err(ListError("List comments were not confirmed."));
        }
        let values = raw["messages"]
            .as_array()
            .ok_or(ListError("List comments are malformed."))?;
        if values.len() > 1000 {
            return Err(ListError("List comment page is too large."));
        }
        let has_more = match raw.get("has_more") {
            None => false,
            Some(Value::Bool(v)) => *v,
            _ => return Err(ListError("List comment continuation is malformed.")),
        };
        let cursor = match raw.pointer("/response_metadata/next_cursor") {
            None | Some(Value::Null) => None,
            Some(Value::String(v)) if v.is_empty() => None,
            Some(Value::String(v)) if v.len() <= 4096 && !v.chars().any(char::is_control) => {
                Some(v.clone())
            }
            _ => return Err(ListError("List comment continuation is malformed.")),
        };
        if raw
            .get("response_metadata")
            .is_some_and(|v| !v.is_object() && !v.is_null())
            || raw
                .get("unchanged_messages")
                .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
        {
            return Err(ListError(
                "List comment response needs unavailable cached messages.",
            ));
        }
        if let Some(deleted) = raw.get("deleted") {
            let deleted = deleted
                .as_array()
                .ok_or(ListError("List comment deletions are malformed."))?;
            if deleted.iter().any(|v| !v.as_str().is_some_and(timestamp)) {
                return Err(ListError("List comment deletions are malformed."));
            }
        }
        let mut seen = HashSet::new();
        let mut messages = Vec::new();
        let mut root_index = None;
        let mut resolved = target.clone();
        for value in values {
            let message = parse_message(value, target.channel_id())?;
            let ts = message.ts.as_deref().unwrap();
            if !seen.insert(ts.to_owned()) {
                return Err(ListError("List comment identities are ambiguous."));
            }
            if is_root(&message) {
                if root_index.is_some()
                    || resolved
                        .root
                        .as_deref()
                        .is_some_and(|expected| expected != ts)
                {
                    return Err(ListError("List comment root did not match."));
                }
                if !item_root(&message, target) {
                    return Err(ListError("List comment root belongs to another item."));
                }
                resolved.root = Some(ts.into());
                root_index = Some(messages.len());
            }
            messages.push(message);
        }
        let root = resolved
            .root
            .as_deref()
            .ok_or(ListError("List comment root is missing."))?;
        if messages
            .iter()
            .any(|m| !is_root(m) && m.thread_ts.as_deref() != Some(root))
        {
            return Err(ListError("List comment reply belongs to another thread."));
        }
        Ok(Self {
            target: resolved,
            raw,
            messages,
            root_index,
            cursor,
            has_more,
        })
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    /// Original HTTP source receipt. Realtime deltas update typed messages,
    /// never this receipt or its pagination/target witnesses.
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }
    pub fn root(&self) -> Option<&Message> {
        self.root_index.map(|i| &self.messages[i])
    }
    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }
    pub fn has_more(&self) -> bool {
        self.has_more
    }
}
pub(super) fn is_root(message: &Message) -> bool {
    message
        .thread_ts
        .as_deref()
        .is_none_or(|ts| ts.is_empty() || Some(ts) == message.ts.as_deref())
}
pub(super) fn item_root(message: &Message, target: &Target) -> bool {
    is_root(message)
        && message.subtype.as_deref() == Some("list_record_comment")
        && message.extra.get("slack_list").is_some_and(|v| {
            v["list_id"].as_str() == Some(target.list_id())
                && v["list_record_id"].as_str() == Some(target.record_id())
        })
}
pub(super) fn parse_message(raw: &Value, channel: &str) -> Result<Message, ListError> {
    let message: Message = serde_json::from_value(raw.clone())
        .map_err(|_| ListError("List comment message is malformed."))?;
    if !message.ts.as_deref().is_some_and(timestamp)
        || message.channel.as_deref().is_some_and(|c| c != channel)
        || message
            .thread_ts
            .as_deref()
            .is_some_and(|s| !s.is_empty() && !timestamp(s))
    {
        return Err(ListError("List comment message identity is malformed."));
    }
    Ok(message)
}

#[cfg(test)]
mod tests;

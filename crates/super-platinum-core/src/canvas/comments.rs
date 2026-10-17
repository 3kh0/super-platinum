//! Read-only Canvas comments. Source: Slack's Oo8B/rGEL/IIdT client modules.
//! No Quip IDs are synthesized into Slack timestamps and no read marks are sent.
use crate::slack::models::Message;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    Recent,
    Unread,
    Resolved,
}
impl Filter {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Recent => "latest_reply",
            Self::Unread => "unread",
            Self::Resolved => "archived",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Recent => "Most recent",
            Self::Unread => "Unread",
            Self::Resolved => "Resolved",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Thread {
    pub root: Message,
    pub last_reply: Option<Message>,
}
#[derive(Clone, Debug)]
pub struct StreamPage {
    pub threads: Vec<Thread>,
    pub cursor: Option<String>,
}
#[derive(Clone, Debug)]
pub struct ReplyPage {
    pub messages: Vec<Message>,
    pub cursor: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    Identity,
    Envelope,
    Message,
    Pagination,
    Limit,
}

pub fn channel(file: &str) -> Result<String, Invalid> {
    let suffix = file.strip_prefix('F').ok_or(Invalid::Identity)?;
    if suffix.is_empty() || suffix.len() > 127 || !suffix.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(Invalid::Identity);
    }
    Ok(format!("C{suffix}"))
}
pub fn timestamp(value: &str) -> bool {
    let Some((seconds, fraction)) = value.split_once('.') else {
        return false;
    };
    !seconds.is_empty()
        && seconds.len() <= 16
        && seconds.bytes().all(|b| b.is_ascii_digit())
        && (1..=6).contains(&fraction.len())
        && fraction.bytes().all(|b| b.is_ascii_digit())
}
pub fn cursor(value: Option<&str>) -> Result<Option<&str>, Invalid> {
    value
        .filter(|v| !v.is_empty())
        .map(|v| {
            if v.len() <= 4096 && !v.chars().any(char::is_control) {
                Ok(v)
            } else {
                Err(Invalid::Pagination)
            }
        })
        .transpose()
}
fn envelope(value: &Value) -> Result<(&Vec<Value>, Option<String>), Invalid> {
    if value.get("ok").and_then(Value::as_bool) != Some(true)
        || value.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(Invalid::Envelope);
    }
    // Local bounds, not claimed server maxima. Decode is called off the UI thread.
    if serde_json::to_vec(value)
        .map_err(|_| Invalid::Envelope)?
        .len()
        > 2 * 1024 * 1024
    {
        return Err(Invalid::Limit);
    }
    let messages = value
        .get("messages")
        .and_then(Value::as_array)
        .ok_or(Invalid::Envelope)?;
    if messages.len() > 1000 {
        return Err(Invalid::Limit);
    }
    let more = match value.get("has_more") {
        None => false,
        Some(Value::Bool(v)) => *v,
        _ => return Err(Invalid::Pagination),
    };
    let raw = value.pointer("/response_metadata/next_cursor");
    if raw.is_some_and(|v| !v.is_string() && !v.is_null()) {
        return Err(Invalid::Pagination);
    }
    let next = cursor(raw.and_then(Value::as_str))?.map(str::to_owned);
    if more && next.is_none() {
        return Err(Invalid::Pagination);
    }
    Ok((messages, if more { next } else { None }))
}
fn message(raw: &Value, channel: &str) -> Result<Message, Invalid> {
    let msg: Message = serde_json::from_value(raw.clone()).map_err(|_| Invalid::Message)?;
    if !msg.ts.as_deref().is_some_and(timestamp)
        || msg.channel.as_deref().is_some_and(|id| id != channel)
        || msg.kind.as_deref() != Some("message")
    {
        return Err(Invalid::Message);
    }
    Ok(msg)
}
fn root(msg: &Message) -> bool {
    msg.subtype.as_deref() == Some("document_comment_root")
        && (msg.thread_ts.is_none() || msg.thread_ts == msg.ts)
        && msg
            .extra
            .get("document_comment")
            .and_then(|v| v.get("thread_id"))
            .and_then(Value::as_str)
            .is_some_and(|id| !id.is_empty() && id.len() <= 256)
}
impl StreamPage {
    pub fn parse(file: &str, value: &Value) -> Result<Self, Invalid> {
        let channel = channel(file)?;
        let (raw, cursor) = envelope(value)?;
        let mut roots = Vec::new();
        let mut replies: BTreeMap<String, Message> = BTreeMap::new();
        let mut seen = HashSet::new();
        for raw in raw {
            let msg = message(raw, &channel)?;
            if !seen.insert(msg.ts.clone().unwrap()) {
                return Err(Invalid::Message);
            }
            if root(&msg) {
                roots.push(msg);
            } else if msg.subtype.as_deref() == Some("document_comment_root") {
                return Err(Invalid::Message);
            } else if let Some(parent) = msg
                .thread_ts
                .as_ref()
                .filter(|parent| Some(*parent) != msg.ts.as_ref())
            {
                if !timestamp(parent) {
                    return Err(Invalid::Message);
                }
                let replace = replies
                    .get(parent)
                    .is_none_or(|old| ts_key(&msg) > ts_key(old));
                if replace {
                    replies.insert(parent.clone(), msg);
                }
            }
        }
        Ok(Self {
            threads: roots
                .into_iter()
                .map(|root| Thread {
                    last_reply: replies.remove(root.ts.as_ref().unwrap()),
                    root,
                })
                .collect(),
            cursor,
        })
    }
}
fn ts_key(msg: &Message) -> (u64, u32) {
    let (s, f) = msg.ts.as_deref().unwrap().split_once('.').unwrap();
    (
        s.parse().unwrap(),
        f.parse::<u32>().unwrap() * 10u32.pow(6 - f.len() as u32),
    )
}
impl ReplyPage {
    pub fn parse(file: &str, root_ts: &str, initial: bool, value: &Value) -> Result<Self, Invalid> {
        if !timestamp(root_ts) {
            return Err(Invalid::Identity);
        }
        let channel = channel(file)?;
        let (raw, cursor) = envelope(value)?;
        let mut messages = Vec::new();
        let mut seen = HashSet::new();
        let mut found = false;
        for raw in raw {
            let msg = message(raw, &channel)?;
            if !seen.insert(msg.ts.clone().unwrap()) {
                return Err(Invalid::Message);
            }
            if msg.ts.as_deref() == Some(root_ts) {
                if !root(&msg) {
                    return Err(Invalid::Message);
                }
                found = true;
            } else if msg.thread_ts.as_deref() != Some(root_ts) {
                return Err(Invalid::Message);
            }
            messages.push(msg);
        }
        if initial && !found {
            return Err(Invalid::Message);
        }
        messages.sort_by_key(ts_key);
        Ok(Self { messages, cursor })
    }
}

#[cfg(test)]
mod tests;

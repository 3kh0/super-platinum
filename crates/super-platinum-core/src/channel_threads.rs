//! Qualified initial channel discussion read. No global feed or write authority.
use crate::{
    CoreAppState,
    config::WorkspaceSession,
    domain::ThreadKey,
    slack::{Error, PreparedRequest, SlackClient, models::Message},
    state::{ChannelMessages, cmp_ts},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
mod baseline;
#[cfg(test)]
mod tests;

fn unavailable() -> Error {
    Error::Api("channel_threads_unavailable".into())
}
fn channel_id(id: &str) -> bool {
    (9..=64).contains(&id.len())
        && matches!(id.as_bytes()[0], b'C' | b'G')
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn timestamp(v: &Value) -> Result<&str, Error> {
    let s = v.as_str().ok_or_else(unavailable)?;
    let (a, b) = s.split_once('.').ok_or_else(unavailable)?;
    if a.is_empty()
        || a.len() != 10
        || b.len() != 6
        || !a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit())
    {
        return Err(unavailable());
    }
    Ok(s)
}
/// Data observation only. Native callers must retain original assignment ownership.
pub fn enabled(raw: &Value) -> bool {
    raw["ok"] == true
        && raw.get("error").is_none_or(Value::is_null)
        && raw
            .get("assignments")
            .and_then(Value::as_object)
            .and_then(|m| m.get("channel_tabs_threads"))
            .and_then(|v| v.get("group"))
            .and_then(Value::as_str)
            == Some("on")
}
/// Native post-boot ordinary-channel branch only. Caller proves that cohort and
/// current original gate; this request builder alone grants no IO authority.
pub fn request(
    client: &SlackClient,
    session: &WorkspaceSession,
    channel: &str,
) -> Result<PreparedRequest, Error> {
    if !channel_id(channel) {
        return Err(unavailable());
    }
    Ok(client.rest_form(
        session,
        "subscriptions.thread.getView",
        vec![
            ("limit", "5".into()),
            ("fetch_threads_state", "true".into()),
            ("priority_mode", "all".into()),
            ("channel_id", channel.into()),
        ],
    ))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discussion {
    root: String,
    timestamps: Vec<String>,
    sort_ts: String,
    has_unreads: bool,
    reached_start: bool,
}
impl Discussion {
    pub fn root(&self) -> &str {
        &self.root
    }
    pub fn timestamps(&self) -> &[String] {
        &self.timestamps
    }
    pub fn reached_start(&self) -> bool {
        self.reached_start
    }
}
struct Incoming {
    descriptor: Discussion,
    messages: Vec<Message>,
}
pub struct Page {
    channel: String,
    items: Vec<Incoming>,
    has_more: bool,
}
impl Page {
    /// Strict, bounded supported ordinary payload. Whole-batch refusal is a
    /// native safety adaptation; unsupported rows never manufacture emptiness.
    pub fn decode(raw: &Value, channel: &str) -> Result<Self, Error> {
        if !channel_id(channel)
            || raw["ok"] != true
            || raw.get("error").is_some_and(|v| !v.is_null())
        {
            return Err(unavailable());
        }
        let rows = raw["threads"].as_array().ok_or_else(unavailable)?;
        let has_more = raw["has_more"].as_bool().ok_or_else(unavailable)?;
        if rows.len() > 5
            || serde_json::to_vec(raw).map_err(|_| unavailable())?.len() > 8 * 1024 * 1024
        {
            return Err(unavailable());
        }
        let mut items = Vec::new();
        let mut roots = BTreeSet::new();
        for row in rows {
            let root = &row["root_msg"];
            let ts = timestamp(&root["ts"])?;
            if root["channel"] != channel
                || root["thread_ts"] != ts
                || root["subscribed"] != true
                || !roots.insert(ts.to_owned())
            {
                return Err(unavailable());
            }
            // Source UV is reply_count||0. This initial native supported cohort
            // additionally requires an observed nonnegative count, never length.
            let count = root["reply_count"]
                .as_u64()
                .filter(|n| *n <= u32::MAX as u64)
                .ok_or_else(unavailable)?;
            let array = |name: &str| -> Result<&[Value], Error> {
                match row.get(name) {
                    None | Some(Value::Null) => Ok(&[]),
                    Some(Value::Array(v)) => Ok(v),
                    _ => Err(unavailable()),
                }
            };
            let unread = array("unread_replies")?;
            let latest = array("latest_replies")?;
            if unread.len() + latest.len() == 0 || unread.len() + latest.len() > 1000 {
                return Err(unavailable());
            }
            let mut replies = BTreeMap::new();
            for reply in unread.iter().chain(latest) {
                let rts = timestamp(&reply["ts"])?;
                if rts == ts
                    || cmp_ts(Some(rts), Some(ts)).is_lt()
                    || reply.get("channel").is_some_and(|v| v != channel)
                    || reply.get("thread_ts").is_some_and(|v| v != ts)
                {
                    return Err(unavailable());
                }
                // Source uniqBy keeps the first: unread rows precede latest rows.
                replies.entry(rts.to_owned()).or_insert(reply);
            }
            let mut keys = replies.keys().cloned().collect::<Vec<_>>();
            keys.sort_by(|a, b| cmp_ts(Some(a), Some(b)));
            let sort_ts = keys.last().cloned().ok_or_else(unavailable)?;
            let mut messages =
                vec![serde_json::from_value::<Message>(root.clone()).map_err(|_| unavailable())?];
            for rts in &keys {
                let mut value = (*replies[rts]).clone();
                value["channel"] = channel.into();
                value["thread_ts"] = ts.into();
                messages.push(serde_json::from_value(value).map_err(|_| unavailable())?);
            }
            let reached_start = count <= keys.len() as u64;
            items.push(Incoming {
                descriptor: Discussion {
                    root: ts.into(),
                    timestamps: keys,
                    sort_ts,
                    has_unreads: !unread.is_empty(),
                    reached_start,
                },
                messages,
            });
        }
        items.sort_by(|a, b| {
            b.descriptor
                .has_unreads
                .cmp(&a.descriptor.has_unreads)
                .then_with(|| cmp_ts(Some(&b.descriptor.sort_ts), Some(&a.descriptor.sort_ts)))
        });
        Ok(Self {
            channel: channel.into(),
            items,
            has_more,
        })
    }
}
#[derive(Clone, Debug)]
pub struct Accepted {
    discussions: Vec<Discussion>,
    has_more: bool,
}
impl Accepted {
    pub fn discussions(&self) -> &[Discussion] {
        &self.discussions
    }
    pub fn has_more(&self) -> bool {
        self.has_more
    }
}
/// Original bounded canonical preimages, captured at read admission only.
/// Successful publication still requires the native original route/read ticket.
pub struct Read {
    epoch: u64,
    team: String,
    channel: String,
    before: baseline::Before,
}
impl Read {
    pub fn capture(core: &CoreAppState, team: &str, channel: &str) -> Result<Self, Error> {
        if core.active_team.as_deref() != Some(team)
            || !channel_id(channel)
            || core
                .workspaces
                .get(team)
                .and_then(|w| w.channels.get(channel))
                .is_none_or(|c| c.id != channel)
        {
            return Err(unavailable());
        }
        Ok(Self {
            epoch: core.account_epoch,
            team: team.into(),
            channel: channel.into(),
            before: baseline::Before::capture(core, team, channel)?,
        })
    }
    pub fn current(&self, core: &CoreAppState) -> bool {
        core.account_epoch == self.epoch
            && core.active_team.as_deref() == Some(&self.team)
            && baseline::Before::capture(core, &self.team, &self.channel)
                .is_ok_and(|n| n == self.before)
    }
    pub fn publish(self, core: &mut CoreAppState, page: Page) -> Result<Accepted, Error> {
        if page.channel != self.channel || !self.current(core) {
            return Err(unavailable());
        }
        // No awaits or fallible operations below this atomic validation boundary.
        let mut discussions = Vec::new();
        for incoming in page.items {
            let key: ThreadKey = (
                self.team.clone(),
                self.channel.clone(),
                incoming.descriptor.root.clone(),
            );
            let channel_bag = core.workspaces[&self.team].messages.get(&self.channel);
            let deleted = |ts: &str| {
                channel_bag.is_some_and(|b| b.was_deleted(ts))
                    || core.threads.get(&key).is_some_and(|b| b.was_deleted(ts))
            };
            if deleted(&incoming.descriptor.root) {
                continue;
            }
            let messages = incoming
                .messages
                .into_iter()
                .filter(|m| m.ts.as_deref().is_some_and(|t| !deleted(t)))
                .collect::<Vec<_>>();
            let retained = messages
                .iter()
                .filter_map(|m| m.ts.as_ref())
                .collect::<BTreeSet<_>>();
            let mut descriptor = incoming.descriptor;
            descriptor.timestamps.retain(|t| retained.contains(t));
            // Only already-present timeline rows are updated. Thread replies may
            // populate canonical thread bags, never the channel timeline window.
            if let Some(bag) = core
                .workspaces
                .get_mut(&self.team)
                .and_then(|w| w.messages.get_mut(&self.channel))
            {
                for message in &messages {
                    if bag.messages.iter().any(|m| m.ts == message.ts) {
                        bag.merge_update(message.clone());
                    }
                }
            }
            let fresh = !core.threads.contains_key(&key);
            let bag = core.threads.entry(key).or_default();
            for message in messages {
                bag.merge_update(message);
            }
            if fresh {
                bag.loaded = true;
                bag.has_more_older = !descriptor.reached_start;
            }
            // Existing cursors, history/window flags and rows are retained. No
            // read marks, subscription mutation or global-feed settlement.
            discussions.push(descriptor);
        }
        Ok(Accepted {
            discussions,
            has_more: page.has_more,
        })
    }
}
/// Exact Source isHiddenInThread truthiness; _hidden_reply is unrelated.
pub fn hidden(message: &Message) -> bool {
    ["isNonExistent", "no_display", "_hidden_stub"]
        .iter()
        .any(|k| truthy(message.extra.get(*k)))
}
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}
/// Default loaded context only; this exposes no older-reply transport/window.
pub fn minimum_visible(
    root: &Message,
    timestamps: &[String],
    last_read: Option<&str>,
    reached_start: bool,
) -> Option<String> {
    if root.reply_count.unwrap_or(0) <= 4 {
        return None;
    }
    let read = timestamps
        .iter()
        .filter(|t| t.as_str() <= last_read.unwrap_or(""))
        .collect::<Vec<_>>();
    if reached_start && read.len() <= 2 {
        return None;
    }
    if read.len() == timestamps.len() {
        read.iter()
            .rev()
            .nth(1)
            .or_else(|| read.last())
            .map(|s| (*s).to_string())
    } else {
        read.last().map(|s| (*s).to_string())
    }
}

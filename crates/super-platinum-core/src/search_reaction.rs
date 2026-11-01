//! Fresh, isolated Search reaction data; never a canonical model or action grant.
//!
//! The native caller owns the original reads, authenticated actor, selected row,
//! namespace and currentness. These parsers cannot supply that lineage. The
//! named first-insertion profile uses the retained vkwm/ZGtP construction rules
//! (policy-schema/channel-insertion reports, 2026-11-01), not an existing store.
use crate::slack::models::Reaction;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
mod selection;
pub use selection::SelectedReaction;

const UNAVAILABLE: &str = "Search reaction data is unavailable.";
type Result<T> = std::result::Result<T, &'static str>;

/// A finite data classification only. It contains no request or mutable model.
#[derive(Debug)]
pub struct FreshPolicy {
    channel: String,
    timestamp: String,
}
impl FreshPolicy {
    /// Both envelopes must be the caller's original exact-coordinate reads.
    /// A cache, indexed row or reconstructed DTO cannot acquire native custody
    /// by passing this parser. No predecessor is read or merged here.
    pub fn capture(channel: &str, ts: &str, history: &Value, info: &Value) -> Result<Self> {
        coordinate(channel, ts)?;
        envelope(history)?;
        envelope(info)?;
        optional_channel(history, channel)?;
        if history.get("has_more").is_some_and(|v| !v.is_boolean()) {
            return Err(UNAVAILABLE);
        }
        let messages = history
            .get("messages")
            .and_then(Value::as_array)
            .ok_or(UNAVAILABLE)?;
        if messages.len() != 1 {
            return Err(UNAVAILABLE);
        }
        ordinary_message(&messages[0], channel, ts)?;
        let record = info
            .get("channel")
            .filter(|v| v.is_object())
            .ok_or(UNAVAILABLE)?;
        let name = record
            .get("name")
            .and_then(Value::as_str)
            .ok_or(UNAVAILABLE)?;
        if record.get("id").and_then(Value::as_str) != Some(channel)
            || name.is_empty()
            || name.trim() != name
            || name.len() > 255
            || name.chars().any(char::is_control)
            || record.get("is_channel") != Some(&Value::Bool(true))
            || record.get("is_member") != Some(&Value::Bool(true))
            || record.get("is_frozen") != Some(&Value::Bool(false))
        {
            return Err(UNAVAILABLE);
        }
        // ZGtP's fresh constructor normalizes IM/group/MPIM, preserves an
        // absent is_file, and establishes knownness from the nonempty name.
        // Only this isolated classification applies the absent-property fold;
        // no missing flag is written into a shared/canonical record.
        for key in [
            "is_im",
            "is_group",
            "is_mpim",
            "is_file",
            "is_archived",
            "isUnknown",
            "isNonExistent",
        ] {
            absent_or_false(record, key)?;
        }
        Ok(Self {
            channel: channel.into(),
            timestamp: ts.into(),
        })
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }
}

/// Complete explicit reaction-name data, independently of policy eligibility.
#[derive(Debug)]
pub struct Membership {
    reactions: Vec<Reaction>,
}
impl Membership {
    /// Requires the original reactions.get(full:true) response. The endpoint's
    /// authenticated-user inclusion, not every other user's completeness, backs
    /// the caller's exact original-actor membership comparison.
    pub fn decode(channel: &str, ts: &str, raw: &Value) -> Result<Self> {
        coordinate(channel, ts)?;
        envelope(raw)?;
        if raw.get("type").and_then(Value::as_str) != Some("message")
            || raw.get("channel").and_then(Value::as_str) != Some(channel)
        {
            return Err(UNAVAILABLE);
        }
        let message = raw.get("message").ok_or(UNAVAILABLE)?;
        ordinary_message(message, channel, ts)?;
        // Omission/null has no retained normalization proof. In particular,
        // do not use Reaction's serde defaults to manufacture empty membership.
        let rows = message
            .get("reactions")
            .and_then(Value::as_array)
            .ok_or(UNAVAILABLE)?;
        let mut names = BTreeSet::new();
        let mut reactions = Vec::with_capacity(rows.len());
        for row in rows {
            let name = row
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| reaction_name(s))
                .ok_or(UNAVAILABLE)?;
            let count = row
                .get("count")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or(UNAVAILABLE)?;
            let users = row
                .get("users")
                .and_then(Value::as_array)
                .ok_or(UNAVAILABLE)?;
            if !names.insert(name) || users.len() > count as usize {
                return Err(UNAVAILABLE);
            }
            let mut unique = BTreeSet::new();
            let mut ids = Vec::with_capacity(users.len());
            for user in users {
                let id = user.as_str().filter(|id| user_id(id)).ok_or(UNAVAILABLE)?;
                if !unique.insert(id) {
                    return Err(UNAVAILABLE);
                }
                ids.push(id.to_owned());
            }
            reactions.push(Reaction {
                name: name.into(),
                users: ids,
                count,
                extra: BTreeMap::new(),
            });
        }
        Ok(Self { reactions })
    }
    pub fn reactions(&self) -> &[Reaction] {
        &self.reactions
    }
    /// Pure exact-name fold. The caller must bind self_user to the original
    /// authenticated actor and name to the current genuine picker selection.
    /// This result does not authorize an add or a remove.
    pub fn adding(&self, name: &str, self_user: &str) -> bool {
        reaction_name(name)
            && user_id(self_user)
            && !self
                .reactions
                .iter()
                .any(|r| r.name == name && r.users.iter().any(|id| id == self_user))
    }
}

fn envelope(raw: &Value) -> Result<()> {
    if !raw.is_object()
        || raw.get("ok") != Some(&Value::Bool(true))
        || raw.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn coordinate(channel: &str, ts: &str) -> Result<()> {
    let Some((seconds, fraction)) = ts.split_once('.') else {
        return Err(UNAVAILABLE);
    };
    if !(2..=255).contains(&channel.len())
        || !matches!(channel.as_bytes()[0], b'C' | b'G')
        || !channel
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        || seconds.len() != 10
        || fraction.len() != 6
        || !seconds
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn optional_channel(raw: &Value, channel: &str) -> Result<()> {
    if raw
        .get("channel")
        .is_some_and(|v| v.as_str() != Some(channel))
    {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn absent_or_false(raw: &Value, key: &str) -> Result<()> {
    if raw.get(key).is_some_and(|v| v != &Value::Bool(false)) {
        return Err(UNAVAILABLE);
    }
    Ok(())
}
fn ordinary_message(message: &Value, channel: &str, ts: &str) -> Result<()> {
    if !message.is_object()
        || message.get("type").and_then(Value::as_str) != Some("message")
        || message.get("ts").and_then(Value::as_str) != Some(ts)
        || message.get("subtype").is_some_and(|v| !v.is_null())
        || message
            .get("thread_ts")
            .is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
    {
        return Err(UNAVAILABLE);
    }
    optional_channel(message, channel)?;
    // vkwm's fresh incoming whitelist carries is_ephemeral and excludes the
    // failed outgoing-message isEKMError property. No previous model or send
    // placeholder is inherited. Present malformed/denying values still refuse.
    for key in [
        "is_ephemeral",
        "isEKMError",
        "isUnknown",
        "isNonExistent",
        "hidden",
        "is_hidden",
        "deleted",
        "is_deleted",
        "is_tombstone",
    ] {
        absent_or_false(message, key)?;
    }
    Ok(())
}
fn reaction_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && !name.chars().any(|c| c.is_whitespace() || c.is_control())
}
fn user_id(id: &str) -> bool {
    (2..=255).contains(&id.len())
        && matches!(id.as_bytes()[0], b'U' | b'W')
        && id
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

#[cfg(test)]
mod tests;

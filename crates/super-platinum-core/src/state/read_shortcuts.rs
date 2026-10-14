//! Explicit reading shortcuts; renderer focus and keyboard precedence stay outside core.
//!
//! Matching retained source (decoded character offsets):
//! R a66a78eeac9bb080 SHA 6050af061bf031abcf78b5f19300a89f8c328bb4f2413b2bc88de72ee24a9382,
//! 608486/610819 confirmation and bulk clear.
//! B 457d4b99bff69d13 SHA 7cbc5803c7f0e18ab41d443c254205dae9bc58b9da2e1d7cff2b99c4cd2dc88f,
//! 4606620 channel eligibility; 16648779 thread Escape; 2518757 real thread timestamp.
//! S c8d6ded6605ce42d SHA 104c0dd99720b7391f5724ea2eb6d439a4249c017fb6f743f664fdc91ec4a693,
//! 3705366 latest metadata versus non-temporary loaded timestamps; 5337753 raw pref.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const CONFIRM_CLEAR_UNREADS: &str = "confirm_clear_all_unreads";

/// Separate from the two-key Reading radio patch. An absent boot container or
/// old cache supplies no authority; a complete empty prefs object supplies a
/// missing (source-falsy) value. Retain malformed values to revoke old policy.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClearUnreadsPreference {
    #[serde(default)]
    loaded: bool,
    #[serde(default)]
    raw: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearUnreadsPolicyError {
    Unavailable,
    UnsupportedValue,
}

impl ClearUnreadsPreference {
    pub fn from_prefs(prefs: &BTreeMap<String, Value>) -> Self {
        Self {
            loaded: true,
            raw: prefs
                .iter()
                .filter(|(key, _)| key.as_str() == CONFIRM_CLEAR_UNREADS)
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        }
    }

    /// Caller must preserve BootData.prefs presence before invoking this.
    pub fn apply_boot(&mut self, prefs: &crate::slack::models::BootPrefs) {
        *self = Self::from_prefs(&prefs.extra);
    }

    /// Keep partial events partial: a targeted complete GET or boot establishes
    /// authority. This matches the existing synchronized Reading model.
    pub fn apply_change(&mut self, name: &str, value: Value) -> bool {
        if name != CONFIRM_CLEAR_UNREADS || self.raw.get(name) == Some(&value) {
            return false;
        }
        self.raw.insert(name.to_owned(), value);
        true
    }

    pub fn loaded(&self) -> bool {
        self.loaded
    }
    pub fn raw(&self) -> &BTreeMap<String, Value> {
        &self.raw
    }

    pub fn requires_confirmation(&self) -> Result<bool, ClearUnreadsPolicyError> {
        if !self.loaded {
            return Err(ClearUnreadsPolicyError::Unavailable);
        }
        match self.raw.get(CONFIRM_CLEAR_UNREADS) {
            None => Ok(false),
            Some(Value::Bool(value)) => Ok(*value),
            // Defensive boundary, not a claim that Slack's JS coercion is strict.
            Some(_) => Err(ClearUnreadsPolicyError::UnsupportedValue),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LoadedReadTimestamp<'a> {
    pub ts: &'a str,
    /// App optimistic IDs need not use Slack's temporary `x` encoding.
    pub pending: bool,
}

/// Source S3705366 uses channelLatests plus loaded timestamps, excluding temp
/// timestamps (S3856314). Numeric comparison retains exact microseconds and
/// rejects malformed coordinates instead of silently treating them as zero.
pub fn latest_channel_read_target<'a>(
    metadata_latest: Option<&'a str>,
    loaded: impl IntoIterator<Item = LoadedReadTimestamp<'a>>,
) -> Option<&'a str> {
    metadata_latest
        .into_iter()
        .chain(
            loaded
                .into_iter()
                .filter(|row| !row.pending)
                .map(|row| row.ts),
        )
        .filter_map(|ts| timestamp(ts).map(|key| (key, ts)))
        .max_by_key(|(key, _)| *key)
        .map(|(_, ts)| ts)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelEscape<'a> {
    /// Does not consume the renderer's ordinary close behavior.
    NoRead,
    /// Consume Escape and clear the ghost divider; no cursor request is needed.
    AlreadyAtLatest {
        repair_unread_count: bool,
    },
    Mark {
        ts: &'a str,
    },
}

/// Caller establishes the active primary channel and shortcut attention first.
/// An explicit keyboard mark is independent of the automatic Reading mode.
pub fn channel_escape<'a>(
    unread_count: Option<u64>,
    ghost_divider: bool,
    last_read: Option<&str>,
    latest: Option<&'a str>,
) -> ChannelEscape<'a> {
    if !ghost_divider && !unread_count.is_some_and(|count| count > 0) {
        return ChannelEscape::NoRead;
    }
    let Some((latest, key)) = latest.and_then(|ts| timestamp(ts).map(|key| (ts, key))) else {
        return ChannelEscape::NoRead;
    };
    // Source uses equality and permits opting out of backwards movement. Our
    // bounded cache cannot guarantee its complete latest-store invariant: never
    // turn an explicit read into a backwards unread cursor from stale history.
    if last_read
        .and_then(timestamp)
        .is_some_and(|cursor| cursor > key)
    {
        ChannelEscape::AlreadyAtLatest {
            repair_unread_count: false,
        }
    } else if last_read.and_then(timestamp) == Some(key) {
        ChannelEscape::AlreadyAtLatest {
            repair_unread_count: unread_count.is_some_and(|n| n > 0),
        }
    } else {
        ChannelEscape::Mark { ts: latest }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadEscape<'a> {
    NoRead,
    ClearDivider,
    Mark { ts: &'a str },
}

/// Ordinary subscribed message threads only. Canvas/Quip uses another mutation
/// path (B5542869); the renderer must route that type separately. `max_markable`
/// must come from present loaded thread rows, excluding pending/ephemeral rows,
/// not channel metadata or an arbitrary root/latest_reply fallback.
/// S3856198/3856314 defines temporary timestamps; S3838502 exports iub5.aw
/// as w9uO.isEphemeral (S3861208), not a nonexistent/deleted-message predicate.
pub fn thread_escape<'a>(
    subscribed: Option<bool>,
    initial_last_read: Option<&str>,
    last_read: Option<&str>,
    max_markable: Option<&'a str>,
) -> ThreadEscape<'a> {
    if subscribed != Some(true) {
        return ThreadEscape::NoRead;
    }
    let Some((ts, maximum)) = max_markable.and_then(|ts| timestamp(ts).map(|key| (ts, key))) else {
        return ThreadEscape::NoRead;
    };
    // A malformed present cursor cannot authorize a new mark. Missing lastRead
    // remains distinct and can be marked for a known subscribed thread.
    if last_read.is_some_and(|ts| timestamp(ts).is_none())
        || initial_last_read.is_some_and(|ts| timestamp(ts).is_none())
    {
        return ThreadEscape::NoRead;
    }
    // As above, protect an authoritative cursor ahead of a partial thread bag.
    if last_read
        .and_then(timestamp)
        .is_some_and(|cursor| cursor >= maximum)
    {
        if initial_last_read.is_some_and(|initial| Some(initial) != last_read) {
            ThreadEscape::ClearDivider
        } else {
            ThreadEscape::NoRead
        }
    } else {
        ThreadEscape::Mark { ts }
    }
}

fn timestamp(ts: &str) -> Option<(u64, u32)> {
    let (seconds, fraction) = ts.split_once('.')?;
    if seconds.is_empty()
        || !seconds.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_empty()
        || fraction.len() > 6
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some((
        seconds.parse().ok()?,
        fraction.parse::<u32>().ok()? * 10u32.pow(6 - fraction.len() as u32),
    ))
}

#[cfg(test)]
mod tests;

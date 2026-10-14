//! Synchronized conversation opening and automatic read policy.
//!
//! Retained Slack B (SHA 1681ecc18bbfead52efb391b1ccfb3e15f1b4a1093a8475d847b221ce196c537)
//! at 10080231 / 26072315 defines the three modes and their two boolean prefs.
//! S (SHA f085df09fb4f938caf425ea66fb0244c8f92a8012eea8eb298e4f629b377dbf7)
//! at 5014585 defines opening targets; B at 6277510 / 28126472 defines read
//! eligibility. Offsets are decoded characters, not byte offsets.
//!
//! Renderer attention, divider visibility and saved scroll offsets are inputs,
//! never inferred from which messages happen to be loaded in the cache.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::slack::models::BootPrefs;

pub const READ_PREFERENCE_NAMES: [&str; 2] =
    ["mark_msgs_read_immediately", "start_scroll_at_oldest"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadMode {
    LeftOffAndRead,
    NewestAndRead,
    NewestKeepUnseen,
}

impl ReadMode {
    /// B 26072315 sends both keys, as JSON booleans, for every radio choice.
    pub fn patch(self) -> BTreeMap<String, Value> {
        BTreeMap::from([
            (
                READ_PREFERENCE_NAMES[0].into(),
                Value::Bool(self != Self::NewestKeepUnseen),
            ),
            (
                READ_PREFERENCE_NAMES[1].into(),
                Value::Bool(self == Self::LeftOffAndRead),
            ),
        ])
    }
}

/// Missing and malformed keys remain distinct. Older cache records have no
/// preference authority; they must not authorize a read before boot/refresh.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReadPreferences {
    #[serde(default)]
    loaded: bool,
    #[serde(default)]
    raw: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadPreferenceError {
    Unavailable,
    UnsupportedValue,
}

impl ReadPreferences {
    /// A complete requested snapshot, unlike one partial pref_change event.
    pub fn from_prefs(prefs: &BTreeMap<String, Value>) -> Self {
        Self {
            loaded: true,
            raw: prefs
                .iter()
                .filter(|(key, _)| is_read_preference(key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        }
    }

    pub fn apply_boot(&mut self, prefs: &BootPrefs) {
        *self = Self::from_prefs(&prefs.extra);
    }

    /// A partial event changes only its own key and does not turn an old cache
    /// into a complete snapshot. Malformed values invalidate policy instead of
    /// retaining an older permission to mark conversations read.
    pub fn apply_change(&mut self, name: &str, value: Value) -> bool {
        if !is_read_preference(name) || self.raw.get(name) == Some(&value) {
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

    pub fn mode(&self) -> Result<ReadMode, ReadPreferenceError> {
        if !self.loaded {
            return Err(ReadPreferenceError::Unavailable);
        }
        // S 5314629 returns the raw stored preference without injecting a
        // default. Missing fields are falsy in B's selector. The supported wire
        // shape is boolean; string/object/null coercion is deliberately refused
        // rather than guessing what a malformed server preference means.
        let immediate = flag(self.raw.get(READ_PREFERENCE_NAMES[0]))?;
        let oldest = flag(self.raw.get(READ_PREFERENCE_NAMES[1]))?;
        Ok(match (immediate, oldest) {
            (true, true) => ReadMode::LeftOffAndRead,
            (true, false) => ReadMode::NewestAndRead,
            (false, _) => ReadMode::NewestKeepUnseen,
        })
    }

    pub fn opening_target<'a>(
        &self,
        explicit: Option<&'a str>,
        has_unreads: bool,
        last_read: Option<&'a str>,
        saved: Option<ReadScrollmark<'a>>,
    ) -> Result<ReadOpeningTarget<'a>, ReadPreferenceError> {
        // An explicit timestamp is independent of the user's opening mode.
        if let Some(ts) = explicit.filter(|ts| !ts.is_empty()) {
            return Ok(ReadOpeningTarget::Explicit(ts));
        }
        if self.mode()? != ReadMode::LeftOffAndRead {
            return Ok(ReadOpeningTarget::Latest);
        }
        if has_unreads {
            // S 5014585 clears the old scrollmark when a channel has unreads.
            return Ok(last_read
                .filter(|ts| !ts.is_empty())
                .map(ReadOpeningTarget::FirstUnreadAfter)
                .unwrap_or(ReadOpeningTarget::Latest));
        }
        Ok(saved
            .filter(|mark| !mark.ts.is_empty() && mark.offset.is_finite())
            .map(ReadOpeningTarget::Saved)
            .unwrap_or(ReadOpeningTarget::Latest))
    }

    /// S 5015789 marks both immediate modes on a new channel view. The caller
    /// supplies current attention/member/manual-hold/navigation ownership and
    /// excludes navigation with preventMoveFocus. This is not a receipt reducer.
    pub fn mark_on_open(&self, eligible: bool, prevent_move_focus: bool) -> bool {
        eligible
            && !prevent_move_focus
            && matches!(
                self.mode(),
                Ok(ReadMode::LeftOffAndRead | ReadMode::NewestAndRead)
            )
    }

    /// B 6277510: do not substitute "at bottom" for seeing the unread divider.
    /// This controls dispatch/counting only; existing cursor reducers continue
    /// to own acknowledgements, newer arrivals and manual-unread cursor races.
    pub fn should_auto_mark(&self, seen: ReadObservation) -> bool {
        let Ok(mode) = self.mode() else {
            return false;
        };
        seen.eligible
            && (mode == ReadMode::NewestAndRead
                || seen.unread_line_visible()
                || seen.cursor_covers_latest
                || (seen.channel_read && seen.latest_from_self))
    }
}

pub fn is_read_preference(name: &str) -> bool {
    READ_PREFERENCE_NAMES.contains(&name)
}

fn flag(value: Option<&Value>) -> Result<bool, ReadPreferenceError> {
    match value {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(ReadPreferenceError::UnsupportedValue),
    }
}

/// Renderer-owned pixel offset for a particular message in this conversation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadScrollmark<'a> {
    pub ts: &'a str,
    pub offset: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReadOpeningTarget<'a> {
    Explicit(&'a str),
    FirstUnreadAfter(&'a str),
    Saved(ReadScrollmark<'a>),
    Latest,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadObservation {
    /// Current, focused, member conversation in an active message pane, with a
    /// valid latest message, no pending navigation and no manual unread hold.
    /// Account/transport/channel generations are checked by the desktop caller.
    pub eligible: bool,
    pub loading: bool,
    pub divider_in_viewport: bool,
    pub ghost_divider: bool,
    /// Unknown is not zero. This must be the conversation's unread state, not
    /// the count of currently rendered/loaded message rows.
    pub unread_count: Option<u64>,
    pub reached_end: bool,
    pub at_bottom: bool,
    pub cursor_covers_latest: bool,
    pub channel_read: bool,
    pub latest_from_self: bool,
}

impl ReadObservation {
    /// B 28126472. A ghost divider pinned after a read cannot prove that an
    /// unseen backlog has been viewed. Loading suppresses this visibility path.
    pub fn unread_line_visible(self) -> bool {
        !self.loading
            && ((!self.ghost_divider && self.divider_in_viewport)
                || (self.unread_count == Some(0) && self.reached_end && self.at_bottom))
    }
}

#[cfg(test)]
mod tests;

//! Retained calendar-event producer for meeting-channel display acquisition.
//!
//! Pure helpers only: the desktop owns account/query/credential generations,
//! async dispatch, Retry-After, persistence and renderer locale custody.
//! No failed/malformed read or uninitialized store is an observed missing event.
use serde_json::Value;
use std::collections::BTreeMap;

pub mod request;
mod window;
pub use window::{EventWindow, WindowPage, decode_window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    InvalidEnvelope,
    InvalidIdentity,
    InvalidEvent,
    Unresolved,
    Syncing,
    Capacity,
    RefreshRequired,
}

// Bounded native admission policy, not a Slack identifier/response limit.
fn identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4096
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
}
fn positive(raw: &Value) -> Result<(), Fault> {
    if raw.get("ok").and_then(Value::as_bool) == Some(true)
        && raw.get("error").is_none_or(Value::is_null)
    {
        Ok(())
    } else {
        Err(Fault::InvalidEnvelope)
    }
}

/// An actually acquired positive calendar.user.status response. Absence of
/// this value is different from an acquired response with no connected row.
#[derive(Clone, Debug, PartialEq)]
pub struct CalendarStatus {
    raw: Value,
    by_app: BTreeMap<String, Value>,
}
impl CalendarStatus {
    pub fn decode(raw: Value) -> Result<Self, Fault> {
        positive(&raw)?;
        let rows = raw["status"].as_array().ok_or(Fault::InvalidEnvelope)?;
        // CsjJ rejects an empty status array rather than recording disconnect.
        if rows.is_empty() || rows.len() > 128 {
            return Err(Fault::InvalidEnvelope);
        }
        let mut by_app = BTreeMap::new();
        for row in rows {
            let app = row["cal_app_type"].as_str().ok_or(Fault::InvalidEnvelope)?;
            let status = row["user_status"].as_str().ok_or(Fault::InvalidEnvelope)?;
            if !identity(app) || !identity(status) {
                return Err(Fault::InvalidEnvelope);
            }
            // Source dispatches each row in order; later same-app rows win.
            by_app.insert(app.to_owned(), row.clone());
        }
        Ok(Self { raw, by_app })
    }
    pub fn connected(&self) -> bool {
        self.by_app
            .values()
            .any(|row| row["user_status"] == "connected")
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}

/// Full raw event retained; no attachment/title/unknown extension is discarded.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    raw: Value,
}
impl Event {
    fn from_raw(raw: Value) -> Result<Self, Fault> {
        let id = raw
            .get("id")
            .and_then(Value::as_str)
            .ok_or(Fault::InvalidEvent)?;
        if !identity(id)
            || raw.get("isUnknown") == Some(&Value::Bool(true))
            || raw.get("isNonExistent") == Some(&Value::Bool(true))
        {
            return Err(Fault::InvalidEvent);
        }
        if let Some(link) = raw.get("event_link_id").filter(|v| !v.is_null()) {
            if !link.as_str().is_some_and(identity) {
                return Err(Fault::InvalidEvent);
            }
        }
        for key in ["is_deleted", "is_all_day"] {
            if raw.get(key).is_some_and(|v| !v.is_boolean()) {
                return Err(Fault::InvalidEvent);
            }
        }
        Ok(Self { raw })
    }
    pub fn id(&self) -> &str {
        self.raw["id"].as_str().expect("validated")
    }
    pub fn link(&self) -> Option<&str> {
        self.raw["event_link_id"].as_str()
    }
    pub fn deleted(&self) -> bool {
        self.raw["is_deleted"] == true
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    /// The renderer must execute source-equivalent entity decode/truncation
    /// and locale/date formatting. No UTC/English/default preference is guessed.
    pub fn format_input(&self) -> Result<FormatInput<'_>, Fault> {
        if self.deleted() {
            return Err(Fault::Unresolved);
        }
        let title = match self.raw.get("title") {
            None | Some(Value::Null) => "",
            Some(Value::String(s)) => s,
            _ => return Err(Fault::InvalidEvent),
        };
        let start = match self.raw.get("start") {
            None => None,
            Some(v) => Some(
                v.as_f64()
                    .filter(|n| n.is_finite())
                    .ok_or(Fault::InvalidEvent)?,
            ),
        };
        Ok(FormatInput {
            raw_title: title,
            start,
            all_day: self.raw["is_all_day"] == true,
        })
    }
}

/// Unknown, malformed and unsuccessful envelopes remain Err, never absence.
/// Null/missing response event_link_id uses the requested link as in 2vjo.
/// A different positive link is refused by the native exact-identity boundary.
pub fn decode_event(expected_link: &str, raw: &Value) -> Result<Event, Fault> {
    if !identity(expected_link) {
        return Err(Fault::InvalidIdentity);
    }
    positive(raw)?;
    let mut value = raw
        .get("event")
        .filter(|v| v.is_object())
        .cloned()
        .ok_or(Fault::Unresolved)?;
    match value.get("event_link_id") {
        None | Some(Value::Null) => value["event_link_id"] = Value::String(expected_link.into()),
        Some(Value::String(link)) if link == expected_link => (),
        _ => return Err(Fault::InvalidIdentity),
    }
    Event::from_raw(value)
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct FormatInput<'a> {
    #[serde(rename = "title")]
    pub raw_title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(rename = "allDay")]
    pub all_day: bool,
}

/// Explicit acquisition states for parent-owned snapshots. Removed requires
/// positive event deletion evidence; no API error-to-not-found mapping exists.
#[derive(Clone, Debug, PartialEq)]
pub enum Observation {
    Unrequested,
    Loading,
    Known(Event),
    Removed { event_id: String },
    Unavailable,
}

/// Parent-issued nonsecret identity witness. The caller supplies its real
/// generation counters; this type never grants authority from a default value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadOwner {
    pub team: String,
    pub user: String,
    pub credential_generation: u64,
    pub query_generation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRead {
    owner: ReadOwner,
    link: String,
}
impl EventRead {
    pub fn capture(owner: ReadOwner, link: String) -> Result<Self, Fault> {
        if !identity(&owner.team) || !identity(&owner.user) || !identity(&link) {
            return Err(Fault::InvalidIdentity);
        }
        Ok(Self { owner, link })
    }
    pub fn current(&self, owner: &ReadOwner, link: &str) -> bool {
        &self.owner == owner && self.link == link
    }
    pub fn adopt(&self, owner: &ReadOwner, link: &str, raw: &Value) -> Result<Event, Fault> {
        if !self.current(owner, link) {
            return Err(Fault::Unresolved);
        }
        decode_event(&self.link, raw)
    }
}

#[cfg(test)]
mod tests;

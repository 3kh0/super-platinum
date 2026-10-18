//! Captured Calendar shortcut surface (I5zS -> 2nlw -> 9AoV/6T+a).
//! It is not an apps.actions.v2.execute action or a server-owned modal.
use super::{Action, Kind, identifier};
use crate::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};
use serde_json::Value;
use std::collections::BTreeSet;
mod request;
pub mod time;
pub use request::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Google,
    Outlook,
    OutlookGcc,
}
impl Provider {
    pub fn for_action(action: &Action) -> Option<Self> {
        (action.kind == Kind::Global)
            .then(|| Self::for_app(&action.app))
            .flatten()
    }
    pub fn for_app(app: &str) -> Option<Self> {
        match app {
            "A0CNP2EP5" | "ADZ494LHY" => Some(Self::Google),
            "A07H429LK" | "AFV5ECLBZ" => Some(Self::Outlook),
            "A0608DQ3P" | "A1S0QJ0SQ" | "A012Z6404M8" | "A01NK8UJRGC" => Some(Self::OutlookGcc),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Google => "gcal",
            Self::Outlook => "ocal",
            Self::OutlookGcc => "ocal_gcc",
        }
    }
    pub fn wire(self) -> &'static str {
        match self {
            Self::Google => "GCAL",
            Self::Outlook => "OCAL",
            Self::OutlookGcc => "OCAL_GCC",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Google => "Google Calendar",
            Self::Outlook => "Outlook Calendar",
            Self::OutlookGcc => "Outlook Calendar for Government",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Account {
    Modern,
    Legacy { user: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connection {
    Connected(Account),
    Connect { url: String },
    Unavailable,
}
pub fn invalid() -> Error {
    Error::Api("calendar_response_invalid".into())
}
pub fn checked(raw: &Value) -> Result<(), Error> {
    if raw.get("ok") != Some(&Value::Bool(true)) || raw.to_string().len() > 2 * 1024 * 1024 {
        Err(invalid())
    } else {
        Ok(())
    }
}
/// An absent flag in a complete assignment response is the captured legacy arm.
/// An unavailable or malformed response is never silently treated as legacy.
pub fn modern(raw: &Value) -> Result<bool, Error> {
    checked(raw)?;
    let assignments = raw
        .get("assignments")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    match assignments.get("calendar2webapp_create_event") {
        None => Ok(false),
        Some(v) => v
            .get("group")
            .and_then(Value::as_str)
            .map(|s| s == "on")
            .ok_or_else(invalid),
    }
}
pub fn external_url(s: &str) -> bool {
    s.len() <= 8192
        && url::Url::parse(s).is_ok_and(|u| {
            matches!(u.scheme(), "http" | "https")
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
        })
}
pub fn connection(raw: &Value) -> Result<Connection, Error> {
    checked(raw)?;
    if let Some(user) = raw.get("cal_user").filter(|x| !x.is_null()) {
        if user.get("is_provider_connected") == Some(&Value::Bool(true)) {
            let id = user
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| identifier(s))
                .ok_or_else(invalid)?;
            return Ok(Connection::Connected(Account::Legacy { user: id.into() }));
        }
        if let Some(url) = user
            .get("provider_auth_url")
            .and_then(Value::as_str)
            .filter(|s| external_url(s))
        {
            return Ok(Connection::Connect { url: url.into() });
        }
        return Ok(Connection::Unavailable);
    }
    Ok(raw
        .get("auth_url")
        .and_then(Value::as_str)
        .filter(|s| external_url(s))
        .map_or(Connection::Unavailable, |url| Connection::Connect {
            url: url.into(),
        }))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub name: String,
}
fn choices(raw: &Value, key: &str) -> Result<Vec<Choice>, Error> {
    checked(raw)?;
    let rows = raw
        .get(key)
        .and_then(Value::as_array)
        .filter(|v| v.len() <= 512)
        .ok_or_else(invalid)?;
    let mut seen = BTreeSet::new();
    rows.iter()
        .map(|v| {
            let id = v
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| identifier(s))
                .ok_or_else(invalid)?;
            let name = v
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control))
                .ok_or_else(invalid)?;
            if !seen.insert(id) {
                return Err(invalid());
            }
            Ok(Choice {
                id: id.into(),
                name: name.into(),
            })
        })
        .collect()
}
pub fn calendars(raw: &Value) -> Result<Vec<Choice>, Error> {
    choices(raw, "connected_calendars")
}
pub fn conferences(raw: &Value) -> Result<Vec<Choice>, Error> {
    choices(raw, "providers")
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub start: i64,
    pub end: i64,
    pub all_free: bool,
    pub busy: Vec<String>,
}
pub fn slots(raw: &Value) -> Result<Vec<Slot>, Error> {
    checked(raw)?;
    let mut seen = BTreeSet::new();
    raw.get("freebusy_slots")
        .and_then(Value::as_array)
        .filter(|v| v.len() <= 1024)
        .ok_or_else(invalid)?
        .iter()
        .map(|v| {
            let start = v.get("start").and_then(Value::as_i64).ok_or_else(invalid)?;
            let end = v.get("end").and_then(Value::as_i64).ok_or_else(invalid)?;
            if start <= 0
                || end <= start
                || !time::valid_timestamp(start)
                || !time::valid_timestamp(end)
                || !seen.insert(start)
            {
                return Err(invalid());
            }
            let all_free = v
                .get("all_free")
                .and_then(Value::as_bool)
                .ok_or_else(invalid)?;
            let busy = match v.get("busy_attendees") {
                None | Some(Value::Null) => vec![],
                Some(v) => user_ids(v)?,
            };
            Ok(Slot {
                start,
                end,
                all_free,
                busy,
            })
        })
        .collect()
}
fn user_ids(v: &Value) -> Result<Vec<String>, Error> {
    let v = v
        .as_array()
        .filter(|v| v.len() <= 512)
        .ok_or_else(invalid)?;
    let mut seen = BTreeSet::new();
    v.iter()
        .map(|x| {
            let s = x
                .as_str()
                .filter(|s| {
                    identifier(s)
                        && s.starts_with(['U', 'W'])
                        && s.bytes().all(|c| c.is_ascii_alphanumeric())
                })
                .ok_or_else(invalid)?;
            if !seen.insert(s) {
                return Err(invalid());
            }
            Ok(s.into())
        })
        .collect()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub start: i64,
    pub end: i64,
    pub title: String,
    pub description: String,
    pub attendees: Vec<String>,
    pub calendar: String,
    pub meeting_url: String,
    pub share_channel: String,
    pub share_thread: String,
}
impl Event {
    pub fn valid(&self) -> bool {
        self.start > 0
            && self.end > self.start
            && time::valid_timestamp(self.start)
            && time::valid_timestamp(self.end)
            && self.title.len() <= 4096
            && self.description.len() <= 16384
            && !self.title.chars().any(|c| c.is_control() && c != '\n')
            && !self
                .description
                .chars()
                .any(|c| c.is_control() && c != '\n')
            && (self.calendar.is_empty() || identifier(&self.calendar))
            && (self.meeting_url.is_empty() || external_url(&self.meeting_url))
            && user_ids(&serde_json::json!(self.attendees)).is_ok()
            && (self.share_channel.is_empty() || identifier(&self.share_channel))
            && (self.share_thread.is_empty()
                || (!self.share_channel.is_empty()
                    && super::Context {
                        channel: Some(self.share_channel.clone()),
                        message_ts: Some(self.share_thread.clone()),
                    }
                    .validate(Kind::Message)))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventResult {
    Created {
        provider_link: Option<String>,
        app_link: Option<String>,
    },
    Rejected,
    Unknown,
}
pub fn event_result(raw: &Value) -> EventResult {
    if raw.to_string().len() > 2 * 1024 * 1024 {
        return EventResult::Unknown;
    }
    match raw.get("ok").and_then(Value::as_bool) {
        Some(true) => EventResult::Created {
            provider_link: raw
                .get("provider_event_link")
                .and_then(Value::as_str)
                .filter(|s| external_url(s))
                .map(str::to_owned),
            app_link: raw
                .get("app_home_deep_link")
                .and_then(Value::as_str)
                .filter(|s| s.len() <= 8192)
                .map(str::to_owned),
        },
        Some(false)
            if raw
                .get("error")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty() && s != "dispatch_failed") =>
        {
            EventResult::Rejected
        }
        _ => EventResult::Unknown,
    }
}
pub fn room_url(raw: &Value) -> Result<String, Error> {
    checked(raw)?;
    raw.get("url")
        .and_then(Value::as_str)
        .filter(|s| external_url(s))
        .map(str::to_owned)
        .ok_or_else(invalid)
}
#[cfg(test)]
mod tests;

/// Captured isUndefined(value) || !!value; getUserPrefsByApi leaves this non-JSON
/// preference untouched. Absent is true, unlike explicit false/null/zero.
pub fn share_enabled(raw: &Value) -> Result<bool, Error> {
    checked(raw)?;
    let prefs = raw
        .get("prefs")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    Ok(match prefs.get("calendar_share_event_in_channel") {
        None => true,
        Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    })
}

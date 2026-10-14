//! Captured SN+s / DndPreferences / JiU7 recurring allowed-notification hours.
use super::*;
use serde_json::{Value, json};
mod intent;
mod presentation;
pub use intent::*;
pub use presentation::*;
#[cfg(test)]
mod tests;

pub const SCHEDULE_DAYS: [ScheduleDay; 7] = ScheduleDay::ALL;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleDay {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}
impl ScheduleDay {
    pub const ALL: [Self; 7] = [
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
        Self::Sunday,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Monday => "monday",
            Self::Tuesday => "tuesday",
            Self::Wednesday => "wednesday",
            Self::Thursday => "thursday",
            Self::Friday => "friday",
            Self::Saturday => "saturday",
            Self::Sunday => "sunday",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleMode {
    EveryDay,
    Weekdays,
    Custom,
}
impl ScheduleMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EveryDay => "every_day",
            Self::Weekdays => "weekdays",
            Self::Custom => "custom",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleBound {
    Start,
    End,
}
/// Missing keys and explicit null are retained distinctly for preflight/readback.
#[derive(Clone, Debug, PartialEq)]
pub struct NotificationSchedule {
    raw: BTreeMap<String, Value>,
    timezone: Option<(String, String)>,
}
pub fn notification_schedule_pref_names() -> Vec<String> {
    let mut keys = [
        "dnd_enabled",
        "dnd_days",
        "dnd_start_hour",
        "dnd_end_hour",
        "time24",
        "tz",
        "set_tz_automatically",
    ]
    .map(str::to_owned)
    .to_vec();
    for day in ScheduleDay::ALL {
        for prefix in ["enabled", "before", "after"] {
            keys.push(format!("dnd_{prefix}_{}", day.as_str()));
        }
    }
    keys
}
pub fn is_notification_schedule_pref(name: &str) -> bool {
    matches!(
        name,
        "dnd_enabled"
            | "dnd_days"
            | "dnd_start_hour"
            | "dnd_end_hour"
            | "time24"
            | "tz"
            | "set_tz_automatically"
    ) || ScheduleDay::ALL.iter().any(|day| {
        ["enabled", "before", "after"]
            .iter()
            .any(|p| name == format!("dnd_{p}_{}", day.as_str()))
    })
}
impl NotificationSchedule {
    pub fn from_response(value: &Value) -> Result<Self, Error> {
        super::super::transport::notification_preferences_success(value)?;
        let prefs = value
            .get("prefs")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        Self::from_prefs(&prefs.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }
    pub fn from_prefs(prefs: &BTreeMap<String, Value>) -> Result<Self, Error> {
        let schedule = Self {
            timezone: None,
            raw: prefs
                .iter()
                .filter(|(k, _)| is_notification_schedule_pref(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };
        schedule.view()?;
        Ok(schedule)
    }
    pub fn raw(&self) -> &BTreeMap<String, Value> {
        &self.raw
    }
    pub fn with_user_timezone(mut self, user: &User) -> Result<Self, Error> {
        let zone = user
            .tz
            .as_deref()
            .filter(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
            .ok_or_else(|| Error::Api("notification_schedule_timezone_unavailable".into()))?;
        if user.id.is_empty() {
            return Err(invalid());
        }
        self.timezone = Some((user.id.clone(), zone.into()));
        Ok(self)
    }
    pub fn timezone(&self) -> Option<&str> {
        self.timezone.as_ref().map(|(_, zone)| zone.as_str())
    }
    fn value(&self, key: &str) -> Option<&Value> {
        self.raw.get(key)
    }
}
fn invalid() -> Error {
    Error::Transport("Slack returned unsupported notification schedule preferences".into())
}
fn rejected() -> Error {
    Error::Api("notification_schedule_selection_unavailable".into())
}
pub fn notification_schedule_request(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    c.rest_form(
        w,
        "users.prefs.get",
        vec![("prefs", notification_schedule_pref_names().join(","))],
    )
}
pub async fn fetch_notification_schedule(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
) -> Result<NotificationSchedule, Error> {
    let snapshot = NotificationSchedule::from_response(
        &t.execute_notification_schedule(notification_schedule_request(c, w))
            .await?,
    )?;
    let user = fetch_users_info(t, c, w, vec![w.user_id.clone()])
        .await?
        .into_iter()
        .find(|u| u.id == w.user_id)
        .ok_or_else(invalid)?;
    snapshot.with_user_timezone(&user)
}

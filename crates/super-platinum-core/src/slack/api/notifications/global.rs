//! NotificationsChannelDefaults/NotificationsKeywordsInput and Nu4c global writes.
use super::*;
use serde_json::Map;
mod intent;
pub use intent::*;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalNotificationSettings {
    pub desktop: Option<ChannelNotificationMode>,
    pub mobile: Option<ChannelNotificationMode>,
    pub desktop_push: bool,
    pub mobile_enabled: bool,
    /// Initial mounted UI state only. Reveal/separate state belongs to the view.
    pub linked_mobile: bool,
    pub thread_replies: bool,
    pub keywords: String,
}
impl NotificationPreferences {
    pub fn global_settings(&self) -> Result<GlobalNotificationSettings, Error> {
        let global = self.raw.get("global");
        let desktop = global.and_then(|g| g.get("global_desktop"));
        let mobile = global.and_then(|g| g.get("global_mobile"));
        let desktop_push = match global.and_then(|g| g.get("global_desktop_push_enabled")) {
            None | Some(Value::Null) => true,
            Some(Value::Bool(v)) => *v,
            _ => return Err(invalid_preferences()),
        };
        let thread_replies = match global.and_then(|g| g.get("threads_everything")) {
            None | Some(Value::Null) => false,
            Some(Value::Bool(v)) => *v,
            _ => return Err(invalid_preferences()),
        };
        let keywords = match global.and_then(|g| g.get("global_keywords")) {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.clone(),
            _ => return Err(invalid_preferences()),
        };
        Ok(GlobalNotificationSettings {
            desktop: project_mode(desktop)?,
            mobile: project_mode(mobile)?,
            desktop_push,
            mobile_enabled: mobile.and_then(Value::as_str) != Some("nothing"),
            linked_mobile: desktop == mobile,
            thread_replies,
            keywords,
        })
    }
}
fn project_mode(value: Option<&Value>) -> Result<Option<ChannelNotificationMode>, Error> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(v)) => match v.as_str() {
            "" => Ok(None),
            "everything" => Ok(Some(ChannelNotificationMode::Everything)),
            "mentions_dms" | "nothing" => Ok(Some(ChannelNotificationMode::Mentions)),
            _ => Err(invalid_preferences()),
        },
        _ => Err(invalid_preferences()),
    }
}
/// These are storage-edit transforms, not the notification matching algorithm.
/// Stock only inserts/removes one literal ASCII space immediately after commas.
pub fn notification_keywords_display(raw: &str) -> String {
    raw.replace(',', ", ")
}
pub fn normalize_notification_keywords(input: &str) -> Result<String, Error> {
    // Sfh9 split(/\s*,\s*/) retains empty fields; its count is commas + 1.
    if input.bytes().filter(|b| *b == b',').count() >= 100 {
        return Err(Error::Api("notification_keywords_too_many".into()));
    }
    Ok(input.replace(", ", ","))
}

//! SoundAppearancePref (B26124xxx) scalar settings; no platform-local preferences.
use super::*;
use serde_json::Map;
mod intent;
pub use intent::*;
#[cfg(test)]
mod tests;

/// LWDq.$O, S4751978. `none` is a separate choice; VIP `none` inherits messages.
pub const NOTIFICATION_SOUNDS: &[(&str, &str)] = &[
    ("b2.mp3", "Ding"),
    ("animal_stick.mp3", "Boing"),
    ("been_tree.mp3", "Drop"),
    ("complete_quest_requirement.mp3", "Ta-da"),
    ("confirm_delivery.mp3", "Plink"),
    ("flitterbug.mp3", "Wow"),
    ("here_you_go_lighter.mp3", "Here you go"),
    ("hi_flowers_hit.mp3", "Hi"),
    ("knock_brush.mp3", "Knock Brush"),
    ("save_and_checkout.mp3", "Whoa!"),
    ("item_pickup.mp3", "Yoink"),
    ("hummus.mp3", "Hummus"),
    ("boop.mp3", "Boop"),
];
pub fn valid_notification_sound(value: &str) -> bool {
    value == "none" || NOTIFICATION_SOUNDS.iter().any(|(name, _)| *name == value)
}
#[derive(Clone, Debug, PartialEq)]
pub struct PresentationNotificationPreferences {
    raw: Map<String, Value>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationNotificationSettings {
    pub desktop_sound: Option<String>,
    pub priority_sound: Option<String>,
    pub include_preview: bool,
    pub mute_sounds: bool,
    pub desktop_disabled: bool,
    /// Raw stored duration; unknown strings are preserved, never reauthored.
    pub mac_bounce: Option<String>,
}
impl PresentationNotificationPreferences {
    pub fn from_response(response: &Value) -> Result<Self, Error> {
        let canonical = parse_notification_preferences(response)?;
        let mut raw = response
            .get("prefs")
            .and_then(Value::as_object)
            .ok_or_else(invalid_preferences)?
            .clone();
        raw.insert("all_notifications_prefs".into(), canonical.raw);
        let result = Self { raw };
        result.settings()?;
        Ok(result)
    }
    pub fn raw(&self) -> &Map<String, Value> {
        &self.raw
    }
    fn global(&self) -> Option<&Value> {
        self.raw["all_notifications_prefs"].get("global")
    }
    pub fn settings(&self) -> Result<PresentationNotificationSettings, Error> {
        let global = self.global();
        Ok(PresentationNotificationSettings {
            desktop_sound: sound(global.and_then(|g| g.get("desktop_sound")))?,
            priority_sound: sound(global.and_then(|g| g.get("priority_desktop_sound")))?,
            include_preview: !truthy(global.and_then(|g| g.get("no_text_in_notifications"))),
            mute_sounds: truthy(self.raw.get("mute_sounds")),
            mac_bounce: sound(self.raw.get("mac_ssb_bounce"))?,
            desktop_disabled: global
                .and_then(|g| g.get("global_desktop"))
                .and_then(Value::as_str)
                == Some("nothing"),
        })
    }
}
fn sound(value: Option<&Value>) -> Result<Option<String>, Error> {
    match value {
        None | Some(Value::Null) => Ok(None),
        // Unknown canonical names remain visible/preserved, but cannot be authored.
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(invalid_preferences()),
    }
}
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(v)) => *v,
        Some(Value::String(v)) => !v.is_empty(),
        Some(Value::Number(v)) => v.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}
pub fn presentation_notification_preferences_request(
    c: &SlackClient,
    w: &WorkspaceSession,
) -> PreparedRequest {
    c.rest_form(
        w,
        "users.prefs.get",
        vec![(
            "prefs",
            "all_notifications_prefs,mute_sounds,mac_ssb_bounce".into(),
        )],
    )
}
pub async fn fetch_presentation_notification_preferences(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
) -> Result<PresentationNotificationPreferences, Error> {
    let response = t
        .execute_presentation_notifications(presentation_notification_preferences_request(c, w))
        .await?;
    PresentationNotificationPreferences::from_response(&response)
}

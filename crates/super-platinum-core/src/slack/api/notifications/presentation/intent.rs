use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentationNotificationAction {
    DesktopSound(String),
    PrioritySound(String),
    IncludePreview(bool),
    MuteSounds(bool),
    MacBounce(String),
}
#[derive(Clone)]
pub struct PresentationNotificationIntent {
    team: String,
    user: String,
    baseline: Option<Value>,
    sound_mute_baseline: Option<Option<Value>>,
    bounce_desktop_baseline: Option<Option<Value>>,
    desired: Map<String, Value>,
    keys: Vec<String>,
    user_pref: bool,
    reason: &'static str,
}
impl PresentationNotificationIntent {
    pub fn capture(
        w: &WorkspaceSession,
        prefs: &PresentationNotificationPreferences,
        action: PresentationNotificationAction,
    ) -> Result<Self, Error> {
        let settings = prefs.settings()?;
        let sound_action = matches!(
            &action,
            PresentationNotificationAction::DesktopSound(_)
                | PresentationNotificationAction::PrioritySound(_)
        );
        if (sound_action && (settings.mute_sounds || settings.desktop_disabled))
            || (matches!(
                &action,
                PresentationNotificationAction::IncludePreview(_)
                    | PresentationNotificationAction::MacBounce(_)
            ) && settings.desktop_disabled)
        {
            return Err(Error::Api(
                "notification_presentation_control_unavailable".into(),
            ));
        }
        let bounce_action = matches!(&action, PresentationNotificationAction::MacBounce(_));
        let (key, value, user_pref, reason) = match action {
            PresentationNotificationAction::DesktopSound(v) => {
                ("desktop_sound", checked_sound(v)?, false, "")
            }
            PresentationNotificationAction::PrioritySound(v) => (
                "priority_desktop_sound",
                checked_sound(v)?,
                false,
                "select-new-vip-notification-sound",
            ),
            PresentationNotificationAction::IncludePreview(v) => {
                ("no_text_in_notifications", json!(!v), false, "")
            }
            PresentationNotificationAction::MuteSounds(v) => {
                ("mute_sounds", json!(v), true, "prefs-api/setUserPrefByApi")
            }
            PresentationNotificationAction::MacBounce(v) => {
                if !matches!(v.as_str(), "" | "short" | "long") {
                    return Err(Error::Api("invalid_notification_bounce".into()));
                }
                (
                    "mac_ssb_bounce",
                    json!(v),
                    true,
                    "prefs-api/setUserPrefByApi",
                )
            }
        };
        Ok(Self {
            team: w.team_id.clone(),
            user: w.user_id.clone(),
            baseline: if user_pref {
                prefs.raw.get(key).cloned()
            } else {
                prefs.global().cloned()
            },
            sound_mute_baseline: sound_action.then(|| prefs.raw.get("mute_sounds").cloned()),
            bounce_desktop_baseline: bounce_action.then(|| {
                prefs
                    .global()
                    .and_then(|g| g.get("global_desktop"))
                    .cloned()
            }),
            desired: Map::from_iter([(key.into(), value)]),
            keys: vec![key.into()],
            user_pref,
            reason,
        })
    }
    pub fn team_id(&self) -> &str {
        &self.team
    }
    pub fn user_id(&self) -> &str {
        &self.user
    }
    pub fn desired_patch(&self) -> &Map<String, Value> {
        &self.desired
    }
    pub fn changed_keys(&self) -> &[String] {
        &self.keys
    }
    pub fn is_user_pref(&self) -> bool {
        self.user_pref
    }
    pub fn revalidate(&self, fresh: &PresentationNotificationPreferences) -> Result<(), Error> {
        fresh.settings()?;
        let current = if self.user_pref {
            fresh.raw.get(&self.keys[0])
        } else {
            fresh.global()
        };
        if current == self.baseline.as_ref()
            && self
                .bounce_desktop_baseline
                .as_ref()
                .is_none_or(|baseline| {
                    fresh.global().and_then(|g| g.get("global_desktop")) == baseline.as_ref()
                })
            && self
                .sound_mute_baseline
                .as_ref()
                .is_none_or(|baseline| fresh.raw.get("mute_sounds") == baseline.as_ref())
        {
            Ok(())
        } else {
            Err(Error::Api(
                "presentation_notification_preferences_changed".into(),
            ))
        }
    }
    pub fn confirms(&self, fresh: &PresentationNotificationPreferences) -> bool {
        self.desired.iter().all(|(k, v)| {
            let actual = if self.user_pref {
                fresh.raw.get(k)
            } else {
                fresh.global().and_then(|g| g.get(k))
            };
            actual == Some(v)
        })
    }
}
fn checked_sound(value: String) -> Result<Value, Error> {
    if valid_notification_sound(&value) {
        Ok(json!(value))
    } else {
        Err(Error::Api("invalid_notification_sound".into()))
    }
}
pub fn presentation_notification_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &PresentationNotificationIntent,
    fresh: &PresentationNotificationPreferences,
) -> Result<PreparedRequest, Error> {
    if w.team_id != intent.team || w.user_id != intent.user {
        return Err(Error::Api("notification_owner_changed".into()));
    }
    intent.revalidate(fresh)?;
    let (key, value) = intent.desired.iter().next().expect("one typed setting");
    let mut fields = vec![
        ("name", key.clone()),
        (
            "value",
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string()),
        ),
        ("_x_reason", intent.reason.into()),
    ];
    let endpoint = if intent.user_pref {
        "users.prefs.set"
    } else {
        fields.extend([("global", "true".into()), ("sync", "false".into())]);
        "users.prefs.setNotifications"
    };
    Ok(c.rest_form(w, endpoint, fields))
}
pub async fn set_presentation_notification(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &PresentationNotificationIntent,
    fresh: &PresentationNotificationPreferences,
) -> Result<(), Error> {
    t.execute_presentation_notifications(presentation_notification_request(c, w, intent, fresh)?)
        .await?;
    Ok(())
}

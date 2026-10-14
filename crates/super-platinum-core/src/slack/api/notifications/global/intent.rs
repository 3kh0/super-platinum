use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlobalNotificationAction {
    DesktopMode {
        mode: ChannelNotificationMode,
        linked_mobile: bool,
    },
    MobileMode(ChannelNotificationMode),
    DesktopPush(bool),
    MobilePush(bool),
    RelinkMobile,
    ThreadReplies(bool),
    Keywords(String),
}
#[derive(Clone)]
pub struct GlobalNotificationIntent {
    team: String,
    user: String,
    baseline: Option<Value>,
    desired: Map<String, Value>,
    keys: Vec<String>,
    wire: Vec<Value>,
    multi: bool,
    reason: &'static str,
}
impl GlobalNotificationIntent {
    pub fn capture(
        w: &WorkspaceSession,
        prefs: &NotificationPreferences,
        action: GlobalNotificationAction,
    ) -> Result<Self, Error> {
        let settings = prefs.global_settings()?;
        let mut intent = Self {
            team: w.team_id.clone(),
            user: w.user_id.clone(),
            baseline: prefs.raw.get("global").cloned(),
            desired: Map::new(),
            keys: vec![],
            wire: vec![],
            multi: false,
            reason: "NotificationsGlobalPrefCheckbox",
        };
        use GlobalNotificationAction::*;
        match action {
            DesktopMode {
                mode,
                linked_mobile,
            } => {
                intent.multi = true;
                intent.reason = "NotificationsChannelDefaults";
                intent.add("global_desktop", json!(mode.as_str()));
                if linked_mobile && settings.mobile_enabled {
                    intent.add("global_mobile", json!(mode.as_str()));
                }
            }
            MobileMode(mode) => {
                if !settings.mobile_enabled {
                    return Err(Error::Api("mobile_notifications_disabled".into()));
                }
                intent.multi = true;
                intent.reason = "NotificationsChannelDefaults";
                intent.add("global_mobile", json!(mode.as_str()));
            }
            DesktopPush(value) => intent.add("global_desktop_push_enabled", json!(value)),
            MobilePush(value) => {
                let desktop = prefs.raw["global"]
                    .get("global_desktop")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("mentions_dms");
                intent.add(
                    "global_mobile",
                    json!(if value { desktop } else { "nothing" }),
                );
            }
            RelinkMobile => {
                if !settings.mobile_enabled {
                    return Err(Error::Api("mobile_notifications_disabled".into()));
                }
                // Stock relink passes raw desktop without a fallback. Nullish
                // values fail its thunk; do not fabricate a canonical setting.
                let desktop = prefs.raw["global"]
                    .get("global_desktop")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::Api("global_notification_mode_unavailable".into()))?;
                intent.reason = "NotificationsChannelDefaults";
                intent.add("global_mobile", json!(desktop));
            }
            ThreadReplies(value) => intent.add("threads_everything", json!(value)),
            Keywords(input) => {
                intent.reason = "NotificationsKeywordsInput";
                intent.add(
                    "global_keywords",
                    json!(normalize_notification_keywords(&input)?),
                );
            }
        }
        Ok(intent)
    }
    fn add(&mut self, key: &str, value: Value) {
        self.wire.push(json!({"name":key,"value":value}));
        self.desired.insert(key.into(), value);
        self.keys.push(key.into());
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
    pub fn revalidate(&self, fresh: &NotificationPreferences) -> Result<(), Error> {
        fresh.global_settings()?;
        if fresh.raw.get("global") == self.baseline.as_ref() {
            Ok(())
        } else {
            Err(Error::Api("global_notification_preferences_changed".into()))
        }
    }
    pub fn confirms(&self, fresh: &NotificationPreferences) -> bool {
        fresh
            .raw
            .get("global")
            .is_some_and(|global| self.desired.iter().all(|(k, v)| global.get(k) == Some(v)))
    }
}
pub fn global_notification_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &GlobalNotificationIntent,
    fresh: &NotificationPreferences,
) -> Result<PreparedRequest, Error> {
    if w.team_id != intent.team || w.user_id != intent.user {
        return Err(Error::Api("notification_owner_changed".into()));
    }
    intent.revalidate(fresh)?;
    let mut fields = vec![
        ("global", "true".into()),
        ("_x_reason", intent.reason.into()),
    ];
    if intent.multi {
        fields.push(("prefs", Value::Array(intent.wire.clone()).to_string()));
    } else {
        let pref = &intent.wire[0];
        let value = &pref["value"];
        fields.extend([
            ("name", pref["name"].as_str().unwrap().into()),
            (
                "value",
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            ),
            ("sync", "false".into()),
        ]);
    }
    Ok(c.rest_form(w, "users.prefs.setNotifications", fields))
}
pub async fn set_global_notification(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &GlobalNotificationIntent,
    fresh: &NotificationPreferences,
) -> Result<(), Error> {
    t.execute_global_notifications(global_notification_request(c, w, intent, fresh)?)
        .await?;
    Ok(())
}

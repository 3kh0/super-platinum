//! BtIR mode presets and R7xz independent Advanced writes. Only named keys are
//! written/adopted; the complete canonical snapshot is never sent back to Slack.
use super::*;
use serde_json::Map;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelNotificationAction {
    Mode(ChannelNotificationMode),
    DesktopPush(bool),
    MobilePush(bool),
    MobileOverride(ChannelNotificationMode),
    FollowAllThreads(bool),
    SuppressAtChannel(bool),
    BadgeAllUnreads(bool),
}
#[derive(Clone)]
pub struct ChannelNotificationIntent {
    team: String,
    user: String,
    channel: String,
    baseline_channel: Option<Value>,
    baseline_global: Option<Value>,
    baseline_muted: bool,
    is_direct: bool,
    desired: Map<String, Value>,
    changed_keys: Vec<String>,
    wire: Vec<Value>,
    sync: bool,
    primary: bool,
    unmutes: bool,
}
impl ChannelNotificationIntent {
    pub fn capture(
        workspace: &WorkspaceSession,
        channel: &str,
        is_direct: bool,
        baseline: &NotificationPreferences,
        action: ChannelNotificationAction,
        now: i64,
    ) -> Result<Self, Error> {
        let settings = baseline.channel_settings(channel, is_direct)?;
        let muted = baseline.channel_state(channel)?.effective_muted(now);
        let mut intent = Self {
            team: workspace.team_id.clone(),
            user: workspace.user_id.clone(),
            channel: channel.into(),
            baseline_channel: baseline.channel_raw(channel)?.cloned(),
            baseline_global: baseline.raw.get("global").cloned(),
            baseline_muted: muted,
            is_direct,
            desired: Map::new(),
            changed_keys: vec![],
            wire: vec![],
            sync: false,
            primary: matches!(action, ChannelNotificationAction::Mode(_)),
            unmutes: false,
        };
        use ChannelNotificationAction::*;
        match action {
            Mode(mode) => {
                intent.sync = !settings.has_different_mobile;
                intent.add("desktop", json!(mode.as_str()));
                intent.wire[0]["sync"] = json!(intent.sync);
                intent.add("follow_all_threads", json!(false));
                match mode {
                    ChannelNotificationMode::Everything => {
                        intent.add("badge_all_unreads", json!(false))
                    }
                    ChannelNotificationMode::Mentions => {
                        intent.add("suppress_at_channel", json!(false))
                    }
                }
                if muted {
                    intent.add("muted", json!(false));
                    intent.unmutes = true;
                    intent.changed_keys.push("temp_mute_expiration_ts".into());
                }
                if intent.sync {
                    intent.desired.insert("mobile".into(), json!(mode.as_str()));
                    intent.changed_keys.push("mobile".into());
                    for pref in intent.wire.iter_mut().skip(1) {
                        pref["sync"] = json!(false);
                    }
                }
            }
            DesktopPush(value) => intent.add("desktop_push_enabled", json!(value)),
            MobilePush(value) => intent.add(
                "mobile",
                json!(if value {
                    settings.desktop.as_str()
                } else {
                    "nothing"
                }),
            ),
            MobileOverride(mode) => {
                if !settings.mobile_enabled {
                    return Err(Error::Api("mobile_notifications_disabled".into()));
                }
                intent.add("mobile", json!(mode.as_str()));
            }
            FollowAllThreads(value) => intent.add("follow_all_threads", json!(value)),
            SuppressAtChannel(value) => intent.add("suppress_at_channel", json!(value)),
            BadgeAllUnreads(value) => {
                if settings.desktop != ChannelNotificationMode::Everything {
                    return Err(Error::Api("badge_all_unreads_unavailable".into()));
                }
                intent.add("badge_all_unreads", json!(value));
            }
        }
        Ok(intent)
    }
    fn add(&mut self, key: &str, value: Value) {
        let text = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        self.wire.push(json!({"name":key,"value":text}));
        self.changed_keys.push(key.into());
        self.desired.insert(key.into(), value);
    }
    pub fn team_id(&self) -> &str {
        &self.team
    }
    pub fn user_id(&self) -> &str {
        &self.user
    }
    pub fn channel_id(&self) -> &str {
        &self.channel
    }
    pub fn changed_keys(&self) -> &[String] {
        &self.changed_keys
    }
    pub fn desired_patch(&self) -> &Map<String, Value> {
        &self.desired
    }
    pub fn revalidate(&self, fresh: &NotificationPreferences, now: i64) -> Result<(), Error> {
        fresh.channel_settings(&self.channel, self.is_direct)?;
        if fresh.channel_raw(&self.channel)? != self.baseline_channel.as_ref()
            || fresh.raw.get("global") != self.baseline_global.as_ref()
            || fresh.channel_state(&self.channel)?.effective_muted(now) != self.baseline_muted
        {
            return Err(Error::Api("notification_preferences_changed".into()));
        }
        Ok(())
    }
    pub fn confirms(&self, fresh: &NotificationPreferences, now: i64) -> bool {
        let Ok(Some(raw)) = fresh.channel_raw(&self.channel) else {
            return false;
        };
        self.desired
            .iter()
            .all(|(key, value)| raw.get(key) == Some(value))
            && (!self.unmutes
                || fresh
                    .channel_state(&self.channel)
                    .is_ok_and(|s| !s.effective_muted(now)))
    }
}
pub fn channel_notification_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &ChannelNotificationIntent,
    fresh: &NotificationPreferences,
    now: i64,
) -> Result<PreparedRequest, Error> {
    if w.team_id != intent.team || w.user_id != intent.user {
        return Err(Error::Api("notification_owner_changed".into()));
    }
    intent.revalidate(fresh, now)?;
    let mut fields = vec![
        ("global", "false".into()),
        ("sync", intent.sync.to_string()),
    ];
    if intent.primary {
        fields.extend([
            ("channel_ids", intent.channel.clone()),
            ("prefs", Value::Array(intent.wire.clone()).to_string()),
            (
                "_x_reason",
                "prefs-store/setMultiChannelNotificationOverride".into(),
            ),
        ]);
    } else {
        fields.extend([
            ("channel_id", intent.channel.clone()),
            ("name", intent.wire[0]["name"].as_str().unwrap().into()),
            ("value", intent.wire[0]["value"].as_str().unwrap().into()),
            (
                "_x_reason",
                "prefs-store/setChannelNotificationOverride".into(),
            ),
        ]);
    }
    Ok(c.rest_form(w, "users.prefs.setNotifications", fields))
}
pub async fn set_channel_notification(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &ChannelNotificationIntent,
    fresh: &NotificationPreferences,
    now: i64,
) -> Result<(), Error> {
    t.execute_notification_preferences(channel_notification_request(c, w, intent, fresh, now)?)
        .await?;
    Ok(())
}

//! Narrow channel preference patches from Nu4c/uhjK/BtIR in the captured client.
use super::*;
use serde_json::{Value, json};
mod global;
mod presentation;
pub use presentation::*;
mod intent;
mod settings;
mod temporary;
pub use global::*;
pub use intent::*;
pub use settings::*;
pub use temporary::*;

/// Preserve the complete canonical object for normal preference adoption. Never
/// serialize this snapshot back as a mutation: unknown and concurrent keys belong
/// to Slack, and the mute API changes only the named channel preference.
#[derive(Clone, Debug, PartialEq)]
pub struct NotificationPreferences {
    raw: Value,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelMuteState {
    pub muted: bool,
    pub temp_mute_expiration_ts: Option<i64>,
}
impl ChannelMuteState {
    pub fn effective_muted(self, now: i64) -> bool {
        self.muted
            || self
                .temp_mute_expiration_ts
                .is_some_and(|until| until > now)
    }
}
impl NotificationPreferences {
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn channel_state(&self, channel: &str) -> Result<ChannelMuteState, Error> {
        let Some(prefs) = self.raw["channels"].get(channel) else {
            return Ok(ChannelMuteState {
                muted: false,
                temp_mute_expiration_ts: None,
            });
        };
        let prefs = prefs.as_object().ok_or_else(invalid_preferences)?;
        let muted = match prefs.get("muted") {
            None => false,
            Some(Value::Bool(muted)) => *muted,
            _ => return Err(invalid_preferences()),
        };
        let expiry = match prefs.get("temp_mute_expiration_ts") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_i64()
                    .or_else(|| value.as_str()?.parse().ok())
                    .filter(|until| *until >= 0)
                    .ok_or_else(invalid_preferences)?,
            ),
        };
        Ok(ChannelMuteState {
            muted,
            temp_mute_expiration_ts: expiry,
        })
    }
}
fn invalid_preferences() -> Error {
    Error::Transport("Slack returned malformed notification preferences".into())
}
fn valid_channel(channel: &str) -> bool {
    channel.len() >= 2
        && matches!(channel.as_bytes()[0], b'C' | b'G' | b'D')
        && channel
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
/// The stock header sends a multi-pref patch, with string boolean values. In
/// particular, Unmute does not add an inferred temporary-expiry clear field.
pub fn channel_mute_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
    muted: bool,
) -> Result<PreparedRequest, Error> {
    channel_mute_change_request(c, w, channel, ChannelMuteChange::Permanent(muted))
}

pub async fn set_channel_muted(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
    muted: bool,
) -> Result<(), Error> {
    t.execute_notification_preferences(channel_mute_request(c, w, channel, muted)?)
        .await?;
    Ok(())
}
/// Nu4c getUserPrefsByApi uses a comma-separated selection. Reading a single
/// named preference avoids treating client.userBoot as a mutation receipt.
pub fn notification_preferences_request(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    c.rest_form(
        w,
        "users.prefs.get",
        vec![("prefs", "all_notifications_prefs".into())],
    )
}
pub async fn fetch_notification_preferences(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
) -> Result<NotificationPreferences, Error> {
    let response = t
        .execute_notification_preferences(notification_preferences_request(c, w))
        .await?;
    parse_notification_preferences(&response)
}
pub fn parse_notification_preferences(response: &Value) -> Result<NotificationPreferences, Error> {
    super::super::transport::notification_preferences_success(response)?;
    let value = response
        .get("prefs")
        .and_then(|p| p.get("all_notifications_prefs"))
        .ok_or_else(invalid_preferences)?;
    let raw: Value = match value {
        Value::String(text) => serde_json::from_str(text).map_err(|_| invalid_preferences())?,
        value => value.clone(),
    };
    if !raw.is_object()
        || !raw.get("channels").is_some_and(Value::is_object)
        || raw.get("global").is_some_and(|v| !v.is_object())
    {
        return Err(invalid_preferences());
    }
    Ok(NotificationPreferences { raw })
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod settings_tests;

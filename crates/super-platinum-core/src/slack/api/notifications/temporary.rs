//! Captured 0+9p / m3Jj / e5qt: one hour, or tomorrow's local DND end.
use super::*;
use chrono::{DateTime, Datelike, Days, NaiveTime, TimeZone};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporaryMuteDuration {
    OneHour,
    UntilTomorrow,
}
impl TemporaryMuteDuration {
    pub fn expires_at_local(self, now: i64, prefs: &BTreeMap<String, Value>) -> Result<i64, Error> {
        let now = chrono::Local
            .timestamp_opt(now, 0)
            .single()
            .ok_or_else(|| Error::Api("invalid_temporary_mute_duration".into()))?;
        self.expires_at(now, prefs)
    }
    pub fn expires_at_fixed_offset(
        self,
        now: i64,
        offset_seconds: i32,
        prefs: &BTreeMap<String, Value>,
    ) -> Result<i64, Error> {
        let zone = chrono::FixedOffset::east_opt(offset_seconds)
            .ok_or_else(|| Error::Api("invalid_temporary_mute_duration".into()))?;
        let now = zone
            .timestamp_opt(now, 0)
            .single()
            .ok_or_else(|| Error::Api("invalid_temporary_mute_duration".into()))?;
        self.expires_at(now, prefs)
    }

    pub fn expires_at<Tz: TimeZone>(
        self,
        now: DateTime<Tz>,
        prefs: &BTreeMap<String, Value>,
    ) -> Result<i64, Error> {
        let invalid = || Error::Api("invalid_temporary_mute_duration".into());
        let current = now.timestamp();
        if current <= 0 {
            return Err(invalid());
        }
        if self == Self::OneHour {
            return current.checked_add(3600).ok_or_else(invalid);
        }
        let tomorrow = now
            .date_naive()
            .checked_add_days(Days::new(1))
            .ok_or_else(invalid)?;
        let day = match tomorrow.weekday() {
            chrono::Weekday::Mon => "monday",
            chrono::Weekday::Tue => "tuesday",
            chrono::Weekday::Wed => "wednesday",
            chrono::Weekday::Thu => "thursday",
            chrono::Weekday::Fri => "friday",
            chrono::Weekday::Sat => "saturday",
            chrono::Weekday::Sun => "sunday",
        };
        let custom = prefs.get("dnd_days").and_then(Value::as_str) == Some("custom")
            && prefs
                .get(&format!("dnd_enabled_{day}"))
                .and_then(Value::as_str)
                == Some("partial");
        let selected = custom
            .then(|| prefs.get(&format!("dnd_before_{day}")))
            .flatten()
            .filter(|value| value.is_string())
            .or_else(|| prefs.get("dnd_end_hour"));
        let text = match selected {
            None => "09:00",
            Some(Value::String(text)) => text.as_str(),
            _ => return Err(invalid()),
        };
        let time = NaiveTime::parse_from_str(text, "%H:%M").map_err(|_| invalid())?;
        // Unknown/ambiguous local wall times are unavailable rather than guessed.
        let target = now
            .timezone()
            .from_local_datetime(&tomorrow.and_time(time))
            .single()
            .ok_or_else(invalid)?
            .timestamp();
        let delta = target
            .checked_sub(current)
            .filter(|delta| *delta > 0)
            .ok_or_else(invalid)?;
        // e5qt returns ceil(fractional minutes); 0+9p adds whole minutes to now.
        current
            .checked_add(delta.checked_add(59).ok_or_else(invalid)? / 60 * 60)
            .ok_or_else(invalid)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelMuteChange {
    Permanent(bool),
    TemporaryUntil(i64),
}
impl ChannelMuteChange {
    pub fn confirms(self, state: ChannelMuteState, now: i64) -> bool {
        match self {
            Self::Permanent(muted) => state.effective_muted(now) == muted,
            Self::TemporaryUntil(expiry) => {
                !state.muted && state.temp_mute_expiration_ts == Some(expiry)
            }
        }
    }
}

pub fn channel_mute_change_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
    change: ChannelMuteChange,
) -> Result<PreparedRequest, Error> {
    if !valid_channel(channel) {
        return Err(Error::Api("invalid_channel".into()));
    }
    let (name, value) = match change {
        ChannelMuteChange::Permanent(muted) => ("muted", muted.to_string()),
        ChannelMuteChange::TemporaryUntil(expiry) if expiry > 0 => {
            ("temp_mute_expiration_ts", expiry.to_string())
        }
        _ => return Err(Error::Api("invalid_temporary_mute_duration".into())),
    };
    Ok(c.rest_form(
        w,
        "users.prefs.setNotifications",
        vec![
            ("global", "false".into()),
            ("channel_ids", channel.to_owned()),
            ("prefs", json!([{"name":name,"value":value}]).to_string()),
            ("sync", "false".into()),
        ],
    ))
}
pub async fn set_channel_mute_change(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
    change: ChannelMuteChange,
) -> Result<(), Error> {
    t.execute_notification_preferences(channel_mute_change_request(c, w, channel, change)?)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests;

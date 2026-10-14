//! Fpyc calculated settings and R7xz raw device/checkbox projections.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelNotificationMode {
    Everything,
    Mentions,
}
impl ChannelNotificationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Everything => "everything",
            Self::Mentions => "mentions_dms",
        }
    }
    fn projected(value: ModeValue<'_>) -> Self {
        if value == ModeValue::Text("everything") {
            Self::Everything
        } else {
            Self::Mentions
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelNotificationSettings {
    pub desktop: ChannelNotificationMode,
    pub mobile: ChannelNotificationMode,
    pub mobile_enabled: bool,
    pub desktop_push: bool,
    pub has_different_mobile: bool,
    pub follow_all_threads: bool,
    pub suppress_at_channel: bool,
    pub badge_all_unreads: bool,
}

impl NotificationPreferences {
    pub fn channel_settings(
        &self,
        channel: &str,
        is_direct: bool,
    ) -> Result<ChannelNotificationSettings, Error> {
        if !valid_channel(channel) {
            return Err(Error::Api("invalid_channel".into()));
        }
        let raw = self.channel_raw(channel)?;
        let global = self.raw.get("global");
        let desktop = calculated(raw, global, "desktop", is_direct)?;
        let mobile = calculated(raw, global, "mobile", is_direct)?;
        let projected_desktop = ChannelNotificationMode::projected(desktop);
        let desktop_push = match raw.and_then(|r| r.get("desktop_push_enabled")) {
            None => match global.and_then(|g| g.get("global_desktop_push_enabled")) {
                None => true,
                Some(Value::Null) => false,
                Some(Value::Bool(value)) => *value,
                _ => return Err(invalid_preferences()),
            },
            Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            _ => return Err(invalid_preferences()),
        };
        Ok(ChannelNotificationSettings {
            desktop: projected_desktop,
            mobile: if mobile == ModeValue::Text("") {
                projected_desktop
            } else {
                ChannelNotificationMode::projected(mobile)
            },
            // R7xz deliberately reads the raw override for its on/off switch.
            mobile_enabled: mode(raw.and_then(|r| r.get("mobile")))? != ModeValue::Text("nothing"),
            desktop_push,
            has_different_mobile: mobile != ModeValue::Undefined && mobile != desktop,
            follow_all_threads: boolean(raw, "follow_all_threads")?,
            suppress_at_channel: boolean(raw, "suppress_at_channel")?,
            badge_all_unreads: boolean(raw, "badge_all_unreads")?,
        })
    }
    pub(super) fn channel_raw(&self, channel: &str) -> Result<Option<&Value>, Error> {
        let value = self.raw["channels"].get(channel);
        if value.is_some_and(|v| !v.is_object()) {
            return Err(invalid_preferences());
        }
        Ok(value)
    }
}
fn boolean(raw: Option<&Value>, key: &str) -> Result<bool, Error> {
    match raw.and_then(|r| r.get(key)) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(invalid_preferences()),
    }
}
// JS undefined and null differ in BtIR's strict comparison. Keep both until
// after synchronization is calculated, even though pA displays both as Mentions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModeValue<'a> {
    Undefined,
    Null,
    Text(&'a str),
}
impl ModeValue<'_> {
    fn truthy(self) -> bool {
        matches!(self, Self::Text(value) if !value.is_empty())
    }
}
fn mode(value: Option<&Value>) -> Result<ModeValue<'_>, Error> {
    match value {
        None => Ok(ModeValue::Undefined),
        Some(Value::Null) => Ok(ModeValue::Null),
        Some(Value::String(value))
            if matches!(
                value.as_str(),
                "" | "everything" | "mentions_dms" | "nothing"
            ) =>
        {
            Ok(ModeValue::Text(value))
        }
        _ => Err(invalid_preferences()),
    }
}
fn normalized(value: ModeValue<'_>) -> ModeValue<'_> {
    if value == ModeValue::Text("nothing") {
        ModeValue::Text("mentions_dms")
    } else {
        value
    }
}
fn calculated<'a>(
    raw: Option<&'a Value>,
    global: Option<&'a Value>,
    platform: &str,
    direct: bool,
) -> Result<ModeValue<'a>, Error> {
    let own = mode(raw.and_then(|r| r.get(platform)))?;
    if own.truthy() {
        return Ok(normalized(own));
    }
    let key = if direct {
        format!("global_mpdm_{platform}")
    } else {
        format!("global_{platform}")
    };
    let fallback = normalized(mode(global.and_then(|g| g.get(&key)))?);
    if direct && !fallback.truthy() {
        Ok(ModeValue::Text("everything"))
    } else {
        Ok(fallback)
    }
}

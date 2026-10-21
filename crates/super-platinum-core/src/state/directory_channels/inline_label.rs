//! Ordinary-channel EXeY/J+l2/OK9l policy and numeric 3Yi5 frozen grace.
//!
//! Callers retain canonical channel, current-team PlanInfo and clock ownership.
//! This is display policy, not navigation authority or a foreign-team resolver.
//! `None` PlanInfo means unobserved, never an invented source default.
use chrono::{DateTime, Local, TimeZone};
use serde_json::Value;

#[cfg(test)]
mod tests;
mod time;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    UnsupportedChannel,
    MissingPlan,
    UnsupportedTimestamp,
    UnsupportedLocalTime,
}

/// Icon shape and text suffix are independent: an active frozen channel can
/// have an archive icon without acquiring the "(archived)" text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub public: bool,
    pub private: bool,
    pub archived: bool,
    pub frozen: bool,
    pub archive_icon: bool,
    pub archived_suffix: bool,
    pub in_grace: bool,
    pub grace_eligible: bool,
    pub captured_now_ms: i64,
    /// Actual local-calendar deadline, useful to schedule a bounded repaint.
    /// It is not an authorization lease or a fixed five-day duration.
    pub grace_end_ms: Option<i64>,
}

impl Policy {
    /// Re-evaluate only expiry against a newer clock observation; no timezone
    /// lookup or I/O. Source, plan and timezone changes require fresh projection.
    pub fn at_millis(&self, now: i64) -> Result<Self, Error> {
        time::valid_instant(now)?;
        let in_grace = self.grace_eligible && self.grace_end_ms.is_some_and(|end| end > now);
        Ok(Self {
            archive_icon: !in_grace && (self.archived || self.frozen),
            archived_suffix: !in_grace && self.archived,
            in_grace,
            ..*self
        })
    }
}

/// Native acquisition boundary: call on a background worker, not during render.
/// OS-local timezone resolution may read system zone data on some platforms.
pub fn project_local(channel: &Value, plan_info: Option<&Value>) -> Result<Policy, Error> {
    project(channel, plan_info, &Local::now())
}

/// `now` must be an actual clock observation in the target's local timezone.
/// `DateTime<Local>` retains the OS timezone, whereas converting it to
/// `FixedOffset` loses the transition rules needed for five local calendar days.
///
/// No clock, network or file read is initiated here. A supplied timezone may
/// consult OS data when resolving offsets: Chrono Local uses Windows timezone
/// APIs and may load/cache Unix zone files. Hosts must perform that work off a
/// render path on platforms where it can involve synchronous filesystem I/O.
pub fn project<T: TimeZone>(
    channel: &Value,
    plan_info: Option<&Value>,
    now: &DateTime<T>,
) -> Result<Policy, Error> {
    let (public, private) = ordinary(channel)?;
    let frozen = truth(&channel["is_frozen"]);
    let archived = truth(&channel["is_archived"]);
    let captured_now_ms = now.timestamp_millis();
    time::valid_instant(captured_now_ms)?;
    if now.timestamp_subsec_nanos() >= 1_000_000_000 {
        return Err(Error::UnsupportedTimestamp);
    }
    let (grace_eligible, grace_end_ms) = grace(channel, plan_info, now, public, frozen)?;
    Policy {
        public,
        private,
        archived,
        frozen,
        archive_icon: false,
        archived_suffix: false,
        in_grace: false,
        grace_eligible,
        captured_now_ms,
        grace_end_ms,
    }
    .at_millis(captured_now_ms)
}

fn grace<T: TimeZone>(
    channel: &Value,
    plan_info: Option<&Value>,
    now: &DateTime<T>,
    public: bool,
    frozen: bool,
) -> Result<(bool, Option<i64>), Error> {
    // Original ordering allows these answers without consulting PlanInfo.
    if (public && !truth(&channel["grace_period_end"])) || !frozen || !truth(&channel["unlinked"]) {
        return Ok((false, None));
    }
    let plan = plan_info.ok_or(Error::MissingPlan)?;
    if !plan.is_object() {
        return Err(Error::UnsupportedTimestamp);
    }
    let start = match plan.get("reconnect_grace_period_start_ts") {
        None | Some(Value::Null) => 0.0,
        Some(value) => number(value)?,
    };
    if start == 0.0 {
        return Ok((false, None));
    }

    let unlinked = number(&channel["unlinked"])?;
    let start_ms = time::clip(start * 1000.0)?;
    let unlinked_ms = time::clip(unlinked * 1000.0)?;
    // Deliberately preserve the shipped mixed-unit comparison:
    // dayjs.unix(unlinked).isAfter(start), NOT isAfter(dayjs.unix(start)).
    // Date's TimeClip truncation applies to both constructed dates.
    let comparison_ms = time::clip(start)?;
    time::valid_instant(unlinked_ms)?;
    time::valid_instant(comparison_ms)?;
    let end = time::five_days(&now.timezone(), start_ms)?;
    Ok((start == unlinked || unlinked_ms > comparison_ms, Some(end)))
}

fn number(value: &Value) -> Result<f64, Error> {
    value
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or(Error::UnsupportedTimestamp)
}

fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|v| v != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn ordinary(channel: &Value) -> Result<(bool, bool), Error> {
    if !channel.is_object() {
        return Err(Error::UnsupportedChannel);
    }
    let private = (truth(&channel["is_private"]) || truth(&channel["is_group"]))
        && !truth(&channel["is_mpim"]);
    let public = truth(&channel["is_channel"]) && !private && !truth(&channel["is_mpim"]);
    if !(public || private)
        || ["is_im", "is_mpim", "is_file", "isUnknown", "isNonExistent"]
            .iter()
            .any(|key| truth(&channel[*key]))
        || channel["name"]
            .as_str()
            .is_none_or(|v| v.is_empty() || v.starts_with("mc:"))
    {
        return Err(Error::UnsupportedChannel);
    }
    let properties = channel.get("properties").filter(|v| !v.is_null());
    if properties.is_some_and(|v| !v.is_object()) {
        return Err(Error::UnsupportedChannel);
    }
    for (group, field) in [
        ("huddles", "is_huddle_channel"),
        ("meetings_channel", "is_meeting_channel"),
        ("record_channel", "record_id"),
    ] {
        let row = properties
            .and_then(|p| p.get(group))
            .filter(|v| !v.is_null());
        if row.is_some_and(|v| !v.is_object()) || row.and_then(|v| v.get(field)).is_some_and(truth)
        {
            return Err(Error::UnsupportedChannel);
        }
    }
    // Shared flags alone never alter this base policy. The host separately
    // requires its current ordinary-mode canonical fromAnotherTeam=false.
    Ok((public, private))
}

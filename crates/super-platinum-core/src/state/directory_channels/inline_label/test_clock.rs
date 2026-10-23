//! Protected deterministic calendar observation; no OS timezone mutation.
use super::*;
use chrono::{FixedOffset, LocalResult, NaiveDate, NaiveDateTime, Utc};
#[derive(Clone)]
pub(super) struct Eastern2026;
pub(super) fn wall(value: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f").unwrap()
}
fn offset(hours: i32) -> FixedOffset {
    FixedOffset::east_opt(hours * 3600).unwrap()
}
impl TimeZone for Eastern2026 {
    type Offset = FixedOffset;
    fn from_offset(_: &Self::Offset) -> Self {
        Self
    }
    fn offset_from_local_date(&self, value: &NaiveDate) -> LocalResult<Self::Offset> {
        self.offset_from_local_datetime(&value.and_hms_opt(12, 0, 0).unwrap())
    }
    fn offset_from_local_datetime(&self, value: &NaiveDateTime) -> LocalResult<Self::Offset> {
        if *value >= wall("2026-03-08 02:00:00") && *value < wall("2026-03-08 03:00:00") {
            LocalResult::None
        } else if *value >= wall("2026-11-01 01:00:00") && *value < wall("2026-11-01 02:00:00") {
            // Intentionally return later offset first; policy must choose by instant.
            LocalResult::Ambiguous(offset(-5), offset(-4))
        } else {
            LocalResult::Single(
                if *value >= wall("2026-03-08 03:00:00") && *value < wall("2026-11-01 01:00:00") {
                    offset(-4)
                } else {
                    offset(-5)
                },
            )
        }
    }
    fn offset_from_utc_date(&self, value: &NaiveDate) -> Self::Offset {
        self.offset_from_utc_datetime(&value.and_hms_opt(12, 0, 0).unwrap())
    }
    fn offset_from_utc_datetime(&self, value: &NaiveDateTime) -> Self::Offset {
        if *value >= wall("2026-03-08 07:00:00") && *value < wall("2026-11-01 06:00:00") {
            offset(-4)
        } else {
            offset(-5)
        }
    }
}

/// Calls the same production calendar projection with an explicit test zone.
pub fn project_at(
    channel: &Value,
    plan: Option<&Value>,
    now_ms: i64,
    eastern: bool,
) -> Result<Policy, Error> {
    let utc = DateTime::<Utc>::from_timestamp_millis(now_ms).ok_or(Error::UnsupportedTimestamp)?;
    if eastern {
        project(channel, plan, &utc.with_timezone(&Eastern2026))
    } else {
        project(channel, plan, &utc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_zone_replacement_uses_actual_calendar_projection() {
        let raw = serde_json::json!({"id":"C123","name":"grace","is_channel":true,"is_frozen":true,"is_archived":true,"unlinked":1772643600_i64,"grace_period_end":1});
        let plan = serde_json::json!({"reconnect_grace_period_start_ts":1772643600_i64});
        let utc = project_at(&raw, Some(&plan), 1773073800000, false).unwrap();
        let east = project_at(&raw, Some(&plan), 1773073800000, true).unwrap();
        assert_eq!(utc.grace_end_ms, Some(1773075600000));
        assert_eq!(east.grace_end_ms, Some(1773072000000));
        assert!(utc.in_grace && !utc.archived_suffix);
        assert!(!east.in_grace && east.archived_suffix);
        assert!(!utc.at_millis(1773075600000).unwrap().in_grace);
    }
}

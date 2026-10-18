//! Local wall-clock selection corresponds to the captured SOLS/Dayjs caller.
use chrono::{
    DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Offset,
    TimeZone, Timelike, Utc,
};
pub fn day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .filter(|d| (1970..=9999).contains(&d.year()) && d.format("%Y-%m-%d").to_string() == s)
}
pub fn now() -> i64 {
    Utc::now().timestamp()
}
pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}
pub fn valid_timestamp(n: i64) -> bool {
    DateTime::<Utc>::from_timestamp(n, 0).is_some_and(|d| (1970..=9999).contains(&d.year()))
}
pub fn label(n: i64) -> String {
    DateTime::<Utc>::from_timestamp(n, 0)
        .map(|d| d.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_default()
}
pub fn date_label(n: i64) -> String {
    DateTime::<Utc>::from_timestamp(n, 0)
        .map(|d| d.with_timezone(&Local).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}
pub fn parse(day_value: &str, time: &str) -> Option<i64> {
    let date = day(day_value)?;
    let t = NaiveTime::parse_from_str(time, "%H:%M").ok()?;
    if t.format("%H:%M").to_string() != time {
        return None;
    }
    resolve(&Local, date.and_time(t))
}
fn earliest<T: TimeZone>(v: LocalResult<DateTime<T>>) -> Option<DateTime<T>> {
    match v {
        LocalResult::Single(v) => Some(v),
        LocalResult::Ambiguous(a, b) => Some(if a.timestamp() <= b.timestamp() { a } else { b }),
        LocalResult::None => None,
    }
}
/// JS Date/Dayjs chooses the earlier repeated instant and moves a missing local
/// time forward by the transition gap. Resolve against the OS timezone, not a
/// cached offset (which is wrong across daylight-saving transitions).
fn resolve<T: TimeZone>(zone: &T, wall: NaiveDateTime) -> Option<i64> {
    if let Some(v) = earliest(zone.from_local_datetime(&wall)) {
        return Some(v.timestamp());
    }
    let mut before = None;
    let mut after = None;
    for minutes in 1..=1440 {
        if before.is_none() {
            before = wall
                .checked_sub_signed(Duration::minutes(minutes))
                .and_then(|v| earliest(zone.from_local_datetime(&v)));
        }
        if after.is_none() {
            after = wall
                .checked_add_signed(Duration::minutes(minutes))
                .and_then(|v| earliest(zone.from_local_datetime(&v)));
        }
        if before.is_some() && after.is_some() {
            break;
        }
    }
    let gap = after?.offset().fix().local_minus_utc() - before?.offset().fix().local_minus_utc();
    if gap <= 0 {
        return None;
    }
    earliest(zone.from_local_datetime(&wall.checked_add_signed(Duration::seconds(gap.into()))?))
        .map(|v| v.timestamp())
}
pub fn custom_slots(date: &str, minutes: u16, now: i64) -> Vec<(i64, i64)> {
    let Some(start) = parse(date, "00:00") else {
        return vec![];
    };
    let Some(next) = day(date)
        .and_then(|d| d.succ_opt())
        .and_then(|d| parse(&d.format("%Y-%m-%d").to_string(), "00:00"))
    else {
        return vec![];
    };
    if minutes == 0 || minutes > 1440 {
        return vec![];
    }
    let mut time = if date_label(now) == date {
        now + (15
            - DateTime::<Utc>::from_timestamp(now, 0)
                .unwrap()
                .with_timezone(&Local)
                .minute()
                % 15) as i64
            * 60
    } else {
        start
    };
    let mut out = Vec::new();
    while time + minutes as i64 * 60 <= next + 60 && out.len() < 104 {
        out.push((time, time + minutes as i64 * 60));
        time += 900;
    }
    out
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;

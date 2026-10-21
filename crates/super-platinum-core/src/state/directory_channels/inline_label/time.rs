use super::Error;
use chrono::{DateTime, Days, Duration, LocalResult, NaiveDateTime, Offset, TimeZone, Utc};

/// ECMAScript Date TimeClip: finite, within +/-100,000,000 days, truncate
/// milliseconds toward zero. Chrono's narrower range is refused separately.
pub(super) fn clip(ms: f64) -> Result<i64, Error> {
    if !ms.is_finite() || ms.abs() > 8_640_000_000_000_000.0 {
        return Err(Error::UnsupportedTimestamp);
    }
    Ok(ms.trunc() as i64)
}

pub(super) fn valid_instant(ms: i64) -> Result<DateTime<Utc>, Error> {
    DateTime::<Utc>::from_timestamp_millis(ms).ok_or(Error::UnsupportedTimestamp)
}

pub(super) fn five_days<T: TimeZone>(zone: &T, start_ms: i64) -> Result<i64, Error> {
    let instant = valid_instant(start_ms)?;
    let offset = zone.offset_from_utc_datetime(&instant.naive_utc()).fix();
    // Avoid DateTime::naive_local's panic at the edge of Chrono's date range.
    let wall = instant
        .naive_utc()
        .checked_add_signed(Duration::seconds(offset.local_minus_utc().into()))
        .and_then(|v| v.checked_add_days(Days::new(5)))
        .ok_or(Error::UnsupportedLocalTime)?;
    resolve(zone, wall)
        .map(|v| v.timestamp_millis())
        .ok_or(Error::UnsupportedLocalTime)
}

fn earliest<T: TimeZone>(value: LocalResult<DateTime<T>>) -> Option<DateTime<T>> {
    match value {
        LocalResult::Single(value) => Some(value),
        LocalResult::Ambiguous(a, b) => Some(if a <= b { a } else { b }),
        LocalResult::None => None,
    }
}

/// JS compatible disambiguation: earliest repeated instant, or shift a missing
/// wall time by the forward offset transition. This is the same bounded local
/// policy already used by core calendar/time.rs. Unresolved/oversized gaps are
/// unavailable, never converted to false grace or a fixed-offset approximation.
fn resolve<T: TimeZone>(zone: &T, wall: NaiveDateTime) -> Option<DateTime<T>> {
    if let Some(value) = earliest(zone.from_local_datetime(&wall)) {
        return Some(value);
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
}

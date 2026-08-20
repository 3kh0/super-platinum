use crate::slack::models::Message as SlackMessage;
use crate::state::now_secs;

pub fn format_relative_ts(ts: &str) -> String {
    let (secs, _) = ts_key(ts);
    let elapsed = now_secs().saturating_sub(secs as i64).max(0) as u64;
    match elapsed {
        0..=59 => "now".to_owned(),
        60..=3_599 => format!("{}m ago", elapsed / 60),
        3_600..=86_399 => format!("{}h ago", elapsed / 3_600),
        86_400..=604_799 => format!("{}d ago", elapsed / 86_400),
        _ => format_ts_date_label(ts),
    }
}

pub fn ts_key(ts: &str) -> (u64, u64) {
    let mut parts = ts.splitn(2, '.');
    let secs = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let seq = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (secs, seq)
}

pub fn cmp_ts(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => ts_key(a).cmp(&ts_key(b)),
        (None, None) => std::cmp::Ordering::Equal,
        (None, _) => std::cmp::Ordering::Less,
        (_, None) => std::cmp::Ordering::Greater,
    }
}

pub fn format_ts_hm(ts: &str) -> String {
    use chrono::{Local, TimeZone};
    let (secs, _) = ts_key(ts);
    match Local.timestamp_opt(secs as i64, 0).single() {
        Some(dt) => dt.format("%H:%M").to_string(),
        None => secs.to_string(),
    }
}

/// Local wall-clock time for a user based on their Slack `tz_offset` (seconds east of UTC).
pub fn format_user_local_time(tz_offset: Option<i32>) -> Option<String> {
    use chrono::{FixedOffset, Utc};
    let offset = FixedOffset::east_opt(tz_offset?)?;
    Some(
        Utc::now()
            .with_timezone(&offset)
            .format("%H:%M local time")
            .to_string(),
    )
}

pub fn date_key_for_ts(ts: &str) -> Option<String> {
    use chrono::{Datelike, Local, TimeZone};
    let (secs, _) = ts_key(ts);
    let date = Local.timestamp_opt(secs as i64, 0).single()?.date_naive();
    Some(format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month(),
        date.day()
    ))
}

pub fn format_ts_date_label(ts: &str) -> String {
    use chrono::{Datelike, Local, TimeZone};
    let (secs, _) = ts_key(ts);
    let Some(date_time) = Local.timestamp_opt(secs as i64, 0).single() else {
        return ts.to_owned();
    };
    let date = date_time.date_naive();
    let today = Local::now().date_naive();
    if date == today {
        return "Today".to_owned();
    }
    if today.signed_duration_since(date).num_days() == 1 {
        return "Yesterday".to_owned();
    }
    format!(
        "{}, {} {}",
        date.format("%A"),
        date.format("%B"),
        ordinal_day(date.day())
    )
}

pub(crate) fn ordinal_day(day: u32) -> String {
    let suffix = match day % 100 {
        11..=13 => "th",
        _ => match day % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{day}{suffix}")
}

pub fn scroll_ratio_for_ts(messages: &[SlackMessage], ts: &str) -> Option<f32> {
    let index = messages.iter().position(|m| m.ts.as_deref() == Some(ts))?;
    let last = messages.len() - 1;
    if last == 0 {
        Some(0.0)
    } else {
        Some(index as f32 / last as f32)
    }
}

/// Slack's message-unfurl footer stamp: `Today at 10:47`, `Yesterday at 10:47`,
/// then `Aug 18th` once the referenced message is older than that.
pub fn format_ts_unfurl_label(ts: &str) -> String {
    use chrono::{Datelike, Local, TimeZone};
    let (secs, _) = ts_key(ts);
    let Some(date_time) = Local.timestamp_opt(secs as i64, 0).single() else {
        return ts.to_owned();
    };
    let date = date_time.date_naive();
    let today = Local::now().date_naive();
    let clock = date_time.format("%H:%M");
    match today.signed_duration_since(date).num_days() {
        0 => format!("Today at {clock}"),
        1 => format!("Yesterday at {clock}"),
        _ => format!("{} {}", date.format("%b"), ordinal_day(date.day())),
    }
}

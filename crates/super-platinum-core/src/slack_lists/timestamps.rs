//! Read-only metadata timestamps: B FieldRenderer -> eoW4 -> JO00; S nL6P/CFnj.
//! The sampled calendar is explicit. Formatting never changes filter/sort values.
use super::{Column, Record, saved_views::DateContext};
use serde_json::Value;

mod locale;
#[cfg(test)]
mod tests;
mod tooltip;

#[derive(Clone, Debug)]
pub struct TimestampContext {
    calendar: DateContext,
    locale: String,
    time24: bool,
}
impl TimestampContext {
    pub fn new(calendar: DateContext, locale: Option<&str>, time24: bool) -> Self {
        Self {
            calendar,
            locale: locale
                .filter(|locale| super::numbers::LOCALES.contains(locale))
                .unwrap_or("en-US")
                .into(),
            time24,
        }
    }
    pub fn local_now(locale: Option<&str>, time24: bool) -> Self {
        Self::new(DateContext::local_now(None), locale, time24)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimestampPresentation {
    Empty,
    Value { label: String, tooltip: String },
    Unsupported { fallback: String },
}

pub fn supports(column: &Column) -> bool {
    matches!(column.kind.as_str(), "created_time" | "last_edited_time")
}

pub fn project(
    column: &Column,
    record: &Record,
    context: &TimestampContext,
) -> Option<TimestampPresentation> {
    if !supports(column) {
        return None;
    }
    Some(project_inner(column, record, context).unwrap_or_else(|| {
        TimestampPresentation::Unsupported {
            fallback: "Unsupported timestamp".into(),
        }
    }))
}

fn project_inner(
    column: &Column,
    record: &Record,
    context: &TimestampContext,
) -> Option<TimestampPresentation> {
    // VBDB supplies the canonical creation timestamp after spreading decoded
    // fields. Never substitute the HLC updated_timestamp for a typed cell.
    let raw = if column.kind == "created_time" && column.key == "created_time" {
        record.raw.get("date_created")
    } else {
        let Some(field) = record.fields.get(&column.key) else {
            return Some(TimestampPresentation::Empty);
        };
        if field
            .get("column_id")
            .is_some_and(|id| id.as_str() != Some(column.id.as_str()))
            || field
                .get("row_id")
                .is_some_and(|id| id.as_str() != Some(record.id.as_str()))
            || [
                "attachment",
                "channel",
                "checkbox",
                "date",
                "email",
                "message",
                "link",
                "number",
                "phone",
                "rating",
                "rich_text",
                "richText",
                "select",
                "user",
                "reference",
            ]
            .iter()
            .any(|key| {
                field.get(key).is_some_and(|value| {
                    !value.is_null() && !value.as_array().is_some_and(Vec::is_empty)
                })
            })
        {
            return None;
        }
        match field.get("timestamp") {
            None | Some(Value::Null) => None,
            Some(Value::Array(values)) if values.len() <= 1 => values.first(),
            _ => return None,
        }
    };
    let Some(raw) = raw.filter(|value| !value.is_null()) else {
        return Some(TimestampPresentation::Empty);
    };
    if raw.as_str() == Some("") {
        return Some(TimestampPresentation::Empty);
    }
    let value = match raw {
        Value::Number(value) => value.as_f64()?,
        // The source accepts strings through parseFloat. Bound native support
        // to a whole numeric spelling instead of accepting trailing garbage.
        Value::String(value) if value.len() <= 128 && !value.trim().is_empty() => {
            value.trim().parse::<f64>().ok()?
        }
        _ => return None,
    };
    // FieldRenderer's empty predicate hides numeric zero (string "0" is truthy).
    if raw.is_number() && value == 0.0 {
        return Some(TimestampPresentation::Empty);
    }
    let milliseconds = milliseconds(value)?;
    let date = context.calendar.timestamp_millis(milliseconds)?;
    let elapsed = ((context.calendar.now().timestamp_millis() as i128 - milliseconds as i128)
        / 1000)
        .abs() as f64;
    Some(TimestampPresentation::Value {
        label: relative(elapsed, &context.locale),
        tooltip: tooltip::format(date, context),
    })
}

fn milliseconds(value: f64) -> Option<i64> {
    // nL6P's Date fast path has a numeric threshold, not a digit-length test.
    let value = if value >= 10_000_000_000.0 {
        value
    } else {
        value * 1000.0
    };
    // Match Date TimeClip and truncate fractional milliseconds toward zero.
    (value.is_finite() && value.abs() <= 8_640_000_000_000_000.0).then(|| value.trunc() as i64)
}

fn relative(seconds: f64, locale: &str) -> String {
    let minutes = seconds / 60.0;
    let hours = minutes / 60.0;
    let days = hours / 24.0;
    let months = days / (365.0 / 12.0);
    let years = days / 365.0;
    let (index, count) = if seconds < 45.0 {
        (0, 0.0)
    } else if seconds < 90.0 {
        (1, 1.0)
    } else if minutes < 45.0 {
        (2, minutes)
    } else if minutes < 90.0 {
        (3, 1.0)
    } else if hours < 24.0 {
        (4, hours)
    } else if days < 2.0 {
        (5, 1.0)
    } else if days < 30.0 {
        (6, days)
    } else if days < 365.0 {
        (if months < 1.5 { 7 } else { 8 }, months)
    } else if years < 1.5 {
        (9, 1.0)
    } else {
        (10, years)
    };
    // alwaysAgo=true uses the past template even for future metadata timestamps.
    locale::past(locale)[index].replace("%d", &(count.round() as u64).to_string())
}

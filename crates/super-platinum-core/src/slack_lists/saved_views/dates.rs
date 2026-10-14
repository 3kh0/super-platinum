//! Captured +E6Z date filters/sorts, LsSA presets, and iLfW cell decoding.
//! Clock and calendar are explicit; ordinary table rows never use raw date_due.
use super::*;
use chrono::{DateTime, Datelike, Days, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone};

#[derive(Clone, Copy, Debug)]
enum Calendar {
    Local,
    Fixed(FixedOffset),
}
#[derive(Clone, Debug)]
pub struct DateContext {
    now: DateTime<FixedOffset>,
    last_read: Option<DateTime<FixedOffset>>,
    calendar: Calendar,
}
impl DateContext {
    pub fn local_now(last_read_seconds: Option<u64>) -> Self {
        Self {
            now: Local::now().fixed_offset(),
            last_read: None,
            calendar: Calendar::Local,
        }
        .with_last_read_timestamp(last_read_seconds)
    }
    pub fn from_local(now: &str, last_read: Option<&str>) -> Result<Self> {
        Self::from_local_with_offset(now, last_read, 0)
    }
    pub fn from_local_with_offset(
        now: &str,
        last_read: Option<&str>,
        offset_seconds: i32,
    ) -> Result<Self> {
        let offset = FixedOffset::east_opt(offset_seconds)
            .ok_or_else(|| unsupported("Invalid date calendar offset."))?;
        let parse = |text: &str| -> Result<DateTime<FixedOffset>> {
            let naive = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S")
                .map_err(|_| unsupported("Invalid local date context."))?;
            offset
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| unsupported("Invalid local date context."))
        };
        Ok(Self {
            now: parse(now)?,
            last_read: last_read.map(parse).transpose()?,
            calendar: Calendar::Fixed(offset),
        })
    }
    pub fn with_last_read_timestamp(&self, seconds: Option<u64>) -> Self {
        let mut result = self.clone();
        result.last_read = seconds
            .and_then(|v| i64::try_from(v).ok())
            .and_then(|v| self.timestamp(v).ok());
        result
    }
    pub fn calendar_key(&self) -> String {
        format!(
            "{}:{}",
            self.now.date_naive(),
            self.now.offset().local_minus_utc()
        )
    }
    /// A sampled instant shared by display projection and saved-view evaluation.
    pub fn now(&self) -> DateTime<FixedOffset> {
        self.now
    }
    /// Resolve the timestamp's own historical local offset, not today's offset.
    /// Fixed calendars are used only by deterministic callers such as fixtures.
    pub fn timestamp_millis(&self, value: i64) -> Option<DateTime<FixedOffset>> {
        match self.calendar {
            Calendar::Local => Local
                .timestamp_millis_opt(value)
                .single()
                .map(|v| v.fixed_offset()),
            Calendar::Fixed(offset) => offset.timestamp_millis_opt(value).single(),
        }
    }
    fn timestamp(&self, value: i64) -> Result<DateTime<FixedOffset>> {
        match self.calendar {
            Calendar::Local => Local
                .timestamp_opt(value, 0)
                .single()
                .map(|v| v.fixed_offset()),
            Calendar::Fixed(offset) => offset.timestamp_opt(value, 0).single(),
        }
        .ok_or_else(|| unsupported("Date timestamp is outside the supported range."))
    }
    fn midnight(&self, date: NaiveDate) -> Result<DateTime<FixedOffset>> {
        let value = date.and_hms_opt(0, 0, 0).unwrap();
        match self.calendar {
            Calendar::Local => Local
                .from_local_datetime(&value)
                .single()
                .map(|v| v.fixed_offset()),
            Calendar::Fixed(offset) => offset.from_local_datetime(&value).single(),
        }
        .ok_or_else(|| unsupported("This local date has an ambiguous or unavailable midnight."))
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateDependencies {
    pub calendar: bool,
    pub clock: bool,
    pub prior_read: bool,
}
pub fn date_dependencies(document: &Document, view: &SavedView) -> DateDependencies {
    let mut out = DateDependencies::default();
    for member in ["filters", "info_column_filters"] {
        for filter in view
            .raw
            .get(member)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if !document
                .columns
                .iter()
                .any(|c| Some(c.key.as_str()) == filter["key"].as_str() && kind(&c.kind))
            {
                continue;
            }
            // Fixed-date filters can compare Unix timestamps in the local zone.
            out.calendar = true;
            for value in filter["values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                match value {
                    "today" | "yesterday" | "tomorrow" | "this_week" | "next_week"
                    | "last_week" | "upcoming" => out.calendar = true,
                    "overdue" => {
                        out.calendar = true;
                        out.clock = true;
                    }
                    "since_i_last_viewed" => out.prior_read = true,
                    _ => {}
                }
            }
        }
    }
    out
}
pub(super) fn kind(kind: &str) -> bool {
    matches!(
        kind,
        "date" | "due_date" | "todo_due_date" | "created_time" | "last_edited_time"
    )
}
#[derive(Clone, Debug)]
enum DateValue {
    Missing,
    Day(NaiveDate),
    Timestamp(i64),
}
fn parse_day(text: &str) -> Result<NaiveDate> {
    let date = NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| unsupported("Saved date is malformed or unsupported."))?;
    if date.format("%Y-%m-%d").to_string() != text {
        return Err(unsupported("Saved date is not an ISO calendar date."));
    }
    Ok(date)
}
fn scalar<'a>(field: &'a Value, key: &str) -> Result<Option<&'a Value>> {
    match field.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(values)) if values.len() <= 1 => Ok(values.first()),
        _ => Err(unsupported("Date cell has malformed or multiple values.")),
    }
}
fn value(column: &Column, row: &Record, context: &ProjectionContext) -> Result<DateValue> {
    // VBDB adds created_time from canonical date_created after spreading fields.
    if column.kind == "created_time" && column.key == "created_time" {
        return match row.raw.get("date_created") {
            None | Some(Value::Null) => Ok(DateValue::Missing),
            Some(v) => v
                .as_i64()
                .map(DateValue::Timestamp)
                .ok_or_else(|| unsupported("Created-time value is malformed.")),
        };
    }
    let Some(field) = row.fields.get(&column.key) else {
        return Ok(DateValue::Missing);
    };
    if field
        .get("column_id")
        .is_some_and(|id| id.as_str() != Some(&column.id))
        || field
            .get("row_id")
            .is_some_and(|id| id.as_str() != Some(&row.id))
    {
        return Err(unsupported(
            "Date cell identity does not match its column or row.",
        ));
    }
    let date = scalar(field, "date")?
        .map(|v| {
            v.as_str()
                .ok_or_else(|| unsupported("Date cell is not a calendar string."))
        })
        .transpose()?;
    let timestamp = scalar(field, "timestamp")?
        .map(|v| {
            v.as_i64()
                .ok_or_else(|| unsupported("Date timestamp is malformed."))
        })
        .transpose()?;
    let use_timestamp = match context.date_uses_timestamps {
        Some(value) => value,
        None if date.is_some() && timestamp.is_some_and(|v| v != -1)
            || date.is_none() && timestamp == Some(-1) =>
        {
            return Err(unsupported(
                "This date needs a verified personal timestamp policy.",
            ));
        }
        None => false,
    };
    if use_timestamp && let Some(timestamp) = timestamp {
        if timestamp != -1 {
            return Ok(DateValue::Timestamp(timestamp));
        }
        if date.is_none() {
            return Ok(DateValue::Missing);
        }
    }
    if let Some(date) = date {
        return if date.is_empty() {
            Ok(DateValue::Missing)
        } else {
            parse_day(date).map(DateValue::Day)
        };
    }
    Ok(timestamp
        .map(DateValue::Timestamp)
        .unwrap_or(DateValue::Missing))
}
fn falsy(value: &Value) -> bool {
    matches!(value, Value::Null | Value::Bool(false))
        || value.as_str() == Some("")
        || value.as_f64() == Some(0.0)
}
fn text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
fn needs_clock(value: &str) -> bool {
    matches!(
        value,
        "today"
            | "yesterday"
            | "tomorrow"
            | "this_week"
            | "next_week"
            | "last_week"
            | "upcoming"
            | "overdue"
            | "since_i_last_viewed"
    )
}
pub(super) fn validate(filter: &model::Filter, context: &ProjectionContext) -> Result<()> {
    if !matches!(
        filter.operator.as_str(),
        "equal"
            | "not_equal"
            | "greater_than"
            | "less_than"
            | "greater_than_or_equal"
            | "less_than_or_equal"
            | "contains"
            | "does_not_contain"
            | "is_empty"
            | "is_not_empty"
    ) {
        return Err(unsupported("This date filter operator is unsupported."));
    }
    if matches!(filter.operator.as_str(), "is_empty" | "is_not_empty")
        || filter.values.first().is_none_or(falsy)
    {
        return Ok(());
    }
    for v in &filter.values {
        if falsy(v) {
            continue;
        }
        let v = text(v);
        if needs_clock(&v) {
            let clock = context.date.as_ref().ok_or_else(|| {
                unsupported("This saved date filter requires a local calendar context.")
            })?;
            if v == "since_i_last_viewed" && clock.last_read.is_none() {
                return Err(unsupported(
                    "This saved date filter requires the prior List read time.",
                ));
            }
        } else if v != "later" {
            parse_day(&v)?;
        }
    }
    Ok(())
}
fn shift(date: NaiveDate, days: i64) -> Result<NaiveDate> {
    if days >= 0 {
        date.checked_add_days(Days::new(days as u64))
    } else {
        date.checked_sub_days(Days::new((-days) as u64))
    }
    .ok_or_else(|| unsupported("Date filter is outside the supported calendar."))
}
// Static date filters still need the machine-local calendar to interpret numeric
// Unix timestamps; no wall clock is read for their evaluation.
fn calendar(context: &ProjectionContext) -> DateContext {
    context.date.clone().unwrap_or(DateContext {
        now: DateTime::from_timestamp(0, 0).unwrap().fixed_offset(),
        last_read: None,
        calendar: Calendar::Local,
    })
}
fn moment(value: &DateValue, clock: &DateContext) -> Result<Option<DateTime<FixedOffset>>> {
    match value {
        DateValue::Missing => Ok(None),
        DateValue::Day(day) => clock.midnight(*day).map(Some),
        DateValue::Timestamp(v) => clock.timestamp(*v).map(Some),
    }
}
fn compare(op: &str, filter: DateTime<FixedOffset>, record: DateTime<FixedOffset>) -> bool {
    match op {
        "not_equal" => filter != record,
        "greater_than" => filter < record,
        "greater_than_or_equal" => filter <= record,
        "less_than" => filter > record,
        "less_than_or_equal" => filter >= record,
        _ => filter == record,
    }
}
pub(super) fn filter(
    column: &Column,
    row: &Record,
    filter: &model::Filter,
    context: &ProjectionContext,
) -> Result<bool> {
    if !matches!(filter.operator.as_str(), "is_empty" | "is_not_empty")
        && filter.values.first().is_none_or(falsy)
    {
        return Ok(true);
    }
    let value = value(column, row, context)?;
    let empty = matches!(value, DateValue::Missing);
    match filter.operator.as_str() {
        "is_empty" => return Ok(empty),
        "is_not_empty" => return Ok(!empty),
        _ => {}
    }
    if filter.values.first().is_none_or(falsy) {
        return Ok(true);
    }
    if empty {
        return Ok(false);
    }
    if filter.values.iter().all(|v| falsy(v) || v == "later") {
        let zero = matches!(value, DateValue::Timestamp(0));
        return Ok(match filter.operator.as_str() {
            "not_equal" | "does_not_contain" => !zero,
            "equal" | "contains" => zero,
            _ => false,
        });
    }
    if matches!(value, DateValue::Timestamp(_)) && context.date.is_none() {
        return Err(unsupported(
            "Timestamp filters require an explicit local calendar.",
        ));
    }
    let exact = filter.values[0] == "since_i_last_viewed";
    let op = if exact {
        "greater_than"
    } else {
        filter.operator.as_str()
    };
    let clock = calendar(context);
    let record = moment(&value, &clock)?;
    let zero = matches!(value, DateValue::Timestamp(0));
    for v in &filter.values {
        if falsy(v) {
            continue;
        }
        let v = text(v);
        if v == "later" {
            if match op {
                "not_equal" | "does_not_contain" => !zero,
                "equal" | "contains" => zero,
                _ => false,
            } {
                return Ok(true);
            }
            continue;
        }
        let Some(record) = record else {
            continue;
        };
        if v == "overdue" {
            if !zero
                && match op {
                    "not_equal" | "does_not_contain" => record > clock.now,
                    "equal" | "contains" => record < clock.now,
                    _ => false,
                }
            {
                return Ok(true);
            }
            continue;
        }
        let today = clock.now.date_naive();
        if matches!(v.as_str(), "this_week" | "next_week" | "last_week") {
            let delta = match v.as_str() {
                "next_week" => 7,
                "last_week" => -7,
                _ => 0,
            };
            let monday = shift(
                today,
                delta - i64::from(today.weekday().num_days_from_monday()),
            )?;
            let days = (0..7)
                .map(|n| clock.midnight(shift(monday, n)?))
                .collect::<Result<Vec<_>>>()?;
            // Captured week NotEqual compares midnight dates to the unnormalized
            // timestamp. Noon can therefore satisfy equal AND not_equal.
            let found = if op == "not_equal" {
                !days.contains(&record)
            } else {
                let record = clock.midnight(record.date_naive())?;
                days.into_iter().any(|day| compare(op, day, record))
            };
            if found {
                return Ok(true);
            }
            continue;
        }
        let boundary = match v.as_str() {
            "today" => clock.midnight(today)?,
            "yesterday" => clock.midnight(shift(today, -1)?)?,
            "tomorrow" => clock.midnight(shift(today, 1)?)?,
            "upcoming" => clock.midnight(shift(today, 2)?)?,
            "since_i_last_viewed" => clock
                .last_read
                .ok_or_else(|| unsupported("Prior List read time is unavailable."))?,
            _ => clock.midnight(parse_day(&v)?)?,
        };
        let record = if exact {
            record
        } else {
            clock.midnight(record.date_naive())?
        };
        if v == "upcoming" {
            if match op {
                "not_equal" | "does_not_contain" => record < boundary,
                "equal" | "contains" => record > boundary,
                _ => false,
            } {
                return Ok(true);
            }
        } else {
            let boundary = if exact {
                boundary
            } else {
                clock.midnight(boundary.date_naive())?
            };
            if compare(op, boundary, record) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
pub(super) fn sort_key(
    column: &Column,
    row: &Record,
    context: &ProjectionContext,
    ascending: bool,
) -> Result<f64> {
    // Captured new Date(c): ISO date is UTC midnight; numeric c is milliseconds
    // here, despite Unix-seconds interpretation by the filtering consumer.
    Ok(match value(column, row, context)? {
        DateValue::Missing => {
            if ascending {
                9_007_199_254_740_990.0
            } else {
                0.0
            }
        }
        DateValue::Timestamp(0) => {
            if ascending {
                9_007_199_254_740_991.0
            } else {
                f64::NEG_INFINITY
            }
        }
        DateValue::Timestamp(v) => v as f64,
        DateValue::Day(day) => day
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis() as f64,
    })
}
#[cfg(test)]
mod tests;

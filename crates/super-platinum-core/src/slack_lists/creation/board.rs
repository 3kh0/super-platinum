//! Board lane Add: B BoardView17964742, JETx18252085, 5QWf2840942,
//! t0Mq7276635 and uDYD7312962. See docs/slack-lists-protocol.md.
//! Only existing native Board group types and flat saved filters are admitted.
use super::*;
use crate::slack_lists::{Column, LinkValue};
use std::collections::{BTreeMap, HashSet};

#[cfg(test)]
mod tests;

const MAX_FILTERS: usize = 128;
const MAX_TEXT_BYTES: usize = 16_384;
const MALFORMED: ListError =
    ListError("This Board's creation defaults are malformed or unsupported.");

impl CreationIntent {
    /// `lane` is the canonical native lane key, not its translated label.
    /// Desktop must also attest that this lane is currently rendered, and bind
    /// `viewer` to the authenticated owner before and after asynchronous work.
    /// Run outside render/event paths because text defaults use the URL detector.
    pub fn capture_board(
        document: &Document,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
        selected_view: &str,
        lane: &str,
        viewer: &str,
    ) -> Result<Self, ListError> {
        let mut intent = Self::capture(document, allow_lists, personal_tasks, Some(selected_view))?;
        let view = document
            .views
            .iter()
            .find(|v| v["id"] == selected_view)
            .ok_or(MALFORMED)?;
        if view["type"] != "board" || view.get("filter_set").is_some_and(|v| !v.is_null()) {
            return Err(MALFORMED);
        }
        let group = group_column(document, view)?;
        let mut seeds = Vec::new();
        if let Some(column) = group {
            // JETx concatenates filters/infoColumnFilters and keeps the first key.
            let filters = array(view, "filters")?;
            let info = array(view, "info_column_filters")?;
            if filters.len() + info.len() > MAX_FILTERS {
                return Err(MALFORMED);
            }
            let mut seen = HashSet::new();
            for filter in filters.iter().chain(info) {
                let key = filter.get("key").and_then(Value::as_str).ok_or(MALFORMED)?;
                if !seen.insert(key) {
                    continue;
                }
                let target = column_by_key(document, key, filter.get("column_id"))?;
                if let Some(seed) = filter_seed(target, filter, viewer)? {
                    seeds.push(seed);
                }
            }
            seeds.push(lane_seed(column, lane)?);
        } else if !lane.is_empty() {
            return Err(MALFORMED);
        }
        // t0Mq appends this after the caller's lane/filter fields, not before.
        if let Some(completed) = intent.initial_fields.as_ref().and_then(Value::as_array) {
            seeds.extend(completed.iter().cloned());
        }
        validate_duplicates(&seeds)?;
        intent.initial_fields = (!seeds.is_empty()).then(|| Value::Array(seeds));
        Ok(intent)
    }
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value], ListError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(values)) => Ok(values),
        _ => Err(MALFORMED),
    }
}
fn column_by_key<'a>(
    document: &'a Document,
    key: &str,
    id: Option<&Value>,
) -> Result<&'a Column, ListError> {
    let mut matches = document.columns.iter().filter(|c| c.key == key);
    let column = matches.next().ok_or(MALFORMED)?;
    if matches.next().is_some()
        || id.is_some_and(|id| !id.is_null() && id.as_str() != Some(&column.id))
    {
        return Err(MALFORMED);
    }
    Ok(column)
}
fn group_column<'a>(document: &'a Document, view: &Value) -> Result<Option<&'a Column>, ListError> {
    let Some(group) = view.get("grouping").filter(|g| !g.is_null()) else {
        return Ok(None);
    };
    let object = group.as_object().ok_or(MALFORMED)?;
    let key = match group.get("group_by") {
        None if object.is_empty() => return Ok(None),
        Some(Value::String(key)) if key.is_empty() => return Ok(None),
        Some(Value::String(key)) => key,
        _ => return Err(MALFORMED),
    };
    let column = column_by_key(document, key, group.get("group_by_column_id"))?;
    if !matches!(
        column.kind.as_str(),
        "select" | "checkbox" | "todo_completed"
    ) {
        return Err(MALFORMED);
    }
    Ok(Some(column))
}
fn choice(column: &Column, value: &str) -> Result<bool, ListError> {
    let options = column.raw.get("options").filter(|v| !v.is_null());
    if options.is_some_and(|v| !v.is_object()) {
        return Err(MALFORMED);
    }
    let values = match options.and_then(|v| v.get("choices")) {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(values)) => values,
        _ => return Err(MALFORMED),
    };
    let mut seen = HashSet::new();
    let mut found = false;
    for option in values {
        let id = option
            .get("value")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or(MALFORMED)?;
        if !seen.insert(id) {
            return Err(MALFORMED);
        }
        found |= id == value;
    }
    Ok(found)
}
fn seed(column: &Column, member: &str, value: Value) -> Value {
    let mut field = json!({"column_id":column.id});
    field[member] = value;
    field
}
fn lane_seed(column: &Column, lane: &str) -> Result<Value, ListError> {
    match column.kind.as_str() {
        "select" => {
            if !lane.is_empty() && !choice(column, lane)? {
                return Err(MALFORMED);
            }
            Ok(seed(
                column,
                "select",
                if lane.is_empty() {
                    json!([])
                } else {
                    json!([lane])
                },
            ))
        }
        "checkbox" | "todo_completed" if matches!(lane, "" | "true") => {
            Ok(seed(column, "checkbox", json!(lane == "true")))
        }
        _ => Err(MALFORMED),
    }
}
fn filter_seed(column: &Column, filter: &Value, viewer: &str) -> Result<Option<Value>, ListError> {
    let operator = filter
        .get("operator")
        .and_then(Value::as_str)
        .ok_or(MALFORMED)?;
    if !matches!(
        column.kind.as_str(),
        "text"
            | "select"
            | "multi_select"
            | "user"
            | "rating"
            | "assignee"
            | "checkbox"
            | "todo_completed"
    ) || !matches!(operator, "contains" | "equal" | "is_me" | "is_not_empty")
    {
        return Ok(None);
    }
    let values = filter
        .get("values")
        .and_then(Value::as_array)
        .ok_or(MALFORMED)?;
    if values.len() > MAX_FILTERS {
        return Err(MALFORMED);
    }
    let first = values.first();
    let result = match column.kind.as_str() {
        "text" => {
            let rich = match first {
                None | Some(Value::Null) | Some(Value::Bool(false)) => json!([]),
                Some(Value::Number(n)) if n.as_f64() == Some(0.0) => json!([]),
                Some(Value::String(s)) if s.is_empty() => json!([]),
                Some(Value::String(s)) if s.len() <= MAX_TEXT_BYTES => LinkValue::board_text(s)?,
                _ => return Err(MALFORMED),
            };
            // [] is truthy in JS: retain an explicit empty rich-text seed.
            seed(column, "rich_text", rich)
        }
        "rating" => {
            let Some(number) = rating(first)? else {
                return Ok(None);
            };
            seed(column, "rating", json!([number]))
        }
        "checkbox" | "todo_completed" => {
            if operator != "is_not_empty" {
                return Ok(None);
            }
            seed(column, "checkbox", json!(true))
        }
        _ => {
            let value = match first {
                None | Some(Value::Null) | Some(Value::Bool(false)) => return Ok(None),
                Some(Value::Number(n)) if n.as_f64() == Some(0.0) => return Ok(None),
                Some(Value::String(s)) if s.is_empty() => return Ok(None),
                Some(Value::String(s)) if s.len() <= MAX_TEXT_BYTES => {
                    if s == "viewer" {
                        viewer
                    } else {
                        s
                    }
                }
                _ => return Err(MALFORMED),
            };
            if value.is_empty() {
                return Ok(None);
            }
            if matches!(column.kind.as_str(), "select" | "multi_select") {
                if !choice(column, value)? {
                    return Ok(None);
                }
                seed(
                    column,
                    "select",
                    json!(value.split('|').collect::<Vec<_>>()),
                )
            } else {
                let ids = value.split('|').collect::<Vec<_>>();
                let mut seen = HashSet::new();
                if ids.iter().any(|id| !valid_user(id) || !seen.insert(*id)) {
                    return Err(MALFORMED);
                }
                seed(column, "user", json!(ids))
            }
        }
    };
    Ok(Some(result))
}
fn valid_user(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() >= 9
        && matches!(b[0], b'U' | b'W')
        && b[1..]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn rating(value: Option<&Value>) -> Result<Option<serde_json::Number>, ListError> {
    let number = match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => return Ok(None),
        Some(Value::Bool(true)) => 1.0,
        Some(Value::Number(n)) => n.as_f64().ok_or(MALFORMED)?,
        Some(Value::String(s)) if s.len() <= 128 => {
            let s = s.trim_matches(js_space);
            if s.is_empty() {
                return Ok(None);
            }
            if let Some((digits, radix)) = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .map(|s| (s, 16))
                .or_else(|| {
                    s.strip_prefix("0b")
                        .or_else(|| s.strip_prefix("0B"))
                        .map(|s| (s, 2))
                })
                .or_else(|| {
                    s.strip_prefix("0o")
                        .or_else(|| s.strip_prefix("0O"))
                        .map(|s| (s, 8))
                })
            {
                // Bounded exact integers; do not guess JS rounding of huge radices.
                if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                    return Ok(None);
                }
                let n = u64::from_str_radix(digits, radix).map_err(|_| MALFORMED)?;
                if n > 9_007_199_254_740_991 {
                    return Err(MALFORMED);
                }
                n as f64
            } else {
                if s.contains("inf") || s.contains("Inf") {
                    return Err(MALFORMED);
                }
                match s.parse::<f64>() {
                    Ok(n) => n,
                    Err(_) => return Ok(None),
                }
            }
        }
        _ => return Err(MALFORMED),
    };
    if number == 0.0 || number.is_nan() {
        return Ok(None);
    }
    if !number.is_finite() {
        return Err(MALFORMED);
    }
    Ok(Some(
        if number.fract() == 0.0 && number.abs() <= 9_007_199_254_740_991.0 {
            serde_json::Number::from(number as i64)
        } else {
            serde_json::Number::from_f64(number).ok_or(MALFORMED)?
        },
    ))
}
fn validate_duplicates(seeds: &[Value]) -> Result<(), ListError> {
    let mut columns = BTreeMap::new();
    for field in seeds {
        let id = field["column_id"].as_str().ok_or(MALFORMED)?;
        if columns.insert(id, field).is_some_and(|old| old != field) {
            return Err(ListError("This lane has conflicting creation defaults."));
        }
    }
    Ok(())
}

/// Validate every typed seed in the creation receipt. Later readback may contain
/// legitimate subsequent edits.
pub(super) fn validate_initial_fields(
    document: &Document,
    seeds: Option<&Value>,
    record: &Record,
) -> Result<(), ListError> {
    let Some(seeds) = seeds else {
        return Ok(());
    };
    let seeds = seeds.as_array().ok_or(MALFORMED)?;
    validate_duplicates(seeds)?;
    for initial in seeds {
        let column = document
            .columns
            .iter()
            .find(|c| initial["column_id"] == c.id)
            .ok_or(MALFORMED)?;
        let member = match column.kind.as_str() {
            "text" => "rich_text",
            "select" | "multi_select" => "select",
            "user" | "assignee" => "user",
            "rating" => "rating",
            "checkbox" | "todo_completed" => "checkbox",
            _ => return Err(MALFORMED),
        };
        let expected = initial.get(member).ok_or(MALFORMED)?;
        let field = record.fields.get(&column.key).ok_or(ListError(
            "Slack has not confirmed the new item's initial fields.",
        ))?;
        if field.as_object().is_none_or(|object| {
            object.iter().any(|(key, value)| {
                !value.is_null()
                    && key != member
                    && !(member == "rich_text" && key == "richText")
                    && !matches!(key.as_str(), "key" | "row_id" | "column_id" | "text")
            })
        }) {
            return Err(MALFORMED);
        }
        let snake = field.get(member);
        let camel = (member == "rich_text")
            .then(|| field.get("richText"))
            .flatten();
        if snake.zip(camel).is_some_and(|(a, b)| a != b) || snake.or(camel) != Some(expected) {
            return Err(ListError(
                "Slack has not confirmed the new item's initial fields.",
            ));
        }
    }
    Ok(())
}

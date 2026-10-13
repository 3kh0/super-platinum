use super::*;
use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use std::collections::HashSet;

pub(super) fn wire_type(kind: &InputKind) -> &str {
    match kind {
        InputKind::Text { .. } => "plain_text_input",
        InputKind::StaticSelect {
            multiple: false, ..
        } => "static_select",
        InputKind::StaticSelect { multiple: true, .. } => "multi_static_select",
        InputKind::Checkboxes { .. } => "checkboxes",
        InputKind::Radio { .. } => "radio_buttons",
        InputKind::Date => "datepicker",
        InputKind::Time { .. } => "timepicker",
        InputKind::DateTime => "datetimepicker",
        InputKind::Unsupported { kind } => kind,
    }
}
pub(super) fn matches(kind: &InputKind, value: &InputValue) -> bool {
    matches!(
        (kind, value),
        (InputKind::Text { .. }, InputValue::Text(_))
            | (
                InputKind::StaticSelect { .. }
                    | InputKind::Checkboxes { .. }
                    | InputKind::Radio { .. },
                InputValue::Selected(_)
            )
            | (InputKind::Date, InputValue::Date(_))
            | (InputKind::Time { .. }, InputValue::Time(_))
            | (InputKind::DateTime, InputValue::DateTime(_))
    )
}
fn choices(kind: &InputKind) -> Option<(&[OptionSpec], bool, Option<usize>)> {
    match kind {
        InputKind::StaticSelect {
            options,
            multiple,
            max_selected_items,
        } => Some((options, *multiple, *max_selected_items)),
        InputKind::Checkboxes { options } => Some((options, true, None)),
        InputKind::Radio { options } => Some((options, false, None)),
        _ => None,
    }
}
pub(super) fn error(field: &FormField) -> Option<String> {
    if matches!(field.kind, InputKind::Unsupported { .. }) {
        return Some("This control is not supported yet.".into());
    }
    if !matches(&field.kind, &field.value) {
        return Some("This value no longer matches the field.".into());
    }
    let empty = match &field.value {
        InputValue::Text(text) => text.is_empty(),
        InputValue::Selected(values) => values.is_empty(),
        InputValue::Date(value) | InputValue::Time(value) => {
            value.as_ref().is_none_or(|v| v.is_empty())
        }
        InputValue::DateTime(value) => value.is_none(),
        InputValue::Unsupported => true,
    };
    if empty {
        return (!field.optional).then(|| "Complete this required field.".into());
    }
    match (&field.kind, &field.value) {
        (
            InputKind::Text {
                min_length,
                max_length,
                multiline,
            },
            InputValue::Text(text),
        ) => {
            // Captured Slack validation and HTML length constraints use JS
            // string.length (UTF-16 code units), not UTF-8 byte length.
            let len = text.encode_utf16().count();
            if !multiline && (text.contains('\n') || text.contains('\r')) {
                return Some("Use a single line of text.".into());
            }
            if min_length.is_some_and(|min| len < min) {
                return Some(format!(
                    "Enter at least {} characters.",
                    min_length.unwrap()
                ));
            }
            if max_length.is_some_and(|max| len > max) {
                return Some(format!(
                    "Enter no more than {} characters.",
                    max_length.unwrap()
                ));
            }
        }
        (kind, InputValue::Selected(selected)) => {
            let (options, multiple, max) = choices(kind)?;
            let mut seen = HashSet::new();
            if selected.iter().any(|value| {
                !seen.insert(value) || !options.iter().any(|option| &option.value == value)
            }) {
                return Some("One of your selected options is no longer available.".into());
            }
            if !multiple && selected.len() > 1 {
                return Some("Choose one option.".into());
            }
            if max.is_some_and(|max| selected.len() > max) {
                return Some(format!("Choose no more than {} options.", max.unwrap()));
            }
        }
        (InputKind::Date, InputValue::Date(Some(date))) => {
            if date.len() != 10
                || NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .ok()
                    .is_none_or(|parsed| parsed.format("%Y-%m-%d").to_string() != *date)
            {
                return Some("Enter a valid date in YYYY-MM-DD format.".into());
            }
        }
        (InputKind::Time { .. }, InputValue::Time(Some(time))) => {
            let valid = time.len() == 5
                && time.as_bytes()[2] == b':'
                && time
                    .as_bytes()
                    .iter()
                    .enumerate()
                    .all(|(index, byte)| index == 2 || byte.is_ascii_digit())
                && time[..2].parse::<u8>().is_ok_and(|n| n < 24)
                && time[3..].parse::<u8>().is_ok_and(|n| n < 60);
            if !valid {
                return Some("Enter a valid time in HH:mm format.".into());
            }
        }
        (InputKind::DateTime, InputValue::DateTime(Some(value))) => {
            if *value < 0 || chrono::DateTime::from_timestamp(*value, 0).is_none() {
                return Some("Enter a valid date and time.".into());
            }
        }
        _ => {}
    }
    None
}

pub(super) fn wire(field: &FormField) -> Value {
    let mut value = json!({"type":wire_type(&field.kind)});
    match &field.value {
        InputValue::Text(text) => {
            value["value"] = if text.is_empty() {
                Value::Null
            } else {
                json!(text)
            }
        }
        InputValue::Selected(selected) => {
            let (options, multiple, _) = choices(&field.kind).expect("validated choices");
            let selected: Vec<_> = selected
                .iter()
                .map(|value| {
                    options
                        .iter()
                        .find(|option| &option.value == value)
                        .expect("validated option")
                        .raw
                        .clone()
                })
                .collect();
            if multiple {
                value["selected_options"] = json!(selected);
            } else {
                value["selected_option"] = selected.into_iter().next().unwrap_or(Value::Null);
            }
        }
        InputValue::Date(date) => {
            value["selected_date"] = json!(date.as_ref().filter(|v| !v.is_empty()))
        }
        InputValue::Time(time) => {
            value["selected_time"] = json!(time.as_ref().filter(|v| !v.is_empty()));
            if let InputKind::Time {
                timezone: Some(timezone),
            } = &field.kind
            {
                value["timezone"] = json!(timezone);
            }
        }
        InputValue::DateTime(timestamp) => value["selected_date_time"] = json!(timestamp),
        InputValue::Unsupported => {}
    }
    value
}

pub(super) fn from_wire(kind: &InputKind, value: &Value) -> Result<InputValue, FormError> {
    let invalid = || FormError::InvalidView("Malformed saved input state".into());
    if value.get("type").and_then(Value::as_str) != Some(wire_type(kind)) {
        return Err(invalid());
    }
    let string = |key: &str| -> Result<Option<String>, FormError> {
        match value.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_str().map(|s| Some(s.into())).ok_or_else(invalid),
        }
    };
    Ok(match kind {
        InputKind::Text { .. } => InputValue::Text(string("value")?.unwrap_or_default()),
        InputKind::Date => InputValue::Date(string("selected_date")?),
        InputKind::Time { .. } => InputValue::Time(string("selected_time")?),
        InputKind::DateTime => InputValue::DateTime(match value.get("selected_date_time") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.as_i64().ok_or_else(invalid)?),
        }),
        InputKind::Unsupported { .. } => InputValue::Unsupported,
        kind => {
            let (options, multiple, _) = choices(kind).ok_or_else(invalid)?;
            let selected: Vec<&Value> = if multiple {
                match value.get("selected_options") {
                    None | Some(Value::Null) => vec![],
                    Some(v) => v.as_array().ok_or_else(invalid)?.iter().collect(),
                }
            } else {
                value
                    .get("selected_option")
                    .filter(|v| !v.is_null())
                    .into_iter()
                    .collect()
            };
            let mut values = vec![];
            for selected in selected {
                let id = selected
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                if !options
                    .iter()
                    .any(|option| option.value == id && option.raw == *selected)
                {
                    return Err(invalid());
                }
                values.push(id.into());
            }
            InputValue::Selected(values)
        }
    })
}

pub fn datetime_local_value(value: Option<i64>) -> String {
    value
        .and_then(|value| Local.timestamp_opt(value, 0).single())
        .map(|value| value.format("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default()
}
pub fn parse_datetime_local(value: &str) -> Result<Option<i64>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    let date = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .map_err(|_| "Enter a valid date and time.".to_string())?;
    if date.format("%Y-%m-%dT%H:%M").to_string() != value {
        return Err("Enter a valid date and time.".into());
    }
    Local
        .from_local_datetime(&date)
        .single()
        .map(|date| Some(date.timestamp()))
        .ok_or_else(|| {
            "This local time is ambiguous or does not exist. Choose another time.".into()
        })
}

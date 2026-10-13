//! Ordinary timepicker values reuse modal time validation without modal requiredness.
use super::*;

pub fn parse_message_time(element: &Value) -> Result<FormField, FormError> {
    if element.get("type").and_then(Value::as_str) != Some("timepicker") {
        return Err(FormError::ValueTypeMismatch);
    }
    let initial = match element.get("initial_time") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value.is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        _ => return Err(FormError::ValueTypeMismatch),
    };
    let timezone = match element.get("timezone") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value.is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        _ => return Err(FormError::ValueTypeMismatch),
    };
    let placeholder = match element.get("placeholder") {
        None | Some(Value::Null) => None,
        Some(value) if value.get("type").and_then(Value::as_str) == Some("plain_text") => Some(
            value
                .get("text")
                .and_then(Value::as_str)
                .ok_or(FormError::ValueTypeMismatch)?
                .to_owned(),
        ),
        _ => return Err(FormError::ValueTypeMismatch),
    };
    let field = FormField {
        label: "Choose a time".into(),
        hint: None,
        placeholder,
        optional: true,
        kind: InputKind::Time { timezone },
        initial_value: InputValue::Time(initial.clone()),
        value: InputValue::Time(initial),
        dirty: false,
        error: None,
        revision: 0,
        raw: element.clone(),
    };
    if values::error(&field).is_some() {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(field)
}

/// None is the captured explicit-null action. Empty or malformed strings are not times.
pub fn validate_message_time(
    field: &FormField,
    selected: Option<&str>,
) -> Result<Value, FormError> {
    if !matches!(field.kind, InputKind::Time { .. }) || selected.is_some_and(str::is_empty) {
        return Err(FormError::ValueTypeMismatch);
    }
    let mut field = field.clone();
    field.value = InputValue::Time(selected.map(str::to_owned));
    if values::error(&field).is_some() {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(values::wire(&field))
}

//! Ordinary datepicker values reuse modal date validation without modal requiredness.
use super::*;

pub fn parse_message_date(element: &Value) -> Result<FormField, FormError> {
    if element.get("type").and_then(Value::as_str) != Some("datepicker") {
        return Err(FormError::ValueTypeMismatch);
    }
    let initial = match element.get("initial_date") {
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
        label: "Choose a date".into(),
        hint: None,
        placeholder,
        optional: true,
        kind: InputKind::Date,
        initial_value: InputValue::Date(initial.clone()),
        value: InputValue::Date(initial),
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

/// None is the captured explicit-null action. Empty or malformed strings are not dates.
pub fn validate_message_date(
    field: &FormField,
    selected: Option<&str>,
) -> Result<Value, FormError> {
    if !matches!(field.kind, InputKind::Date) || selected.is_some_and(str::is_empty) {
        return Err(FormError::ValueTypeMismatch);
    }
    let mut field = field.clone();
    field.value = InputValue::Date(selected.map(str::to_owned));
    if values::error(&field).is_some() {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(values::wire(&field))
}

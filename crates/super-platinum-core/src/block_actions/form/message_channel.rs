//! Captured cdYm message channel values. Directory enforces public/nonarchived policy.
use super::*;

pub fn parse_message_channel(element: &Value) -> Result<FormField, FormError> {
    if element.get("type").and_then(Value::as_str) != Some("channels_select") {
        return Err(FormError::ValueTypeMismatch);
    }
    if modal_channel::unsupported_settings(element) {
        return Err(FormError::ValueTypeMismatch);
    }
    let InputValue::Channel(initial) = modal_channel::initial(element)? else {
        unreachable!()
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
        label: "Select a channel".into(),
        hint: None,
        placeholder,
        optional: true,
        kind: InputKind::Channel,
        initial_value: InputValue::Channel(initial.clone()),
        value: InputValue::Channel(initial),
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

/// Ordinary message controls are required in cdYm: no clear action is exposed.
pub fn validate_message_channel(
    field: &FormField,
    selected: Option<&str>,
) -> Result<Value, FormError> {
    if !matches!(field.kind, InputKind::Channel) || selected.is_none_or(str::is_empty) {
        return Err(FormError::ValueTypeMismatch);
    }
    let mut field = field.clone();
    field.value = InputValue::Channel(selected.map(str::to_owned));
    if values::error(&field).is_some() {
        return Err(FormError::ValueTypeMismatch);
    }
    let mut wire = values::wire(&field);
    if field.raw.get("response_url_enabled") == Some(&Value::Bool(true)) {
        wire["response_url_enabled"] = json!(true);
    }
    Ok(wire)
}

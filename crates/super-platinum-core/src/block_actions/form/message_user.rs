//! Ordinary users_select values reuse modal user validation without modal requiredness.
use super::*;

pub fn parse_message_user(element: &Value) -> Result<FormField, FormError> {
    if element.get("type").and_then(Value::as_str) != Some("users_select") {
        return Err(FormError::ValueTypeMismatch);
    }
    let initial = match element.get("initial_user") {
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
        label: "Select a user".into(),
        hint: None,
        placeholder,
        optional: true,
        kind: InputKind::User,
        initial_value: InputValue::User(initial.clone()),
        value: InputValue::User(initial),
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

/// None is the captured explicit-null action. Empty or malformed strings are not users.
pub fn validate_message_user(
    field: &FormField,
    selected: Option<&str>,
) -> Result<Value, FormError> {
    if !matches!(field.kind, InputKind::User) || selected.is_some_and(str::is_empty) {
        return Err(FormError::ValueTypeMismatch);
    }
    let mut field = field.clone();
    field.value = InputValue::User(selected.map(str::to_owned));
    if values::error(&field).is_some() {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(values::wire(&field))
}

/// Canonical user IDs, including the built-in Slackbot identity accepted by the
/// captured member search defaults. Synthetic short IDs are not wire identities.
pub fn valid_message_user(id: &str) -> bool {
    id == "USLACKBOT"
        || (id.len() >= 9
            && matches!(id.as_bytes().first(), Some(b'U' | b'W'))
            && id
                .bytes()
                .skip(1)
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
}

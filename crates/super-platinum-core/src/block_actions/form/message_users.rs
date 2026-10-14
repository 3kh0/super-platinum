//! Ordinary hGZA multi-user selections; staging itself never commits wire state.
use super::*;
use std::collections::BTreeSet;

pub fn parse_message_users(element: &Value) -> Result<FormField, FormError> {
    if element.get("type").and_then(Value::as_str) != Some("multi_users_select") {
        return Err(FormError::ValueTypeMismatch);
    }
    let max = element
        .get("max_selected_items")
        .map(|value| {
            value
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or(FormError::ValueTypeMismatch)
        })
        .transpose()?;
    let initial = match element.get("initial_users") {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(values)) if values.len() <= 1000 => values
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or(FormError::ValueTypeMismatch)
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(FormError::ValueTypeMismatch),
    };
    let required = element.get("required").map_or(Ok(false), |v| {
        v.as_bool().ok_or(FormError::ValueTypeMismatch)
    })?;
    let placeholder = match element.get("placeholder") {
        None | Some(Value::Null) => None,
        Some(value) if value["type"] == "plain_text" => Some(
            value
                .get("text")
                .and_then(Value::as_str)
                .filter(|s| s.encode_utf16().count() <= 150)
                .ok_or(FormError::ValueTypeMismatch)?
                .to_owned(),
        ),
        _ => return Err(FormError::ValueTypeMismatch),
    };
    let mut field = FormField {
        label: "Select users".into(),
        hint: None,
        placeholder,
        optional: true,
        kind: InputKind::MultiUser {
            max_selected_items: None,
        },
        initial_value: InputValue::Users(initial.clone()),
        value: InputValue::Users(initial.clone()),
        dirty: false,
        error: None,
        revision: 0,
        raw: element.clone(),
    };
    // A newly reduced maximum must leave existing selections visible/removable.
    // Validate their identities here; enforce requiredness and the current limit
    // when the user applies a new selection, not on untouched sibling state.
    validate_message_users(&field, &initial)?;
    field.kind = InputKind::MultiUser {
        max_selected_items: max,
    };
    field.optional = !required;
    Ok(field)
}

pub fn validate_message_users(field: &FormField, selected: &[String]) -> Result<Value, FormError> {
    let InputKind::MultiUser { max_selected_items } = field.kind else {
        return Err(FormError::ValueTypeMismatch);
    };
    let mut seen = BTreeSet::new();
    if selected.len() > 1000
        || (!field.optional && selected.is_empty())
        || max_selected_items.is_some_and(|max| selected.len() > max)
        || selected
            .iter()
            .any(|id| !valid_message_user(id) || !seen.insert(id))
    {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(json!({"type":"multi_users_select", "selected_users":selected}))
}

//! Message choices have no required-field submission gate. Empty arrays are real
//! selections and confirmations are handled by the caller before commit.
use super::*;
use std::collections::BTreeSet;

fn invalid() -> FormError {
    FormError::InvalidView("Malformed message choices".into())
}

pub fn parse_message_multi(element: &Value) -> Result<FormField, FormError> {
    let checkbox = match element.get("type").and_then(Value::as_str) {
        Some("checkboxes") => true,
        Some("multi_static_select") => false,
        _ => return Err(invalid()),
    };
    if checkbox && element.get("option_groups").is_some() {
        return Err(invalid());
    }
    let options = parse::options(element)?;
    for option in &options {
        for name in ["text", "description"] {
            let Some(value) = option.raw.get(name) else {
                continue;
            };
            let kind = value.get("type").and_then(Value::as_str);
            if !(kind == Some("plain_text") || checkbox && kind == Some("mrkdwn"))
                || value.get("text").and_then(Value::as_str).is_none()
                || ["emoji", "verbatim"]
                    .iter()
                    .any(|key| value.get(key).is_some_and(|v| !v.is_boolean()))
            {
                return Err(invalid());
            }
        }
    }
    let max = match element.get("max_selected_items") {
        None => None,
        Some(value) if !checkbox => Some(
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value > 0)
                .ok_or_else(invalid)?,
        ),
        Some(_) => return Err(invalid()),
    };
    let initial = match element.get("initial_options") {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(values)) => values.as_slice(),
        _ => return Err(invalid()),
    };
    let mut seen = BTreeSet::new();
    let mut selected = vec![];
    for value in initial {
        let option = options
            .iter()
            .find(|option| &option.raw == value)
            .ok_or_else(invalid)?;
        if !seen.insert(&option.value) {
            return Err(invalid());
        }
        selected.push(option.value.clone());
    }
    let placeholder = match element.get("placeholder") {
        None => None,
        Some(value) if value["type"] == "plain_text" => Some(
            value
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?
                .to_owned(),
        ),
        _ => return Err(invalid()),
    };
    let kind = if checkbox {
        InputKind::Checkboxes { options }
    } else {
        InputKind::StaticSelect {
            options,
            multiple: true,
            max_selected_items: max,
        }
    };
    let initial_value = InputValue::Selected(selected);
    Ok(FormField {
        label: "Selection".into(),
        hint: None,
        placeholder,
        optional: true,
        kind,
        initial_value: initial_value.clone(),
        value: initial_value,
        dirty: false,
        error: None,
        revision: 0,
        raw: element.clone(),
    })
}

/// Retained options come only from the caller's trusted mounted control state,
/// never DOM metadata. Slack keeps those raw objects through option-list updates.
pub(crate) fn validate_selection(
    field: &FormField,
    selected: &[Value],
    retained: &[Value],
) -> Result<(), FormError> {
    let (options, max) = match &field.kind {
        InputKind::Checkboxes { options } => (options, None),
        InputKind::StaticSelect {
            options,
            multiple: true,
            max_selected_items,
        } => (options, *max_selected_items),
        _ => return Err(invalid()),
    };
    if selected.len() > 1000 || retained.len() > 1000 || max.is_some_and(|max| selected.len() > max)
    {
        return Err(invalid());
    }
    let mut seen = BTreeSet::new();
    for option in selected {
        let value = option
            .get("value")
            .and_then(Value::as_str)
            .filter(|v| v.len() <= 2000)
            .ok_or_else(invalid)?;
        if !seen.insert(value)
            || !options.iter().any(|current| &current.raw == option) && !retained.contains(option)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

/// etBz's untouched, uninitialized picker sends undefined rather than an empty
/// array. Checkbox state is always an array, and existing selections may not be
/// erased by claiming this uninitialized state.
pub(crate) fn validate_optional_selection(
    field: &FormField,
    selected: Option<&[Value]>,
    retained: &[Value],
) -> Result<(), FormError> {
    if selected.is_none()
        && (!matches!(field.kind, InputKind::StaticSelect { multiple: true, .. })
            || field
                .raw
                .get("initial_options")
                .is_some_and(|value| !value.is_null())
            || !retained.is_empty())
    {
        return Err(invalid());
    }
    validate_selection(field, selected.unwrap_or_default(), retained)
}

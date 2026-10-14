//! JQ84 ordered dynamic selections; initial/saved options need no static membership.
use super::*;
use std::collections::BTreeSet;

// Local work bound, independent of the app's current selection maximum.
const MAX_SELECTIONS: usize = 1000;

pub(super) fn parse_options(value: &Value) -> Result<Vec<ExternalOption>, FormError> {
    let options = value
        .as_array()
        .filter(|values| values.len() <= MAX_SELECTIONS)
        .ok_or(FormError::ValueTypeMismatch)?
        .iter()
        .map(ExternalOption::parse)
        .collect::<Result<Vec<_>, _>>()?;
    unique(&options)?;
    Ok(options)
}

fn unique(options: &[ExternalOption]) -> Result<(), FormError> {
    let mut values = BTreeSet::new();
    if options.len() > MAX_SELECTIONS || options.iter().any(|o| !values.insert(o.value())) {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(())
}

pub fn parse_message_external_options(element: &Value) -> Result<FormField, FormError> {
    if element["type"] != "multi_external_select" || element.get("initial_option").is_some() {
        return Err(FormError::ValueTypeMismatch);
    }
    // Both selectors share routing/placeholder/minimum/unsupported settings.
    // Only the private temporary shape changes; the canonical element is retained.
    let mut settings = element.clone();
    settings["type"] = json!("external_select");
    let mut field = parse_message_external(&settings)?;
    let InputKind::ExternalSelect { min_query_length } = field.kind else {
        unreachable!("single settings parser")
    };
    let max_selected_items = element
        .get("max_selected_items")
        .map(|value| {
            value
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or(FormError::ValueTypeMismatch)
        })
        .transpose()?;
    let initial = element
        .get("initial_options")
        .filter(|v| !v.is_null())
        .map(parse_options)
        .transpose()?
        .unwrap_or_default();
    // Current maximum/requiredness constrain Apply, not visibility of existing
    // selections or untouched sibling state after a maximum reduction.
    field.kind = InputKind::MultiExternalSelect {
        min_query_length,
        max_selected_items,
    };
    field.label = "Select options".into();
    field.initial_value = InputValue::ExternalOptions(initial.clone());
    field.value = InputValue::ExternalOptions(initial);
    field.raw = element.clone();
    Ok(field)
}

pub fn validate_message_external_options(
    field: &FormField,
    selected: &[ExternalOption],
) -> Result<Value, FormError> {
    let InputKind::MultiExternalSelect {
        max_selected_items, ..
    } = field.kind
    else {
        return Err(FormError::ValueTypeMismatch);
    };
    unique(selected)?;
    if (!field.optional && selected.is_empty())
        || max_selected_items.is_some_and(|n| selected.len() > n)
    {
        return Err(FormError::ValueTypeMismatch);
    }
    Ok(
        json!({"type":"multi_external_select", "selected_options":selected.iter().map(ExternalOption::raw).collect::<Vec<_>>()}),
    )
}

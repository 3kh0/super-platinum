//! First normal-modal number scope: required canonical nonnegative integers.
//! cxJV preserves the initial literal; dd8t Number(raw).toString and blur are
//! identity operations in 0..999999999. Home retains its separate numeric path.
use super::*;

pub(super) fn initial(element: &Value) -> Result<Option<String>, FormError> {
    element
        .get("initial_value")
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| FormError::InvalidView("Invalid initial number".into()))
        })
        .transpose()
}

pub(super) fn canonical(value: &str) -> bool {
    value == "0"
        || (!value.is_empty()
            && value.len() <= 9
            && matches!(value.as_bytes()[0], b'1'..=b'9')
            && value.bytes().all(|c| c.is_ascii_digit()))
}

pub(super) fn supported(block: &Value, element: &Value) -> bool {
    block
        .get("optional")
        .is_none_or(|v| v.as_bool() == Some(false))
        && block
            .get("dispatch_action")
            .is_none_or(|v| v.as_bool() == Some(false))
        && element
            .get("is_decimal_allowed")
            .is_some_and(|v| v.as_bool() == Some(false))
        && element.get("min_value").is_none()
        && element.get("max_value").is_none()
        && ["dispatch_action_config", "confirm", "visible_to_user_ids"]
            .iter()
            .all(|name| element.get(*name).is_none())
}

pub(super) fn canonical_signed(value: &str) -> bool {
    canonical(value)
        || value
            .strip_prefix('-')
            .is_some_and(|magnitude| magnitude != "0" && canonical(magnitude))
}

pub(super) fn error(field: &FormField) -> Option<String> {
    error_domain(field, false)
}

pub(super) fn error_signed(field: &FormField) -> Option<String> {
    error_domain(field, true)
}

fn error_domain(field: &FormField, signed: bool) -> Option<String> {
    let (InputKind::Number { .. }, InputValue::Number(value)) = (&field.kind, &field.value) else {
        return Some("This value no longer matches the field.".into());
    };
    let Some(value) = value.as_deref().filter(|v| !v.is_empty()) else {
        // Exact OL35 default required mapping (number has no special branch).
        return Some("Please complete this required field.".into());
    };
    let valid = if signed {
        canonical_signed(value)
    } else {
        canonical(value)
    };
    (!valid).then(|| "This number format is not supported yet.".into())
}

#[cfg(test)]
mod signed_tests;
#[cfg(test)]
mod tests;

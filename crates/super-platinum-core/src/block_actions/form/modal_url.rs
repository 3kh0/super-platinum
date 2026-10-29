//! Quiet normal-Modal URL input. Exact B2 MjMC/OqK3/2Ztu preserve literals;
//! sxgo/uKWJ validate original, then an HTTP-prefixed candidate without rewriting.
//! Fresh prefill without saved/owned rows has commuting parent/leaf/default
//! writes of the same literal. Saved-row and remount semantics remain excluded.
use super::*;

pub(super) fn initial(element: &Value) -> Result<Option<String>, FormError> {
    match element.get("initial_value") {
        None => Ok(None),
        Some(Value::String(initial)) => Ok((!initial.is_empty()).then(|| initial.clone())),
        Some(_) => Err(FormError::InvalidView("Invalid initial URL".into())),
    }
}

fn valid(literal: &str) -> bool {
    url::Url::parse(literal).is_ok() || url::Url::parse(&format!("http://{literal}")).is_ok()
}

pub(super) fn error(field: &FormField) -> Option<String> {
    let (InputKind::Url, InputValue::Url(value)) = (&field.kind, &field.value) else {
        return Some("This value no longer matches the field.".into());
    };
    let Some(literal) = value.as_deref().filter(|value| !value.is_empty()) else {
        return (!field.optional).then(|| "A URL is required".into());
    };
    (!valid(literal)).then(|| "You must enter a valid URL".into())
}

#[cfg(test)]
mod prefill_tests;
#[cfg(test)]
mod tests;

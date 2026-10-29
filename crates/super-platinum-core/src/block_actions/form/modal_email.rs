//! Submit-only normal Modal email admission. Exact captured Modal5Fwt → INPUT
//! hvzX → EZY9/+Key/2Ztu uses the same rhb/ validator as Home. cxJV/iGIl
//! omit email initialization; absent/empty initial values need no mount edit.
//! Truthy initials require separately verified effect ordering and stay closed.
use super::*;

pub(super) fn supported(element: &Value) -> Result<bool, FormError> {
    match element.get("initial_value") {
        None => Ok(true),
        Some(Value::String(initial)) => Ok(initial.is_empty()),
        Some(_) => Err(FormError::InvalidView("Invalid initial email".into())),
    }
}

#[cfg(test)]
mod tests;

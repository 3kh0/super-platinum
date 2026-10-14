//! Overflow is stateless; selected URL routing stays separate from its action.
use super::*;
use std::collections::BTreeSet;

pub fn parse_message_overflow(element: &Value) -> Result<Vec<OptionSpec>, FormError> {
    let invalid = || FormError::InvalidView("Malformed overflow options".into());
    if element.get("type").and_then(Value::as_str) != Some("overflow")
        || element.get("option_groups").is_some()
    {
        return Err(invalid());
    }
    let options = element
        .get("options")
        .and_then(Value::as_array)
        .filter(|options| !options.is_empty() && options.len() <= 5)
        .ok_or_else(invalid)?;
    let mut seen = BTreeSet::new();
    options
        .iter()
        .map(|raw| {
            let text = raw
                .get("text")
                .filter(|v| v["type"] == "plain_text")
                .ok_or_else(invalid)?;
            let label = text
                .get("text")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && s.encode_utf16().count() <= 75)
                .ok_or_else(invalid)?;
            if text.get("emoji").is_some_and(|v| !v.is_boolean()) {
                return Err(invalid());
            }
            let value = raw
                .get("value")
                .and_then(Value::as_str)
                .filter(|s| s.encode_utf16().count() <= 150)
                .ok_or_else(invalid)?;
            if !seen.insert(value)
                || raw.get("url").is_some_and(|url| {
                    url.as_str().is_none_or(|url| {
                        url.is_empty()
                            || url.encode_utf16().count() > 3000
                            || url.chars().any(char::is_control)
                    })
                })
            {
                return Err(invalid());
            }
            if let Some(description) = raw.get("description")
                && (description["type"] != "plain_text"
                    || description
                        .get("text")
                        .and_then(Value::as_str)
                        .is_none_or(|s| s.encode_utf16().count() > 75))
            {
                return Err(invalid());
            }
            Ok(OptionSpec {
                text: label.to_owned(),
                value: value.to_owned(),
                raw: raw.clone(),
                group: None,
            })
        })
        .collect()
}

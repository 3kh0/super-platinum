//! IKXd/urb3 external options retain original composition objects, not static IDs.
use super::form::FormError;
use serde_json::Value;

pub const MAX_SUGGESTIONS_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_QUERY_UNITS: usize = 4096;

#[derive(Clone, Debug, PartialEq)]
pub struct ExternalOption {
    raw: Value,
}
impl ExternalOption {
    pub fn parse(raw: &Value) -> Result<Self, FormError> {
        raw.as_object().ok_or_else(invalid)?;
        // IKXd passes originalOption through state/actions unchanged. Unknown
        // top-level metadata is opaque data, not a link or rendering directive.
        // Display accessors below read only the validated composition fields.
        plain(&raw["text"], 75)?;
        let value = raw
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if value.encode_utf16().count() > 150 {
            return Err(invalid());
        }
        if let Some(description) = raw.get("description") {
            plain(description, 75)?;
        }
        Ok(Self { raw: raw.clone() })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn text(&self) -> &str {
        self.raw["text"]["text"].as_str().expect("validated text")
    }
    pub fn value(&self) -> &str {
        self.raw["value"].as_str().expect("validated value")
    }
    pub fn description(&self) -> Option<&str> {
        self.raw.get("description")?.get("text")?.as_str()
    }
}
pub(crate) fn plain(value: &Value, max: usize) -> Result<&str, FormError> {
    let object = value.as_object().ok_or_else(invalid)?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "type" | "text" | "emoji"))
        || value["type"] != "plain_text"
        || value.get("emoji").is_some_and(|v| !v.is_boolean())
    {
        return Err(invalid());
    }
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    if text.is_empty() || text.encode_utf16().count() > max {
        return Err(invalid());
    }
    Ok(text)
}
fn invalid() -> FormError {
    FormError::InvalidView("Unsupported or malformed external option".into())
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternalOptionGroup {
    pub label: String,
    pub options: Vec<ExternalOption>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ExternalSuggestions {
    Options(Vec<ExternalOption>),
    Groups(Vec<ExternalOptionGroup>),
}
impl ExternalSuggestions {
    /// `urb3` prefers truthy groups, including an empty array. Never paginate.
    pub fn parse(value: &Value) -> Result<Self, FormError> {
        if !value.is_object() {
            return Err(invalid());
        }
        let mut seen = std::collections::BTreeMap::<String, Value>::new();
        let mut options = |value: &Value| -> Result<Vec<ExternalOption>, FormError> {
            let list = value
                .as_array()
                .filter(|a| a.len() <= 100)
                .ok_or_else(invalid)?;
            list.iter()
                .map(|raw| {
                    let option = ExternalOption::parse(raw)?;
                    if seen
                        .get(option.value())
                        .is_some_and(|old| old != option.raw())
                    {
                        return Err(invalid());
                    }
                    seen.insert(option.value().to_owned(), option.raw().clone());
                    Ok(option)
                })
                .collect()
        };
        if let Some(groups) = value.get("option_groups").filter(|v| !v.is_null()) {
            let groups = groups
                .as_array()
                .filter(|v| v.len() <= 100)
                .ok_or_else(invalid)?;
            return Ok(Self::Groups(
                groups
                    .iter()
                    .map(|group| {
                        let object = group.as_object().ok_or_else(invalid)?;
                        if object
                            .keys()
                            .any(|key| !matches!(key.as_str(), "label" | "options"))
                        {
                            return Err(invalid());
                        }
                        Ok(ExternalOptionGroup {
                            label: plain(&group["label"], 75)?.to_owned(),
                            options: options(&group["options"])?,
                        })
                    })
                    .collect::<Result<_, FormError>>()?,
            ));
        }
        Ok(Self::Options(
            match value.get("options").filter(|v| !v.is_null()) {
                Some(value) => options(value)?,
                None => vec![],
            },
        ))
    }
}

#[cfg(test)]
mod tests;

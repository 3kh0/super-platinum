//! Ordinary Link value, separate from URL navigation authorization.
//! Captured VTjO/uDYD: normalize the URL draft, emit only editable members.
use super::*;

mod detector;

/// Exact v+tG flags used by message preparation. Unsupported source switches
/// (go-links/all-TLDs) are intentionally not invented here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MessageLinkOptions {
    pub ignore_phone_matches: bool,
    pub ignore_email_matches: bool,
    pub ignore_links_without_http: bool,
    pub include_phone_matches_with_tel_prefix: bool,
    pub ignore_unbalanced_parentheses: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageLinkKind {
    Url,
    Email,
    Phone,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageLink {
    pub kind: MessageLinkKind,
    pub offset_utf16: usize,
    pub text: String,
    pub url: String,
}
/// Complete, untrimmed matches for retained rNCs/l8g0/k1tX message processing.
/// This is text conversion, not URL navigation authorization.
pub fn detect_message_links(
    input: &str,
    options: MessageLinkOptions,
) -> Result<Vec<MessageLink>, ListError> {
    detector::detect_message(input, options)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkValue {
    pub original_url: String,
    pub display_name: Option<String>,
    pub display_as_url: bool,
}

impl LinkValue {
    /// Stock Link modal Save: empty URL clears; otherwise first detected link
    /// wins, with original untrimmed text as fallback. Save resets URL display.
    /// This may perform regex work: call outside a UI render/event thread.
    pub fn from_input(input: &str, name: Option<&str>) -> Result<Option<Self>, ListError> {
        if input.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            original_url: detector::normalize(input)?,
            display_name: name.filter(|s| !s.is_empty()).map(str::to_owned),
            display_as_url: false,
        }))
    }
}

fn alias<'a>(
    object: &'a serde_json::Map<String, Value>,
    snake: &str,
    camel: &str,
) -> Result<Option<&'a Value>, ListError> {
    let a = object.get(snake);
    let b = object.get(camel);
    if a.zip(b).is_some_and(|(a, b)| a != b) {
        return Err(ListError(
            "This link has conflicting stored representations.",
        ));
    }
    Ok(a.or(b).filter(|value| !value.is_null()))
}

pub(super) fn read(value: Option<&Value>) -> Result<Option<LinkValue>, ListError> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let item = match value {
        Value::Object(_) => value,
        Value::Array(values) if values.is_empty() => return Ok(None),
        Value::Array(values) if values.len() == 1 => &values[0],
        _ => {
            return Err(ListError(
                "This link contains malformed or multiple stored values.",
            ));
        }
    };
    let object = item
        .as_object()
        .ok_or(ListError("This link field is malformed."))?;
    // Attachment is the one source-backed enrichment member. Do not silently
    // discard arbitrary future semantics while making an existing cell editable.
    if object.keys().any(|k| {
        !matches!(
            k.as_str(),
            "original_url"
                | "originalUrl"
                | "display_name"
                | "displayName"
                | "display_as_url"
                | "displayAsUrl"
                | "attachment"
        )
    }) {
        return Err(ListError("This link contains unsupported stored metadata."));
    }
    let url = alias(object, "original_url", "originalUrl")?
        .and_then(Value::as_str)
        .ok_or(ListError("The original link is unavailable."))?;
    let name = match alias(object, "display_name", "displayName")? {
        None => None,
        Some(Value::String(value)) => Some(value.as_str()).filter(|s| !s.is_empty()),
        _ => return Err(ListError("This link name is malformed.")),
    };
    let display = match alias(object, "display_as_url", "displayAsUrl")? {
        None => false,
        Some(Value::Bool(value)) => *value,
        _ => return Err(ListError("This link display mode is malformed.")),
    };
    if object
        .get("attachment")
        .is_some_and(|v| !v.is_null() && !v.is_object())
    {
        return Err(ListError("This link attachment is malformed."));
    }
    if url.is_empty() {
        return Ok(None);
    }
    Ok(Some(LinkValue {
        original_url: url.into(),
        display_name: name.map(str::to_owned),
        display_as_url: display,
    }))
}

pub(super) fn validate(value: Option<&LinkValue>) -> Result<(), ListError> {
    if value.is_some_and(|v| v.original_url.is_empty() || v.display_name.as_deref() == Some("")) {
        return Err(ListError("Use an empty link value to remove the link."));
    }
    Ok(())
}

pub(super) fn wire(value: Option<&LinkValue>) -> Value {
    match value {
        None => json!([]),
        Some(value) => {
            let mut item =
                json!({"original_url":value.original_url,"display_as_url":value.display_as_url});
            if let Some(name) = &value.display_name {
                item["display_name"] = json!(name);
            }
            json!([item])
        }
    }
}

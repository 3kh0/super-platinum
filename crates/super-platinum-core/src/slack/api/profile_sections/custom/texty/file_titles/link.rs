//! bQFi.linkToText under laT0's default options, bounded to string/nullish
//! fields. Original JavaScript coerces some malformed values; those are not
//! admitted here. This produces inert label text, never a navigation grant.
use super::Unavailable;
use serde_json::Value;

pub(super) fn text(leaf: &Value) -> Result<&str, Unavailable> {
    if leaf.get("type").and_then(Value::as_str) != Some("link") {
        return Err(Unavailable::UnsupportedTitle);
    }
    let text = field(leaf.get("text"))?;
    let url = field(leaf.get("url"))?;
    // Whitespace is truthy in the original. Trimming occurs at the block level;
    // it must not cause a link with a whitespace label to fall back to its URL.
    // expandTruncatedLinks is false for this caller, even if truncated is true.
    Ok(if text.is_empty() { url } else { text })
}

fn field(value: Option<&Value>) -> Result<&str, Unavailable> {
    match value {
        None | Some(Value::Null) => Ok(""),
        Some(Value::String(value)) => Ok(value),
        _ => Err(Unavailable::UnsupportedTitle),
    }
}

#[cfg(test)]
mod tests;

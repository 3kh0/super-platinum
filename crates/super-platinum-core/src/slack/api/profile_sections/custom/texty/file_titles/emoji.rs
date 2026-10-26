//! bQFi/aBXg canonical-name emoji and JSON tone fields, with explicit presentation.
use super::Unavailable;
use serde_json::Value;
mod canonical_names;
mod tone;

/// Deliberately separate from canvas/Untitled. This is an application display
/// choice, not evidence that a Slack account has no emoji translation map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmojiText {
    EnglishCanonicalNames,
}

pub(super) fn text(leaf: &Value, context: Option<EmojiText>) -> Result<String, Unavailable> {
    let Some(EmojiText::EnglishCanonicalNames) = context else {
        return Err(Unavailable::UnsupportedTitle);
    };
    let fields = leaf.as_object().ok_or(Unavailable::UnsupportedTitle)?;
    // Formatting and opaque original payload survive unchanged in the source.
    // Unicode/catalog/alias semantics remain outside this caller's authority.
    // Tone values are interpolated by aBXg, not validated as a 2..6 enum.
    if leaf["type"] != "emoji"
        || fields.keys().any(|key| {
            !matches!(
                key.as_str(),
                "type" | "name" | "style" | "opaque" | "skinTone" | "skin_tone"
            )
        })
    {
        return Err(Unavailable::UnsupportedTitle);
    }
    let name = leaf["name"].as_str().ok_or(Unavailable::UnsupportedTitle)?;
    if canonical_names::NAMES.binary_search(&name).is_err() {
        return Err(Unavailable::UnsupportedTitle);
    }
    let selected = leaf
        .get("skinTone")
        .filter(|value| tone::truthy(value))
        .or_else(|| leaf.get("skin_tone"));
    if let Some(value) = selected.filter(|value| tone::truthy(value)) {
        Ok(format!(":{name}::skin-tone-{}:", tone::string(value)?))
    } else {
        Ok(format!(":{name}:"))
    }
}

#[cfg(test)]
mod tests;

//! bQFi/aBXg ordinary name-only emoji, under explicit caller presentation.
use super::Unavailable;
use serde_json::Value;
mod canonical_names;

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
    // Any additional semantic field is outside the proven name-only branch,
    // including null/false unicode, skinTone/skin_tone, custom or alias fields.
    if leaf["type"] != "emoji"
        || fields
            .keys()
            .any(|key| !matches!(key.as_str(), "type" | "name" | "style" | "opaque"))
    {
        return Err(Unavailable::UnsupportedTitle);
    }
    let name = leaf["name"].as_str().ok_or(Unavailable::UnsupportedTitle)?;
    if canonical_names::NAMES.binary_search(&name).is_err() {
        return Err(Unavailable::UnsupportedTitle);
    }
    Ok(format!(":{name}:"))
}

#[cfg(test)]
mod tests;

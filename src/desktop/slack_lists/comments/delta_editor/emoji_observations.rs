//! Finite original-owned display facts. No picker or negative lookup defaults.
use super::EmojiAppearance;
use serde::Serialize;
use std::sync::Arc;

// serde's rc feature is not enabled by this workspace. Borrow the original
// payload for the one native serialization; never clone it to serialize it.
pub(super) fn serialize_live<S: serde::Serializer>(
    value: &Option<Arc<LiveEmoji>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value.as_deref().serialize(serializer)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Appearance {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct EmojiRow {
    pub name: String,
    pub single: bool,
    pub multiple: bool,
    pub appearance: Appearance,
    pub variants: Vec<(String, Appearance)>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiveEmoji {
    /// Complete acquired membership, including unresolved aliases. A missing
    /// row for a known name is unavailable appearance, not missing membership.
    pub names: Vec<String>,
    pub unicode: Vec<String>,
    pub unicode_names: Vec<(String, String)>,
    pub locale: String,
    pub current_locale: String,
    pub rows: Vec<EmojiRow>,
}

impl LiveEmoji {
    /// Original generated/restored embed identity is never rewritten to the
    /// canonical alias or selected variant. Native live conversion separately
    /// emits this same canonical shortcode grammar before reaching the codec.
    pub(crate) fn appearance(&self, text: &str) -> Option<EmojiAppearance> {
        let body = text.strip_prefix(':')?.strip_suffix(':')?.to_lowercase();
        let (name, tone) = match body.split_once("::skin-tone-") {
            Some((name, tone)) => {
                let pieces: Vec<_> = tone.split('-').collect();
                if !(1..=2).contains(&pieces.len())
                    || !pieces
                        .iter()
                        .all(|v| matches!(*v, "2" | "3" | "4" | "5" | "6"))
                {
                    return None;
                }
                (name, Some(tone))
            }
            None => (body.as_str(), None),
        };
        if name.is_empty() || name.contains(':') {
            return None;
        }
        let row = self.rows.iter().find(|r| r.name == name)?;
        let appearance = tone
            .and_then(|tone| row.variants.iter().find(|(key, _)| key == tone))
            .map_or(&row.appearance, |(_, value)| value);
        Some(EmojiAppearance {
            name: text.into(),
            text: appearance.text.clone(),
            media: appearance.media.clone(),
        })
    }
}

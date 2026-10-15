//! Shared explicit emoji insertion identity for the picker and shortcode UI.
//! Workspace names override standard names and aliases. Callers retain their
//! account, destination, mount, text/selection and catalog-revision guards.
use crate::{
    slack::models::Emoji,
    state::{EmojiSource, custom_emoji_source},
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Representation {
    Rendered,
    /// Code/link context, or the user's explicit text display preference.
    Shortcode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    pub glyph: Option<String>,
    pub image_url: Option<String>,
    pub custom: bool,
}

impl Choice {
    pub fn resolve(custom: &HashMap<String, Emoji>, name: &str) -> Option<Self> {
        if name.is_empty() || name.chars().any(|c| c.is_control() || ":<>".contains(c)) {
            return None;
        }
        if custom.contains_key(name) {
            let (glyph, image_url) = match custom_emoji_source(custom, name)? {
                EmojiSource::Unicode(glyph) => (Some(glyph.to_owned()), None),
                EmojiSource::Url(url) => (None, Some(url.to_owned())),
            };
            return Some(Self {
                name: name.to_owned(),
                glyph,
                image_url,
                custom: true,
            });
        }
        let emoji = emojis::get_by_shortcode(name)?;
        Some(Self {
            name: name.to_owned(),
            glyph: Some(emoji.as_str().to_owned()),
            image_url: None,
            custom: false,
        })
    }

    pub fn insertion(&self, representation: Representation) -> String {
        if self.custom || representation == Representation::Shortcode {
            format!(":{}:", self.name)
        } else {
            self.glyph
                .clone()
                .unwrap_or_else(|| format!(":{}:", self.name))
        }
    }

    /// Re-resolve immediately before insertion; an alias/URL replacement cannot
    /// silently retarget an already displayed choice.
    pub fn current(&self, custom: &HashMap<String, Emoji>) -> bool {
        Self::resolve(custom, &self.name).as_ref() == Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(value: serde_json::Value) -> HashMap<String, Emoji> {
        value
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    serde_json::from_value(serde_json::json!({
                        "name": name, "value": value,
                    }))
                    .unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn standard_alias_keeps_chosen_shortcode_and_inserts_glyph() {
        let choice = Choice::resolve(&HashMap::new(), "thumbsup").unwrap();
        assert_eq!(choice.insertion(Representation::Rendered), "👍");
        assert_eq!(choice.insertion(Representation::Shortcode), ":thumbsup:");
    }

    #[test]
    fn custom_alias_to_unicode_still_keeps_workspace_identity() {
        let custom = custom(serde_json::json!({"party":"alias:thumbsup"}));
        let choice = Choice::resolve(&custom, "party").unwrap();
        assert_eq!(choice.glyph.as_deref(), Some("👍"));
        assert_eq!(choice.insertion(Representation::Rendered), ":party:");
    }

    #[test]
    fn custom_name_shadows_standard_alias_and_replacement_retires_choice() {
        let before = custom(serde_json::json!({"thumbsup":"https://example.invalid/one.png"}));
        let after = custom(serde_json::json!({"thumbsup":"https://example.invalid/two.png"}));
        let choice = Choice::resolve(&before, "thumbsup").unwrap();
        assert_eq!(choice.insertion(Representation::Rendered), ":thumbsup:");
        assert!(choice.current(&before));
        assert!(!choice.current(&after));
        assert!(!choice.current(&HashMap::new()));
    }

    #[test]
    fn invalid_custom_shadow_does_not_fall_back_to_standard() {
        let custom = custom(serde_json::json!({"thumbsup":"alias:missing"}));
        assert!(Choice::resolve(&custom, "thumbsup").is_none());
        assert!(Choice::resolve(&custom, "missing").is_none());
    }
}

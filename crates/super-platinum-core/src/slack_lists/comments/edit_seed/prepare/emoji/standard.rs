//! UHpe.i8 and KdF+/UHpe.qJ over an explicitly selected retained standard
//! catalog. No custom-name absence, current workspace, or locale authority.
use super::*;
use crate::composer_emoji::standard::{Catalog, Version};
use std::collections::{BTreeMap, BTreeSet};

pub struct StandardEmoji {
    catalog: Catalog,
    names: BTreeSet<String>,
    reverse: BTreeMap<String, String>,
}
impl StandardEmoji {
    /// Build outside the renderer thread. Version is supplied by the caller's
    /// original complete assignment observation, not by a global picker cache.
    pub fn build(version: Version) -> Result<Self> {
        let catalog = Catalog::build(version).map_err(|_| SeedUnavailable::InvalidObservation)?;
        let names = catalog.entries.iter().map(|e| e.name.clone()).collect();
        let mut reverse = BTreeMap::new();
        for entry in &catalog.entries {
            if entry.alias_of.as_ref().is_some_and(|a| !a.is_empty())
                && reverse.contains_key(&entry.unicode)
            {
                continue;
            }
            reverse.insert(entry.unicode.clone(), entry.name.clone());
            for variation in entry.skin_variations.values() {
                reverse.insert(variation.unicode.clone(), variation.name.clone());
            }
        }
        Ok(Self {
            catalog,
            names,
            reverse,
        })
    }
    /// Positive standard membership is valid even without a custom catalog.
    pub fn positive_names(&self) -> &BTreeSet<String> {
        &self.names
    }

    /// Existing ue25 parser followed by exact standard membership and literal
    /// skin variation lookup. Missing/unsupported tone falls back to the base,
    /// as the source does; no preferred insertion tone is consulted.
    pub fn encoding(&self, text: &str) -> Result<Option<super::super::EmojiEncoding>> {
        let Some(parsed) = observations::parse(text)? else {
            return Ok(None);
        };
        let Some(entry) = self.catalog.get(&parsed.name) else {
            return Ok(None);
        };
        let variation = parsed
            .tone
            .as_ref()
            .and_then(|tone| entry.skin_variations.get(tone));
        let (name, unicode) = variation
            .map(|v| (&v.name, &v.unicode))
            .unwrap_or((&entry.name, &entry.unicode));
        Ok(Some(super::super::EmojiEncoding {
            name: Some(name.clone()),
            unicode: Some(unicode.clone()),
        }))
    }
    /// Absence is known only within this complete selected standard reverse map.
    pub fn unicode_name(&self, unicode: &str) -> Option<&str> {
        self.reverse.get(unicode).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_standard_encoding_preserves_alias_and_literal_tones() {
        for version in [Version::V16, Version::V17] {
            let catalog = StandardEmoji::build(version).unwrap();
            for (text, name, unicode) in [
                (":smile:", "smile", "1f604"),
                (
                    ":thumbsup::skin-tone-4:",
                    "thumbsup::skin-tone-4",
                    "1f44d-1f3fd",
                ),
                (":smile::skin-tone-4:", "smile", "1f604"),
                (
                    ":handshake::skin-tone-2-3:",
                    "handshake::skin-tone-2-3",
                    "1faf1-1f3fb-200d-1faf2-1f3fc",
                ),
            ] {
                let actual = catalog.encoding(text).unwrap().unwrap();
                assert_eq!(actual.name.as_deref(), Some(name));
                assert_eq!(actual.unicode.as_deref(), Some(unicode));
            }
            assert_eq!(catalog.unicode_name("1f44d-1f3fd"), Some("+1::skin-tone-4"));
            assert_eq!(catalog.unicode_name("unobserved"), None);
            assert_eq!(catalog.encoding(":custom_unacquired:").unwrap(), None);
            assert!(catalog.encoding("constructor").is_err());
        }
    }

    #[test]
    fn selected_version_controls_positive_names_and_reverse_observations() {
        let old = StandardEmoji::build(Version::V16).unwrap();
        let new = StandardEmoji::build(Version::V17).unwrap();
        assert!(!old.positive_names().contains("orca"));
        assert!(new.positive_names().contains("orca"));
        assert_eq!(old.encoding(":orca:").unwrap(), None);
        assert_eq!(
            new.encoding(":orca:").unwrap().unwrap().unicode.as_deref(),
            Some("1facd")
        );
        assert_eq!(old.unicode_name("1facd"), None);
        assert_eq!(new.unicode_name("1facd"), Some("orca"));
    }
}

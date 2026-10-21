//! Retained Slack standard catalog and exact Unicode/tone metadata. Construct
//! off the UI thread; callers own account/catalog generations and localization.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    V16,
    V17,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Entry {
    pub name: String,
    pub unicode: String,
    #[serde(default, rename = "aliasOf")]
    pub alias_of: Option<String>,
    /// UHpe's reaction-name fallback. Both retained full version payloads have
    /// no such entries; keep the optional source branch distinct from that fact.
    #[serde(default)]
    pub obsoletes: Option<String>,
    #[serde(default, rename = "skinVariations")]
    pub skin_variations: BTreeMap<String, Variant>,
    #[serde(default, rename = "multiSkinTone")]
    pub multi_skin_tone: bool,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Variant {
    pub name: String,
    pub unicode: String,
}

pub struct Catalog {
    /// Source order is significant when equal-scoring aliases are compared.
    pub entries: Vec<Entry>,
    by_name: BTreeMap<String, usize>,
    keywords: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Insertion {
    /// Preserve the actual matched canonical alias, not just its group name.
    pub name: String,
    pub shortcode: String,
    pub glyph: String,
    /// Selecting a tone does not replace the source item's base identity.
    pub catalog_id: String,
}

impl Catalog {
    pub fn build(version: Version) -> Result<Self, String> {
        let data = match version {
            Version::V16 => include_str!("slack-emoji-v16-ordered.json"),
            Version::V17 => include_str!("slack-emoji-v17-ordered.json"),
        };
        let entries: Vec<Entry> = serde_json::from_str(data).map_err(|e| e.to_string())?;
        let by_name = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.clone(), i))
            .collect();
        let keywords = serde_json::from_str(include_str!("slack-emoji-keywords-en-US.json"))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            entries,
            by_name,
            keywords,
        })
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.by_name.get(name).map(|i| &self.entries[*i])
    }

    pub fn aliases<'a>(&'a self, canonical: &'a str) -> impl Iterator<Item = &'a Entry> {
        self.entries
            .iter()
            .filter(move |e| e.alias_of.as_deref() == Some(canonical))
    }

    pub fn keywords(&self, name: &str) -> &[String] {
        self.keywords
            .get(name)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn insertion(&self, matched_name: &str, preferred_tone: Option<&str>) -> Option<Insertion> {
        let entry = self.get(matched_name)?;
        let tone = preferred_tone.filter(|s| matches!(*s, "2" | "3" | "4" | "5" | "6"));
        let variant = tone.and_then(|tone| entry.skin_variations.get(tone));
        let (name, unicode) = variant
            .map(|v| (v.name.as_str(), v.unicode.as_str()))
            .unwrap_or((&entry.name, &entry.unicode));
        Some(Insertion {
            name: name.to_owned(),
            shortcode: format!(":{name}:"),
            glyph: unicode_glyph(unicode)?,
            catalog_id: format!("E{}", entry.unicode),
        })
    }
}

fn unicode_glyph(unicode: &str) -> Option<String> {
    if unicode.is_empty() {
        return None;
    }
    unicode
        .split('-')
        .map(|part| char::from_u32(u32::from_str_radix(part, 16).ok()?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_retained_versions_parse_and_every_unicode_variant_is_valid() {
        for (version, count) in [(Version::V16, 1972), (Version::V17, 1980)] {
            let catalog = Catalog::build(version).unwrap();
            assert_eq!(catalog.entries.len(), count);
            assert_eq!(catalog.by_name.len(), count);
            for entry in &catalog.entries {
                assert!(
                    !unicode_glyph(&entry.unicode).unwrap().is_empty(),
                    "{}",
                    entry.name
                );
                if let Some(alias) = &entry.alias_of {
                    assert!(catalog.get(alias).is_some(), "{}", entry.name);
                }
                for variant in entry.skin_variations.values() {
                    assert!(!unicode_glyph(&variant.unicode).unwrap().is_empty());
                }
            }
        }
    }

    #[test]
    fn matched_alias_and_preferred_single_tone_preserve_exact_slack_shortcode() {
        let catalog = Catalog::build(Version::V16).unwrap();
        let result = catalog.insertion("thumbsup", Some("4")).unwrap();
        assert_eq!(result.shortcode, ":thumbsup::skin-tone-4:");
        assert_eq!(result.glyph, "👍🏽");
        assert_eq!(result.catalog_id, "E1f44d");
        assert_eq!(
            catalog.insertion("smile", Some("4")).unwrap().shortcode,
            ":smile:"
        );
        assert_eq!(
            catalog.insertion("thumbsup", Some("04")).unwrap().glyph,
            "👍"
        );
    }

    #[test]
    fn alias_groups_and_english_keywords_are_source_data_not_gemoji_guesses() {
        let catalog = Catalog::build(Version::V16).unwrap();
        assert_eq!(
            catalog.get("poop").unwrap().alias_of.as_deref(),
            Some("hankey")
        );
        assert!(catalog.aliases("hankey").any(|e| e.name == "poop"));
        assert!(catalog.keywords("grinning").iter().any(|k| k == "happy"));
        assert!(catalog.get("handshake").unwrap().multi_skin_tone);
        assert!(
            catalog
                .get("handshake")
                .unwrap()
                .skin_variations
                .contains_key("2-3")
        );
    }

    #[test]
    fn primary_canonical_insertion_oracle() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("source-oracle-next.json")).unwrap();
        let catalog = Catalog::build(Version::V16).unwrap();
        for row in oracle["canonical"].as_array().unwrap() {
            let tone = row["tone"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| row["tone"].as_u64().map(|n| n.to_string()));
            let result = catalog
                .insertion(row["name"].as_str().unwrap(), tone.as_deref())
                .unwrap();
            assert_eq!(result.shortcode, row["result"].as_str().unwrap(), "{row}");
        }
    }
}

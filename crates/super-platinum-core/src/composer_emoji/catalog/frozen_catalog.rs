//! Immutable catalog shared by completion and explicit picker insertion.
//! UHpe builds ordered alias groups from standard entries, then custom records.
//! Build on a worker; the caller publishes a fresh Arc under an owner/revision.
use super::{insertion::Representation, search, standard};
use crate::slack::models::Emoji;
use std::collections::{HashMap, HashSet};

pub struct Catalog {
    pub standard: standard::Catalog,
    pub search_entries: Vec<search::Entry>,
    /// Includes each source name once. Presentation may filter/sort this list;
    /// it must resolve selection against this same immutable catalog.
    pub names: Vec<String>,
    custom: HashMap<String, Emoji>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub name: String,
    pub shortcode: String,
    pub glyph: Option<String>,
    pub image_url: Option<String>,
    pub custom: bool,
    /// az0u resolves a standard name before constructing E+name for custom.
    pub usage_id: String,
}

impl Resolved {
    pub fn insertion(&self, representation: Representation) -> String {
        if self.custom || representation == Representation::Shortcode {
            self.shortcode.clone()
        } else {
            self.glyph.clone().unwrap_or_else(|| self.shortcode.clone())
        }
    }
}

impl Catalog {
    /// `custom` must have source insertion order when available. The current
    /// emoji.list/HashMap adapter loses that order; callers must state their
    /// deterministic fallback rather than claim source tie-order parity.
    pub fn build(version: standard::Version, custom: Vec<Emoji>) -> Result<Self, String> {
        let standard = standard::Catalog::build(version)?;
        let mut groups: Vec<search::Entry> = Vec::new();
        let mut positions = HashMap::<String, usize>::new();
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for entry in &standard.entries {
            if seen.insert(entry.name.clone()) {
                names.push(entry.name.clone());
            }
            if let Some(target) = &entry.alias_of {
                let group = group(&mut groups, &mut positions, target);
                group.aliases.push(search::Name {
                    canonical: entry.name.clone(),
                    display: None,
                });
            } else {
                let group = group(&mut groups, &mut positions, &entry.name);
                group.id = format!("E{}", entry.unicode);
                group.keywords = standard.keywords(&entry.name).to_vec();
            }
        }
        let mut custom_map = HashMap::new();
        for entry in custom {
            if !valid_name(&entry.name) {
                continue;
            }
            if seen.insert(entry.name.clone()) {
                names.push(entry.name.clone());
            }
            if let Some(target) = entry.value.strip_prefix("alias:") {
                let group = group(&mut groups, &mut positions, target);
                group.aliases.push(search::Name {
                    canonical: entry.name.clone(),
                    display: None,
                });
            } else {
                let group = group(&mut groups, &mut positions, &entry.name);
                group.id = format!("E{}", entry.name);
                // UHpe uses the standard-name membership test even when the
                // custom record overrides that standard record's image.
                group.keywords = standard.keywords(&entry.name).to_vec();
            }
            custom_map.insert(entry.name.clone(), entry);
        }
        // A dangling alias-only group has no source identity. Never offer an
        // unresolvable selection; valid groups retain exact source order.
        groups.retain(|entry| !entry.id.is_empty());
        Ok(Self {
            standard,
            search_entries: groups,
            names,
            custom: custom_map,
        })
    }

    pub fn resolve(&self, name: &str, preferred_tone: Option<&str>) -> Option<Resolved> {
        if !valid_name(name) {
            return None;
        }
        let usage_id = self
            .standard
            .get(name)
            .map(|e| format!("E{}", e.unicode))
            .unwrap_or_else(|| format!("E{name}"));
        if self.custom.contains_key(name) {
            let mut current = name;
            let mut visited = HashSet::new();
            loop {
                if !visited.insert(current) {
                    return None;
                }
                if let Some(entry) = self.custom.get(current) {
                    if let Some(target) = entry.value.strip_prefix("alias:") {
                        current = target;
                        continue;
                    }
                    let url = url::Url::parse(&entry.value).ok()?;
                    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                        return None;
                    }
                    return Some(Resolved {
                        name: name.into(),
                        shortcode: format!(":{name}:"),
                        glyph: None,
                        image_url: Some(entry.value.clone()),
                        custom: true,
                        usage_id,
                    });
                }
                // A custom alias retains its own shortcode identity and does
                // not acquire the user's preferred tone automatically.
                let value = self.standard.insertion(current, None)?;
                return Some(Resolved {
                    name: name.into(),
                    shortcode: format!(":{name}:"),
                    glyph: Some(value.glyph),
                    image_url: None,
                    custom: true,
                    usage_id,
                });
            }
        }
        let value = self.standard.insertion(name, preferred_tone)?;
        Some(Resolved {
            name: value.name,
            shortcode: value.shortcode,
            glyph: Some(value.glyph),
            image_url: None,
            custom: false,
            usage_id,
        })
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.chars().any(|c| c.is_control() || ":<>".contains(c))
}

fn group<'a>(
    groups: &'a mut Vec<search::Entry>,
    positions: &mut HashMap<String, usize>,
    name: &str,
) -> &'a mut search::Entry {
    let index = *positions.entry(name.to_owned()).or_insert_with(|| {
        let index = groups.len();
        groups.push(search::Entry {
            id: String::new(),
            is_emoji: false,
            primary: search::Name {
                canonical: name.to_owned(),
                display: None,
            },
            aliases: Vec::new(),
            keywords: Vec::new(),
        });
        index
    });
    &mut groups[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_consolidation_callback_oracle_preserves_overlay_and_alias_order() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("catalog-oracle.json")).unwrap();
        for case in oracle["cases"].as_array().unwrap() {
            let custom = case["custom"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| emoji(pair[0].as_str().unwrap(), pair[1].as_str().unwrap()))
                .collect();
            let catalog = Catalog::build(standard::Version::V16, custom).unwrap();
            for expected in case["rows"].as_array().unwrap() {
                let name = expected["name"].as_str().unwrap();
                let actual = catalog
                    .search_entries
                    .iter()
                    .find(|e| e.primary.canonical == name);
                if expected["id"].is_null() {
                    assert!(
                        actual.is_none(),
                        "dangling alias group must not become selectable"
                    );
                    continue;
                }
                let actual = actual.unwrap();
                assert_eq!(actual.id, expected["id"].as_str().unwrap(), "{name}");
                assert_eq!(
                    serde_json::to_value(
                        actual
                            .aliases
                            .iter()
                            .map(|a| &a.canonical)
                            .collect::<Vec<_>>()
                    )
                    .unwrap(),
                    expected["aliases"],
                    "{name}"
                );
                assert_eq!(
                    serde_json::to_value(&actual.keywords).unwrap(),
                    expected["keywords"],
                    "{name}"
                );
            }
        }
    }
    fn emoji(name: &str, value: &str) -> Emoji {
        Emoji {
            name: name.into(),
            value: value.into(),
            ..Default::default()
        }
    }
    #[test]
    fn shared_resolver_preserves_custom_alias_identity_and_standard_tone() {
        let catalog = Catalog::build(
            standard::Version::V16,
            vec![
                emoji("party", "alias:thumbsup"),
                emoji("smile", "https://example.invalid/custom.png"),
                emoji("loop", "alias:loop"),
            ],
        )
        .unwrap();
        let standard = catalog.resolve("thumbsup", Some("4")).unwrap();
        assert_eq!(standard.glyph.as_deref(), Some("\u{1f44d}\u{1f3fd}"));
        assert_eq!(standard.shortcode, ":thumbsup::skin-tone-4:");
        let alias = catalog.resolve("party", Some("4")).unwrap();
        assert_eq!(alias.insertion(Representation::Rendered), ":party:");
        assert_eq!(alias.glyph.as_deref(), Some("\u{1f44d}"));
        let overridden = catalog.resolve("smile", None).unwrap();
        assert_eq!(
            overridden.image_url.as_deref(),
            Some("https://example.invalid/custom.png")
        );
        assert_eq!(overridden.usage_id, "E1f604");
        assert!(catalog.resolve("loop", None).is_none());
    }
    #[test]
    fn alias_before_target_and_override_retain_order_and_existing_aliases() {
        let catalog = Catalog::build(
            standard::Version::V16,
            vec![
                emoji("second", "alias:custom"),
                emoji("custom", "https://example.invalid/custom.png"),
                emoji("first", "alias:custom"),
                emoji("hankey", "https://example.invalid/hankey.png"),
            ],
        )
        .unwrap();
        let custom = catalog
            .search_entries
            .iter()
            .find(|e| e.primary.canonical == "custom")
            .unwrap();
        assert_eq!(custom.id, "Ecustom");
        assert_eq!(
            custom
                .aliases
                .iter()
                .map(|a| a.canonical.as_str())
                .collect::<Vec<_>>(),
            ["second", "first"]
        );
        let hankey = catalog
            .search_entries
            .iter()
            .find(|e| e.primary.canonical == "hankey")
            .unwrap();
        assert_eq!(hankey.id, "Ehankey");
        assert!(hankey.aliases.iter().any(|a| a.canonical == "poop"));
        assert!(!hankey.is_emoji);
    }
}

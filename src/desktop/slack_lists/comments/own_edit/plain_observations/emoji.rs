//! aBXg original-element text, with positively observed default-English locale.
//! Caller retains the catalog/locale/own-edit owner; this function grants none.
use serde_json::Value;
use std::collections::BTreeMap;
use super_platinum_core::{composer_emoji::standard::Catalog, slack::Error};

fn unavailable() -> Error {
    Error::Api("list_plain_emoji_observation_unavailable".into())
}

/// BUnC explicitly installs only keywords for GQhO.Xn (en-US). Therefore
/// HSTN has no name map for this locale; this is not missing-data localization.
/// Catalog version is supplied, never inferred here: native Loaded currently
/// contains V16, whereas source tCC+ selects V17 only for emoji_v17 == "on".
pub(super) struct EnglishCatalog {
    names: BTreeMap<String, String>,
}
impl EnglishCatalog {
    /// Build once per original observation capture, off the renderer path. The
    /// caller retains Loaded separately; this owned map needs no self-borrow.
    pub(super) fn capture(catalog: &Catalog, locale: &str) -> Result<Self, Error> {
        if locale != "en-US" {
            return Err(unavailable());
        }
        Ok(Self {
            names: reverse(catalog)
                .into_iter()
                .map(|(unicode, name)| (unicode.to_owned(), name.to_owned()))
                .collect(),
        })
    }

    pub(super) fn text(&self, element: &Value) -> Result<String, Error> {
        if element.get("type").and_then(Value::as_str) != Some("emoji") {
            return Err(unavailable());
        }
        let name = element
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(unavailable)?;
        let camel = scalar(element.get("skinTone"))?;
        let tone = if camel.is_some() {
            camel
        } else {
            scalar(element.get("skin_tone"))?
        };
        let unicode = scalar(element.get("unicode"))?;
        match (tone, unicode) {
            (Some(tone), None) => Ok(format!(":{name}::skin-tone-{tone}:")),
            (None, None) => Ok(format!(":{name}:")),
            (_, Some(unicode)) if valid_unicode(&unicode) => {
                // UHpe returns undefined for a valid but unlisted Unicode string;
                // aBXg's template preserves that result rather than guessing name.
                Ok(format!(
                    ":{}:",
                    self.names
                        .get(unicode.as_str())
                        .map(String::as_str)
                        .unwrap_or("undefined")
                ))
            }
            (_, Some(_)) => Ok(String::new()),
        }
    }
}

#[cfg(test)]
fn text(element: &Value, catalog: &Catalog, locale: &str) -> Result<String, Error> {
    EnglishCatalog::capture(catalog, locale)?.text(element)
}

/// Original JS truthiness and interpolation for supported JSON scalars.
/// Object/array coercion is outside the ordinary emoji element contract.
fn scalar(value: Option<&Value>) -> Result<Option<String>, Error> {
    Ok(match value {
        None | Some(Value::Null | Value::Bool(false)) => None,
        Some(Value::Bool(true)) => Some("true".into()),
        Some(Value::String(value)) => (!value.is_empty()).then(|| value.clone()),
        Some(Value::Number(value)) => {
            if value.as_f64() == Some(0.0) {
                None
            } else if let Some(value) = value.as_i64() {
                if value.unsigned_abs() > 9_007_199_254_740_991 {
                    return Err(unavailable());
                }
                Some(value.to_string())
            } else if let Some(value) = value.as_u64() {
                if value > 9_007_199_254_740_991 {
                    return Err(unavailable());
                }
                Some(value.to_string())
            } else {
                // Do not substitute Rust floating formatting for JS ToString.
                return Err(unavailable());
            }
        }
        _ => return Err(unavailable()),
    })
}

fn valid_unicode(value: &str) -> bool {
    value.split('-').all(|part| {
        matches!(part.len(), 4 | 5)
            && part
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn reverse(catalog: &Catalog) -> BTreeMap<&str, &str> {
    let mut names = BTreeMap::new();
    for entry in &catalog.entries {
        // KdF+: a later non-alias overwrites; an alias with an already mapped
        // base Unicode skips its entire body, including all skin variations.
        if entry
            .alias_of
            .as_ref()
            .is_some_and(|alias| !alias.is_empty())
            && names.contains_key(entry.unicode.as_str())
        {
            continue;
        }
        names.insert(entry.unicode.as_str(), entry.name.as_str());
        // Retained V16/V17 have no conflicting Unicode names within one entry's
        // variants, so the typed BTreeMap order cannot change the last write.
        for variant in entry.skin_variations.values() {
            names.insert(variant.unicode.as_str(), variant.name.as_str());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use super_platinum_core::composer_emoji::standard::Version;

    #[test]
    fn plain_emoji_original_elements_match_english_source_vectors() {
        let catalog = Catalog::build(Version::V16).unwrap();
        for (raw, expected) in [
            (
                json!({"type":"emoji","name":"custom_party"}),
                ":custom_party:",
            ),
            (
                json!({"type":"emoji","name":"thumbsup","skin_tone":4}),
                ":thumbsup::skin-tone-4:",
            ),
            (
                json!({"type":"emoji","name":"thumbsup","skinTone":3,"skin_tone":4}),
                ":thumbsup::skin-tone-3:",
            ),
            (
                json!({"type":"emoji","name":"thumbsup","skinTone":0,"skin_tone":4}),
                ":thumbsup::skin-tone-4:",
            ),
            (
                json!({"type":"emoji","name":"ignored","unicode":"1f600"}),
                ":grinning:",
            ),
            (
                json!({"type":"emoji","name":"ignored","unicode":"1f44d-1f3fd"}),
                ":+1::skin-tone-4:",
            ),
            (
                json!({"type":"emoji","name":"ignored","unicode":"1F600"}),
                "",
            ),
            (
                json!({"type":"emoji","name":"ignored","unicode":"fffff"}),
                ":undefined:",
            ),
        ] {
            assert_eq!(text(&raw, &catalog, "en-US").unwrap(), expected);
        }
    }

    #[test]
    fn plain_emoji_reverse_alias_and_variation_order_are_source_ordered() {
        let mut catalog = Catalog::build(Version::V16).unwrap();
        catalog.entries = serde_json::from_value(json!([
            {"name":"early_alias","unicode":"10000","aliasOf":"later","skinVariations":{"2":{"name":"first-tone","unicode":"10001"}}},
            {"name":"later","unicode":"10000","skinVariations":{"2":{"name":"later-tone","unicode":"10001"}}},
            {"name":"skipped","unicode":"10000","aliasOf":"later","skinVariations":{"3":{"name":"must-not-install","unicode":"10002"}}}
        ])).unwrap();
        let names = reverse(&catalog);
        assert_eq!(names.get("10000"), Some(&"later"));
        assert_eq!(names.get("10001"), Some(&"later-tone"));
        assert!(!names.contains_key("10002"));
    }

    #[test]
    fn plain_emoji_missing_locale_or_unsupported_element_never_defaults() {
        let catalog = Catalog::build(Version::V16).unwrap();
        let raw = json!({"type":"emoji","name":"smile"});
        for locale in ["", "en-GB", "fr-FR"] {
            assert!(text(&raw, &catalog, locale).is_err());
        }
        for raw in [
            json!({"type":"emoji"}),
            json!({"type":"emoji","name":"smile","unicode":[]}),
            json!({"type":"emoji","name":"smile","skin_tone":2.5}),
        ] {
            assert!(text(&raw, &catalog, "en-US").is_err());
        }
    }

    #[test]
    fn plain_emoji_typed_variant_order_does_not_lose_retained_conflicts() {
        for version in [Version::V16, Version::V17] {
            let catalog = Catalog::build(version).unwrap();
            for entry in catalog.entries {
                let mut names = BTreeMap::new();
                for variant in entry.skin_variations.values() {
                    if let Some(prior) = names.insert(&variant.unicode, &variant.name) {
                        assert_eq!(prior, &variant.name);
                    }
                }
            }
        }
    }
}

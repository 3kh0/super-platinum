//! 2YmW::filterEmoji's unindexed deterministic path. Catalog construction and
//! publication belong to the immutable account/locale/catalog owner.
use super::{keyword, normalize, ranking::Candidate, score_details, standard};

#[derive(Clone, Debug)]
pub struct Name {
    pub canonical: String,
    pub display: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub is_emoji: bool,
    pub primary: Name,
    /// Primary name is scanned first, then these in captured source order.
    pub aliases: Vec<Name>,
    pub keywords: Vec<String>,
}

/// Builds only the retained standard data. The consolidated workspace catalog
/// must overlay custom identities and preserve the same grouped alias order.
pub fn standard_entries(catalog: &standard::Catalog) -> Vec<Entry> {
    catalog
        .entries
        .iter()
        .filter(|e| e.alias_of.is_none())
        .map(|e| Entry {
            id: format!("E{}", e.unicode),
            is_emoji: false,
            primary: Name {
                canonical: e.name.clone(),
                display: None,
            },
            aliases: catalog
                .aliases(&e.name)
                .map(|a| Name {
                    canonical: a.name.clone(),
                    display: None,
                })
                .collect(),
            keywords: catalog.keywords(&e.name).to_vec(),
        })
        .collect()
}

pub fn candidates(
    query: &str,
    entries: &[Entry],
    keyword_options: keyword::Options,
) -> Vec<Candidate> {
    let query = normalize::normalize(query);
    let mut result = Vec::new();
    for entry in entries {
        let mut best: Option<score_details::NameScore> = None;
        let mut matched_name = entry.primary.canonical.clone();
        let mut canonical = entry.primary.canonical.clone();
        let mut exact = false;
        for name in std::iter::once(&entry.primary).chain(&entry.aliases) {
            // 8KJ5 gives each canonical name its localized display alternative.
            for (text, localized) in std::iter::once((name.canonical.as_str(), false)).chain(
                name.display
                    .as_deref()
                    .filter(|s| *s != name.canonical)
                    .map(|s| (s, true)),
            ) {
                if text.is_empty() {
                    continue;
                }
                let normalized = normalize::normalize(text);
                let score = score_details::score_name(&query, &normalized);
                if best.as_ref().and_then(|b| b.score).is_none()
                    || score.score.is_some_and(|s| {
                        s < best.as_ref().and_then(|b| b.score).unwrap_or(f64::INFINITY)
                    })
                {
                    matched_name = text.to_owned();
                    canonical = name.canonical.clone();
                    exact = query == normalized
                        || !localized && name.canonical == "ok_hand" && query == "ok";
                    best = Some(score);
                }
            }
        }
        let Some(best) = best else {
            continue;
        };
        if best.score.is_none() {
            if !entry.keywords.iter().any(|word| {
                keyword::score(&query, &normalize::normalize(word), keyword_options).is_some()
            }) {
                continue;
            }
            // Source resets the display name only. Last failed alias and its
            // partial combo-break metadata remain authoritative for insertion/rank.
            matched_name = entry
                .primary
                .display
                .as_ref()
                .unwrap_or(&entry.primary.canonical)
                .clone();
        }
        result.push(Candidate {
            id: entry.id.clone(),
            is_emoji: entry.is_emoji,
            name: entry.primary.canonical.clone(),
            matched_name,
            canonical,
            name_score: best.score,
            exact,
            query_ratio: best.query_length_ratio,
            combo_breaks: best.combo_breaks,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::ranking::{SimilarityCache, rank_without_personal_history};
    use super::*;
    use serde_json::Value;

    // serde_json without float_roundtrip can parse a decimal one ULP away.
    // Compare source-produced IEEE754 bits, rather than relaxing the contract.
    fn source_number(value: &Value) -> Option<f64> {
        value
            .as_str()
            .map(|s| f64::from_bits(u64::from_str_radix(s, 16).unwrap()))
    }

    #[test]
    fn complete_source_catalog_matching_metadata_and_final_order() {
        let catalog = standard::Catalog::build(standard::Version::V16).unwrap();
        let entries = standard_entries(&catalog);
        let oracle: Value =
            serde_json::from_str(include_str!("source-oracle-ranking.json")).unwrap();
        for case in oracle["cases"].as_array().unwrap() {
            let query = case["query"].as_str().unwrap();
            let actual = candidates(query, &entries, keyword::Options::default());
            let expected = case["matched"].as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "{query}");
            for (a, e) in actual.iter().zip(expected) {
                assert_eq!(a.id, e["id"].as_str().unwrap(), "{query}");
                assert_eq!(
                    a.matched_name,
                    e["emojiBestNameMatch"].as_str().unwrap(),
                    "{query} {}",
                    a.name
                );
                assert_eq!(
                    a.canonical,
                    e["emojiBestNameMatchCanonical"].as_str().unwrap(),
                    "{query} {}",
                    a.name
                );
                assert_eq!(
                    a.combo_breaks,
                    e["emojiTotalComboBreaks"].as_u64().map(|v| v as usize),
                    "{query} {}",
                    a.name
                );
                assert_eq!(
                    a.exact,
                    e["isExactMatch"].as_bool().unwrap_or(false),
                    "{query} {}",
                    a.name
                );
                assert_eq!(
                    a.name_score,
                    source_number(&e["ieee754"]["score"]),
                    "{query} {}",
                    a.name
                );
                assert_eq!(
                    a.query_ratio,
                    source_number(&e["ieee754"]["ratio"]).unwrap(),
                    "{query} {}",
                    a.name
                );
            }
            let ranked = rank_without_personal_history(
                query,
                actual,
                25,
                false,
                &mut SimilarityCache::default(),
                crate::state::sidebar_name_cmp,
            );
            let expected = case["results"].as_array().unwrap();
            assert_eq!(ranked.len(), expected.len(), "{query}");
            for (a, e) in ranked.iter().zip(expected) {
                assert_eq!(a.candidate.id, e["id"].as_str().unwrap(), "{query}");
                assert!(
                    (a.model_score - e["score"].as_f64().unwrap()).abs() < 1e-12,
                    "{query} {}",
                    a.candidate.name
                );
            }
        }
    }

    #[test]
    fn keyword_match_keeps_last_failed_alias_insertion_identity() {
        let catalog = standard::Catalog::build(standard::Version::V16).unwrap();
        let entries = standard_entries(&catalog);
        let results = candidates("sm", &entries, keyword::Options::default());
        let poop = results.iter().find(|r| r.name == "hankey").unwrap();
        assert_eq!(poop.matched_name, "hankey");
        assert_eq!(poop.canonical, "shit");
        assert_eq!(poop.name_score, None);
        assert_eq!(poop.combo_breaks, Some(1));
    }
}

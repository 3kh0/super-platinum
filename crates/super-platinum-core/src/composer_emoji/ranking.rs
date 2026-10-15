//! Captured local autocompleteFallback ranking with unavailable personal metadata.
//! This is the complete local zero-history ranking path, not a substitute for
//! available frecency/remote features or a remotely supplied model.
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Candidate {
    pub id: String,
    /// Primary catalog name: fuzzy bonus and initial collation use this name.
    pub name: String,
    /// Matched name may be an alias or localized name; prefix feature uses it.
    pub matched_name: String,
    pub canonical: String,
    /// Conditional source bonus flag; raw standard catalog entries omit it.
    pub is_emoji: bool,
    pub name_score: Option<f64>,
    pub exact: bool,
    pub query_ratio: f64,
    /// Failure metadata can retain combo breaks even for keyword-only matches.
    pub combo_breaks: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Ranked {
    pub candidate: Candidate,
    pub model_score: f64,
    pub bonus: f64,
}

type Frequencies = Vec<(String, f64)>;

/// Source 4ucY's shared 1,000-entry ayzK LRU is observable: keys omit the
/// strip-separator mode. Keep this state across queries for source parity.
/// A catalog/account owner must own this cache; never share it across owners.
#[derive(Default)]
pub struct SimilarityCache {
    entries: BTreeMap<String, (u64, Frequencies)>,
    age: BTreeMap<u64, String>,
    clock: u64,
}

impl SimilarityCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.age.clear();
        self.clock = 0;
    }

    fn frequencies(&mut self, text: &str, strip: bool) -> Frequencies {
        // Saturation is unreachable in practice; clear rather than alias ages.
        if self.clock == u64::MAX {
            self.clear();
        }
        self.clock += 1;
        let result = if let Some((age, value)) = self.entries.remove(text) {
            self.age.remove(&age);
            value
        } else {
            if self.entries.len() >= 1_000 {
                if let Some((_, key)) = self.age.pop_first() {
                    self.entries.remove(&key);
                }
            }
            term_frequencies(text, strip)
        };
        self.age.insert(self.clock, text.to_owned());
        self.entries
            .insert(text.to_owned(), (self.clock, result.clone()));
        result
    }

    pub fn similarity(&mut self, query: &str, name: &str) -> f64 {
        if query.is_empty() || name.is_empty() {
            return 0.0;
        }
        let query_tf = self.frequencies(query, false);
        let name_tf = self.frequencies(name, !query.contains(['-', '_']));
        let mut keys: Vec<&str> = query_tf.iter().map(|(key, _)| key.as_str()).collect();
        for (key, _) in &name_tf {
            if !keys.contains(&key.as_str()) {
                keys.push(key);
            }
        }
        let mut dot = 0.0;
        let mut query_norm = 0.0;
        let mut name_norm = 0.0;
        for key in keys {
            let a = query_tf.iter().find(|(k, _)| k == key).map_or(0.0, |v| v.1);
            let b = name_tf.iter().find(|(k, _)| k == key).map_or(0.0, |v| v.1);
            dot += a * b;
            query_norm += a * a;
            name_norm += b * b;
        }
        if query_norm == 0.0 || name_norm == 0.0 {
            0.0
        } else {
            dot / (query_norm.sqrt() * name_norm.sqrt())
        }
    }
}

fn term_frequencies(text: &str, strip: bool) -> Frequencies {
    let text: String = text.chars().filter(|c| !js_space(*c)).collect();
    let chars: Vec<_> = text
        .to_lowercase()
        .chars()
        .filter(|c| !strip || !matches!(c, '-' | '_'))
        .collect();
    let mut result: Frequencies = Vec::new();
    for n in 2..=3 {
        if chars.len() < n {
            continue;
        }
        for start in 0..=chars.len() - n {
            let key: String = chars[start..start + n].iter().collect();
            let weight = 1.0 / (start + 1) as f64;
            if let Some((_, value)) = result.iter_mut().find(|(k, _)| *k == key) {
                *value += weight;
            } else {
                result.push((key, weight));
            }
        }
    }
    result
}

fn js_space(c: char) -> bool {
    matches!(c as u32, 9..=13 | 32 | 160 | 5760 | 8192..=8202 |
        8232 | 8233 | 8239 | 8287 | 12288 | 65279)
}

/// `compare_names` must use the captured locale's numeric collation.
/// Zero-history means personal frecency, offline CTR and remote model absent.
/// Source experiment emoji_exact_match_boost is supplied explicitly.
pub fn rank_without_personal_history(
    query: &str,
    mut candidates: Vec<Candidate>,
    limit: usize,
    exact_boost: bool,
    cache: &mut SimilarityCache,
    compare_names: impl Fn(&str, &str) -> Ordering,
) -> Vec<Ranked> {
    let shortlist = limit.saturating_mul(3).min(500);
    let model_only = candidates.len() < shortlist;
    candidates.sort_by(|a, b| match (a.name_score, b.name_score) {
        // JS Infinity-Infinity is NaN; comparator returns NaN, treated as equal.
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a_score), Some(b_score)) => a_score
            .total_cmp(&b_score)
            .then_with(|| compare_names(&a.name, &b.name)),
    });
    let (mut exact, rest): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|c| c.exact);
    exact.extend(rest);
    // Ejs0 applies Ojx4.formatQuery before normalization, including '+' removal.
    let model_query = match query.chars().next() {
        Some(
            c @ (':' | '@' | '\u{ff20}' | '#' | '\u{ff03}' | '/' | '\u{ff0f}' | '+' | '\u{ff0b}'),
        ) => &query[c.len_utf8()..],
        _ => query,
    };
    let normalized_query = super::normalize::normalize(model_query);
    let mut scored: Vec<_> = exact
        .into_iter()
        .map(|candidate| {
            let mut fuzzy_bonus = None;
            let mut bonus = if candidate.exact {
                100.0 + if exact_boost { 300.0 } else { 100.0 }
            } else {
                let emoji = candidate
                    .name_score
                    .is_some_and(|s| s != 0.0 && s.is_finite());
                let value =
                    cache.similarity(query, &candidate.name) * if emoji { 150.0 } else { 50.0 };
                if !emoji {
                    fuzzy_bonus = Some(value);
                }
                value
            };
            if candidate.is_emoji && matches!(candidate.name.as_str(), "thumbsup" | "point_up") {
                bonus += 1.0;
            }
            // Sum in yL3i.globalFeatures then emojiFeatures order, as ukdy does.
            let mut score = bonus * 0.0083562726103486;
            if fuzzy_bonus == Some(50.0) {
                score += 0.95744512772969;
            }
            if fuzzy_bonus.is_some_and(|v| v != 0.0 && v < 25.0) {
                score -= 2.1179514334905;
            }
            let matched = super::normalize::normalize(&candidate.matched_name);
            let matched = matched.strip_prefix(['@', '#']).unwrap_or(&matched);
            if !matched.is_empty()
                && !normalized_query.is_empty()
                && matched.starts_with(&normalized_query)
            {
                score += 0.4852904832645;
            }
            if candidate.exact {
                score += 0.183283071987;
            }
            score += candidate.query_ratio * 1.0690358111219;
            score += match candidate.combo_breaks {
                Some(0) => 0.034181103626295,
                Some(1) => -1.5404314842404,
                Some(2) => -2.2796470967609,
                _ => 0.0,
            };
            Ranked {
                candidate,
                model_score: score,
                bonus,
            }
        })
        .collect();
    if !model_only {
        scored.sort_by(|a, b| b.bonus.total_cmp(&a.bonus));
    }
    scored.truncate(shortlist);
    scored.sort_by(|a, b| b.model_score.total_cmp(&a.model_score));
    // Ejs0 boosts exact matches only below the first two ranked results.
    let tail = scored.split_off(scored.len().min(2));
    let (mut exact, rest): (Vec<_>, Vec<_>) = tail.into_iter().partition(|r| r.candidate.exact);
    exact.extend(rest);
    scored.extend(exact);
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn check(case: &Value, cache: &mut SimilarityCache) {
        let input = case["matched"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| Candidate {
                id: v["id"].as_str().unwrap().to_owned(),
                name: v["name"].as_str().unwrap().to_owned(),
                matched_name: v["emojiBestNameMatch"].as_str().unwrap().to_owned(),
                canonical: v["emojiBestNameMatchCanonical"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                is_emoji: v["is_emoji"].as_bool().unwrap_or(false),
                name_score: v["score"].as_f64(),
                exact: v["isExactMatch"].as_bool().unwrap_or(false),
                query_ratio: v["emojiQueryLengthRatio"].as_f64().unwrap_or(0.0),
                combo_breaks: v["emojiTotalComboBreaks"].as_u64().map(|v| v as usize),
            })
            .collect();
        let actual = rank_without_personal_history(
            case["query"].as_str().unwrap(),
            input,
            25,
            false,
            cache,
            crate::state::sidebar_name_cmp,
        );
        let expected = case["results"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(expected) {
            assert_eq!(
                a.candidate.id,
                e["id"].as_str().unwrap(),
                "query {}",
                case["query"]
            );
            assert!(
                (a.model_score - e["score"].as_f64().unwrap()).abs() < 1e-12,
                "query {} item {} actual {} expected {}",
                case["query"],
                a.candidate.name,
                a.model_score,
                e["score"]
            );
        }
    }

    #[test]
    fn captured_local_ranker_with_missing_history() {
        let oracle: Value =
            serde_json::from_str(include_str!("source-oracle-ranking.json")).unwrap();
        for case in oracle["cases"].as_array().unwrap() {
            check(case, &mut SimilarityCache::default());
        }
    }

    #[test]
    fn captured_warm_cache_separator_behavior() {
        let oracle: Value =
            serde_json::from_str(include_str!("source-oracle-ranking.json")).unwrap();
        let mut cache = SimilarityCache::default();
        for case in oracle["warmSequence"].as_array().unwrap() {
            check(case, &mut cache);
        }
        assert_ne!(
            oracle["warmSequence"][1]["results"][0]["score"],
            oracle["coldComparison"]["results"][0]["score"]
        );
    }
}

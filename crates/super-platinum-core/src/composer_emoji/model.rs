//! Full global+emoji feature vector from XCJ8/yL3i and ukdy's linear model.
//! Remote weights replace the fallback map; READ_/WRITE_ weights are additive.
use super::history::format_query;
use super::history_ranking::Scored;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct OfflineFeatures {
    pub user_ctr_30dd: Option<f64>,
    pub team_ctr_90dd: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct Metadata {
    pub offline: BTreeMap<String, OfflineFeatures>,
    pub entity_type_ctr: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Model {
    weights: BTreeMap<String, f64>,
}

impl Model {
    pub fn fallback() -> Self {
        // This small retained data file is exactly LBiy.autocompleteFallback.
        let bits: BTreeMap<String, String> =
            serde_json::from_str(include_str!("fallback-model.json")).expect("retained model bits");
        Self {
            weights: bits
                .into_iter()
                .map(|(key, bits)| {
                    (
                        key,
                        f64::from_bits(u64::from_str_radix(&bits, 16).expect("retained IEEE754")),
                    )
                })
                .collect(),
        }
    }

    /// Preserve all remote feature names, including READ_/WRITE_ overrides.
    /// Reject malformed snapshots rather than silently falling back to no history.
    pub fn from_weights(weights: BTreeMap<String, f64>) -> Result<Self, String> {
        if weights.values().any(|w| !w.is_finite()) {
            return Err("non-finite model weight".into());
        }
        Ok(Self { weights })
    }

    pub fn score(&self, features: &[(&str, f64)], source: &str) -> f64 {
        let prefix = if source == "composer_page__destination" || source.starts_with("share") {
            "WRITE"
        } else {
            "READ"
        };
        let mut score = 0.0;
        for (name, value) in features {
            let weight = self.weights.get(*name).copied().unwrap_or(0.0)
                + self
                    .weights
                    .get(&format!("{prefix}_{name}"))
                    .copied()
                    .unwrap_or(0.0);
            score += weight * finite(*value);
        }
        score
    }

    /// Source-specific per-feature intercept, applied before multiplication.
    pub fn score_with_adjustment(
        &self,
        features: &[(&str, f64)],
        source: &str,
        adjustment: impl Fn(&str) -> f64,
    ) -> f64 {
        let prefix = if source == "composer_page__destination" || source.starts_with("share") {
            "WRITE"
        } else {
            "READ"
        };
        let mut score = 0.0;
        for (name, value) in features {
            let weight = self.weights.get(*name).copied().unwrap_or(0.0)
                + self
                    .weights
                    .get(&format!("{prefix}_{name}"))
                    .copied()
                    .unwrap_or(0.0)
                + adjustment(name);
            score += weight * finite(*value);
        }
        score
    }
}

const FEATURE_NAMES: &[&str] = &[
    "ID",
    "FRECENCY",
    "RAW_FRECENCY",
    "FRECENCY_BONUS",
    "FRECENCY_LOG_HIT_COUNT",
    "FRECENCY_LOG_HIT_COUNT_SCALED",
    "FRECENCY_HIT_COUNT",
    "FRECENCY_WEIGHT_HIT_VISITS",
    "FRECENCY_WEIGHT_ITEM_VISITS",
    "FRECENCY_ITEM_COUNT",
    "FRECENCY_LOG_ITEM_COUNT",
    "FRECENCY_LOG_ITEM_COUNT_SCALED",
    "FRECENCY_HIT_VISITS_4H",
    "FRECENCY_HIT_VISITS_1D",
    "FRECENCY_HIT_VISITS_3D",
    "FRECENCY_HIT_VISITS_1W",
    "FRECENCY_HIT_VISITS_1M",
    "FRECENCY_HIT_VISITS_90D",
    "FRECENCY_ITEM_VISITS_4H",
    "FRECENCY_ITEM_VISITS_1D",
    "FRECENCY_ITEM_VISITS_3D",
    "FRECENCY_ITEM_VISITS_1W",
    "FRECENCY_ITEM_VISITS_1M",
    "FRECENCY_ITEM_VISITS_90D",
    "FUZZY_SCORE",
    "FUZZY_SCORE_50",
    "FUZZY_SCORE_25",
    "FUZZY_SCORE_ALL",
    "FUZZY_SCORE_SCALED_EXP",
    "FUZZY_SCORE_SCALED_FLIPPED",
    "FUZZY_SCORE_SCALED_SUBTRACT",
    "NAV_EXACT_MATCH",
    "NAV_EXACT_PREFIX_MATCH",
    "NAV_NORMAL_MATCH",
    "NAV_NORMAL_PREFIX_MATCH",
    "NAV_NORMAL_MATCH_NO_USER",
    "NAV_NORMAL_PREFIX_MATCH_NO_USER",
    "NAV_CONTAINS_QUERY_NORMALIZED",
    "IS_FLANNEL_FUZZY_PASSTHROUGH",
    "NAV_QUERY_LENGTH",
    "NAV_TEXT_LENGTH",
    "NAV_QUERY_RATIO_LENGTH_STRING",
    "EXACT_MATCH_LOG_RESULTS",
    "USER_ENTITY_TYPE_CTR_30DD",
    "NUM_RESULTS",
    "EMOJI_EXACT_MATCH",
    "EMOJI_QUERY_LENGTH_RATIO",
    "EMOJI_TOTAL_COMBO_BREAKS",
    "EMOJI_COMBO_BREAK_0",
    "EMOJI_COMBO_BREAK_1",
    "EMOJI_COMBO_BREAK_2",
    "EMOJI_COMBO_BREAK_3",
    "TYPE_EMOJI",
    "FRECENCY_HIT_VISIT_DECAY_2H",
    "FRECENCY_HIT_VISIT_DECAY_7D",
    "FRECENCY_ITEM_VISIT_DECAY_2H",
    "FRECENCY_ITEM_VISIT_DECAY_7D",
    "EMOJI_TEAM_CTR_90DD",
    "EMOJI_USER_CTR_30DD",
];

/// Numeric model values, in the source's feature-list order. The source encodes
/// null, undefined, strings and non-finite feature values as zero for scoring.
pub fn features(
    query: &str,
    row: &Scored,
    num_results: usize,
    now_ms: f64,
    metadata: &Metadata,
) -> Vec<(&'static str, f64)> {
    let c = &row.candidate;
    let h = &row.stats;
    let text = &c.matched_name;
    let normalized_query = format_query(query);
    let normalized = super::normalize::normalize(text);
    let normalized = normalized.strip_prefix(['@', '#']).unwrap_or(&normalized);
    let prefix = !normalized.is_empty()
        && !normalized_query.is_empty()
        && normalized.starts_with(&normalized_query);
    let raw_prefix = !query.is_empty() && !text.is_empty() && text.starts_with(query);
    let name_score = c.name_score.unwrap_or(f64::INFINITY);
    let integer_score = name_score.is_finite() && name_score.fract() == 0.0;
    let offline = metadata.offline.get(&c.id);
    FEATURE_NAMES
        .iter()
        .enumerate()
        .map(|(i, &name)| {
            let value = match name {
                // FRECENCY deliberately excludes suggestionType emoji, while RAW
                // and count/visit features are still available to its model.
                "ID" | "FRECENCY" | "IS_FLANNEL_FUZZY_PASSTHROUGH" => 0.0,
                "RAW_FRECENCY" => h.raw,
                "FRECENCY_BONUS" => row.bonus,
                "FRECENCY_LOG_HIT_COUNT" => h.hit_count.ln_1p(),
                "FRECENCY_LOG_HIT_COUNT_SCALED" => h.log_hit_scaled,
                "FRECENCY_HIT_COUNT" => h.hit_count,
                "FRECENCY_WEIGHT_HIT_VISITS" => h.hit_weight,
                "FRECENCY_WEIGHT_ITEM_VISITS" => h.item_weight,
                "FRECENCY_ITEM_COUNT" => h.item_count,
                "FRECENCY_LOG_ITEM_COUNT" => h.item_count.ln_1p(),
                "FRECENCY_LOG_ITEM_COUNT_SCALED" => h.log_item_scaled,
                _ if (12..18).contains(&i) => h.hit_buckets[i - 12],
                _ if (18..24).contains(&i) => h.item_buckets[i - 18],
                "FUZZY_SCORE" => name_score,
                "FUZZY_SCORE_50" => yes(row.fuzzy_bonus == Some(50.0)),
                "FUZZY_SCORE_25" => yes(row.fuzzy_bonus == Some(25.0)),
                "FUZZY_SCORE_ALL" => yes(row.fuzzy_bonus.is_some_and(|v| v != 0.0 && v < 25.0)),
                "FUZZY_SCORE_SCALED_EXP" => {
                    if integer_score {
                        0.5_f64.powf(name_score.min(10.0))
                    } else {
                        0.0
                    }
                }
                "FUZZY_SCORE_SCALED_FLIPPED" => {
                    if integer_score {
                        10.0 - name_score.min(10.0)
                    } else {
                        0.0
                    }
                }
                "FUZZY_SCORE_SCALED_SUBTRACT" => {
                    if integer_score {
                        name_score.min(10.0) - 1.0
                    } else {
                        0.0
                    }
                }
                "NAV_EXACT_MATCH" => yes(!text.is_empty() && text == query),
                "NAV_EXACT_PREFIX_MATCH" => yes(raw_prefix),
                "NAV_NORMAL_MATCH" | "NAV_NORMAL_MATCH_NO_USER" => {
                    yes(!text.is_empty() && normalized == normalized_query)
                }
                "NAV_NORMAL_PREFIX_MATCH" | "NAV_NORMAL_PREFIX_MATCH_NO_USER" => yes(prefix),
                "NAV_CONTAINS_QUERY_NORMALIZED" => 0.0, // source only channels/users
                "NAV_QUERY_LENGTH" => {
                    if text.is_empty() {
                        0.0
                    } else {
                        normalized_query.encode_utf16().count() as f64
                    }
                }
                "NAV_TEXT_LENGTH" => text.encode_utf16().count() as f64,
                "NAV_QUERY_RATIO_LENGTH_STRING" => {
                    if text.is_empty() {
                        0.0
                    } else {
                        query.encode_utf16().count() as f64 / text.encode_utf16().count() as f64
                    }
                }
                "EXACT_MATCH_LOG_RESULTS" => yes(c.exact) * (num_results as f64).ln_1p(),
                "USER_ENTITY_TYPE_CTR_30DD" => metadata.entity_type_ctr.unwrap_or(0.0),
                "NUM_RESULTS" => num_results as f64,
                "EMOJI_EXACT_MATCH" => yes(c.exact),
                "EMOJI_QUERY_LENGTH_RATIO" => c.query_ratio,
                "EMOJI_TOTAL_COMBO_BREAKS" => c.combo_breaks.unwrap_or(0) as f64,
                "EMOJI_COMBO_BREAK_0" => yes(c.combo_breaks == Some(0)),
                "EMOJI_COMBO_BREAK_1" => yes(c.combo_breaks == Some(1)),
                "EMOJI_COMBO_BREAK_2" => yes(c.combo_breaks == Some(2)),
                "EMOJI_COMBO_BREAK_3" => yes(c.combo_breaks == Some(3)),
                "TYPE_EMOJI" => 1.0,
                "FRECENCY_HIT_VISIT_DECAY_2H" => decay(now_ms, h.latest_hit, 7200.0),
                "FRECENCY_HIT_VISIT_DECAY_7D" => decay(now_ms, h.latest_hit, 604800.0),
                "FRECENCY_ITEM_VISIT_DECAY_2H" => decay(now_ms, h.latest_item, 7200.0),
                "FRECENCY_ITEM_VISIT_DECAY_7D" => decay(now_ms, h.latest_item, 604800.0),
                "EMOJI_TEAM_CTR_90DD" => offline.and_then(|v| v.team_ctr_90dd).unwrap_or(0.0),
                "EMOJI_USER_CTR_30DD" => offline.and_then(|v| v.user_ctr_30dd).unwrap_or(0.0),
                _ => unreachable!("retained emoji feature {name}"),
            };
            (name, finite(value))
        })
        .collect()
}

fn yes(value: bool) -> f64 {
    if value { 1.0 } else { 0.0 }
}
fn finite(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}
fn decay(now_ms: f64, timestamp_seconds: f64, half_life: f64) -> f64 {
    if timestamp_seconds == 0.0 {
        return 0.0;
    }
    ((0.5_f64.ln() / half_life) * (now_ms / 1000.0 - timestamp_seconds).max(0.0))
        .exp()
        .min(1.0)
}

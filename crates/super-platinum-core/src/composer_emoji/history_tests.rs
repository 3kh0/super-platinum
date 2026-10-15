//! Tests are staged separately; declare under cfg(test) beside the modules.
use super::{history::History, history_ranking, keyword, model, ranking, search, standard};
use serde_json::Value;

fn oracle() -> Value {
    serde_json::from_str(include_str!("source-oracle-history.json")).unwrap()
}

fn history(wire: &str) -> History {
    let decoded: Value = serde_json::from_str(wire).unwrap();
    if decoded.is_string() {
        History::from_pref(&decoded).unwrap()
    } else {
        History::from_wire(wire).unwrap()
    }
}

fn close(actual: f64, expected: &Value, label: &str) {
    if expected.is_null() {
        assert!(
            !actual.is_finite(),
            "{label}: expected non-finite, got {actual}"
        );
    } else {
        let expected = expected.as_f64().unwrap();
        assert!(
            (actual - expected).abs() <= 1e-11 * expected.abs().max(1.0),
            "{label}: actual {actual}, expected {expected}"
        );
    }
}

#[test]
fn actual_history_shortlist_features_and_model_oracle() {
    let catalog = standard::Catalog::build(standard::Version::V16).unwrap();
    let entries = search::standard_entries(&catalog);
    for case in oracle()["cases"].as_array().unwrap() {
        if case.get("portError").is_some() {
            continue;
        }
        let query = case["query"].as_str().unwrap();
        let label = case["label"].as_str().unwrap();
        let history = history(case["historyWire"].as_str().unwrap());
        let weights = serde_json::from_value(case["weights"].clone()).unwrap();
        let model = model::Model::from_weights(weights).unwrap();
        let metadata = model::Metadata {
            offline: serde_json::from_value(case["offline"].clone()).unwrap(),
            entity_type_ctr: case["entityCtr"].as_f64(),
        };
        let context = history_ranking::Context {
            history: &history,
            now_ms: case["now"].as_f64().unwrap(),
            model: &model,
            metadata: &metadata,
            source: case["source"].as_str().unwrap(),
            exact_boost: case["exactBoost"].as_bool().unwrap(),
        };
        let limit = case["limit"].as_u64().unwrap() as usize;
        let candidates = search::candidates(query, &entries, keyword::Options::default());
        let prepared = history_ranking::prepare(
            query,
            candidates.clone(),
            limit,
            &context,
            &mut ranking::SimilarityCache::default(),
            crate::state::sidebar_name_cmp,
        )
        .unwrap();
        assert_eq!(
            prepared.len(),
            case["before"].as_array().unwrap().len(),
            "{label}"
        );
        for (a, e) in prepared.iter().zip(case["before"].as_array().unwrap()) {
            assert_eq!(
                a.candidate.id,
                e["id"].as_str().unwrap(),
                "{label} initial order"
            );
            for (actual, key) in [
                (a.stats.raw, "raw"),
                (a.stats.raw + a.bonus, "frecency"),
                (a.stats.hit_count, "hitCount"),
                (a.stats.item_count, "itemCount"),
                (a.stats.hit_weight, "weightHitVisits"),
                (a.stats.item_weight, "weightItemVisits"),
                (a.stats.log_hit_scaled, "logHitCountScaled"),
                (a.stats.log_item_scaled, "logItemCountScaled"),
                (a.stats.latest_hit, "latestHitVisit"),
                (a.stats.latest_item, "latestItemVisit"),
            ] {
                close(
                    actual,
                    &e[key],
                    &format!("{label} {} {key}", a.candidate.id),
                );
            }
        }
        let shortlist = prepared.len().min(limit.saturating_mul(3).min(500));
        assert_eq!(shortlist, case["prepared"].as_array().unwrap().len());
        for (row, expected) in prepared
            .iter()
            .take(shortlist)
            .zip(case["prepared"].as_array().unwrap())
        {
            let vector = model::features(query, row, shortlist, context.now_ms, &metadata);
            assert_eq!(vector.len(), expected["features"].as_array().unwrap().len());
            for ((name, value), e) in vector.iter().zip(expected["features"].as_array().unwrap()) {
                assert_eq!(*name, e[0].as_str().unwrap());
                close(
                    *value,
                    &e[1],
                    &format!("{label} {} {name}", row.candidate.id),
                );
            }
        }
        let actual = history_ranking::rank(
            query,
            candidates,
            limit,
            &context,
            &mut ranking::SimilarityCache::default(),
            crate::state::sidebar_name_cmp,
        )
        .unwrap();
        let expected = case["results"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(expected) {
            assert_eq!(
                a.candidate.id,
                e["id"].as_str().unwrap(),
                "{label} final order"
            );
            close(
                a.model_score,
                &e["score"],
                &format!("{label} {} model", a.candidate.id),
            );
        }
    }
}

#[test]
fn actual_usage_reducer_oracle_preserves_keys_and_record_shapes() {
    for case in oracle()["usage"].as_array().unwrap() {
        let mut current = history(case["historyWire"].as_str().unwrap());
        for step in case["steps"].as_array().unwrap() {
            current = current
                .recorded(
                    step["event"]["id"].as_str().unwrap(),
                    step["event"]["text"].as_str().unwrap(),
                    step["now"].as_f64().unwrap(),
                )
                .unwrap();
            let actual_wire = current.to_wire().unwrap();
            let expected_wire = step["wire"].as_str().unwrap();
            let actual: Value = serde_json::from_str(&actual_wire).unwrap();
            let expected: Value = serde_json::from_str(expected_wire).unwrap();
            assert_eq!(actual, expected, "{}", case["label"]);
            // Object member order inside a record is irrelevant, but bucket
            // order affects equal-score matching and must survive serialization.
            let expected = History::from_wire(expected_wire).unwrap();
            for query in ["sm", "2", "", ":SM"] {
                let ids = ["E1f604", "E1f60f"];
                let a = current.score(query, &ids, step["now"].as_f64().unwrap());
                let b = expected.score(query, &ids, step["now"].as_f64().unwrap());
                for (a, b) in a.iter().zip(b) {
                    assert_eq!(a.raw.to_bits(), b.raw.to_bits());
                }
            }
        }
    }
}

#[test]
fn zero_history_path_keeps_all_prior_source_orders_and_warm_cache() {
    let old: Value = serde_json::from_str(include_str!("source-oracle-ranking.json")).unwrap();
    let catalog = standard::Catalog::build(standard::Version::V16).unwrap();
    let entries = search::standard_entries(&catalog);
    let history = History::default();
    let model = model::Model::fallback();
    let metadata = model::Metadata::default();
    let context = history_ranking::Context {
        history: &history,
        now_ms: 1760000000000.0,
        model: &model,
        metadata: &metadata,
        source: "texty-autocomplete",
        exact_boost: false,
    };
    let mut cache = ranking::SimilarityCache::default();
    for (cases, cold) in [(&old["cases"], true), (&old["warmSequence"], false)] {
        cache.clear();
        for case in cases.as_array().unwrap() {
            if cold {
                cache.clear();
            }
            let query = case["query"].as_str().unwrap();
            let candidates = search::candidates(query, &entries, keyword::Options::default());
            let actual = history_ranking::rank(
                query,
                candidates,
                25,
                &context,
                &mut cache,
                crate::state::sidebar_name_cmp,
            )
            .unwrap();
            assert_eq!(actual.len(), case["results"].as_array().unwrap().len());
            for (a, e) in actual.iter().zip(case["results"].as_array().unwrap()) {
                assert_eq!(a.candidate.id, e["id"].as_str().unwrap(), "{query}");
                close(a.model_score, &e["score"], query);
            }
        }
    }
}

#[test]
fn ordered_ties_and_malformed_or_unordered_preferences_are_explicit() {
    let o = oracle();
    let cases = o["cases"].as_array().unwrap();
    let first = cases
        .iter()
        .find(|x| x["label"] == "tie-prefix-first")
        .unwrap();
    let second = cases
        .iter()
        .find(|x| x["label"] == "tie-exact-first")
        .unwrap();
    let a = history(first["historyWire"].as_str().unwrap()).score(
        "sm",
        &["E1f604", "E1f60f"],
        1760000000000.0,
    );
    let b = history(second["historyWire"].as_str().unwrap()).score(
        "sm",
        &["E1f604", "E1f60f"],
        1760000000000.0,
    );
    assert_ne!(
        a[0].raw, b[0].raw,
        "equal-score query buckets retain first insertion order"
    );
    assert!(
        History::from_pref(&serde_json::json!({"sm":{"id":"E1","count":1,"visits":[1]}})).is_err()
    );
    assert!(History::from_wire("{\"sm\":{\"id\":\"E1\",\"count\":-1}}").is_err());
    assert!(History::from_wire("null").is_err());
    assert!(
        model::Model::from_weights(std::collections::BTreeMap::from([(
            "RAW_FRECENCY".to_owned(),
            f64::NAN
        )]))
        .is_err()
    );
}

#[test]
fn actual_source_non_total_inputs_are_explicitly_refused_before_sort() {
    use history_ranking::RankError;
    let catalog = standard::Catalog::build(standard::Version::V16).unwrap();
    let entries = search::standard_entries(&catalog);
    let o = oracle();
    let mut refused = 0;
    for case in o["cases"].as_array().unwrap() {
        let Some(error) = case["portError"].as_str() else {
            continue;
        };
        refused += 1;
        let wire = case["historyWire"].as_str().unwrap();
        if error == "malformed-history" {
            // Raw wire rejects the flag and unordered Value admission also refuses;
            // there is no public Deserialize route around these boundaries.
            assert!(History::from_wire(wire).is_err());
            assert!(History::from_pref(&serde_json::from_str::<Value>(wire).unwrap()).is_err());
            let row = case["before"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == "E1f604")
                .unwrap();
            assert_eq!(row["weightHitVisits"].as_f64().unwrap(), 50.0);
            continue;
        }
        let history = History::from_wire(wire).unwrap();
        let weights = serde_json::from_value(case["weights"].clone()).unwrap();
        let model = model::Model::from_weights(weights).unwrap();
        let metadata = model::Metadata::default();
        let context = history_ranking::Context {
            history: &history,
            now_ms: case["now"].as_f64().unwrap(),
            model: &model,
            metadata: &metadata,
            source: case["source"].as_str().unwrap(),
            exact_boost: false,
        };
        let query = case["query"].as_str().unwrap();
        let candidates = search::candidates(query, &entries, keyword::Options::default());
        let expected = if error == "nonfinite-shortlist" {
            assert!(
                candidates.len() > 75,
                "this must enter the shortlist sorter"
            );
            assert!(
                case["before"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["raw"].is_null())
            );
            assert_eq!(
                history_ranking::prepare(
                    query,
                    candidates.clone(),
                    25,
                    &context,
                    &mut ranking::SimilarityCache::default(),
                    crate::state::sidebar_name_cmp
                )
                .unwrap_err(),
                RankError::NonFiniteShortlist
            );
            RankError::NonFiniteShortlist
        } else {
            assert_eq!(error, "nonfinite-model");
            assert!(
                case["results"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["score"].is_null())
            );
            RankError::NonFiniteModel
        };
        assert_eq!(
            history_ranking::rank(
                query,
                candidates,
                25,
                &context,
                &mut ranking::SimilarityCache::default(),
                crate::state::sidebar_name_cmp
            )
            .unwrap_err(),
            expected
        );
    }
    assert_eq!(refused, 6);
}

#[test]
fn admitted_constructors_validate_reduced_flag_and_rank_rejects_invalid_primitives() {
    for flag in [
        serde_json::json!(1),
        serde_json::json!("false"),
        serde_json::Value::Null,
        serde_json::json!([]),
        serde_json::json!({}),
    ] {
        let wire = serde_json::json!({"sm":{"id":"E1","count":1,"visits":[1],"_reduced":flag}})
            .to_string();
        assert!(History::from_wire(&wire).is_err());
    }
    for flag in [true, false] {
        let wire=serde_json::json!({"sm":{"id":"E1","count":1,"visits":[1],"_reduced":flag,"unknown":"retained"}}).to_string();
        assert!(History::from_wire(&wire).is_ok());
    }
    let history = History::default();
    let model = model::Model::fallback();
    let metadata = model::Metadata::default();
    let mut context = history_ranking::Context {
        history: &history,
        now_ms: f64::NAN,
        model: &model,
        metadata: &metadata,
        source: "texty-autocomplete",
        exact_boost: false,
    };
    assert_eq!(
        history_ranking::rank(
            "sm",
            Vec::new(),
            25,
            &context,
            &mut ranking::SimilarityCache::default(),
            str::cmp
        )
        .unwrap_err(),
        history_ranking::RankError::InvalidClock
    );
    context.now_ms = 1.0;
    let invalid = ranking::Candidate {
        id: "E1".into(),
        name: "smile".into(),
        matched_name: "smile".into(),
        canonical: "smile".into(),
        is_emoji: false,
        name_score: Some(f64::NAN),
        exact: false,
        query_ratio: 0.0,
        combo_breaks: None,
    };
    assert_eq!(
        history_ranking::rank(
            "sm",
            vec![invalid],
            25,
            &context,
            &mut ranking::SimilarityCache::default(),
            str::cmp
        )
        .unwrap_err(),
        history_ranking::RankError::NonFiniteNameScore
    );
}

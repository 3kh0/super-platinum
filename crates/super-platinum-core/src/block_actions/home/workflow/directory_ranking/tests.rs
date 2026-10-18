//! Actual retained current-source factory oracle. No Rust execution was claimed
//! when this source package was prepared; root must run these tests after wiring.
use super::*;

#[test]
fn comparison_type_is_distinct_from_known_conversation_observation() {
    let typed = json!({"item":{"id":"BAD_ID","name":"raw-z","facts":{
        "channelType":"channel","conversation":false,"comparisonName":"Alpha2"
    }}});
    assert_eq!(comparison_name(&typed), "alpha2");
    let unknown = json!({"item":{"id":"BAD_ID","name":"raw-z","facts":{
        "channelType":null,"conversation":false,"comparisonName":"Alpha2"
    }}});
    assert_eq!(comparison_name(&unknown), "raw-z");
}
use std::collections::BTreeMap;
#[test]
fn continuous_member_bonus_uses_actual_upsert_derived_names() {
    let data = oracle();
    let state = json!({});
    let history = History::from_wire("{}").unwrap();
    let model = Model::fallback();
    let normalize = |s: &str| Ok(s.to_owned());
    let context = Context {
        state: &state,
        history: &history,
        model: &model,
        now_ms: 1.0,
        normalize: &normalize,
    };
    for case in data["nameProof"].as_array().unwrap() {
        let result = bonus(
            case["input"]["query"].as_str().unwrap(),
            &json!({"item":case["item"]}),
            &context,
            &Options::default(),
            &mut SimilarityCache::default(),
        );
        close(
            result["fuzzyScoreBonus"].as_f64().unwrap(),
            case["expected"].as_f64().unwrap(),
            "actual XbX7/hWCs stored name",
        );
        if case["input"]["id"] == "U_NORM_REAL" || case["input"]["id"] == "U_NORM_DISPLAY" {
            assert_ne!(case["expected"], case["legacyExpected"]);
        }
    }
}
fn oracle() -> Value {
    serde_json::from_str(include_str!("oracle.json")).unwrap()
}
fn normalize_table<'a>(data: &'a Value) -> impl Fn(&str) -> Result<String, Error> + 'a {
    move |text| {
        data["normalization"][text]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::MissingObservation(text.to_owned()))
    }
}
fn collate(a: &str, b: &str, data: &Value) -> Ordering {
    data["collation"][a][b]
        .as_i64()
        .expect("captured renderer numeric collation")
        .cmp(&0)
}
fn close(a: f64, b: f64, context: &str) {
    assert!(
        (a - b).abs() <= 1e-10 * (1.0 + b.abs()),
        "{context}: actual {a}, expected {b}"
    );
}
#[test]
fn actual_member_channel_im_mpim_order_and_every_numeric_feature() {
    let data = oracle();
    let normalize = normalize_table(&data);
    let mut cache = SimilarityCache::default();
    for case in data["cases"].as_array().unwrap() {
        if case["options"]["keepCache"] != true {
            cache.clear();
        }
        let state = &case["state"];
        let history =
            History::from_wire(&serde_json::to_string(&state["history"]).unwrap()).unwrap();
        let model = if let Some(weights) = state["modelWeights"].as_object() {
            Model::from_weights(
                weights
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_f64().unwrap()))
                    .collect(),
            )
            .unwrap()
        } else {
            Model::fallback()
        };
        let opts = &case["options"];
        let options = Options {
            limit: opts["limit"].as_u64().unwrap_or(24) as usize,
            source: opts["source"].as_str().unwrap_or("").to_owned(),
            self_first: opts["selfFirst"].as_bool().unwrap_or(false),
            flat: opts["flat"].as_bool().unwrap_or(false),
            demote_passthrough: state["demotePassthrough"].as_bool().unwrap_or(false),
            current_channel: opts.get("currentChannel").cloned(),
            current_thread_users: opts["currentThreadUsers"]
                .as_array()
                .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect()),
            ..Options::default()
        };
        let context = Context {
            state,
            history: &history,
            model: &model,
            now_ms: state["now"].as_f64().unwrap(),
            normalize: &normalize,
        };
        let rows = rank(
            case["query"].as_str().unwrap(),
            &case["groups"],
            &options,
            &context,
            &mut cache,
            |a, b| collate(a, b, &data),
        )
        .unwrap();
        let expected = case["result"].as_array().unwrap();
        let label = case["label"].as_str().unwrap();
        assert_eq!(rows.len(), expected.len(), "{label}");
        for (r, e) in rows.iter().zip(expected) {
            assert_eq!(r.row["id"], e["id"], "{label}");
            assert_eq!(r.row["suggestionType"], e["suggestionType"], "{label}");
            close(r.model_score, e["modelScore"].as_f64().unwrap(), label);
            assert_eq!(
                r.features.len(),
                e["featureCount"].as_u64().unwrap() as usize,
                "{label}"
            );
            for (name, value) in &r.features {
                close(
                    *value,
                    e["features"][*name].as_f64().unwrap_or(0.0),
                    &format!("{label} {} {name}", e["id"]),
                );
            }
            assert!(
                e["features"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .all(|key| r.features.iter().any(|(name, _)| *name == key))
            );
            assert!(
                r.original_index
                    < case["groups"]
                        .as_object()
                        .unwrap()
                        .values()
                        .map(|x| x.as_array().unwrap().len())
                        .sum::<usize>()
            );
        }
    }
}
#[test]
fn direct_full_v8_feature_lists_preserve_missing_and_legacy_fields() {
    let data = oracle();
    let normalize = normalize_table(&data);
    for case in data["featureCases"].as_array().unwrap() {
        let actual = features::vector(
            &case["row"],
            case["now"].as_f64().unwrap() / 1000.0,
            case["source"].as_str().unwrap(),
            &normalize,
        )
        .unwrap();
        assert_eq!(
            actual.len(),
            case["featureCount"].as_u64().unwrap() as usize
        );
        for (name, value) in &actual {
            close(
                *value,
                case["features"][*name].as_f64().unwrap_or(0.0),
                name,
            );
        }
        assert!(
            case["features"]
                .as_object()
                .unwrap()
                .keys()
                .all(|key| actual.iter().any(|(name, _)| *name == key))
        );
    }
}
#[test]
fn normalized_history_is_not_normalized_twice() {
    let h = History::from_wire(r#"{" alice":{"id":"U1","count":1,"visits":[1000]}}"#).unwrap();
    let a = h.score_normalized(" alice", &["U1"], 2000.0);
    let b = h.score_normalized("alice", &["U1"], 2000.0);
    assert_eq!(a[0].hit_count, 1.0);
    assert_eq!(b[0].hit_count, 1.0); // normalized score rescales, so inspect a mixed comparison below.
    let h=History::from_wire(r#"{" alice":{"id":"U1","count":1,"visits":[1000]},"alice":{"id":"U2","count":1,"visits":[1000]}}"#).unwrap();
    let a = h.score_normalized(" alice", &["U1", "U2"], 2000.0);
    let b = h.score_normalized("alice", &["U1", "U2"], 2000.0);
    assert_eq!(a[0].raw, 100.0);
    assert_eq!(a[1].raw, 50.0);
    assert_eq!(b[0].raw, 50.0);
    assert_eq!(b[1].raw, 100.0);
}
#[test]
fn current_model_external_intercept_is_per_feature_before_product() {
    let model = Model::from_weights(BTreeMap::from([
        ("USER_IS_EXTERNAL".into(), 2.0),
        ("READ_USER_IS_EXTERNAL".into(), 3.0),
    ]))
    .unwrap();
    assert_eq!(model.score(&[("USER_IS_EXTERNAL", 1.0)], ""), 5.0);
    assert_eq!(
        model.score_with_adjustment(&[("USER_IS_EXTERNAL", 1.0)], "", |k| {
            if k == "USER_IS_EXTERNAL" { -1.0 } else { 0.0 }
        }),
        4.0
    );
}
#[test]
fn explicit_infinity_is_distinct_from_missing_score() {
    assert_eq!(match_score(&json!({"score":null})), Some(f64::INFINITY));
    assert_eq!(match_score(&json!({})), None);
    let a = json!({"score":null,"name":"z","item":{}});
    let b = json!({"score":null,"name":"a","item":{}});
    assert_eq!(initial_cmp(&a, &b, &str::cmp), Ordering::Equal);
}
#[test]
fn parse_float_and_missing_age_source_edges() {
    assert_eq!(features::parse_float(&json!(" 123.5suffix")), 123.5);
    assert_eq!(features::parse_float(&json!("1e+tail")), 1.0);
    assert!(features::parse_float(&json!(null)).is_nan());
}
#[test]
fn missing_normalization_and_invalid_owner_clock_refuse() {
    let data = oracle();
    let case = &data["cases"][0];
    let h = History::default();
    let model = Model::fallback();
    let fail = |s: &str| Err(Error::MissingObservation(s.to_owned()));
    let mut ctx = Context {
        state: &case["state"],
        history: &h,
        model: &model,
        now_ms: 1.0,
        normalize: &fail,
    };
    let result = rank(
        "alice",
        &case["groups"],
        &Options::default(),
        &ctx,
        &mut SimilarityCache::default(),
        str::cmp,
    );
    assert!(matches!(result, Err(Error::MissingObservation(_))));
    ctx.now_ms = f64::NAN;
    assert_eq!(
        rank(
            "alice",
            &case["groups"],
            &Options::default(),
            &ctx,
            &mut SimilarityCache::default(),
            str::cmp
        )
        .unwrap_err(),
        Error::InvalidClock
    );
}

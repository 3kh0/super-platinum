use super::*;
use crate::block_actions::home::workflow::directory_ranking::{Options, rank};
use crate::composer_emoji::{history::History, model::Model, ranking::SimilarityCache};

fn oracle() -> Value {
    serde_json::from_str(include_str!("oracle.json")).unwrap()
}

#[test]
fn original_file_factories_features_order_scores_and_shortlist() {
    let oracle = oracle();
    let mut mismatches = Vec::new();
    for case in oracle["cases"].as_array().unwrap() {
        let history = History::from_wire("{}").unwrap();
        let model = Model::fallback();
        let norm = |s: &str| {
            oracle["normalization"][s]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| missing(&format!("normalize {s}")))
        };
        let context = Context {
            state: &case["state"],
            history: &history,
            model: &model,
            now_ms: case["state"]["now"].as_f64().unwrap(),
            normalize: &norm,
        };
        let result = rank(
            case["query"].as_str().unwrap(),
            &case["groups"],
            &Options {
                limit: 25,
                source: "texty-autocomplete".into(),
                ..Default::default()
            },
            &context,
            &mut SimilarityCache::default(),
            |a, b| {
                oracle["collation"][a][b]
                    .as_i64()
                    .expect("observed collation")
                    .cmp(&0)
            },
        );
        let actual = match result {
            Ok(rows) => rows,
            Err(error) => {
                mismatches.push(format!("{} rank failed: {error:?}", case["label"]));
                continue;
            }
        };
        let expected = case["result"].as_array().unwrap();
        if actual.len() != expected.len() {
            mismatches.push(format!(
                "{} row count {} != {}",
                case["label"],
                actual.len(),
                expected.len()
            ));
        }
        for (a, e) in actual.iter().zip(expected) {
            let label = format!("{} {}", case["label"], a.row["id"]);
            if a.row["id"] != e["id"] {
                mismatches.push(format!("{label} order expected {}", e["id"]));
            }
            if a.features.len() != e["featureCount"].as_u64().unwrap() as usize {
                mismatches.push(format!(
                    "{label} feature count {} != {}",
                    a.features.len(),
                    e["featureCount"]
                ));
            }
            if case["groups"]["files"][a.original_index]["id"] != a.row["id"] {
                mismatches.push(format!(
                    "{label} original index {} does not identify row",
                    a.original_index
                ));
            }
            for (key, value) in &a.features {
                match e["features"][*key].as_f64() {
                    Some(expected) if (value - expected).abs() < 1e-9 => {}
                    Some(expected) => {
                        mismatches.push(format!("{label} {key}: {value} != {expected}"))
                    }
                    None => mismatches.push(format!("{label} missing original feature {key}")),
                }
            }
            if !((a.model_score - e["modelScore"].as_f64().unwrap()).abs() < 1e-9) {
                mismatches.push(format!(
                    "{label} score {} != {}",
                    a.model_score, e["modelScore"]
                ));
            }
            if case["groups"]["files"].as_array().unwrap().len() == 90 && a.row["numResults"] != 75
            {
                mismatches.push(format!("{label} shortlist {} != 75", a.row["numResults"]));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} original-file oracle mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

#[test]
fn file_selector_requires_observed_context_and_removes_spoofed_facts() {
    let oracle = oracle();
    let mut state = oracle["cases"][0]["state"].clone();
    let history = History::from_wire("{}").unwrap();
    let model = Model::fallback();
    let norm = |s: &str| Ok(s.into());
    let mut raw = oracle["cases"][0]["groups"]["files"][0]["item"].clone();
    raw["facts"] = json!({"member":true,"im":true,"comparisonName":"spoof","latestAction":999});
    raw["actions"] = json!([{"ts":"12.5tail"},{"ts":999}]);
    let before = raw.clone();
    let make = |state: &Value| {
        item(
            &raw,
            &Context {
                state,
                history: &history,
                model: &model,
                now_ms: 1760000000000.0,
                normalize: &norm,
            },
        )
    };
    let value = make(&state).unwrap();
    assert_eq!(value["facts"], json!({"file":true,"latestAction":12.5}));
    assert_eq!(raw, before);
    state.as_object_mut().unwrap().remove("remoteDm");
    assert!(matches!(make(&state), Err(Error::MissingObservation(_))));
    state["remoteDm"] = Value::Null;
    assert!(make(&state).is_ok());
    state["entityCtr"].as_object_mut().unwrap().remove("file");
    assert!(matches!(make(&state), Err(Error::MissingObservation(_))));
}

#[test]
fn original_latest_action_vectors_and_nonfinite_refusal() {
    let oracle: Value = serde_json::from_str(include_str!("actions-oracle.json")).unwrap();
    for case in oracle.as_array().unwrap() {
        assert_eq!(
            latest_action(&case["raw"]).unwrap().as_f64(),
            case["expected"].as_f64(),
            "{case}"
        );
    }
    assert_eq!(
        latest_action(&json!({"actions":[{"ts":"Infinity"}]})),
        Err(Error::NonFiniteScore)
    );
    assert_eq!(
        latest_action(&json!({"actions":{}})),
        Err(Error::InvalidInput)
    );
}

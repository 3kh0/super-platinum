use super::*;
use crate::composer_emoji::{history::History, model::Model};
fn oracle() -> Value {
    serde_json::from_str(include_str!("oracle.json")).unwrap()
}
pub(super) fn scored<'a>(rows: &'a [Value], scores: &Value) -> Vec<local::Scored<'a>> {
    rows.iter()
        .enumerate()
        .map(|(i, item)| local::Scored {
            item,
            id: item["id"].as_str().unwrap(),
            score: scores[i].as_u64().unwrap() as u32,
            score_details: None,
            is_exact_match: None,
            exact_match_details: None,
            is_self: false,
            is_flannel_fuzzy_passthrough: false,
        })
        .collect()
}
pub(super) fn run(
    case: &Value,
    source: &Value,
    rows: &[Value],
    oracle: &Value,
) -> Result<Vec<Ranked>, Error> {
    let history = History::from_wire("{}").unwrap();
    let model = Model::fallback();
    let norm = |s: &str| {
        oracle["normalization"][s]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::MissingObservation(format!("normalize {s}")))
    };
    rank_members(
        case["query"].as_str().unwrap(),
        &scored(rows, &case["scores"]),
        &Context {
            state: source,
            history: &history,
            model: &model,
            now_ms: source["now"].as_f64().unwrap(),
            normalize: &norm,
        },
        &mut SimilarityCache::default(),
        |raw| Ok(raw["profile"]["display_name"].as_str().unwrap().into()),
        |a, b| {
            oracle["collation"][a][b]
                .as_i64()
                .map(|n| n.cmp(&0))
                .ok_or_else(|| Error::MissingObservation(format!("collate {a}/{b}")))
        },
        false,
    )
}
#[test]
fn original_factories_member_features_scores_order_and_texty_shortlist() {
    let oracle = oracle();
    for case in oracle["cases"].as_array().unwrap() {
        let rows = case["raw"].as_array().unwrap();
        let result = run(case, &case["state"], rows, &oracle).unwrap();
        let expected = case["result"].as_array().unwrap();
        assert_eq!(result.len(), expected.len());
        for (actual, expected) in result.iter().zip(expected) {
            assert_eq!(actual.row["id"], expected["id"], "{}", case["label"]);
            assert_eq!(actual.row["suggestionType"], expected["suggestionType"]);
            assert_eq!(
                actual.features.len(),
                expected["featureCount"].as_u64().unwrap() as usize
            );
            for (name, value) in &actual.features {
                let want = expected["features"][*name].as_f64().unwrap();
                assert!(
                    (value - want).abs() < 1e-9,
                    "{} {name}: {value} != {want}",
                    case["label"]
                );
            }
            assert!(
                (actual.model_score - expected["modelScore"].as_f64().unwrap()).abs() < 1e-9,
                "score {}: actual {}, expected {}",
                actual.row["id"],
                actual.model_score,
                expected["modelScore"]
            );
            assert_eq!(
                actual.row["numResults"],
                case["preparedIds"].as_array().unwrap().len()
            );
            assert_eq!(rows[actual.original_index]["id"], actual.row["id"]);
        }
        if rows.len() == 90 {
            assert_eq!(result.len(), 25);
            assert!(result.iter().all(|r| r.row["numResults"] == 75));
        }
    }
}
#[test]
fn wire_facts_are_sanitized_and_duplicate_original_indexes_survive() {
    let oracle = oracle();
    let case = &oracle["cases"][2];
    let mut rows = case["raw"].as_array().unwrap().clone();
    rows[0]["facts"] = json!({"member":false,"external":true,"comparisonName":"INJECTED","bestName":"INJECTED","self":true});
    let result = run(case, &case["state"], &rows, &oracle).unwrap();
    let first = result.iter().find(|r| r.original_index == 0).unwrap();
    assert_eq!(first.row["item"]["facts"]["member"], true);
    assert_eq!(first.row["item"]["facts"]["external"], false);
    assert_eq!(
        first.row["item"]["facts"]["comparisonName"],
        rows[0]["profile"]["display_name"]
    );
    let mut indexes = result
        .iter()
        .filter(|r| r.row["id"] == "U00000009")
        .map(|r| r.original_index)
        .collect::<Vec<_>>();
    indexes.sort();
    assert_eq!(indexes, [0, 1]);
}
#[test]
fn missing_state_is_not_a_default_zero_observation() {
    let oracle = oracle();
    let case = &oracle["cases"][0];
    let rows = case["raw"].as_array().unwrap();
    for key in [
        "ims",
        "metadata",
        "navigationHistory",
        "experiments",
        "offline",
        "remoteDm",
        "remoteMpim",
        "affinity",
        "entityCtr",
        "self",
    ] {
        let mut state = case["state"].clone();
        state.as_object_mut().unwrap().remove(key);
        assert!(
            matches!(
                run(case, &state, rows, &oracle),
                Err(Error::MissingObservation(_))
            ),
            "{key}"
        );
    }
    let mut state = case["state"].clone();
    state["metadata"]["D00000006"]
        .as_object_mut()
        .unwrap()
        .remove("draft");
    assert!(matches!(
        run(case, &state, rows, &oracle),
        Err(Error::MissingObservation(_))
    ));
}
#[test]
fn missing_locale_observation_and_cross_category_input_refuse() {
    let oracle = oracle();
    // Unequal matcher scores never consult the name comparator. Equal scores
    // exercise the original 4ucY name/collation tiebreak route explicitly.
    let mut tied = oracle["cases"][2].clone();
    tied["scores"] = json!([0, 0, 0]);
    let case = &tied;
    let rows = case["raw"].as_array().unwrap();
    let mut missing = oracle.clone();
    missing["collation"] = json!({});
    assert!(matches!(
        run(case, &case["state"], rows, &missing),
        Err(Error::MissingObservation(_))
    ));
    missing = oracle.clone();
    missing["normalization"] = json!({});
    assert!(matches!(
        run(case, &case["state"], rows, &missing),
        Err(Error::MissingObservation(_))
    ));
    let mut mixed = rows.clone();
    mixed[0]["is_mpim"] = json!(true);
    assert_eq!(
        run(case, &case["state"], &mixed, &oracle).unwrap_err(),
        Error::InvalidInput
    );
}

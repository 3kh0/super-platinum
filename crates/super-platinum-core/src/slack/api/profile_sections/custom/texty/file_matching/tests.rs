use super::*;
use serde_json::json;

#[test]
fn original_file_matcher_vectors_keep_url_and_post_filter_score_policy() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let query = PreparedQuery {
            normalized: case["normalized"].as_str().unwrap().into(),
            matcher_normalized: case["matcher"].as_str().unwrap().into(),
        };
        let mut actual = Vec::new();
        for raw in case["rows"].as_array().unwrap() {
            // This historical matcher oracle explicitly injected these fixture
            // visibility facts. Actual eligibility is tested separately below.
            if truth(&raw["isUnknown"])
                || truth(&raw["isNonExistent"])
                || truth(&raw["notVisible"])
                || truth(&raw["dlp"])
            {
                continue;
            }
            let (score, exact) = if query.normalized.is_empty() {
                (0, Value::Null)
            } else {
                let names = Names::observe(raw, |s| {
                    oracle["normalization"][s]
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| Error::MissingObservation(s.into()))
                })
                .unwrap();
                let Some((score, exact)) = score(
                    &query,
                    names,
                    case["queryHasLink"].as_bool().unwrap(),
                    case["ignoreFuzzy"].as_bool().unwrap(),
                ) else {
                    continue;
                };
                (score, json!(exact))
            };
            actual.push(json!({"id":raw["id"],"score":score,"exact":exact}));
        }
        assert_eq!(json!(actual), case["expected"], "{case}");
    }
}

#[test]
fn original_canned_policy_and_slack_native_predicates() {
    let cases: Value = serde_json::from_str(include_str!("eligibility-oracle.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 180);
    for case in cases.as_array().unwrap() {
        let c = &case["context"];
        let actual = allows(&case["raw"], || {
            Ok(TemplateContext {
                current_team: c["current_team"].as_str().unwrap().into(),
                enterprise: c["enterprise"].as_bool().unwrap(),
                paid: c["paid"].as_bool().unwrap(),
                creator_enterprise: c["creator_enterprise"].as_str().map(str::to_owned),
            })
        })
        .unwrap();
        assert_eq!(actual, case["allowed"].as_bool().unwrap(), "{case}");
    }
}

#[test]
fn ordinary_file_needs_no_template_default_and_canned_missing_observation_refuses() {
    let missing = || Err(Error::MissingObservation("template context".into()));
    assert_eq!(
        allows(&json!({"id":"F00000001","mode":"quip"}), missing),
        Ok(true)
    );
    assert_eq!(
        allows(&json!({"id":"F00000001","mode":"list"}), missing),
        Ok(true)
    );
    assert!(matches!(
        allows(
            &json!({"id":"F00000001","mode":"quip","canvas_template_mode":"published","user":"USLACKBOT"}),
            missing
        ),
        Err(Error::MissingObservation(_))
    ));
    assert!(matches!(
        Names::observe(&json!({"name":4}), |s| Ok(s.into())),
        Err(Error::InvalidInput)
    ));
    assert!(matches!(
        Names::observe(&json!({"name":"Known"}), |_| Err(
            Error::MissingObservation("locale".into())
        )),
        Err(Error::MissingObservation(_))
    ));
}

use super::*;
use serde_json::json;

#[test]
fn actual_retained_mpim_vectors_preserve_each_branch_and_score_metadata() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("cases.json")).unwrap();
    assert_eq!(cases.len(), 27);
    for case in cases {
        let peers: BTreeMap<_, _> = case["users"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, value)| (id.clone(), value.clone()))
            .collect();
        let query = case["normalized_query"].as_str().unwrap();
        let mut options = Options::workflow(
            case["raw_query"].as_str().unwrap(),
            query,
            case["matcher_query"].as_str().unwrap(),
            false,
        );
        options.allow_empty_query = case["allow_empty"].as_bool().unwrap();
        options.match_email = case["match_email"].as_bool().unwrap();
        options.passthrough_remote_fuzzy_filtering = case["passthrough"].as_bool().unwrap();
        let subqueries: Vec<String> = serde_json::from_value(case["subqueries"].clone()).unwrap();
        let remote: BTreeSet<String> = serde_json::from_value(case["remote"].clone()).unwrap();
        let deny: BTreeSet<String> = serde_json::from_value(case["deny"].clone()).unwrap();
        let result = filter_mpims(
            case["items"].as_array().unwrap(),
            &peers,
            &options,
            &subqueries,
            &remote,
            |item| !deny.contains(item["id"].as_str().unwrap()),
            |item| {
                case["named_norm"][item["id"].as_str().unwrap()]
                    .as_str()
                    .map(str::to_owned)
            },
        );
        let actual: Vec<Value> = result.into_iter().map(|row| {
            let details = row.score_details_by_member.map(|members| members.into_iter()
                .map(|(id, details)| (id, json!({"score_details":details})))
                .collect::<serde_json::Map<String, Value>>());
            let scores = row.subquery_scores_by_member.map(|rows| rows.into_iter().map(|members|
                members.into_iter().map(|m| json!({"id":m.id,"score":m.score})).collect::<Vec<_>>()
            ).collect::<Vec<_>>());
            json!({"id":row.result.id,"score":row.result.score,
                "exact":row.result.is_exact_match,"passthrough":row.result.is_flannel_fuzzy_passthrough,
                "by_member":details,"subqueries":scores})
        }).collect();
        assert_eq!(json!(actual), case["expected"], "{}", case["label"]);
    }
}
#[test]
fn split_happens_before_single_at_removal_and_submatcher_normalization() {
    assert_eq!(
        mpim_subquery_inputs("@alice,@@bob\u{ff0c}@\u{3001}carol\u{ff64}dan|eve frank"),
        vec!["alice", "@bob", "", "carol", "dan", "eve", "frank"]
    );
    assert!(mpim_subquery_inputs(",| \u{ff64}").is_empty());
    // Tabs are not separators at this stage; hNDQ normally collapsed them.
    assert_eq!(mpim_subquery_inputs("a\tb"), vec!["a\tb"]);
}
#[test]
fn visible_custom_mpim_name_needs_actual_type_custom_flag_and_observed_on_assignments() {
    let item = json!({"id":"G1","is_mpim":true,"properties":{"has_custom_mpdm_name":true}});
    assert!(visible_mpim_custom_name(&item, true, false, false));
    assert!(!visible_mpim_custom_name(&item, false, false, true));
    assert!(!visible_mpim_custom_name(&item, true, true, false));
    assert!(visible_mpim_custom_name(&item, true, true, true));
    assert!(!visible_mpim_custom_name(
        &json!({"id":"G1","is_mpim":true}),
        true,
        false,
        false
    ));
    assert!(!visible_mpim_custom_name(
        &json!({"id":"C1","properties":{"has_custom_mpdm_name":true}}),
        true,
        false,
        true
    ));
}

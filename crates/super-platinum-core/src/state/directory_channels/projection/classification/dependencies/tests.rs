use super::*;
use serde_json::json;

#[test]
fn retained_selector_access_sets_match_merged_pre_name_dependencies() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("cases.json")).unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        let state = ClassificationState::capture(case["before"].as_array().unwrap()).unwrap();
        let ids = state
            .required_member_ids(case["incoming"].as_array().unwrap(), "USELF")
            .unwrap();
        assert_eq!(json!(ids), case["expected"], "{}", case["name"]);
        // Planning must not advance its private state.
        assert_eq!(state.shadow.len(), case["before"].as_array().unwrap().len());
    }
}

#[test]
fn dependency_plan_refuses_predecessor_gaps_and_malformed_im_without_partial_result() {
    let mut state = ClassificationState::capture(&[]).unwrap();
    state.unavailable.insert("DBLOCKED".into());
    for incoming in [
        vec![json!({"id":"DBLOCKED","is_im":true,"user":"UNEW"})],
        vec![json!({"id":"DMISSING","is_im":true})],
        vec![json!({"id":"DEMPTY","is_im":true,"user":""})],
        vec![json!({"id":"DOBJECT","is_im":true,"user":{}})],
        vec![
            json!({"id":"DGOOD","is_im":true,"user":"UOK"}),
            json!({"id":"DBLOCKED"}),
        ],
    ] {
        assert!(state.required_member_ids(&incoming, "USELF").is_err());
    }
    assert!(state.shadow.is_empty());
    assert!(
        state
            .required_member_ids(&[json!({"id":"COK","is_channel":true})], "USELF")
            .unwrap()
            .is_empty()
    );
}

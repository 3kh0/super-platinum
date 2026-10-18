use super::*;
use serde_json::json;

#[test]
fn conditional_version_uses_post_action_shadow_and_refuses_unobserved_predecessor() {
    let mut state = ClassificationState::capture(&[
        json!({"id":"C00000001","name":"First","_name_lc":"first","updated":1}),
    ])
    .unwrap();
    assert_eq!(state.conditional_version("C00000001").unwrap(), 1);
    let observed = Facts {
        self_id: "USELF".into(),
        current_team: "TONE".into(),
        omit_team: "TONE".into(),
        retain_public_membership_on_archive: false,
        members: BTreeMap::new(),
    };
    state
        .step(&[json!({"id":"C00000001","updated":2})], &observed, false)
        .unwrap();
    assert_eq!(state.conditional_version("C00000001").unwrap(), 2);
    assert_eq!(state.conditional_version("C00000002").unwrap(), 0);
    state.unavailable.insert("C00000002".into());
    assert!(state.conditional_version("C00000002").is_err());
    state
        .step(
            &[json!({"id":"C00000001","updated":"not-a-version"})],
            &observed,
            false,
        )
        .unwrap();
    assert!(state.conditional_version("C00000001").is_err());
}

fn facts(raw: &Value) -> Facts {
    Facts {
        self_id: "USELF".into(),
        current_team: "TONE".into(),
        omit_team: "TONE".into(),
        retain_public_membership_on_archive: false,
        members: serde_json::from_value(raw["members"].clone()).unwrap(),
    }
}
fn summary(row: ClassifiedRow) -> Value {
    json!({"id":row.id,"name_truthy":row.name_truthy,"kind":match row.kind {
        ClassificationKind::Absent=>"Absent",
        ClassificationKind::Existing=>"Existing",
        ClassificationKind::NonExistent=>"NonExistent",
    }})
}
#[test]
fn actual_retained_actions_and_post_page_classification_match_all_streams() {
    let streams: Vec<Value> = serde_json::from_str(include_str!("cases.json")).unwrap();
    assert_eq!(streams.len(), 24);
    let mut pages = 0;
    for stream in streams {
        let mut state = ClassificationState::capture(stream["before"].as_array().unwrap()).unwrap();
        for step in stream["steps"].as_array().unwrap() {
            let receipt = state
                .step(
                    step["incoming"].as_array().unwrap(),
                    &facts(&step["context"]),
                    false,
                )
                .unwrap();
            assert_eq!(json!(receipt.action_ids()), step["actions"], "{stream}");
            let got: Vec<_> = stream["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| summary(state.row(id.as_str().unwrap()).unwrap()))
                .collect();
            assert_eq!(json!(got), step["facts"], "{stream}");
            for row in receipt.rows() {
                assert_eq!(state.row(&row.id).unwrap(), *row);
            }
            pages += 1;
        }
    }
    assert_eq!(pages, 288);
}
#[test]
fn mixed_case_canonical_preimage_does_not_invent_unchanged_actions() {
    let streams: Vec<Value> = serde_json::from_str(include_str!("cases.json")).unwrap();
    let stream = &streams[0];
    assert_eq!(stream["before"][0]["name"], "MiXeD");
    assert_eq!(stream["before"][0]["_name_lc"], "mixed");
    let mut state = ClassificationState::capture(stream["before"].as_array().unwrap()).unwrap();
    for step in &stream["steps"].as_array().unwrap()[..2] {
        let receipt = state
            .step(
                step["incoming"].as_array().unwrap(),
                &facts(&step["context"]),
                false,
            )
            .unwrap();
        assert!(receipt.action_ids().is_empty());
    }
}
#[test]
fn unknown_member_witness_is_synchronous_and_missing_observation_is_not_absence() {
    let mut state = ClassificationState::capture(&[]).unwrap();
    let incoming = [json!({"id":"D00000001","is_im":true,"user":"U00000001"})];
    let missing = facts(&json!({"members":{}}));
    assert!(matches!(
        state.step(&incoming, &missing, false),
        Err(Error::MissingObservation)
    ));
    assert_eq!(
        state.row("D00000001").unwrap().kind,
        ClassificationKind::Absent
    );
    let observed = facts(
        &json!({"members":{"U00000001":{"id":"U00000001","name":"U00000001","isUnknown":true}}}),
    );
    let receipt = state.step(&incoming, &observed, false).unwrap();
    assert_eq!(receipt.action_ids(), &["D00000001"]);
    assert_eq!(
        state.row("D00000001").unwrap().kind,
        ClassificationKind::Existing
    );
    assert!(state.row("D00000001").unwrap().name_truthy);
    // This fact is already available at receipt; there is no renderer/member
    // completion prerequisite or clock wait in the classifier.
    assert!(state.step(&incoming, &missing, false).is_err());
    assert_eq!(
        state.row("D00000001").unwrap().kind,
        ClassificationKind::Existing
    );
}
#[test]
fn refused_later_row_does_not_partially_advance_a_page() {
    let mut state = ClassificationState::capture(&[]).unwrap();
    let incoming = [
        json!({"id":"C00000001","is_channel":true,"name":"first"}),
        json!({"id":"invalid"}),
    ];
    assert!(
        state
            .step(&incoming, &facts(&json!({"members":{}})), false)
            .is_err()
    );
    assert_eq!(
        state.row("C00000001").unwrap().kind,
        ClassificationKind::Absent
    );
    assert!(state.row("invalid").is_err());
    assert!(ClassificationState::capture(&[json!({"id":"C00000001","_name_lc":false})]).is_err());
}
#[test]
fn checked_observation_is_the_only_public_constructor_and_retired_capture_refuses() {
    let owner = WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "USELF".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        token: "synthetic".into(),
        url: "https://fixture.slack.com".into(),
    };
    let native = BTreeMap::new();
    let mut store = Store::default();
    store.begin("A", &owner, &native).unwrap();
    let lease = store.begin_full_boot("A", &owner, 1).unwrap();
    assert!(store.finish_full_boot(lease, "A", &owner, 1));
    let observed = store
        .observation("A", &owner, 1, "original", &native)
        .unwrap();
    assert!(
        observed
            .classification(&store, "A", &owner, 1, "original", &native)
            .is_ok()
    );
    assert!(
        observed
            .classification(&store, "B", &owner, 1, "original", &native)
            .is_err()
    );
    assert!(
        observed
            .classification(&store, "A", &owner, 1, "replacement", &native)
            .is_err()
    );
    store
        .enqueue(
            "A",
            &owner,
            1,
            crate::state::directory_channels::Origin::Direct,
            vec![json!({"id":"CLATER"})],
        )
        .unwrap();
    assert!(
        observed
            .classification(&store, "A", &owner, 1, "original", &native)
            .is_err()
    );
}

#[test]
fn actual_unknown_name_lowercase_order_matches_initial_and_evolving_boundaries() {
    let streams: Vec<Value> = serde_json::from_str(include_str!("boundary-cases.json")).unwrap();
    assert_eq!(streams.len(), 8);
    let mut pages = 0;
    for stream in streams {
        let mut state = ClassificationState::capture(stream["before"].as_array().unwrap()).unwrap();
        for step in stream["steps"].as_array().unwrap() {
            let receipt = state
                .step(
                    step["incoming"].as_array().unwrap(),
                    &facts(&step["context"]),
                    false,
                )
                .unwrap();
            assert_eq!(json!(receipt.action_ids()), step["actions"], "{stream}");
            let rows: Vec<_> = stream["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| summary(state.row(id.as_str().unwrap()).unwrap()))
                .collect();
            assert_eq!(json!(rows), step["facts"], "{stream}");
            pages += 1;
        }
    }
    assert_eq!(pages, 96);
}

#[test]
fn partial_observation_keeps_unrelated_rows_usable_but_never_reads_predecessor_gaps() {
    let owner = WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "USELF".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        token: "synthetic".into(),
        url: "https://fixture.slack.com".into(),
    };
    let native = BTreeMap::from([(
        "CLEGACY".into(),
        serde_json::from_value(json!({"id":"CLEGACY"})).unwrap(),
    )]);
    let mut store = Store::default();
    store.begin("A", &owner, &native).unwrap();
    let lease = store.begin_full_boot("A", &owner, 1).unwrap();
    assert!(store.finish_full_boot(lease, "A", &owner, 1));
    store
        .enqueue(
            "A",
            &owner,
            1,
            crate::state::directory_channels::Origin::Direct,
            vec![json!({"id":"CPENDING","name":"earlier receipt"})],
        )
        .unwrap();
    assert!(store.ready("A", &owner, 1, "q", &native).is_err());
    let observed = store.observation("A", &owner, 1, "q", &native).unwrap();
    let mut state = observed
        .classification(&store, "A", &owner, 1, "q", &native)
        .unwrap();
    assert_eq!(
        state.unavailable_ids(),
        &BTreeSet::from(["CLEGACY".into(), "CPENDING".into()])
    );
    for id in ["CLEGACY", "CPENDING"] {
        assert!(matches!(state.row(id), Err(Error::MissingObservation)));
        assert!(matches!(
            state.step(
                &[json!({"id":id,"name":"new"})],
                &facts(&json!({"members":{}})),
                false
            ),
            Err(Error::MissingObservation)
        ));
    }
    assert_eq!(
        state.row("COTHER").unwrap().kind,
        ClassificationKind::Absent
    );
    state
        .step(
            &[json!({"id":"COTHER","name":"known"})],
            &facts(&json!({"members":{}})),
            false,
        )
        .unwrap();
    assert_eq!(
        state.row("COTHER").unwrap().kind,
        ClassificationKind::Existing
    );
    assert_eq!(native.len(), 1);
    assert!(store.ready("A", &owner, 1, "q", &native).is_err());
}

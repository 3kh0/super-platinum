use super::*;

fn row(id: &str) -> Value {
    json!({"id":id,"is_usergroup":false})
}
fn observations(plan: &MentionPlan, rows: Vec<Value>) -> MentionObservations {
    MentionObservations {
        searches: vec![MentionSearch {
            lookup: plan.lookups()[0].clone(),
            rows,
        }],
        highlight_words: Some(vec!["<@U1".into()]),
        ..Default::default()
    }
}
// Explicit synthetic ASCII normalizer observations for these bounded fixtures.
// This is not a host implementation of J3F or a shipped-function replay.
fn advance(plan: &MentionPlan, obs: &mut MentionObservations) -> MentionStep {
    for _ in 0..64 {
        match plan.advance(obs).unwrap() {
            MentionStep::Need(MentionNeed::Normalization(text)) => {
                assert!(text.is_ascii());
                obs.normalizations.insert(text.clone(), text.to_lowercase());
            }
            result => return result,
        }
    }
    panic!("normalization did not settle")
}
#[test]
fn unknown_lookup_is_a_demand_and_actual_empty_results_keep_input_exact() {
    let input = vec![json!({"insert":"@alex\n","attributes":{"bold":true}})];
    let plan = MentionPlan::capture(&input, false, true).unwrap();
    assert!(matches!(
        plan.advance(&MentionObservations::default()).unwrap(),
        MentionStep::Need(MentionNeed::Search(_))
    ));
    let mut observed = observations(&plan, vec![]);
    assert_eq!(
        advance(&plan, &mut observed),
        MentionStep::Complete {
            ops: input.clone(),
            choices: vec![]
        }
    );
    assert!(plan.matches_input(&input, false, true));
    assert!(!plan.matches_input(&input, true, true));
}
#[test]
fn unique_member_expands_label_using_original_utf16_span_and_empty_formats() {
    let input = vec![json!({"insert":"\u{1f642} @alex\n","attributes":{"italic":true}})];
    let before = input.clone();
    let plan = MentionPlan::capture(&input, false, true).unwrap();
    let mut observed = observations(&plan, vec![row("U1")]);
    observed.names.push(MentionNames {
        raw: row("U1"),
        primary: "@Alex Doe".into(),
        secondary: String::new(),
    });
    let MentionStep::Complete { ops, choices } = advance(&plan, &mut observed) else {
        panic!()
    };
    assert_eq!(choices[0].global_index_utf16, 3);
    assert_eq!(choices[0].length_utf16, 5);
    assert_eq!(choices[0].id, "U1");
    let mention = ops
        .iter()
        .find(|op| op["attributes"]["slackmention"].is_object())
        .unwrap();
    assert_eq!(mention["insert"], "@Alex Doe");
    assert_eq!(
        mention["attributes"],
        json!({"slackmention":{"id":"U1","label":"@Alex Doe","mention":true,"unverified":false}})
    );
    assert_eq!(input, before);
}
#[test]
fn partial_group_is_not_promoted_but_full_group_match_is() {
    let group = json!({"id":"S1","is_usergroup":true});
    for (input, expected) in [("@alex\n", false), ("@alex_team\n", true)] {
        let plan = MentionPlan::capture(&[json!({"insert":input})], false, true).unwrap();
        let mut observed = observations(&plan, vec![group.clone()]);
        observed.names.push(MentionNames {
            raw: group.clone(),
            primary: "@alex_team".into(),
            secondary: String::new(),
        });
        let MentionStep::Complete { choices, .. } = advance(&plan, &mut observed) else {
            panic!()
        };
        assert_eq!(!choices.is_empty(), expected);
    }
}
#[test]
fn tied_full_names_require_actual_history_and_only_thirty_point_gap_resolves() {
    let plan = MentionPlan::capture(&[json!({"insert":"@alex\n"})], false, true).unwrap();
    let mut observed = observations(&plan, vec![row("U1"), row("U2")]);
    for id in ["U1", "U2"] {
        observed.names.push(MentionNames {
            raw: row(id),
            primary: "@alex".into(),
            secondary: String::new(),
        });
    }
    assert_eq!(
        advance(&plan, &mut observed),
        MentionStep::Need(MentionNeed::History)
    );
    observed.history = Some(History::from_entries(vec![]).unwrap());
    observed
        .history_queries
        .insert("@alex".into(), "@alex".into());
    observed.now_ms = Some(1000.0);
    let MentionStep::Complete { choices, .. } = advance(&plan, &mut observed) else {
        panic!()
    };
    assert!(choices.is_empty());
    observed.history = Some(
        History::from_entries(vec![(
            "U2".into(),
            json!({"id":"U2","count":10,"visits":[1]}),
        )])
        .unwrap(),
    );
    let MentionStep::Complete { choices, .. } = advance(&plan, &mut observed) else {
        panic!()
    };
    assert_eq!(choices[0].id, "U2");
}
#[test]
fn channel_selection_preserves_order_prefers_actual_local_and_filters_exact_name() {
    let plan = MentionPlan::capture(&[json!({"insert":"#ship\n"})], true, true).unwrap();
    let remote = json!({"id":"C1","name":"ship","fromAnotherTeam":true});
    let local = json!({"id":"C2","name":"ship","fromAnotherTeam":false});
    let wrong = json!({"id":"C3","name":"Ship","fromAnotherTeam":false});
    let mut observed = observations(&plan, vec![wrong, remote.clone(), local.clone()]);
    let MentionStep::Complete { choices, .. } = advance(&plan, &mut observed) else {
        panic!()
    };
    assert_eq!(choices[0].raw, local);
    observed.searches[0].rows = vec![remote.clone()];
    let MentionStep::Complete { choices, .. } = advance(&plan, &mut observed) else {
        panic!()
    };
    assert_eq!(choices[0].raw, remote);
}

#[test]
fn ordinary_member_and_channel_splices_are_identical_in_both_emoji_modes() {
    for (input, rows) in [
        ("@alex :smile:\n", vec![row("U1")]),
        (
            "#ship :smile:\n",
            vec![json!({"id":"C1","name":"ship","fromAnotherTeam":false})],
        ),
    ] {
        let input = vec![json!({"insert":input})];
        let text = MentionPlan::capture(&input, true, true).unwrap();
        let display = MentionPlan::capture(&input, true, false).unwrap();
        assert_eq!(text.candidates(), display.candidates());
        assert_eq!(text.lookups(), display.lookups());
        assert_eq!(
            display.lookups().len(),
            1,
            "rNCs filters emoji lookups first"
        );
        assert!(display.matches_input(&input, true, false));
        assert!(!display.matches_input(&input, true, true));
        let mut obs = observations(&text, rows);
        obs.names.push(MentionNames {
            raw: row("U1"),
            primary: "@Alex".into(),
            secondary: String::new(),
        });
        let expected = advance(&text, &mut obs);
        assert!(matches!(&expected, MentionStep::Complete { choices, .. } if choices.len() == 1));
        assert_eq!(advance(&display, &mut obs), expected);
    }
}
#[test]
fn duplicate_lookup_or_names_and_missing_highlights_cannot_be_fabricated() {
    let plan = MentionPlan::capture(&[json!({"insert":"@alex\n"})], false, true).unwrap();
    let mut observed = observations(&plan, vec![row("U1")]);
    assert_eq!(
        advance(&plan, &mut observed),
        MentionStep::Need(MentionNeed::Names(row("U1")))
    );
    observed.names.push(MentionNames {
        raw: row("U1"),
        primary: "@alex".into(),
        secondary: String::new(),
    });
    observed.highlight_words = None;
    assert_eq!(
        advance(&plan, &mut observed),
        MentionStep::Need(MentionNeed::Highlights)
    );
    observed.names.push(observed.names[0].clone());
    assert!(plan.advance(&observed).is_err());
    observed.names.pop();
    observed.searches.push(observed.searches[0].clone());
    assert!(plan.advance(&observed).is_err());
}

use super::*;

fn kind(value: MentionKind) -> &'static str {
    match value {
        MentionKind::Member => "member",
        MentionKind::Channel => "channel",
    }
}

#[test]
fn actual_candidate_queries_offsets_and_skipped_tokens_match() {
    let vectors: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    for (index, v) in vectors["results"].as_array().unwrap().iter().enumerate() {
        let input = v["input"].as_array().unwrap();
        let channel = v["channel"].as_bool().unwrap();
        let plan = MentionPlan::capture(input, channel, true).unwrap();
        assert_eq!(plan.original_ops(), input);
        assert!(plan.matches_input(input, channel, true));
        assert!(!plan.matches_input(input, !channel, true));
        let actual: Vec<_> = plan
            .candidates()
            .iter()
            .map(|c| {
                json!({
                    "type":kind(c.kind),"text":c.text.as_ref(),"matchLength":c.match_length_utf16,
                    "label":c.label,"labelWithoutPrefix":c.query,
                    "localIndex":c.local_index_utf16,"globalIndex":c.global_index_utf16
                })
            })
            .collect();
        assert_eq!(json!(actual), v["candidates"], "candidate {index}");
        let lookups: Vec<_> = plan
            .lookups()
            .iter()
            .map(|c| json!({"kind":kind(c.kind),"query":c.query}))
            .collect();
        assert_eq!(json!(lookups), v["lookups"], "lookups {index}");
    }
}

#[test]
fn repeated_candidates_share_source_text_and_bound_count() {
    let input = vec![json!({"insert":"@a ".repeat(32_768)})];
    let plan = MentionPlan::capture(&input, true, true).unwrap();
    assert_eq!(plan.candidates().len(), 32_768);
    assert_eq!(plan.lookups().len(), 1);
    let first = &plan.candidates()[0].text;
    assert!(
        plan.candidates()
            .iter()
            .all(|c| Arc::ptr_eq(first, &c.text))
    );
    assert_eq!(first.as_ref(), format!("{}\n", "@a ".repeat(32_768)));
    assert!(matches!(
        MentionPlan::capture(&[json!({"insert":"@a ".repeat(32_769)})], true, true),
        Err(SeedUnavailable::Limit)
    ));
}

#[test]
fn many_unique_queries_keep_first_occurrence_order() {
    let names: Vec<_> = (0..4096).rev().map(|i| format!("user{i}")).collect();
    let text = names.iter().map(|s| format!("@{s} ")).collect::<String>();
    let plan =
        MentionPlan::capture(&[json!({"insert":format!("{text}{text}")})], true, true).unwrap();
    assert_eq!(plan.lookups().len(), names.len());
    assert_eq!(plan.candidates().len(), names.len() * 2);
    assert!(
        plan.lookups()
            .iter()
            .zip(&names)
            .all(|(a, b)| a.kind == MentionKind::Member && a.query == *b)
    );
}

#[test]
fn lookup_plan_preserves_original_and_requires_exact_input() {
    let input =
        vec![json!({"insert":"@Alex @Alex #Alex","attributes":{"future":{"opaque":[true,7]}}})];
    let plan = MentionPlan::capture(&input, true, true).unwrap();
    assert_eq!(
        plan.lookups(),
        &[
            MentionLookup {
                kind: MentionKind::Member,
                query: "Alex".into()
            },
            MentionLookup {
                kind: MentionKind::Channel,
                query: "Alex".into()
            }
        ]
    );
    assert_eq!(plan.candidates().len(), 3);
    let mut changed = input.clone();
    changed[0]["attributes"]["future"]["opaque"][1] = json!(8);
    assert!(!plan.matches_input(&changed, true, true));
    assert_eq!(plan.original_ops(), input);
    for op in [
        json!({"retain":1}),
        json!({"insert":1}),
        json!({"insert":"@Alex","attributes":false}),
    ] {
        assert!(MentionPlan::capture(&[op], true, true).is_err());
    }
}

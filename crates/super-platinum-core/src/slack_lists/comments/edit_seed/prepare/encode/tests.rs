use super::*;
fn fixture() -> Value {
    serde_json::from_str(include_str!("vectors.json")).unwrap()
}
fn observations(state: &Value) -> EncodingObservations {
    let mut result = EncodingObservations::default();
    for (code, value) in state["emoji"].as_object().unwrap() {
        result.emoji.insert(
            code.clone(),
            value
                .get("unicode")
                .filter(|v| !v.is_null())
                .map(|_| EmojiEncoding {
                    name: value.get("name").and_then(Value::as_str).map(str::to_owned),
                    unicode: value
                        .get("unicode")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                }),
        );
    }
    for (id, value) in state["files"].as_object().unwrap() {
        result.files.insert(
            id.clone(),
            if value.is_null() {
                None
            } else {
                Some(value["isList"].as_bool().unwrap())
            },
        );
    }
    for (code, value) in state["unicodeNames"].as_object().unwrap() {
        result
            .unicode_names
            .insert(code.clone(), value.as_str().map(str::to_owned));
    }
    result
}
#[test]
fn actual_encoder_vectors_match_sections_semantics_and_portable_partition() {
    let fixture = fixture();
    let observed = observations(&fixture["state"]);
    for (index, case) in fixture["results"].as_array().unwrap().iter().enumerate() {
        let input = case["input"].as_array().unwrap();
        let mode = if case["portable"] == true {
            EditMode::Portable
        } else {
            EditMode::Rich
        };
        let plan = EncodePlan::capture(input, mode, case["headers"].as_bool().unwrap()).unwrap();
        let expected = case["output"]["blocks"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            plan.advance(&observed).unwrap(),
            EncodingStep::Complete(expected),
            "case {index}"
        );
        assert_eq!(plan.original_ops(), input);
    }
}
#[test]
fn state_lookups_are_reached_in_order_and_do_not_invent_missing_facts() {
    let input = vec![
        json!({"insert":{"slackemoji":{"text":":smile:"}}}),
        json!({"insert":"\n","attributes":{"header":1}}),
        json!({"insert":"List","attributes":{"slackmention":{"id":"F12345678","label":"List"}}}),
    ];
    let plan = EncodePlan::capture(&input, EditMode::Portable, false).unwrap();
    let mut observed = EncodingObservations::default();
    assert_eq!(
        plan.advance(&observed).unwrap(),
        EncodingStep::Need(EncodingNeed::Emoji(":smile:".into()))
    );
    observed.emoji.insert(
        ":smile:".into(),
        Some(EmojiEncoding {
            name: Some("smile".into()),
            unicode: Some("1f604".into()),
        }),
    );
    assert_eq!(
        plan.advance(&observed).unwrap(),
        EncodingStep::Need(EncodingNeed::UnicodeName("1f604".into()))
    );
    observed
        .unicode_names
        .insert("1f604".into(), Some("smile".into()));
    assert_eq!(
        plan.advance(&observed).unwrap(),
        EncodingStep::Need(EncodingNeed::File("F12345678".into()))
    );
    observed.files.insert("F12345678".into(), Some(true));
    let EncodingStep::Complete(blocks) = plan.advance(&observed).unwrap() else {
        panic!("observations are complete")
    };
    assert_eq!(blocks[0]["type"], "header");
    assert_eq!(
        blocks[1]["elements"][0]["elements"][0]["type"],
        "list_record"
    );
    assert!(plan.matches_input(&input, EditMode::Portable, false));
    assert!(!plan.matches_input(&input, EditMode::Portable, true));
}
#[test]
fn code_emoji_does_not_require_unreached_workspace_emoji_observation() {
    let input =
        vec![json!({"insert":{"slackemoji":{"text":":unknown:"}},"attributes":{"code":true}})];
    let plan = EncodePlan::capture(&input, EditMode::PlainText, false).unwrap();
    let EncodingStep::Complete(blocks) = plan.advance(&EncodingObservations::default()).unwrap()
    else {
        panic!("code fallback does not look up emoji")
    };
    assert_eq!(
        blocks[0]["elements"][0]["elements"][0],
        json!({"type":"text","text":":unknown:"})
    );
}
#[test]
fn opaque_original_blocks_are_preserved_and_duplicate_id_removed_only_from_output() {
    let block =
        json!({"type":"unknown_future","block_id":"B","opaque":{"original":[1,false,null]}});
    let input = vec![
        json!({"insert":{"slackopaqueblock":{"block":block}}}),
        json!({"insert":{"slackopaqueblock":{"block":block}}}),
    ];
    let plan = EncodePlan::capture(&input, EditMode::Portable, false).unwrap();
    let EncodingStep::Complete(blocks) = plan.advance(&EncodingObservations::default()).unwrap()
    else {
        panic!()
    };
    assert_eq!(blocks[0], block);
    let mut second = block.clone();
    second.as_object_mut().unwrap().remove("block_id");
    assert_eq!(blocks[1], second);
    assert_eq!(plan.original_ops(), input);
}
#[test]
fn unsupported_numeric_coercion_refuses_without_rewriting_the_input() {
    let input = vec![
        json!({"insert":"code"}),
        json!({"insert":"\n","attributes":{"code-block":true,"border":"1"}}),
    ];
    let plan = EncodePlan::capture(&input, EditMode::Rich, false).unwrap();
    assert!(plan.advance(&EncodingObservations::default()).is_err());
    assert_eq!(plan.original_ops(), input);
}

#[test]
fn malformed_normal_emoji_text_is_not_an_empty_observation_or_successful_drop() {
    let mut observed = EncodingObservations::default();
    observed.emoji.insert("".into(), None);
    for text in [json!(7), json!({}), json!([]), json!(false), Value::Null] {
        let input = vec![json!({"insert":{"slackemoji":{"text":text}}})];
        let plan = EncodePlan::capture(&input, EditMode::Rich, false).unwrap();
        assert_eq!(
            plan.advance(&observed).unwrap_err(),
            SeedUnavailable::MalformedMessage
        );
        assert_eq!(plan.original_ops(), input);
        let coded = vec![json!({"insert":{"slackemoji":{"text":text}},"attributes":{"code":true}})];
        assert!(matches!(
            EncodePlan::capture(&coded, EditMode::Rich, false)
                .unwrap()
                .advance(&EncodingObservations::default()),
            Ok(EncodingStep::Complete(_))
        ));
        let mut preformatted = input;
        preformatted.push(json!({"insert":"\n","attributes":{"code-block":true}}));
        assert!(matches!(
            EncodePlan::capture(&preformatted, EditMode::Rich, false)
                .unwrap()
                .advance(&EncodingObservations::default()),
            Ok(EncodingStep::Complete(_))
        ));
    }
    let source: Value = serde_json::from_str(include_str!("boundary.json")).unwrap();
    for case in source.as_array().unwrap() {
        let input = case["input"].as_array().unwrap();
        let result = EncodePlan::capture(input, EditMode::Rich, false)
            .unwrap()
            .advance(&observed);
        if case["branch"] == "normal" {
            assert!(result.is_err());
        } else {
            assert_eq!(
                result.unwrap(),
                EncodingStep::Complete(case["result"]["blocks"].as_array().unwrap().clone())
            );
        }
    }
}

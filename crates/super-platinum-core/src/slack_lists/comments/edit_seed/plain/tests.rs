use super::*;
use crate::slack_lists::comments::ordered::ResponseWitness;

fn witness(bytes: &str) -> MessageWitness {
    ResponseWitness::decode(bytes.as_bytes(), &serde_json::from_str(bytes).unwrap())
        .unwrap()
        .message(0)
        .unwrap()
}
fn message(value: Value) -> MessageWitness {
    witness(&json!({"ok":true,"messages":[value]}).to_string())
}
fn observations() -> PlainObservations {
    let fixture: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    let mut observed = PlainObservations {
        seed: serde_json::from_value(fixture["observations"]["seed"].clone()).unwrap(),
        highlight_words: Some(
            serde_json::from_value(fixture["observations"]["highlight_words"].clone()).unwrap(),
        ),
        ..Default::default()
    };
    fn bind(value: &Value, observed: &mut PlainObservations) {
        if value["type"] == "channel" {
            if let Some(id) = value["channel_id"].as_str() {
                if let Some(label) = observed.seed.channels.get(id) {
                    if !observed.channel_labels.iter().any(|(raw, _)| raw == value) {
                        observed.channel_labels.push((value.clone(), label.clone()));
                    }
                }
            }
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    bind(value, observed)
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    bind(value, observed)
                }
            }
            _ => {}
        }
    }
    for case in fixture["results"].as_array().unwrap() {
        bind(
            &serde_json::from_str::<Value>(case["response"].as_str().unwrap()).unwrap(),
            &mut observed,
        );
    }
    observed
}

#[test]
fn observed_empty_group_is_omitted_in_both_initialization_routes() {
    let mut obs = observations();
    obs.seed.usergroups.insert("S1".into(), String::new());
    for raw in [
        json!({"ts":"1","text":"<!subteam^S1>"}),
        json!({"ts":"1","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"usergroup","usergroup_id":"S1"}]}]}]}),
    ] {
        let original = message(raw);
        assert_eq!(
            PlainSeed::capture(&original, &obs).unwrap().delta_ops(),
            &[json!({"insert":"\n"})]
        );
        let mut missing = obs.clone();
        missing.seed.usergroups.remove("S1");
        assert!(
            matches!(PlainSeed::capture(&original,&missing),Err(SeedUnavailable::MissingUsergroup(id)) if id=="S1")
        );
    }
}

#[test]
fn actual_observed_name_bytes_are_preserved_in_text_and_block_routes() {
    let mut obs = observations();
    obs.seed
        .users
        .insert("U1".into(), "@First\nLast\t\u{1f642}".into());
    for raw in [
        json!({"ts":"1","text":"<@U1>"}),
        json!({"ts":"1","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"user","user_id":"U1"}]}]}]}),
    ] {
        let seed = PlainSeed::capture(&message(raw), &obs).unwrap();
        assert_eq!(seed.delta_ops()[0]["insert"], "@First\nLast\t\u{1f642}");
    }
}

#[test]
fn reached_missing_facts_name_exact_original_tokens_and_duplicates_still_refuse() {
    let mut obs = observations();
    let token = json!({"type":"channel","channel_id":"CUNKNOWN","tab_id":"original-tab"});
    let row = |token: Value| {
        message(
            json!({"ts":"1","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[token]}]}]}),
        )
    };
    assert_eq!(
        PlainSeed::capture(&row(token.clone()), &obs).unwrap_err(),
        SeedUnavailable::MissingChannelToken(token)
    );
    let emoji = json!({"type":"emoji","name":"party"});
    obs.seed.emoji_as_text = Some(true);
    assert_eq!(
        PlainSeed::capture(&row(emoji.clone()), &obs).unwrap_err(),
        SeedUnavailable::MissingEmojiText(emoji.clone())
    );
    obs.emoji_text = vec![
        (emoji.clone(), ":party:".into()),
        (emoji.clone(), ":party:".into()),
    ];
    assert_eq!(
        PlainSeed::capture(&row(emoji), &obs).unwrap_err(),
        SeedUnavailable::InvalidObservation
    );
    let slug = json!({"type":"canvas","file_id":"F2","url":"https://example.test/canvas"});
    assert_eq!(
        PlainSeed::capture(&row(slug.clone()), &obs).unwrap_err(),
        SeedUnavailable::MissingSlug(slug)
    );
    obs.highlight_words = None;
    assert_eq!(
        PlainSeed::capture(&row(json!({"type":"text","text":"x"})), &obs).unwrap_err(),
        SeedUnavailable::MissingHighlights
    );
}

#[test]
fn same_channel_id_does_not_borrow_another_tokens_tab_label() {
    let mut obs = observations();
    let first = json!({"type":"channel","channel_id":"C2"});
    let tab = json!({"type":"channel","channel_id":"C2","tab_id":"other"});
    obs.channel_labels = vec![(first.clone(), "#general".into())];
    obs.seed.channels.insert("C2".into(), "#general".into());
    let original = message(
        json!({"ts":"1","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[first,tab.clone()]}]}]}),
    );
    assert_eq!(
        PlainSeed::capture(&original, &obs).unwrap_err(),
        SeedUnavailable::MissingChannelToken(tab.clone())
    );
    obs.channel_labels
        .push((tab.clone(), "#observed-tab".into()));
    let result = PlainSeed::capture(&original, &obs).unwrap();
    assert!(
        result
            .delta_ops()
            .iter()
            .any(|op| op["insert"] == "#observed-tab"
                && op["attributes"]["slackmention"]["tabId"] == "other")
    );
    obs.channel_labels.push((tab, "#duplicate".into()));
    assert_eq!(
        PlainSeed::capture(&original, &obs).unwrap_err(),
        SeedUnavailable::InvalidObservation
    );
}

#[test]
fn actual_xcq_and_tsf_edit_vectors_match_full_ops_and_raw_preservation() {
    let fixture: Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    let cases = fixture["results"].as_array().unwrap();
    assert_eq!(cases.len(), 37);
    for case in cases {
        let original = witness(case["response"].as_str().unwrap());
        let before = original.raw_message().clone();
        if case.get("refusal").is_some() {
            assert_eq!(case["refusal"], "InvalidUnicode");
            assert!(matches!(
                PlainSeed::capture(&original, &observations()),
                Err(SeedUnavailable::InvalidUnicode)
            ));
            continue;
        }
        let seed = PlainSeed::capture(&original, &observations()).unwrap();
        assert_eq!(json!(seed.delta_ops()), case["ops"], "{}", case["name"]);
        assert_eq!(
            json!(seed.preserved_blocks()),
            case["preserved"],
            "{}",
            case["name"]
        );
        assert_eq!(original.raw_message(), &before);
        assert!(seed.source_current(&original));
    }
}

#[test]
fn exact_style_order_not_sorted_value_controls_literal_markup_and_currentness() {
    let raw = r#"{"ok":true,"messages":[{"ts":"1","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"😀","style":{"italic":false,"bold":true}}]}]}]}]}"#;
    let a = witness(raw);
    let b = witness(&raw.replace(
        r#""italic":false,"bold":true"#,
        r#""bold":true,"italic":false"#,
    ));
    assert_eq!(a.raw_message(), b.raw_message());
    let seed = PlainSeed::capture(&a, &observations()).unwrap();
    assert_eq!(seed.delta_ops(), &[json!({"insert":"_*😀*_\n"})]);
    assert!(!seed.source_current(&b));
    assert_eq!(
        PlainSeed::capture(&b, &observations()).unwrap().delta_ops(),
        &[json!({"insert":"*_😀_*\n"})]
    );
}

#[test]
fn unknown_required_selectors_refuse_without_id_or_emoji_text_fallback() {
    let original = message(
        json!({"ts":"1","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"user","user_id":"U1"}]}]}]}),
    );
    let mut obs = observations();
    obs.highlight_words = None;
    assert!(PlainSeed::capture(&original, &obs).is_err());
    obs = observations();
    obs.seed.emoji_as_text = None;
    assert!(PlainSeed::capture(&original, &obs).is_err());
    obs = observations();
    obs.seed.users.clear();
    assert!(matches!(
        PlainSeed::capture(&original, &obs),
        Err(SeedUnavailable::MissingUser(_))
    ));

    let emoji = json!({"type":"emoji","name":"party"});
    let original = message(
        json!({"ts":"1","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[emoji]}]}]}),
    );
    obs = observations();
    obs.seed.emoji_as_text = Some(true);
    assert!(PlainSeed::capture(&original, &obs).is_err());
    obs.emoji_text.push((emoji.clone(), ":party:".into()));
    assert_eq!(
        PlainSeed::capture(&original, &obs).unwrap().delta_ops(),
        &[json!({"insert":":party:\n"})]
    );
    obs.emoji_text.push((emoji, "different".into()));
    assert!(PlainSeed::capture(&original, &obs).is_err());
}

#[test]
fn stateful_slug_omission_is_positive_and_raw_bound_not_an_unknown_lookup() {
    let slug = json!({"type":"canvas","file_id":"F2","url":"https://example.test/canvas"});
    let original = message(
        json!({"ts":"1","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[slug]}]}]}),
    );
    let mut obs = observations();
    assert!(PlainSeed::capture(&original, &obs).is_err());
    obs.slugs.push((slug.clone(), None));
    assert_eq!(
        PlainSeed::capture(&original, &obs).unwrap().delta_ops(),
        &[json!({"insert":"\n"})]
    );
    obs.slugs[0].1 = Some(json!({"label":"Plan","id":"F2","opaque":[1,true]}));
    assert_eq!(
        PlainSeed::capture(&original, &obs).unwrap().delta_ops(),
        &[
            json!({"insert":"Plan","attributes":{"slackslug":{"label":"Plan","id":"F2","opaque":[1,true]}}}),
            json!({"insert":"\n"}),
        ]
    );
    obs.slugs[0].0["file_id"] = json!("F_OTHER");
    assert!(PlainSeed::capture(&original, &obs).is_err());
}

#[test]
fn source_identity_and_seed_changes_refuse_without_blocking_reaction_only_change() {
    let raw = json!({"ts":"1","channel":"F1","thread_ts":"0","text":"body","files":[{"id":"F2","extra":true}]});
    let original = message(raw.clone());
    let seed = PlainSeed::capture(&original, &observations()).unwrap();
    let mut fresh = raw.clone();
    fresh["reactions"] = json!([{"name":"eyes","count":1}]);
    assert!(seed.source_current(&message(fresh)));
    for key in ["ts", "channel", "thread_ts", "text", "files"] {
        let mut changed = raw.clone();
        changed[key] = json!("changed");
        assert!(!seed.source_current(&message(changed)), "{key}");
    }
}

#[test]
fn malformed_structures_and_bounded_work_never_fall_back_to_display_text() {
    for element in [
        json!({"type":"rich_text_section","elements":{}}),
        json!({"type":"rich_text_section","elements":[{"type":"text","text":"x","style":true}]}),
        json!({"type":"rich_text_list","style":"ordered","indent":33,"elements":[]}),
        json!({"type":"rich_text_list","style":"ordered","indent":1.5,"elements":[]}),
    ] {
        let original = message(
            json!({"ts":"1","text":"do not use fallback","blocks":[{"type":"rich_text","elements":[element]}]}),
        );
        assert!(PlainSeed::capture(&original, &observations()).is_err());
    }
    let original = message(json!({"ts":"1","text":"x".repeat(65_537)}));
    assert!(matches!(
        PlainSeed::capture(&original, &observations()),
        Err(SeedUnavailable::Limit)
    ));
}

#[test]
fn quote_prefix_utf16_quirk_preserves_valid_tail_and_refuses_unpaired_surrogate() {
    let capture = |text: &str| {
        let original = message(
            json!({"ts":"1","blocks":[{"type":"rich_text","elements":[{"type":"rich_text_quote","elements":[{"type":"text","text":text}]}]}]}),
        );
        PlainSeed::capture(&original, &observations())
    };
    assert_eq!(
        capture("> \u{1f600}abc").unwrap().delta_ops(),
        &[json!({"insert":"> > \u{1f600}abcc\n"})]
    );
    assert!(matches!(
        capture("> \u{1f600}"),
        Err(SeedUnavailable::InvalidUnicode)
    ));
    assert_eq!(
        capture("> ordinary").unwrap().delta_ops(),
        &[json!({"insert":"> > ordinary\n"})]
    );
}

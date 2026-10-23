use super::*;
use std::collections::BTreeSet;
fn settings() -> PreparationSettings {
    PreparationSettings {
        is_emoji_displayed_as_text: true,
        add_text_presentation_selectors: false,
        skip_markdown_code: true,
        use_channel_tagging: false,
    }
}
fn emoji() -> EmojiObservations {
    EmojiObservations {
        current_locale: Some("en-US".into()),
        locale: Some("en-US".into()),
        names: Some(BTreeSet::new()),
        translation: EmojiTranslation::KnownAbsent,
        ..Default::default()
    }
}

#[test]
fn staged_plain_preparation_encodes_changed_mark_and_link_without_mutating_input() {
    let input = vec![json!({"insert":"*hi* https://example.com\n"})];
    let before = input.clone();
    let plan =
        PlainPreparation::capture(&input, settings(), &SyntaxObservations::default(), &emoji())
            .unwrap();
    assert!(plan.matches_input(&input, &settings()));
    let PreparationStep::Complete(prepared) =
        plan.advance(&MentionObservations::default()).unwrap()
    else {
        panic!()
    };
    let EncodingStep::Complete(blocks) = prepared
        .encoding(false)
        .unwrap()
        .advance(&EncodingObservations::default())
        .unwrap()
    else {
        panic!()
    };
    let elements = blocks[0]["elements"][0]["elements"].as_array().unwrap();
    assert!(
        elements
            .iter()
            .any(|value| value["text"] == "hi" && value["style"]["bold"] == true)
    );
    assert!(
        elements
            .iter()
            .any(|value| value["type"] == "link" && value["url"] == "https://example.com")
    );
    assert_eq!(input, before);
    assert_eq!(plan.original_ops(), &input);
    assert!(
        !prepared
            .comparison(&[json!({"insert":"old\n"})], &Value::Null)
            .unwrap()
            .unchanged()
    );
    assert!(
        prepared
            .comparison(&input, &Value::Null)
            .unwrap()
            .unchanged()
    );
    assert!(!prepared.comparison(&input, &json!([])).unwrap().unchanged());
}
#[test]
fn actual_missing_search_blocks_final_preparation_until_positive_empty_observation() {
    let plan = PlainPreparation::capture(
        &[json!({"insert":"hello @alex\n"})],
        settings(),
        &SyntaxObservations::default(),
        &emoji(),
    )
    .unwrap();
    let PreparationStep::Need(MentionNeed::Search(lookup)) =
        plan.advance(&MentionObservations::default()).unwrap()
    else {
        panic!()
    };
    let observed = MentionObservations {
        searches: vec![MentionSearch {
            lookup,
            rows: vec![],
        }],
        ..Default::default()
    };
    let PreparationStep::Complete(prepared) = plan.advance(&observed).unwrap() else {
        panic!()
    };
    assert_eq!(
        prepared.processed_ops(),
        &[json!({"insert":"hello @alex\n"})]
    );
    assert!(prepared.choices().is_empty());
}
#[test]
fn reached_emoji_catalog_is_required_and_explicit_symbol_gate_changes_stage_only() {
    assert!(
        PlainPreparation::capture(
            &[json!({"insert":":wave:\n"})],
            settings(),
            &SyntaxObservations::default(),
            &EmojiObservations::default()
        )
        .is_err()
    );
    let input = vec![json!({"insert":"\u{2194}\n"})];
    let off =
        PlainPreparation::capture(&input, settings(), &SyntaxObservations::default(), &emoji())
            .unwrap();
    let mut on = settings();
    on.add_text_presentation_selectors = true;
    let on =
        PlainPreparation::capture(&input, on, &SyntaxObservations::default(), &emoji()).unwrap();
    assert_eq!(off.symbol_ops(), off.syntax_ops());
    assert_ne!(on.symbol_ops(), off.symbol_ops());
}

#[test]
fn display_emoji_mode_skips_colon_conversion_and_preserves_original_semantic_embed() {
    let input = vec![
        json!({"insert":":unacquired: "}),
        json!({"insert":{"slackemoji":{"text":":smile:"}}}),
        json!({"insert":"\n"}),
    ];
    let original = input.clone();
    let mut display = settings();
    display.is_emoji_displayed_as_text = false;
    let plan = PlainPreparation::capture(
        &input,
        display.clone(),
        &SyntaxObservations::default(),
        &EmojiObservations::default(),
    )
    .unwrap();
    assert_eq!(plan.emoji_ops(), plan.symbol_ops());
    assert!(plan.matches_input(&input, &display));
    assert!(!plan.matches_input(&input, &settings()));
    assert!(!plan.mention_plan().is_emoji_displayed_as_text());
    assert!(plan.mention_plan().lookups().is_empty());
    let PreparationStep::Complete(prepared) =
        plan.advance(&MentionObservations::default()).unwrap()
    else {
        panic!("ordinary rNCs does not search emoji candidates");
    };
    assert_eq!(prepared.processed_ops(), input);
    assert_eq!(input, original);
    assert!(
        prepared
            .comparison(&input, &Value::Null)
            .unwrap()
            .unchanged()
    );
    assert!(matches!(
        prepared.encoding(false).unwrap().advance(&EncodingObservations::default()).unwrap(),
        EncodingStep::Need(EncodingNeed::Emoji(text)) if text == ":smile:"
    ));
    assert!(
        PlainPreparation::capture(
            &input,
            settings(),
            &SyntaxObservations::default(),
            &EmojiObservations::default()
        )
        .is_err(),
        "as_text must still acquire reached colon facts"
    );
}

#[test]
fn display_literal_colon_never_creates_emoji_encoding_or_lookup_demand() {
    let input = vec![json!({"insert":":smile: :custom_unknown: \u{1f604}\n"})];
    let mut display = settings();
    display.is_emoji_displayed_as_text = false;
    let plan = PlainPreparation::capture(
        &input,
        display,
        &SyntaxObservations::default(),
        &EmojiObservations::default(),
    )
    .unwrap();
    let PreparationStep::Complete(prepared) =
        plan.advance(&MentionObservations::default()).unwrap()
    else {
        panic!()
    };
    assert_eq!(prepared.processed_ops(), input);
    let EncodingStep::Complete(blocks) = prepared
        .encoding(false)
        .unwrap()
        .advance(&EncodingObservations::default())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        blocks,
        vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":":smile: :custom_unknown: \u{1f604}"}]}]})
        ]
    );
}

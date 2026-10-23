use super::*;
use serde_json::json;

#[path = "tests/custom.rs"]
mod custom_tests;

#[test]
fn reached_channel_search_and_highlights_are_explicit_save_demands() {
    use super_platinum_core::slack_lists::comments::edit_seed::prepare::{
        MentionKind, MentionLookup, MentionNeed, MentionObservations, MentionSearch,
    };
    let ops = vec![json!({"insert":"Known #project-harbor and #not-found\n"})];
    let (settings, _) = processing::settings(&proof()).unwrap();
    let mut observed = MentionObservations::default();
    let run = |observed: &MentionObservations| {
        processing::attempt_with_mentions(
            &ops,
            settings.clone(),
            "en-US",
            None,
            None,
            None,
            observed,
        )
        .unwrap()
    };
    let lookup = MentionLookup {
        kind: MentionKind::Channel,
        query: "project-harbor".into(),
    };
    assert!(
        matches!(run(&observed), processing::Attempt::NeedMention(MentionNeed::Search(actual)) if actual == lookup)
    );
    observed.searches.push(MentionSearch {
        lookup,
        rows: vec![json!({"id":"CFOUND","name":"project-harbor","fromAnotherTeam":false})],
    });
    assert!(
        matches!(run(&observed), processing::Attempt::NeedMention(MentionNeed::Search(actual)) if actual.query == "not-found")
    );
    observed.searches.push(MentionSearch {
        lookup: MentionLookup {
            kind: MentionKind::Channel,
            query: "not-found".into(),
        },
        rows: vec![],
    });
    assert!(matches!(
        run(&observed),
        processing::Attempt::NeedMention(MentionNeed::Highlights)
    ));
    observed.highlight_words = Some(vec!["<@USELF".into()]);
    let processing::Attempt::Ready(prepared) = run(&observed) else {
        panic!("observed search did not finish")
    };
    assert_eq!(prepared.original_ops(), ops);
    assert!(
        prepared
            .processed_ops()
            .iter()
            .any(|op| op["attributes"]["slackmention"]["id"] == "CFOUND")
    );
    assert!(prepared.processed_ops().iter().any(|op| {
        op["insert"]
            .as_str()
            .is_some_and(|s| s.contains("#not-found"))
    }));
}

fn proof() -> Value {
    json!({"plainTextMode":true,
        "userPreferences":{"ok":true,"prefs":{"plain_text_mode":true,"emoji_mode":"as_text"}},
        "assignments":{"ok":true,"assignments":{}}})
}

#[test]
fn ordinary_save_settings_use_actual_complete_policy_and_source_call_arguments() {
    let mut raw = proof();
    let (settings, headers) = processing::settings(&raw).unwrap();
    assert!(settings.is_emoji_displayed_as_text);
    assert!(!settings.skip_markdown_code);
    assert!(settings.use_channel_tagging);
    assert!(!settings.add_text_presentation_selectors);
    assert!(!headers);
    raw["assignments"]["assignments"] = json!({
        "composer_smart_symbols":{"group":"on"},
        "rich_text_headers_and_dividers":{"group":"on"}});
    let (settings, headers) = processing::settings(&raw).unwrap();
    assert!(settings.add_text_presentation_selectors && headers);
    raw["userPreferences"]["prefs"]["disable_smart_symbols"] = json!([]);
    assert!(
        !processing::settings(&raw)
            .unwrap()
            .0
            .add_text_presentation_selectors
    );
    for path in ["userPreferences", "assignments"] {
        let mut missing = raw.clone();
        missing[path] = Value::Null;
        assert!(processing::settings(&missing).is_err());
    }
    for mode in [Value::Null, json!("native"), json!("AS_TEXT"), json!(false)] {
        raw["userPreferences"]["prefs"]["emoji_mode"] = mode;
        assert!(
            !processing::settings(&raw)
                .unwrap()
                .0
                .is_emoji_displayed_as_text
        );
    }
}

#[test]
fn changed_ordinary_text_runs_preparation_and_encoder_without_invented_lookup_results() {
    let original = vec![json!({"insert":"changed *bold* text example.com\n"})];
    let retained = original.clone();
    let (settings, headers) = processing::settings(&proof()).unwrap();
    let prepared = processing::prepare(&original, settings, "en-US").unwrap();
    assert_eq!(prepared.original_ops(), original);
    assert!(
        prepared
            .processed_ops()
            .iter()
            .any(|op| op["attributes"]["bold"] == true)
    );
    assert!(
        prepared
            .processed_ops()
            .iter()
            .any(|op| op["attributes"]["link"].is_string())
    );
    let EncodingStep::Complete(blocks) = prepared
        .encoding(headers)
        .unwrap()
        .advance(&EncodingObservations::default())
        .unwrap()
    else {
        panic!("unexpected reached lookup");
    };
    assert_eq!(blocks[0]["type"], "rich_text");
    assert_eq!(original, retained);
}

#[test]
fn reached_unknown_automatic_mentions_and_catalog_are_not_fabricated_absence() {
    for text in ["hello @alex\n", "hello #general\n", "hello :smile:\n"] {
        let (settings, _) = processing::settings(&proof()).unwrap();
        assert!(
            processing::prepare(&[json!({"insert":text})], settings, "en-US").is_err(),
            "{text}"
        );
    }
    let (settings, _) = processing::settings(&proof()).unwrap();
    assert!(processing::prepare(&[json!({"insert":"ordinary\n"})], settings, "").is_err());
}

#[test]
fn observed_display_mode_skips_catalog_but_keeps_reached_automatic_mention_refusal() {
    let mut raw = proof();
    raw["userPreferences"]["prefs"]["emoji_mode"] = json!("native");
    let (settings, _) = processing::settings(&raw).unwrap();
    assert!(!settings.is_emoji_displayed_as_text);
    let literal = vec![json!({"insert":":smile: :unacquired_custom: \u{1f604}\n"})];
    let prepared = processing::prepare(&literal, settings.clone(), "en-US").unwrap();
    assert_eq!(prepared.original_ops(), literal);
    assert_eq!(prepared.processed_ops(), literal);
    for text in ["@alex :smile:\n", "#general :smile:\n"] {
        assert!(processing::prepare(&[json!({"insert":text})], settings.clone(), "en-US").is_err());
    }
    raw["userPreferences"] = Value::Null;
    assert!(
        processing::settings(&raw).is_err(),
        "missing carrier is not observed non-as_text"
    );
}

#[test]
fn original_semantic_mention_does_not_request_an_automatic_name_search() {
    let ops = vec![
        json!({"insert":"@Alex", "attributes":{"slackmention":{"id":"U123","label":"@Alex"}}}),
        json!({"insert":" changed\n"}),
    ];
    let (settings, headers) = processing::settings(&proof()).unwrap();
    let prepared = processing::prepare(&ops, settings, "en-US").unwrap();
    let EncodingStep::Complete(blocks) = prepared
        .encoding(headers)
        .unwrap()
        .advance(&EncodingObservations::default())
        .unwrap()
    else {
        panic!("semantic member asked for lookup");
    };
    assert_eq!(blocks[0]["elements"][0]["elements"][0]["type"], "user");
    assert_eq!(blocks[0]["elements"][0]["elements"][0]["user_id"], "U123");
    assert_eq!(prepared.original_ops(), ops);
}

#[test]
fn positive_standard_save_encodes_named_emoji_and_preserves_literal_unicode() {
    use super_platinum_core::{
        composer_emoji::standard::Version, slack_lists::comments::edit_seed::prepare::StandardEmoji,
    };
    let catalog = StandardEmoji::build(Version::V16).unwrap();
    let ops = vec![json!({"insert":"Named :smile: Unicode \u{1f604}\n"})];
    let before = ops.clone();
    let (settings, headers) = processing::settings(&proof()).unwrap();
    assert!(matches!(
        processing::attempt(&ops, settings.clone(), "en-US", Some("en-US"), None, None).unwrap(),
        processing::Attempt::NeedStandard
    ));
    let processing::Attempt::Ready(prepared) =
        processing::attempt(&ops, settings, "en-US", Some("en-US"), Some(&catalog), None).unwrap()
    else {
        panic!("catalog already observed");
    };
    let plan = prepared.encoding(headers).unwrap();
    let mut observed = EncodingObservations::default();
    let EncodingStep::Need(EncodingNeed::Emoji(text)) = plan.advance(&observed).unwrap() else {
        panic!("expected actual encoding demand");
    };
    assert_eq!(text, ":smile:");
    observed
        .emoji
        .insert(text.clone(), catalog.encoding(&text).unwrap());
    let EncodingStep::Complete(blocks) = plan.advance(&observed).unwrap() else {
        panic!("unresolved encoding");
    };
    assert_eq!(
        blocks,
        vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Named "},{"type":"emoji","name":"smile","unicode":"1f604"},
        {"type":"text","text":" Unicode \u{1f604}"}]}]})
        ]
    );
    assert_eq!(ops, before);
}

#[test]
fn save_catalog_does_not_grant_custom_absence_or_replace_user_locale_with_workspace_locale() {
    use super_platinum_core::{
        composer_emoji::standard::Version, slack_lists::comments::edit_seed::prepare::StandardEmoji,
    };
    let catalog = StandardEmoji::build(Version::V16).unwrap();
    let (settings, _) = processing::settings(&proof()).unwrap();
    let named = vec![json!({"insert":":smile:\n"})];
    for user_locale in [None, Some("fr-FR")] {
        // Unavailable translation must not become workspace English.
        assert!(
            processing::attempt(
                &named,
                settings.clone(),
                "en-US",
                user_locale,
                Some(&catalog),
                None
            )
            .is_err()
        );
    }
    let unknown = vec![json!({"insert":":custom_unacquired:\n"})];
    assert!(matches!(
        processing::attempt(
            &unknown,
            settings.clone(),
            "en-US",
            Some("en-US"),
            Some(&catalog),
            None
        )
        .unwrap(),
        processing::Attempt::NeedCustom
    ));
    let fullwidth = vec![json!({"insert":"\u{ff1a}smile\u{ff1a}\n"})];
    let processing::Attempt::Ready(prepared) = processing::attempt(
        &fullwidth,
        settings.clone(),
        "ja-JP",
        Some("en-US"),
        Some(&catalog),
        None,
    )
    .unwrap() else {
        panic!("known fullwidth branch");
    };
    assert!(
        prepared
            .processed_ops()
            .iter()
            .any(|op| op["insert"]["slackemoji"]["text"] == ":smile:")
    );
    assert!(matches!(
        processing::attempt(
            &[json!({"insert":"ordinary text\n"})],
            settings,
            "en-US",
            None,
            None,
            None
        )
        .unwrap(),
        processing::Attempt::Ready(_)
    ));
}

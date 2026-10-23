use super::*;
use super_platinum_core::{
    composer_emoji::standard::Version, slack_lists::comments::edit_seed::prepare::StandardEmoji,
};

#[test]
fn complete_names_preserve_custom_alias_identity_and_known_absence_without_mutating_delta() {
    let standard = StandardEmoji::build(Version::V16).unwrap();
    let raw = json!({"ok":true,"emoji":{"party_parrot":"https://fixture.invalid/party.png",
        "parrot_alias":"alias:party_parrot","smile":"alias:party_parrot"}});
    let receipt = custom::Complete::decode(raw.clone()).unwrap();
    let ops = vec![
        json!({"insert":"Custom :party_parrot: Alias :parrot_alias: :smile: :missing_name:\n"}),
    ];
    let original = ops.clone();
    let (settings, headers) = processing::settings(&proof()).unwrap();
    let processing::Attempt::Ready(prepared) = processing::attempt(
        &ops,
        settings,
        "en-US",
        Some("en-US"),
        Some(&standard),
        Some(&receipt),
    )
    .unwrap() else {
        panic!("complete original names are acquired");
    };
    let plan = prepared.encoding(headers).unwrap();
    let mut observed = EncodingObservations::default();
    for text in [":party_parrot:", ":parrot_alias:", ":smile:"] {
        observed.emoji.insert(
            text.into(),
            custom::encoding(&standard, Some(&receipt), text).unwrap(),
        );
    }
    assert!(observed.emoji[":party_parrot:"].is_none());
    assert!(observed.emoji[":parrot_alias:"].is_none());
    assert!(observed.emoji[":smile:"].is_some());
    assert!(custom::encoding(&standard, Some(&receipt), ":missing_name:").is_err());
    let EncodingStep::Complete(blocks) = plan.advance(&observed).unwrap() else {
        panic!("all actual demands supplied");
    };
    assert_eq!(
        blocks,
        vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":"Custom "},{"type":"emoji","name":"party_parrot"},
        {"type":"text","text":" Alias "},{"type":"emoji","name":"parrot_alias"},
        {"type":"text","text":" "},{"type":"emoji","name":"smile","unicode":"1f604"},
        {"type":"text","text":" :missing_name:"}]}]})
        ]
    );
    assert_eq!(ops, original);
    assert_eq!(receipt.raw(), &raw);
}

#[test]
fn actual_empty_and_complete_normalization_are_distinct_from_unacquired_custom_names() {
    let standard = StandardEmoji::build(Version::V16).unwrap();
    let empty = custom::Complete::decode(json!({"ok":true,"emoji":{}})).unwrap();
    let custom =
        custom::Complete::decode(json!({"ok":true,"emoji":{"party_parrot":"alias:target"}}))
            .unwrap();
    let ops = vec![json!({"insert":":party-parrot:\n"})];
    let (settings, _) = processing::settings(&proof()).unwrap();
    assert!(matches!(
        processing::attempt(
            &ops,
            settings.clone(),
            "en-US",
            Some("en-US"),
            Some(&standard),
            None
        )
        .unwrap(),
        processing::Attempt::NeedCustom
    ));
    let processing::Attempt::Ready(literal) = processing::attempt(
        &ops,
        settings.clone(),
        "en-US",
        Some("en-US"),
        Some(&standard),
        Some(&empty),
    )
    .unwrap() else {
        panic!("empty is positive");
    };
    assert!(
        !literal
            .processed_ops()
            .iter()
            .any(|op| op["insert"].get("slackemoji").is_some())
    );
    let processing::Attempt::Ready(prepared) = processing::attempt(
        &ops,
        settings,
        "en-US",
        Some("en-US"),
        Some(&standard),
        Some(&custom),
    )
    .unwrap() else {
        panic!("normalization can now prove prior candidate absent");
    };
    assert!(
        prepared
            .processed_ops()
            .iter()
            .any(|op| op["insert"]["slackemoji"]["text"] == ":party_parrot:")
    );
    assert!(
        custom::encoding(&standard, Some(&custom), ":party_parrot::skin-tone-4:")
            .unwrap()
            .is_none()
    );
    assert!(custom::encoding(&standard, None, ":party_parrot:").is_err());
}

#[test]
fn display_custom_embed_first_reaches_complete_catalog_at_encoding() {
    let standard = StandardEmoji::build(Version::V16).unwrap();
    let mut raw = proof();
    raw["userPreferences"]["prefs"]["emoji_mode"] = json!("native");
    let (settings, headers) = processing::settings(&raw).unwrap();
    let ops = vec![
        json!({"insert":":literal_custom: "}),
        json!({"insert":{"slackemoji":{"text":":parrot_alias:"}}}),
        json!({"insert":"\n"}),
    ];
    let processing::Attempt::Ready(prepared) =
        processing::attempt(&ops, settings, "en-US", None, None, None).unwrap()
    else {
        panic!("N=false does not run colon catalog conversion");
    };
    assert_eq!(prepared.processed_ops(), ops);
    let plan = prepared.encoding(headers).unwrap();
    let mut observed = EncodingObservations::default();
    let EncodingStep::Need(EncodingNeed::Emoji(text)) = plan.advance(&observed).unwrap() else {
        panic!()
    };
    assert_eq!(text, ":parrot_alias:");
    assert!(custom::needs_catalog(&standard, None, &text).unwrap());
    assert!(custom::encoding(&standard, None, &text).is_err());
    let raw = json!({"ok":true,"emoji":{"parrot_alias":"alias:party_parrot","party_parrot":"https://fixture.invalid/parrot.png"}});
    let complete = custom::Complete::decode(raw.clone()).unwrap();
    assert!(!custom::needs_catalog(&standard, Some(&complete), &text).unwrap());
    observed.emoji.insert(
        text.clone(),
        custom::encoding(&standard, Some(&complete), &text).unwrap(),
    );
    let EncodingStep::Complete(blocks) = plan.advance(&observed).unwrap() else {
        panic!()
    };
    assert_eq!(
        blocks,
        vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                {"type":"text","text":":literal_custom: "},{"type":"emoji","name":"parrot_alias"}
            ]}]})
        ]
    );
    assert_eq!(complete.raw(), &raw);
    assert!(!custom::needs_catalog(&standard, None, ":smile:").unwrap());
    let empty = custom::Complete::decode(json!({"ok":true,"emoji":{}})).unwrap();
    assert!(
        !custom::needs_catalog(&standard, Some(&empty), &text).unwrap(),
        "do not duplicate complete reads"
    );
    assert!(
        custom::encoding(&standard, Some(&empty), &text).is_err(),
        "complete negative does not invent custom membership"
    );
    assert!(custom::needs_catalog(&standard, None, "malformed").is_err());
}

#[test]
fn reached_custom_revision_and_original_socket_editor_and_read_retire_receipt() {
    let (mut shell, binding, _) = crate::slack_lists::comments::own_edit::tests::setup();
    shell.core.workspaces.get_mut("T1").unwrap().locale = Some("en-US".into());
    let delta = PlainDelta::capture(vec![json!({"insert":":party:\n"})]).unwrap();
    let edit = session_mut(&mut shell, &binding).unwrap();
    edit.blocks.clear();
    edit.plain = Some(delta.clone());
    let (_, serial, _, _) = controller::begin_preflight(&mut shell).unwrap();
    let mut facts = Facts {
        binding: binding.clone(),
        context: Context::capture(&shell, "T1").unwrap(),
        serial,
        revision: session(&shell, &binding).unwrap().revision,
        delta,
        preferences_revision: crate::app_home::autocomplete::preferences_revision(&shell),
        locale: "en-US".into(),
        files: vec![],
        custom_revision: None,
        mentions: Default::default(),
    };
    assert!(facts.current(&shell));
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .custom_emoji_revision += 1;
    assert!(
        facts.current(&shell),
        "unreached custom facts do not retire ordinary Save"
    );
    facts.custom_revision = Some(shell.core.workspaces["T1"].custom_emoji_revision);
    assert!(facts.current(&shell));
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .custom_emoji_revision += 1;
    assert!(!facts.current(&shell));
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .custom_emoji_revision -= 1;
    assert!(facts.current(&shell));
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
    assert!(!facts.current(&shell));
    shell.core.workspaces.get_mut("T1").unwrap().rt_generation -= 1;
    session_mut(&mut shell, &binding).unwrap().revision += 1;
    assert!(!facts.current(&shell));
    session_mut(&mut shell, &binding).unwrap().revision -= 1;
    assert!(facts.current(&shell));
    // Native finish removes only native custody; the no-op return path may
    // permit this expected phase, but must retain original content/read guards.
    session_mut(&mut shell, &binding).unwrap().finishing = true;
    assert!(!facts.current(&shell));
    assert!(facts.current_with_finish(&shell, true));
    session_mut(&mut shell, &binding).unwrap().revision += 1;
    assert!(!facts.current_with_finish(&shell, true));
    session_mut(&mut shell, &binding).unwrap().revision -= 1;
    publish::finish_read(&mut shell, &binding.source.owner, serial, None);
    assert!(!facts.current(&shell));
    assert!(!facts.current_with_finish(&shell, true));
}

use super::*;
use serde_json::json;
use super_platinum_core::composer_emoji::standard::Version;
use super_platinum_core::slack_lists::comments::edit_seed::prepare::StandardEmoji;

fn project(version: Option<Version>, value: &Value) -> Option<RichNode> {
    let core = crate::fixture::fixture_core();
    let media = crate::media::MediaRegistry::default();
    let standard = version.map(|version| StandardEmoji::build(version).unwrap());
    node(
        BlockCtx::new(&core.workspaces["T1"], &media).with_standard_emoji(standard.as_ref()),
        value,
    )
}

#[test]
fn selected_known_unicode_keeps_supplied_url_and_canonical_reverse_name() {
    let core = crate::fixture::fixture_core();
    let media = crate::media::MediaRegistry::default();
    let raw = json!({"type":"emoji","name":"smile","display_alias":"conflicting",
        "unicode":"1f44d-1f3fd","display_url":"https://fixture.invalid/combined.png"});
    let before = raw.clone();
    let expected = media.register_image(
        super_platinum_core::MediaAssetKind::Emoji,
        "https://fixture.invalid/combined.png",
        "image/png",
    );
    for version in [Version::V16, Version::V17] {
        let standard = StandardEmoji::build(version).unwrap();
        let result = node(
            BlockCtx::new(&core.workspaces["T1"], &media).with_standard_emoji(Some(&standard)),
            &raw,
        );
        assert_eq!(
            result,
            Some(RichNode::EmojiImage {
                id: expected.clone(),
                name: "+1::skin-tone-4".into()
            })
        );
    }
    assert_eq!(raw, before);
}

#[test]
fn selected_unknown_unicode_ignores_url_and_invalid_unicode_stays_empty() {
    for version in [Version::V16, Version::V17] {
        let unknown = json!({"type":"emoji","name":"smile","unicode":"0041",
            "display_url":"https://fixture.invalid/unused.png"});
        assert!(matches!(project(Some(version), &unknown),
            Some(RichNode::Emoji { glyph, .. }) if glyph == "A"));
        let invalid = json!({"type":"emoji","name":"smile","unicode":"1F44D",
            "display_url":"https://fixture.invalid/unused.png"});
        assert_eq!(project(Some(version), &invalid), None);
    }
}

#[test]
fn selected_catalog_version_changes_combined_orca_rendering() {
    let raw = json!({"type":"emoji","name":"conflicting","unicode":"1facd",
        "displayUrl":"https://fixture.invalid/orca.png"});
    assert!(matches!(project(Some(Version::V16), &raw),
        Some(RichNode::Emoji { glyph, .. }) if glyph == "\u{1facd}"));
    assert!(matches!(project(Some(Version::V17), &raw),
        Some(RichNode::EmojiImage { name, .. }) if name == "orca"));
}

#[test]
fn absent_observation_does_not_infer_catalog_membership() {
    let raw = json!({"type":"emoji","name":"thumbsup","unicode":"1f44d-1f3fd",
        "display_url":"https://fixture.invalid/unused.png"});
    assert!(matches!(project(None, &raw),
        Some(RichNode::Emoji { glyph, .. }) if glyph == "\u{1f44d}\u{1f3fd}"));
}

#[test]
fn combined_image_preserves_snake_presence_and_existing_url_policy() {
    for url in [Value::Null, json!("file:///untrusted.png"), json!("")] {
        let raw = json!({"type":"emoji","unicode":"1f44d","display_url":url,
            "displayUrl":"https://fixture.invalid/ignored.png"});
        assert!(matches!(project(Some(Version::V17), &raw),
            Some(RichNode::Emoji { glyph, .. }) if glyph == "\u{1f44d}"));
    }
}

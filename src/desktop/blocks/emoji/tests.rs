use super::*;
use crate::media::MediaRegistry;
use serde_json::json;

fn projected(value: Value) -> Vec<RichNode> {
    let core = crate::fixture::fixture_core();
    let media = MediaRegistry::default();
    super::super::block_nodes(BlockCtx::new(&core.workspaces["T1"], &media), &value)
}

fn glyph(value: Value, expected: &str) {
    let nodes = projected(value);
    assert!(
        matches!(nodes.as_slice(), [RichNode::Emoji { glyph, .. }] if glyph == expected),
        "{nodes:?}"
    );
}

#[test]
fn rich_emoji_initial_and_saved_unicode_keep_actual_tone() {
    glyph(
        json!({"type":"emoji","name":"thumbsup","skin_tone":4,"unicode":"1f44d-1f3fd"}),
        "\u{1f44d}\u{1f3fd}",
    );
    glyph(
        json!({"type":"emoji","name":"thumbsup","unicode":"1f44d-1f3fe"}),
        "\u{1f44d}\u{1f3fe}",
    );
}

#[test]
fn rich_emoji_unicode_wins_conflicting_name_and_tone() {
    glyph(
        json!({"type":"emoji","name":"smile","skin_tone":2,"unicode":"1f44d-1f3fe"}),
        "\u{1f44d}\u{1f3fe}",
    );
    glyph(
        json!({"type":"emoji","unicode":"1f9d1-200d-1f680"}),
        "\u{1f9d1}\u{200d}\u{1f680}",
    );
}

#[test]
fn rich_emoji_explicit_standard_tone_uses_existing_resolver() {
    glyph(
        json!({"type":"emoji","name":"thumbsup","skin_tone":4}),
        "\u{1f44d}\u{1f3fd}",
    );
    glyph(
        json!({"type":"emoji","name":"thumbsup","skinTone":5}),
        "\u{1f44d}\u{1f3fe}",
    );
    glyph(
        json!({"type":"emoji","name":"thumbsup","skin_tone":null,"skinTone":5}),
        "\u{1f44d}",
    );
}

#[test]
fn rich_emoji_invalid_positive_unicode_does_not_paint_base() {
    for unicode in [
        "1F44D",
        "1f44d-",
        " 1f44d",
        "110000",
        "d800",
        "xyz",
        "1f44d--1f3fd",
    ] {
        assert!(
            projected(json!({"type":"emoji","name":"thumbsup","unicode":unicode})).is_empty(),
            "{unicode}"
        );
    }
}

#[test]
fn rich_emoji_absent_unicode_and_unsupported_tone_preserve_base() {
    for tone in [
        json!(null),
        json!(0),
        json!(7),
        json!(false),
        json!({}),
        json!("nonsense"),
    ] {
        glyph(
            json!({"type":"emoji","name":"thumbsup","skin_tone":tone}),
            "\u{1f44d}",
        );
    }
    glyph(
        json!({"type":"emoji","name":"smile","unicode":""}),
        "\u{1f604}",
    );
    glyph(
        json!({"type":"emoji","name":"parrot_alias"}),
        ":parrot_alias:",
    );
}

#[test]
fn rich_emoji_projection_keeps_original_name_for_tooltip() {
    let raw = json!({"type":"emoji","name":"thumbsup","unicode":"1f44d-1f3fd","style":{"bold":true},"opaque":{"x":1}});
    let nodes = projected(raw);
    assert!(
        matches!(nodes.as_slice(), [RichNode::Emoji { name, glyph }] if name == "thumbsup" && glyph == "\u{1f44d}\u{1f3fd}")
    );
}

#[test]
fn rich_emoji_name_only_custom_media_path_is_preserved() {
    let mut core = crate::fixture::fixture_core();
    let workspace = core.workspaces.get_mut("T1").unwrap();
    workspace.replace_emojis(vec![super_platinum_core::slack::models::Emoji {
        name: "parrot_alias".into(),
        value: "https://example.test/parrot.png".into(),
        ..Default::default()
    }]);
    let media = MediaRegistry::default();
    let ctx = BlockCtx::new(workspace, &media);
    let raw = json!({"type":"emoji","name":"parrot_alias","skin_tone":4});
    assert_eq!(node(ctx, &raw), Some(emoji_node(ctx, "parrot_alias")));
    assert!(matches!(node(ctx, &raw), Some(RichNode::EmojiImage { .. })));
}

#[test]
fn rich_emoji_display_url_uses_original_alias_and_unslotted_media() {
    let core = crate::fixture::fixture_core();
    let media = MediaRegistry::default();
    let ctx = BlockCtx::new(&core.workspaces["T1"], &media);
    let url = "https://example.test/external.png";
    let raw = json!({"type":"emoji","name":"external-original","display_alias":"external-alias","display_url":url,"display_team_id":"TFOREIGN"});
    let expected =
        media.register_image(super_platinum_core::MediaAssetKind::Emoji, url, "image/png");
    assert_eq!(
        node(ctx, &raw),
        Some(RichNode::EmojiImage {
            id: expected,
            name: "external-alias".into()
        })
    );
    assert!(
        ctx.workspace
            .custom_emoji_url("external-original")
            .is_none()
    );
    assert!(ctx.workspace.custom_emoji_url("external-alias").is_none());
}

#[test]
fn rich_emoji_display_url_wins_standard_name_collision() {
    let nodes = projected(
        json!({"type":"emoji","name":"smile","displayAlias":"shared-smile","displayUrl":"https://example.test/smile.png"}),
    );
    assert!(
        matches!(nodes.as_slice(), [RichNode::EmojiImage { name, .. }] if name == "shared-smile")
    );
}

#[test]
fn rich_emoji_display_snake_presence_and_nullish_alias_are_exact() {
    glyph(
        json!({"type":"emoji","name":"smile","display_url":null,"displayUrl":"https://example.test/ignored.png"}),
        "\u{1f604}",
    );
    for (snake, expected) in [
        (json!(null), "smile"),
        (json!(""), ""),
        (json!("snake"), "snake"),
    ] {
        let nodes = projected(
            json!({"type":"emoji","name":"smile","display_url":"https://example.test/shared.png","display_alias":snake,"displayAlias":"ignored"}),
        );
        assert!(
            matches!(nodes.as_slice(), [RichNode::EmojiImage { name, .. }] if name == expected)
        );
    }
}

#[test]
fn rich_emoji_display_replacement_does_not_reuse_old_url_bytes() {
    let core = crate::fixture::fixture_core();
    let media = MediaRegistry::default();
    let ctx = BlockCtx::new(&core.workspaces["T1"], &media);
    let image = |url: &str| {
        let Some(RichNode::EmojiImage { id, .. }) = node(
            ctx,
            &json!({"type":"emoji","name":"shared","display_alias":"same-alias","display_url":url}),
        ) else {
            panic!("expected image")
        };
        id
    };
    let first = image("https://example.test/first.png");
    media.insert(first.clone(), "image/png", b"original bytes".as_slice());
    let second = image("https://example.test/second.png");
    assert_ne!(first, second);
    assert!(media.is_ready(&first));
    assert!(!media.is_ready(&second));
    assert_eq!(first, image("https://example.test/first.png"));
}

#[test]
fn rich_emoji_display_rejects_non_http_urls_without_changing_name_path() {
    for url in [
        "",
        "file:///tmp/emoji.png",
        "javascript:alert(1)",
        "data:image/png;base64,AA==",
        "//example.test/emoji.png",
    ] {
        glyph(
            json!({"type":"emoji","name":"smile","display_alias":"ignored","display_url":url}),
            "\u{1f604}",
        );
    }
}

#[test]
fn rich_emoji_display_preserves_unicode_and_no_url_behavior() {
    glyph(
        json!({"type":"emoji","name":"thumbsup","unicode":"1f44d-1f3fe","display_alias":"ignored","display_url":"https://example.test/ignored.png"}),
        "\u{1f44d}\u{1f3fe}",
    );
    assert!(projected(json!({"type":"emoji","name":"thumbsup","unicode":"invalid","display_url":"https://example.test/ignored.png"})).is_empty());
    glyph(
        json!({"type":"emoji","name":"smile","display_alias":"not-a-catalog-entry","display_team_id":"TFOREIGN"}),
        "\u{1f604}",
    );
}

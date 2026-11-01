use super::*;

#[test]
fn actual_typed_body_preserves_multiline_styles_lists_and_code_without_media_or_action_handles() {
    let mut shell = crate::state::ShellState::fixture(crate::media::MediaRegistry::default());
    crate::thread_feed::fixture(&mut shell, "thread-feed-rich-root");
    let workspace = &shell.core.workspaces[shell.core.active_team.as_ref().unwrap()];
    let mut message = shell.core.threads_view.items[0].root_msg.clone();
    message.files.push(serde_json::from_value(serde_json::json!({"id":"F00000001","title":"release.pdf","url_private":"https://fixture.invalid/private.pdf"})).unwrap());
    let result = body(workspace, &message);
    let debug = format!("{result:?}");
    for absent in [
        "Link {",
        "Button {",
        "Media {",
        "ImageBlock",
        "InlineImage",
        "EmojiImage",
        "FileMention",
        "Canvas(",
        "action_id",
        "fixture.invalid",
        "Fallback must not duplicate",
    ] {
        assert!(!debug.contains(absent), "{absent}");
    }
    for expected in [
        "bold: true",
        "List {",
        "Code(",
        "Quote(",
        "release.pdf",
        "Release diagram",
        "Review action",
    ] {
        assert!(debug.contains(expected), "{expected}");
    }
    assert_eq!(
        shell
            .media
            .registered_source_count("https://fixture.invalid/root.png"),
        0
    );
    assert_eq!(
        shell
            .media
            .registered_source_count("https://fixture.invalid/private.pdf"),
        0
    );
}

#[test]
fn fallback_and_nested_mentions_controls_and_custom_emoji_remain_text_only() {
    let mut shell = crate::state::ShellState::fixture(crate::media::MediaRegistry::default());
    let workspace = shell
        .core
        .workspaces
        .get_mut(shell.core.active_team.as_ref().unwrap())
        .unwrap();
    let message = serde_json::from_value(serde_json::json!({"text":"First paragraph\nSecond paragraph <https://fixture.invalid|notes> <@U1>","ts":"1719800400.000000"})).unwrap();
    let result = body(workspace, &message);
    assert_eq!(result.len(), 2);
    assert!(matches!(&result[0], RichNode::Paragraph(_)));
    assert!(matches!(&result[1], RichNode::Paragraph(_)));
    assert!(!format!("{result:?}").contains("Link {"));
    let id = MediaRegistry::default().register_image(
        MediaAssetKind::Attachment,
        "https://fixture.invalid/opaque.png",
        "image/png",
    );
    let nested = RichNode::Section {
        text: vec![RichNode::Underlined(Box::new(RichNode::Link {
            label: "Visible label".into(),
            url: "https://fixture.invalid/hidden".into(),
        }))],
        fields: vec![vec![
            RichNode::UserMention {
                user_id: "U1".into(),
                label: "Avery".into(),
            },
            RichNode::EmojiImage {
                id: id.clone(),
                name: "custom".into(),
            },
        ]],
        accessory: Some(Box::new(RichNode::InlineImage {
            id,
            alt: "Image label".into(),
        })),
    };
    let safe = text_only(nested, 0);
    for text in ["Visible label", "Avery", ":custom:", "Image label"] {
        assert!(safe.plain_text().contains(text));
    }
    let debug = format!("{safe:?}");
    for absent in [
        "fixture.invalid",
        "UserMention",
        "EmojiImage",
        "InlineImage",
        "Link {",
    ] {
        assert!(!debug.contains(absent));
    }
}

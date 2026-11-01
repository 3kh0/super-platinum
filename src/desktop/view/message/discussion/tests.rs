use super::*;
use crate::state::{ButtonStyle, CanvasTarget};
#[test]
fn nested_rich_actions_files_mentions_and_attachment_content_become_inert_without_losing_text_styles()
 {
    let metadata = Box::new(crate::state::BlockElementMetadata {
        source: super_platinum_core::block_actions::BlockElementSource::Button,
        block_id: Some("block".into()),
        action_id: Some("unsafe".into()),
        value: Some("opaque".into()),
        confirm: super_platinum_core::block_actions::BlockConfirmationState::Missing,
        accessibility_label: None,
    });
    let nodes = RichNode::Section {
        text: vec![RichNode::Paragraph(vec![
            RichNode::StyledText {
                text: "Keep bold".into(),
                bold: true,
                italic: false,
                strike: false,
                code: false,
            },
            RichNode::Link {
                label: "Read notes".into(),
                url: "https://untrusted.invalid".into(),
            },
        ])],
        fields: vec![vec![RichNode::List {
            ordered: true,
            indent: 1,
            offset: 0,
            items: vec![vec![
                RichNode::UserMention {
                    user_id: "U00000001".into(),
                    label: "Avery".into(),
                },
                RichNode::FileMention {
                    team: "T".into(),
                    raw: serde_json::json!({"file_id":"F00000001","opaque":"keep source outside action"}),
                    fallback: "launch.pdf".into(),
                },
            ]],
        }]],
        accessory: Some(Box::new(RichNode::Actions(vec![
            RichNode::Button {
                label: "Unsafe action".into(),
                url: Some("https://untrusted.invalid/action".into()),
                style: ButtonStyle::Primary,
                metadata,
            },
            RichNode::Canvas(CanvasTarget {
                id: "F00000001".into(),
                title: "Launch notes".into(),
                permalink: Some("https://untrusted.invalid/canvas".into()),
            }),
        ]))),
    };
    let safe = inert(&nodes, 0);
    let text = safe.plain_text();
    for value in [
        "Keep bold",
        "Read notes",
        "Avery",
        "launch.pdf",
        "Unsafe action",
        "Launch notes",
    ] {
        assert!(text.contains(value), "{value}");
    }
    let debug = format!("{safe:?}");
    for forbidden in [
        "untrusted.invalid",
        "FileMention",
        "Button {",
        "Canvas(",
        "UserMention",
        "file_id",
        "action_id",
    ] {
        assert!(!debug.contains(forbidden), "{forbidden}");
    }
    assert!(debug.contains("bold: true"));
}

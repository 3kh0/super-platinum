use super::*;
use crate::blocks::{BlockCtx, block_nodes};
use crate::media::MediaRegistry;
use serde_json::json;

fn project(raw: &Value) -> Vec<RichNode> {
    let core = crate::fixture::fixture_core();
    let media = MediaRegistry::default();
    block_nodes(BlockCtx::new(&core.workspaces["T1"], &media), raw)
}

#[test]
fn underline_preserves_unicode_companion_marks_and_newline_siblings() {
    let raw = json!({"type":"rich_text_section","elements":[
        {"type":"text","text":"A &<\u{4e2d}\u{1f642}\nB","style":{
            "underline":true,"bold":true,"italic":true,"strike":true,"code":true}},
        {"type":"text","text":" tail"}
    ]});
    let nodes = project(&raw);
    assert_eq!(nodes.len(), 2);
    for (index, expected) in ["A &<\u{4e2d}\u{1f642}", "B"].iter().enumerate() {
        let RichNode::Paragraph(children) = &nodes[index] else {
            panic!("missing paragraph")
        };
        let RichNode::Underlined(inner) = &children[0] else {
            panic!("underline dropped")
        };
        assert!(matches!(inner.as_ref(), RichNode::StyledText {
            text,bold:true,italic:true,strike:true,code:true
        } if text.as_str() == *expected));
    }
    assert_eq!(nodes[0].plain_text(), "A &<\u{4e2d}\u{1f642}");
    assert_eq!(nodes[1].plain_text(), "B tail");
    let RichNode::Paragraph(children) = &nodes[1] else {
        unreachable!()
    };
    assert_eq!(children[1], RichNode::Text(" tail".into()));
}

#[test]
fn underline_carries_original_link_and_typed_mention_identity_only_when_supplied() {
    for raw in [
        json!({"type":"link","url":"https://example.test/?a=1&b=2","text":"A <&","style":{"underline":true,"italic":true}}),
        json!({"type":"user","user_id":"U1","style":{"underline":true}}),
        json!({"type":"channel","channel_id":"C1","style":{"underline":true}}),
    ] {
        let mut plain = raw.clone();
        plain["style"].as_object_mut().unwrap().remove("underline");
        let baseline = project(&plain);
        let underlined = project(&raw);
        assert_eq!(underlined.len(), baseline.len());
        for (actual, original) in underlined.iter().zip(&baseline) {
            assert_eq!(actual, &RichNode::Underlined(Box::new(original.clone())));
            assert_eq!(actual.plain_text(), original.plain_text());
        }
        // An anchor-inner underline discarded by the source must not be
        // reconstructed from its label, URL, or typed user/channel identity.
        assert!(
            project(&plain)
                .iter()
                .all(|n| !matches!(n, RichNode::Underlined(_)))
        );
    }
}

#[test]
fn underline_requires_exact_boolean_not_truthy_or_unrelated_fields() {
    for value in [
        Value::Null,
        json!(false),
        json!(0),
        json!(1),
        json!("true"),
        json!([]),
        json!({}),
    ] {
        let raw = json!({"type":"text","text":"same","style":{"underline":value}});
        assert_eq!(project(&raw), vec![RichNode::Text("same".into())]);
    }
    assert_eq!(
        project(&json!({"type":"text","text":"<u>literal</u>","underline":true})),
        project(&json!({"type":"text","text":"<u>literal</u>"}))
    );
}

#[test]
fn underline_does_not_change_layout_or_file_occurrence_authority() {
    for raw in [
        json!({"type":"canvas","file_id":"FILE1","text":"wire","style":{"underline":true}}),
        json!({"type":"rich_text_section","style":{"underline":true},"elements":[{"type":"text","text":"same"}]}),
    ] {
        let nodes = project(&raw);
        assert!(
            nodes
                .iter()
                .all(|node| !matches!(node, RichNode::Underlined(_)))
        );
        if let Some(RichNode::FileMention { raw: retained, .. }) = nodes.first() {
            assert_eq!(retained, &raw);
        }
    }
}

#[test]
fn underline_text_entity_expansion_retains_channel_and_user_nodes() {
    let plain = json!({"type":"text","text":"Before <@U1> <#C1> after"});
    let mut raw = plain.clone();
    raw["style"] = json!({"underline":true});
    assert_eq!(
        project(&raw),
        project(&plain)
            .into_iter()
            .map(|n| RichNode::Underlined(Box::new(n)))
            .collect::<Vec<_>>()
    );
}

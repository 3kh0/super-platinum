use super::*;
use serde_json::json;

#[test]
fn thread_edited_plain_styled_profile_requires_actual_timestamp_and_exact_supported_paragraphs() {
    let shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "thread-feed-rich-root",
    );
    let workspace = &shell.core.workspaces["T1"];
    let base: Message = serde_json::from_value(json!({
        "type":"message","user":"U1","channel":"C1","ts":"1719800000.000001",
        "text":"fallback must not replace blocks","edited":{"ts":"1719800001.000001"},
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"First Ω\n"},{"type":"text","text":"second","style":{"bold":true}}
        ]}]}]
    }))
    .unwrap();
    let nodes = crate::message_vm::preview::body(workspace, &base);
    assert!(ordinary(&base));
    assert!(corresponds(&base, &nodes));
    assert_eq!(expected_paragraphs(&base).unwrap(), ["First Ω", "second"]);
    assert!(
        base.edited
            .as_ref()
            .unwrap()
            .get("ts")
            .and_then(Value::as_str)
            .is_some_and(timestamp)
    );
    for value in [
        "",
        "0.1",
        "12",
        "12.",
        "-12.1",
        "12.0000001",
        "12.x",
        "18446744073709551616.0",
    ] {
        assert!(!timestamp(value), "{value}");
    }
    for value in ["1.0", "1719800000.000001", "0001.1"] {
        assert!(timestamp(value));
    }
    for bot in ["B1", " "] {
        let mut message = base.clone();
        message.bot_id = Some(bot.into());
        assert!(!ordinary(&message));
    }
    let mut empty_bot = base.clone();
    empty_bot.bot_id = Some(String::new());
    assert!(ordinary(&empty_bot));
    for key in ["document_comment", "workflow_id", "ai_context"] {
        for value in [json!({}), json!([]), json!("ambiguous"), json!(true)] {
            let mut message = base.clone();
            message.extra.insert(key.into(), value);
            assert!(!ordinary(&message));
        }
        let mut null = base.clone();
        null.extra.insert(key.into(), Value::Null);
        assert!(ordinary(&null));
    }
    for subtype in ["tombstone", "bot_message", "system"] {
        let mut message = base.clone();
        message.subtype = Some(subtype.into());
        assert!(!ordinary(&message));
    }
    for blocks in [
        json!([]),
        json!([{"type":"section","text":{"type":"plain_text","text":"First Ω\nsecond"}}]),
        json!([{"type":"rich_text","elements":[{"type":"rich_text_preformatted","elements":[{"type":"text","text":"code"}]}]}]),
        json!([{"type":"rich_text","elements":[{"type":"rich_text_list","elements":[]}]}]),
        json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"emoji","name":"wave"}]}]}]),
        json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"trailing\n"}]}]}]),
        json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"CR\rLF"}]}]}]),
        json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"x","style":{"unknown":false}}]}]}]),
    ] {
        let mut message = base.clone();
        message.blocks = serde_json::from_value(blocks).unwrap();
        assert!(expected_paragraphs(&message).is_none());
        assert!(!corresponds(&message, &nodes));
    }
    let mut wrong = nodes.clone();
    wrong.push(RichNode::Paragraph(vec![RichNode::Text(
        "trailing content".into(),
    )]));
    assert!(!corresponds(&base, &wrong));
    let wrong = vec![
        RichNode::Paragraph(vec![RichNode::Text("same count wrong text".into())]),
        RichNode::Paragraph(vec![RichNode::Text("second".into())]),
    ];
    assert!(!corresponds(&base, &wrong));
    let mut large = base.clone();
    large.blocks[0]["elements"][0]["elements"][0]["text"] = json!("x".repeat(MAX_TEXT_BYTES + 1));
    assert!(expected_paragraphs(&large).is_none());
    // A valid-looking equal DTO is not a current visible feed row/reference.
    let item = ThreadViewItem {
        root_msg: base.clone(),
        ..Default::default()
    };
    assert!(!qualifies(&shell, &item, &item.root_msg, &nodes));
}

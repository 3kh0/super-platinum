use super::*;
use crate::FormatMark;

#[test]
fn whole_code_block_roundtrips_toolbar_wire_literal_content_and_exact_metadata() {
    for body in [
        "hello",
        "日本語 🚀 e\u{301}\n\t*literal* <@U1> :party: `tick`",
        "\nleading and trailing\n",
        " ",
    ] {
        let mut composer = ComposerState::with_text(body);
        composer.set_selection(0, body.len());
        composer.apply_format(FormatMark::CodeBlock);
        assert_eq!(composer.text, format!("```\n{body}\n```"));
        let snapshot = encode_composer(&key(), &composer).unwrap();
        assert_eq!(
            snapshot.blocks,
            vec![json!({"type":"rich_text","elements":[{
                "type":"rich_text_preformatted","elements":[{"type":"text","text":body}]
            }]})]
        );
        assert_roundtrip(&composer.text);
        let mut thread = key();
        thread.thread = Some("1719800000.000001".into());
        composer.reply_broadcast = true;
        let mut raw = encode_composer(&thread, &composer).unwrap();
        let projection = decode(&thread, &raw).unwrap();
        assert_eq!(projection.composer().text, composer.text);
        assert!(projection.composer().reply_broadcast);
        assert_eq!(projection.raw_snapshot(), &raw);
        let mut selection = projection.composer().clone();
        selection.set_selection(0, 0);
        assert_eq!(projection.snapshot_for(&selection).unwrap(), raw);
        raw.destinations[0]["broadcast"] = json!(false);
        let projection = decode(&thread, &raw).unwrap();
        assert!(!projection.composer().reply_broadcast);
        assert_eq!(projection.snapshot_for(projection.composer()).unwrap(), raw);
    }
    for wire in [
        "```\n\n```",
        "```inline```",
        " ```\nx\n```",
        "```rust\nx\n```",
        "before\n```\nx\n```",
        "```\nx\n```\nafter",
        "```\nx```y\n```",
        "```\nx\r\ny\n```",
        "```\nx\0y\n```",
    ] {
        assert!(encode(&key(), wire).is_err(), "{wire:?}");
    }
    let snapshot = encode(&key(), "```\none\n```").unwrap();
    let mut variants = Vec::new();
    for (field, value) in [("border", json!(0)), ("language", json!("rust"))] {
        let mut raw = snapshot.clone();
        raw.blocks[0]["elements"][0][field] = value;
        variants.push(raw);
    }
    for run in [
        json!({"type":"text","text":"one","style":{}}),
        json!({"type":"text","text":"one","unknown":false}),
        json!({"type":"link","url":"https://example.test"}),
        json!({"type":"text","text":""}),
    ] {
        let mut raw = snapshot.clone();
        raw.blocks[0]["elements"][0]["elements"] = json!([run]);
        variants.push(raw);
    }
    let mut split = snapshot.clone();
    split.blocks[0]["elements"][0]["elements"] =
        json!([{"type":"text","text":"o"},{"type":"text","text":"ne"}]);
    variants.push(split);
    for raw in variants {
        assert!(decode(&key(), &raw).is_err(), "{raw:?}");
    }
    assert_eq!(
        decode(
            &key(),
            &raw(json!([{"type":"text","text":"```\none\n```"}]))
        )
        .unwrap_err(),
        Unsupported::NotReversible,
        "a remote ordinary section cannot be reinterpreted as a code block"
    );
    let projection = decode(&key(), &snapshot).unwrap();
    let edited = ComposerState::with_text("```\nnew 日本語\n```");
    assert_eq!(
        projection.snapshot_for(&edited).unwrap(),
        encode_composer(&key(), &edited).unwrap()
    );
    assert_eq!(projection.raw_snapshot(), &snapshot);
}

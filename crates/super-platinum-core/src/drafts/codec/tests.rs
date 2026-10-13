use super::*;

fn key() -> DraftKey {
    DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    }
}
fn raw(runs: Value) -> Snapshot {
    Snapshot {
        blocks: vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":runs}]}),
        ],
        file_ids: vec![],
        destinations: vec![json!({"channel_id":"C1"})],
    }
}
fn assert_roundtrip(text: &str) {
    let snapshot = encode(&key(), text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
    let projected = decode(&key(), &snapshot).unwrap_or_else(|e| panic!("{text:?}: {e}"));
    assert_eq!(projected.composer().text, text);
    assert_eq!(projected.raw_snapshot(), &snapshot);
    assert_eq!(
        projected.snapshot_for(projected.composer()).unwrap(),
        snapshot
    );
    assert_eq!(
        encode(&key(), &projected.composer().text).unwrap(),
        snapshot
    );
}

#[test]
fn unicode_linebreaks_and_literal_unmatched_markers_roundtrip() {
    for text in [
        "",
        "hello",
        "🦊 café e\u{301} 日本語 مرحبا",
        "one\n\ntwo\n",
        "\n\n",
        "tab\there",
        "a * b",
        "literal _ end",
        "~unfinished",
        "single `",
        "1 < 2",
        "&amp; &lt; &gt;",
        "a_b",
        "https://example.test/path",
        "  spaces  ",
    ] {
        assert_roundtrip(text);
    }
}

#[test]
fn separate_known_styles_and_literal_code_roundtrip() {
    for text in [
        "*bold*",
        "_italic_",
        "~strike~",
        "`code`",
        "*a* _b_ ~c~ `d`",
        "你好 *世界*\n_next_\n",
        "*a*_b_",
        "*a**b*",
        "`*_~ <@U1> :party:`",
    ] {
        assert_roundtrip(text);
    }
    let snapshot = encode(&key(), "*b* _i_ ~s~ `c`").unwrap();
    assert_eq!(
        snapshot.blocks[0]["elements"][0]["elements"],
        json!([
            {"type":"text","text":"b","style":{"bold":true}}, {"type":"text","text":" "},
            {"type":"text","text":"i","style":{"italic":true}}, {"type":"text","text":" "},
            {"type":"text","text":"s","style":{"strike":true}}, {"type":"text","text":" "},
            {"type":"text","text":"c","style":{"code":true}}
        ])
    );
}

#[test]
fn local_unsupported_syntax_never_falls_back_to_plain_text() {
    for text in [
        "*_nested_*",
        "*nested `code`*",
        "**bold**",
        "\\*escape*",
        "> quote",
        "one\n> quote",
        "```code```",
        "  ```\nx\n```",
        "windows\r\n",
        "control\0",
        "*trailing *later*",
        "``",
    ] {
        assert!(
            encode(&key(), text).is_err(),
            "unexpectedly encoded {text:?}"
        );
    }
    for text in [
        "<@U1>",
        "<#C1|general>",
        "<!here>",
        "<!subteam^S1>",
        "<https://example.test|label>",
        "<mailto:a@example.test>",
        ":party:",
        "*:smile:*",
    ] {
        assert_eq!(encode(&key(), text), Err(Unsupported::Entity), "{text:?}");
    }
}

#[test]
fn literal_remote_markdown_is_not_reinterpreted_as_formatting() {
    for text in ["*literal*", "_literal_", "~literal~", "`literal`"] {
        assert_eq!(
            decode(&key(), &raw(json!([{"type":"text","text":text}]))).unwrap_err(),
            Unsupported::NotReversible
        );
    }
    for text in ["<@U1>", ":smile:"] {
        assert_eq!(
            decode(&key(), &raw(json!([{"type":"text","text":text}]))).unwrap_err(),
            Unsupported::Entity
        );
    }
}

#[test]
fn rejects_common_metadata_and_semantically_redundant_shapes() {
    let base = encode(&key(), "one").unwrap();
    let mut block_id = base.clone();
    block_id.blocks[0]["block_id"] = "B1".into();
    let mut section_meta = base.clone();
    section_meta.blocks[0]["elements"][0]["indent"] = 0.into();
    let mut run_meta = base.clone();
    run_meta.blocks[0]["elements"][0]["elements"][0]["verbatim"] = true.into();
    let empty_style = raw(json!([{"type":"text","text":"one","style":{}}]));
    let false_style = raw(json!([{"type":"text","text":"one","style":{"bold":false}}]));
    let combined = raw(json!([{"type":"text","text":"one","style":{"bold":true,"italic":true}}]));
    let unknown = raw(json!([{"type":"text","text":"one","style":{"highlight":true}}]));
    let split = raw(json!([{"type":"text","text":"o"},{"type":"text","text":"ne"}]));
    let empty_run = raw(json!([{"type":"text","text":""}]));
    let styled_newline = raw(json!([{"type":"text","text":"a\nb","style":{"bold":true}}]));
    for snapshot in [
        block_id,
        section_meta,
        run_meta,
        empty_style,
        false_style,
        combined,
        unknown,
        split,
        empty_run,
        styled_newline,
    ] {
        assert!(decode(&key(), &snapshot).is_err(), "{snapshot:?}");
    }
}

#[test]
fn rejects_unsupported_blocks_files_threads_and_multiple_destinations() {
    let base = encode(&key(), "one").unwrap();
    for block in [
        json!({"type":"section","text":{"type":"mrkdwn","text":"one"}}),
        json!({"type":"image","image_url":"https://example.test/a.png"}),
        json!({"type":"rich_text","elements":[{"type":"rich_text_list","elements":[]}]}),
        json!({"type":"rich_text","elements":[]}),
        json!({"type":"rich_text","elements":null}),
    ] {
        let mut snapshot = base.clone();
        snapshot.blocks = vec![block];
        assert_eq!(decode(&key(), &snapshot).unwrap_err(), Unsupported::Shape);
    }
    for run in [
        json!({"type":"user","user_id":"U1"}),
        json!({"type":"emoji","name":"smile"}),
        json!({"type":"link","url":"https://example.test"}),
        json!(null),
    ] {
        assert_eq!(
            decode(&key(), &raw(json!([run]))).unwrap_err(),
            Unsupported::Shape
        );
    }
    let mut files = base.clone();
    files.file_ids.push("F1".into());
    assert_eq!(decode(&key(), &files).unwrap_err(), Unsupported::Files);
    let mut multiple = base.clone();
    multiple.destinations.push(json!({"channel_id":"C2"}));
    let mut wrong = base.clone();
    wrong.destinations[0]["channel_id"] = "C2".into();
    let mut extra = base.clone();
    extra.destinations[0]["broadcast"] = false.into();
    for snapshot in [multiple, wrong, extra] {
        assert_eq!(
            decode(&key(), &snapshot).unwrap_err(),
            Unsupported::Destination
        );
    }
    let mut thread = key();
    thread.thread = Some("1.2".into());
    assert_eq!(
        encode(&thread, "text").unwrap().destinations,
        vec![json!({"channel_id":thread.channel,"thread_ts":"1.2"})]
    );
    assert_eq!(
        decode(&thread, &base).unwrap_err(),
        Unsupported::Destination
    );
}

#[test]
fn projection_retains_original_and_validates_edited_text() {
    let original = encode(&key(), "*original*").unwrap();
    let projection = decode(&key(), &original).unwrap();
    let mut composer = projection.composer().clone();
    composer.set_selection(0, 0);
    assert_eq!(projection.snapshot_for(&composer).unwrap(), original);
    composer.text = "_edited_".into();
    assert_eq!(
        projection.snapshot_for(&composer).unwrap(),
        encode(&key(), "_edited_").unwrap()
    );
    composer.text = "<@U1>".into();
    assert!(projection.snapshot_for(&composer).is_err());
    assert_eq!(projection.raw_snapshot(), &original);
}

#[test]
fn every_accepted_short_local_string_is_reversible() {
    let alphabet = ['a', ' ', '*', '_', '~', '`', 'é', '\n'];
    for length in 0..=4_u32 {
        for mut index in 0..alphabet.len().pow(length) {
            let mut text = String::new();
            for _ in 0..length {
                text.push(alphabet[index % alphabet.len()]);
                index /= alphabet.len();
            }
            if encode(&key(), &text).is_ok() {
                assert_roundtrip(&text);
            }
        }
    }
}

#[test]
fn thread_broadcast_projection_keeps_false_absence_and_true_without_crossing_channel() {
    let mut thread = key();
    thread.thread = Some("123.456".into());
    let mut composer = ComposerState::with_text("Reply");
    composer.reply_broadcast = true;
    let true_wire = encode_composer(&thread, &composer).unwrap();
    assert_eq!(true_wire.destinations[0]["broadcast"], true);
    assert!(
        decode(&thread, &true_wire)
            .unwrap()
            .composer()
            .reply_broadcast
    );
    assert!(encode_composer(&key(), &composer).is_err());
    let mut false_wire = true_wire.clone();
    false_wire.destinations[0]["broadcast"] = json!(false);
    let projected = decode(&thread, &false_wire).unwrap();
    assert!(!projected.composer().reply_broadcast);
    assert_eq!(
        projected.snapshot_for(projected.composer()).unwrap(),
        false_wire
    );
    let mut changed = projected.composer().clone();
    changed.reply_broadcast = true;
    assert_eq!(projected.snapshot_for(&changed).unwrap(), true_wire);
}

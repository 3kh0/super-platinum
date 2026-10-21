use super::*;
use serde_json::{Value, json};

fn render(fields: Value) -> (Message, MessageVm) {
    let mut raw = json!({
        "type":"message", "subtype":"document_comment_root", "ts":"1.0",
        "user":"U1", "document_comment":{"thread_id":"original-control"}
    });
    raw.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let message: Message = serde_json::from_value(raw).unwrap();
    let core = crate::fixture::fixture_core();
    let vm = message_vm(&core.workspaces["T1"], &message, &MediaRegistry::default());
    (message, vm)
}

fn text(vm: &MessageVm) -> String {
    vm.body
        .iter()
        .map(RichNode::plain_text)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn deferred_file_root_metadata_never_traverses_superseded_body_or_decoy_media() {
    let core = crate::fixture::fixture_core();
    let media = MediaRegistry::default();
    let raw = json!({"type":"message","subtype":"document_comment_root","ts":"1.0",
        "text":"Superseded","blocks":[{"type":"image","image_url":"https://example.test/block.png","alt_text":"decoy"}],
        "files":[{"id":"F12345678","name":"decoy","url_private":"https://example.test/file.png"}],
        "attachments":[{"text":"decoy","image_url":"https://example.test/attachment.png"}]});
    let message: Message = serde_json::from_value(raw).unwrap();
    let before = serde_json::to_value(&message).unwrap();
    let vm = metadata_vm(&core.workspaces["T1"], &message, &media);
    assert!(vm.body.is_empty());
    assert!(vm.attachments.is_empty());
    assert_eq!(vm.ts, "1.0");
    assert!(!media.has_pending());
    assert_eq!(serde_json::to_value(&message).unwrap(), before);
}

#[test]
fn root_blocks_win_without_changing_thread_or_control_identity() {
    let (message, vm) = render(json!({
        "text":"do not show this fallback", "markdown_text":"nor this",
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"Rich & \u{4e2d}\u{1f98a}","style":{"bold":true,"underline":true}},
            {"type":"user","user_id":"U1","style":{"underline":true}}
        ]}]}]
    }));
    assert!(text(&vm).starts_with("Rich & \u{4e2d}\u{1f98a}"));
    assert!(!text(&vm).contains("fallback"));
    let RichNode::Paragraph(children) = &vm.body[0] else {
        panic!("typed paragraph")
    };
    assert!(matches!(&children[0], RichNode::Underlined(node)
        if matches!(node.as_ref(), RichNode::StyledText { bold:true, .. })));
    assert!(matches!(&children[1], RichNode::Underlined(node)
        if matches!(node.as_ref(), RichNode::UserMention { user_id, .. } if user_id=="U1")));
    assert_eq!(vm.ts, "1.0");
    assert_eq!(
        message.extra["document_comment"]["thread_id"],
        "original-control"
    );
    assert_eq!(message.text.as_deref(), Some("do not show this fallback"));
}

#[test]
fn fallback_formats_text_and_uses_markdown_text_only_when_text_empty() {
    let (_, vm) = render(json!({"text":"*Bold* _italic_", "markdown_text":"wrong"}));
    assert_eq!(text(&vm), "Bold italic");
    let RichNode::Paragraph(children) = &vm.body[0] else {
        panic!("paragraph")
    };
    assert!(
        children
            .iter()
            .any(|node| matches!(node, RichNode::StyledText { bold: true, .. }))
    );
    assert!(
        children
            .iter()
            .any(|node| matches!(node, RichNode::StyledText { italic: true, .. }))
    );
    let (_, fallback) = render(json!({"text":"", "markdown_text":"_Fallback_"}));
    assert_eq!(text(&fallback), "Fallback");
}

#[test]
fn unsupported_blocks_never_restore_superseded_text() {
    let (_, vm) = render(json!({"text":"superseded", "blocks":[{"type":"unknown_future"}]}));
    assert!(vm.body.is_empty());
}

#[test]
fn ordinary_replies_keep_existing_projection() {
    let (message, vm) = render(json!({"subtype":null,"thread_ts":"0.9","text":"*ordinary reply*"}));
    let core = crate::fixture::fixture_core();
    let original = crate::message_vm::message_vm_with_pending(
        &core.workspaces["T1"],
        &message,
        &MediaRegistry::default(),
        false,
    );
    assert_eq!(vm.body, original.body);
}

#[test]
fn trailing_space_remains_inside_the_formatted_root_span() {
    let (_, vm) = render(json!({"text":"_New &\u{1f98a} _"}));
    let RichNode::Paragraph(children) = &vm.body[0] else {
        panic!("paragraph")
    };
    assert!(
        matches!(&children[0], RichNode::StyledText { text, italic:true, .. }
        if text == "New &\u{1f98a} ")
    );
}

#[test]
fn no_specials_keeps_tokens_but_never_reparses_decoded_text() {
    let (_, vm) = render(json!({
        "text":"*literal* _marks_ &amp;lt; &lt;@U1&gt; <@U1> :wave:", "mrkdwn":false
    }));
    let RichNode::Paragraph(children) = &vm.body[0] else {
        panic!("paragraph")
    };
    assert_eq!(
        children
            .iter()
            .filter(|n| matches!(n, RichNode::UserMention { .. }))
            .count(),
        1
    );
    assert!(children.iter().any(|n| matches!(n, RichNode::Emoji { .. })));
    assert!(
        !children
            .iter()
            .any(|n| matches!(n, RichNode::StyledText { .. }))
    );
    assert!(text(&vm).starts_with("*literal* _marks_ &lt; <@U1> "));
}

#[test]
fn entity_decoding_keeps_link_identity_and_hidden_body_registers_no_media() {
    let (_, vm) = render(json!({"text":"<https://example.test/?a=1&amp;b=2|A &amp; B>"}));
    let RichNode::Paragraph(children) = &vm.body[0] else {
        panic!("paragraph")
    };
    assert!(matches!(&children[0], RichNode::Link {label,url}
        if label=="A & B" && url=="https://example.test/?a=1&amp;b=2"));
    let (message, vm) = render(json!({
        "document_comment":{"thread_id":"original-control","is_visible":false},
        "text":"hidden text", "blocks":[{"type":"image", "image_url":"https://example.test/hidden.png", "alt_text":"hidden"}]
    }));
    assert_eq!(text(&vm), "This content was deleted.");
    let core = crate::fixture::fixture_core();
    let media = MediaRegistry::default();
    message_vm(&core.workspaces["T1"], &message, &media);
    assert!(!media.has_pending());
}

fn special_root(visible: Option<Value>) -> Message {
    let mut raw = json!({
        "type":"message","subtype":"document_comment_root","ts":"1.0","thread_ts":"1.0","user":"U1",
        "document_comment":{"thread_id":"original-control","opaque":{"keep":true}},
        "text":"hidden fallback","markdown_text":"hidden markdown fallback",
        "blocks":[{"type":"image","image_url":"https://example.test/block.png","alt_text":"concealed block"}],
        "files":[{"id":"F123","name":"concealed file","filetype":"png","mimetype":"image/png","url_private":"https://example.test/file.png","thumb_360":"https://example.test/thumb.png"}],
        "attachments":[{"text":"concealed attachment","image_url":"https://example.test/attachment.png","image_width":100,"image_height":100}]
    });
    if let Some(visible) = visible {
        raw["document_comment"]["is_visible"] = visible;
    }
    serde_json::from_value(raw).unwrap()
}

#[test]
fn hidden_file_blocks_and_attachments_never_register_media_or_change_root_custody() {
    let core = crate::fixture::fixture_core();
    for files in [true, false] {
        let mut message = special_root(Some(json!(false)));
        if !files {
            message.files.clear();
        }
        let before = serde_json::to_value(&message).unwrap();
        let media = MediaRegistry::default();
        let vm = message_vm(&core.workspaces["T1"], &message, &media);
        assert_eq!(
            vm.body,
            vec![RichNode::Text("This content was deleted.".into())]
        );
        assert!(vm.attachments.is_empty());
        assert!(!media.has_pending());
        for url in ["block.png", "file.png", "thumb.png", "attachment.png"] {
            assert_eq!(
                media.registered_source_count(&format!("https://example.test/{url}")),
                0
            );
        }
        assert_eq!(vm.ts, "1.0");
        assert_eq!(
            message.extra["document_comment"]["thread_id"],
            "original-control"
        );
        assert_eq!(serde_json::to_value(&message).unwrap(), before);
    }
}

#[test]
fn visible_file_root_keeps_selected_block_media_but_never_appends_file_or_attachment_media() {
    let core = crate::fixture::fixture_core();
    for visible in [
        None,
        Some(Value::Null),
        Some(json!(true)),
        Some(json!(0)),
        Some(json!("false")),
    ] {
        let message = special_root(visible);
        let before = serde_json::to_value(&message).unwrap();
        let media = MediaRegistry::default();
        let vm = message_vm(&core.workspaces["T1"], &message, &media);
        assert!(media.registered_source_count("https://example.test/block.png") > 0);
        for name in ["file.png", "thumb.png", "attachment.png"] {
            assert_eq!(
                media.registered_source_count(&format!("https://example.test/{name}")),
                0
            );
        }
        let mut fallback = message.clone();
        fallback.files.clear();
        fallback.attachments.clear();
        let expected = crate::message_vm::message_vm_with_pending(
            &core.workspaces["T1"],
            &fallback,
            &media,
            false,
        );
        assert_eq!(vm.body, expected.body);
        assert_eq!(vm.attachments, expected.attachments);
        assert!(vm.attachments.is_empty());
        assert!(
            media.has_pending(),
            "original selected block media is still supported"
        );
        assert_eq!(serde_json::to_value(&message).unwrap(), before);
    }
}

fn fallback_root(fields: Value, multiple: bool) -> Message {
    let mut message = special_root(Some(json!(true)));
    message.blocks.clear();
    let mut raw = serde_json::to_value(message).unwrap();
    if multiple {
        raw["files"].as_array_mut().unwrap().push(json!({
            "id":"FSECOND","name":"second file decoy","title":"second title decoy",
            "filetype":"quip","permalink":"https://example.test/second"
        }));
    }
    raw.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    serde_json::from_value(raw).unwrap()
}

fn project_fallback(message: &Message) -> MessageVm {
    let core = crate::fixture::fixture_core();
    let original = serde_json::to_value(message).unwrap();
    let media = MediaRegistry::default();
    let vm = message_vm(&core.workspaces["T1"], message, &media);
    assert_eq!(serde_json::to_value(message).unwrap(), original);
    assert_eq!(vm.ts, "1.0");
    assert_eq!(
        message.extra["document_comment"]["thread_id"],
        "original-control"
    );
    assert!(vm.attachments.is_empty());
    for name in ["file.png", "thumb.png", "attachment.png"] {
        assert_eq!(
            media.registered_source_count(&format!("https://example.test/{name}")),
            0
        );
    }
    assert!(!media.has_pending());
    vm
}

#[test]
fn first_and_multiple_embedded_files_do_not_override_original_rich_blocks() {
    for multiple in [false, true] {
        let message = fallback_root(
            json!({
                "text":"superseded text","markdown_text":"superseded markdown",
                "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                    {"type":"text","text":"Original & 中🦊","style":{"bold":true,"underline":true}}
                ]}]}]
            }),
            multiple,
        );
        let vm = project_fallback(&message);
        assert_eq!(text(&vm), "Original & 中🦊");
        let RichNode::Paragraph(children) = &vm.body[0] else {
            panic!("paragraph")
        };
        assert!(
            matches!(&children[0],RichNode::Underlined(node) if matches!(node.as_ref(),RichNode::StyledText{bold:true,..}))
        );
        assert_eq!(vm.body.len(), 1, "no appended embedded-file card");
    }
}

#[test]
fn file_root_nonempty_unsupported_blocks_never_disclose_other_fallbacks() {
    let message = fallback_root(
        json!({
            "blocks":[{"type":"unsupported_future"}],"text":"superseded text",
            "markdown_text":"superseded markdown"
        }),
        true,
    );
    assert!(project_fallback(&message).body.is_empty());
}

#[test]
fn file_root_fallback_uses_text_then_markdown_and_preserves_literal_typed_content() {
    for multiple in [false, true] {
        let vm = project_fallback(&fallback_root(
            json!({
                "text":"*Top* &amp; 中🦊","markdown_text":"wrong"
            }),
            multiple,
        ));
        assert_eq!(text(&vm), "Top & 中🦊");
        let RichNode::Paragraph(children) = &vm.body[0] else {
            panic!("paragraph")
        };
        assert!(
            children
                .iter()
                .any(|node| matches!(node, RichNode::StyledText { bold: true, .. }))
        );
        let vm = project_fallback(&fallback_root(
            json!({
                "text":"","markdown_text":"_Fallback_ &amp; 中🦊"
            }),
            multiple,
        ));
        assert_eq!(text(&vm), "Fallback & 中🦊");
        let vm = project_fallback(&fallback_root(
            json!({
                "text":"*literal* &amp; <@U1> :wave:","mrkdwn":false
            }),
            multiple,
        ));
        let RichNode::Paragraph(children) = &vm.body[0] else {
            panic!("paragraph")
        };
        assert!(text(&vm).starts_with("*literal* & "));
        assert!(
            children
                .iter()
                .any(|node| matches!(node, RichNode::UserMention { .. }))
        );
        assert!(
            children
                .iter()
                .any(|node| matches!(node, RichNode::Emoji { .. }))
        );
        assert!(
            !children
                .iter()
                .any(|node| matches!(node, RichNode::StyledText { .. }))
        );
    }
}

#[test]
fn malformed_file_root_fallback_is_empty_without_decoy_media_or_text() {
    for malformed in [
        json!({"text":"decoy"}),
        json!(["decoy"]),
        json!(42),
        json!(true),
    ] {
        let message = fallback_root(json!({"text":null,"markdown_text":malformed}), true);
        assert!(project_fallback(&message).body.is_empty());
    }
}

#[test]
fn nonroot_files_and_fileless_root_attachments_keep_existing_behavior() {
    let core = crate::fixture::fixture_core();
    let mut reply = special_root(Some(json!(false)));
    reply.subtype = None;
    reply.thread_ts = Some("0.9".into());
    let media = MediaRegistry::default();
    let before = serde_json::to_value(&reply).unwrap();
    let vm = message_vm(&core.workspaces["T1"], &reply, &media);
    let expected =
        crate::message_vm::message_vm_with_pending(&core.workspaces["T1"], &reply, &media, false);
    assert_eq!(vm.body, expected.body);
    assert_eq!(vm.attachments, expected.attachments);
    assert!(media.registered_source_count("https://example.test/file.png") > 0);
    assert_eq!(serde_json::to_value(reply).unwrap(), before);
    let mut root = special_root(Some(json!(true)));
    root.files.clear();
    let media = MediaRegistry::default();
    let vm = message_vm(&core.workspaces["T1"], &root, &media);
    assert!(!vm.attachments.is_empty());
    assert!(media.registered_source_count("https://example.test/attachment.png") > 0);
}

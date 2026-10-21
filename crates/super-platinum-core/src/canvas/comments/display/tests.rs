use super::*;
use serde_json::json;

fn raw() -> Value {
    json!({"type":"message","subtype":"document_comment_root","channel":"C123",
        "ts":"10.000001","thread_ts":"10.000001","text":"top text",
        "document_comment":{"thread_id":"quip-thread","opaque":{"retained":true}}})
}
fn message(value: Value) -> Message {
    serde_json::from_value(value).unwrap()
}

#[test]
fn only_exact_false_hides_and_does_not_mutate_original() {
    for visible in [
        Value::Null,
        json!(true),
        json!(0),
        json!("false"),
        json!({}),
    ] {
        let mut value = raw();
        value["document_comment"]["is_visible"] = visible;
        assert!(matches!(
            root_body(&message(value)),
            Some(RootBody::Text {
                text: "top text",
                ..
            })
        ));
    }
    let mut value = raw();
    value["document_comment"]["is_visible"] = json!(false);
    value["blocks"] = json!([{"type":"rich_text","elements":[]}]);
    let m = message(value);
    let before = serde_json::to_value(&m).unwrap();
    assert_eq!(root_body(&m), Some(RootBody::Hidden));
    assert_eq!(serde_json::to_value(&m).unwrap(), before);
}
#[test]
fn nonempty_top_level_blocks_win_without_renderability_fallback() {
    for blocks in [
        json!([null]),
        json!([{"type":"rich_text","elements":[]}]),
        json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"under","style":{"underline":true}}]}]}]),
    ] {
        let mut value = raw();
        value["blocks"] = blocks;
        value["markdown_text"] = json!("other fallback");
        let m = message(value);
        let Some(RootBody::Blocks(actual)) = root_body(&m) else {
            panic!("original blocks must win")
        };
        assert!(std::ptr::eq(actual, m.blocks.as_slice()));
    }
}
#[test]
fn empty_blocks_use_top_text_then_markdown_text_without_trimming() {
    for text in [Some("top text"), Some(" "), Some(""), None] {
        let mut value = raw();
        value["blocks"] = json!([]);
        value["markdown_text"] = json!("_New &fox _");
        if let Some(text) = text {
            value["text"] = json!(text);
        } else {
            value.as_object_mut().unwrap().remove("text");
        }
        let m = message(value);
        let expected = text.filter(|s| !s.is_empty()).unwrap_or("_New &fox _");
        assert_eq!(
            root_body(&m),
            Some(RootBody::Text {
                text: expected,
                mrkdwn: true
            })
        );
    }
}
#[test]
fn only_false_disables_markdown_and_descriptor_keeps_special_text_literal() {
    for flag in [json!(true), Value::Null, json!(0), json!("false")] {
        let mut value = raw();
        value["mrkdwn"] = flag;
        assert!(matches!(
            root_body(&message(value)),
            Some(RootBody::Text { mrkdwn: true, .. })
        ));
    }
    let mut value = raw();
    value["mrkdwn"] = json!(false);
    value["text"] = json!("*literal* &amp; <@U1> <#C1> :wave:");
    let m = message(value);
    assert_eq!(
        root_body(&m),
        Some(RootBody::Text {
            text: "*literal* &amp; <@U1> <#C1> :wave:",
            mrkdwn: false
        })
    );
}
#[test]
fn nested_request_snippet_fields_are_never_response_body_authority() {
    let mut value = raw();
    value["text"] = Value::Null;
    value["document_comment"]["rich_text"] = json!({"blocks":[{"type":"rich_text"}]});
    value["document_comment"]["richText"] = json!("forged snippet");
    value["document_comment"]["mrkdwn"] = json!("forged fallback");
    assert_eq!(
        root_body(&message(value)),
        Some(RootBody::Text {
            text: "",
            mrkdwn: true
        })
    );
}
#[test]
fn nonroots_and_invalid_thread_identity_keep_existing_path() {
    for mode in 0..4 {
        let mut value = raw();
        match mode {
            0 => value["subtype"] = json!("document_comment"),
            1 => value["thread_ts"] = json!("10.000002"),
            2 => value["document_comment"]["thread_id"] = json!(""),
            _ => value["document_comment"] = Value::Null,
        }
        assert_eq!(root_body(&message(value)), None);
    }
}
#[test]
fn absent_thread_ts_is_existing_root_and_malformed_fallback_is_not_coerced() {
    let mut value = raw();
    value.as_object_mut().unwrap().remove("thread_ts");
    assert!(root_body(&message(value.clone())).is_some());
    value["text"] = json!("");
    value["markdown_text"] = json!({"text":"not a string"});
    assert_eq!(root_body(&message(value)), None);
}
#[test]
fn parsed_read_root_and_identity_remain_owned_by_original_page() {
    let value = json!({"ok":true,"messages":[raw()],"has_more":true,"response_metadata":{"next_cursor":"next"}});
    let page = super::super::ReplyPage::parse("F123", "10.000001", true, &value).unwrap();
    let before = serde_json::to_value(&page.messages[0]).unwrap();
    let Some(RootBody::Text { text, .. }) = root_body(&page.messages[0]) else {
        panic!("ordinary root")
    };
    assert!(std::ptr::eq(
        text,
        page.messages[0].text.as_deref().unwrap()
    ));
    assert_eq!(page.cursor.as_deref(), Some("next"));
    assert_eq!(serde_json::to_value(&page.messages[0]).unwrap(), before);
}

#[test]
fn hidden_file_root_precedes_all_body_sources_without_changing_original_read() {
    let mut value = raw();
    value["document_comment"]["is_visible"] = json!(false);
    value["files"] =
        json!([{"id":"F123","filetype":"png","url_private":"https://example.test/hidden.png"}]);
    value["blocks"] = json!([{"type":"image","image_url":"https://example.test/block.png"}]);
    value["attachments"] =
        json!([{"text":"hidden attachment","image_url":"https://example.test/attachment.png"}]);
    let response = json!({"ok":true,"messages":[value],"has_more":true,"response_metadata":{"next_cursor":"next"}});
    let page = super::super::ReplyPage::parse("F123", "10.000001", true, &response).unwrap();
    let before = serde_json::to_value(&page.messages[0]).unwrap();
    assert_eq!(root_body(&page.messages[0]), Some(RootBody::Hidden));
    assert_eq!(page.cursor.as_deref(), Some("next"));
    assert_eq!(serde_json::to_value(&page.messages[0]).unwrap(), before);
}

#[test]
fn nonhidden_file_roots_use_original_fallback_and_nonroots_are_not_hidden() {
    for visible in [
        None,
        Some(Value::Null),
        Some(json!(true)),
        Some(json!(0)),
        Some(json!("false")),
    ] {
        let mut value = raw();
        value["files"] = json!([{"id":"F123","filetype":"quip"}]);
        if let Some(visible) = visible {
            value["document_comment"]["is_visible"] = visible;
        }
        assert_eq!(
            root_body(&message(value)),
            Some(RootBody::Text {
                text: "top text",
                mrkdwn: true
            })
        );
    }
    let mut value = raw();
    value["subtype"] = json!("document_comment");
    value["document_comment"]["is_visible"] = json!(false);
    value["files"] = json!([{"id":"F123"}]);
    assert_eq!(root_body(&message(value)), None);
}

#[test]
fn first_or_later_embedded_file_metadata_never_replaces_original_blocks() {
    for files in [
        json!([{"id":"F123","name":"first","permalink":"https://example.test/first"}]),
        json!([{},{"id":"F456","name":"later","permalink":"https://example.test/later"}]),
    ] {
        let mut value = raw();
        value["files"] = files;
        value["blocks"] = json!([{"type":"unsupported","block_id":"original"}]);
        let m = message(value);
        let before = serde_json::to_value(&m).unwrap();
        let Some(RootBody::Blocks(blocks)) = root_body(&m) else {
            panic!("original blocks")
        };
        assert!(std::ptr::eq(blocks, m.blocks.as_slice()));
        assert_eq!(serde_json::to_value(&m).unwrap(), before);
    }
}

#[test]
fn file_root_text_fallback_preserves_precedence_whitespace_and_literal_flag() {
    for text in [Some(" *top* "), Some(" "), Some(""), None] {
        for flag in [json!(false), json!("false"), Value::Null] {
            let mut value = raw();
            value["files"] = json!([{"id":"F123"},{"id":"F456"}]);
            value["text"] = text.map_or(Value::Null, |text| json!(text));
            value["markdown_text"] = json!("_fallback_ &amp;");
            value["mrkdwn"] = flag.clone();
            let m = message(value);
            assert_eq!(
                root_body(&m),
                Some(RootBody::Text {
                    text: text.filter(|s| !s.is_empty()).unwrap_or("_fallback_ &amp;"),
                    mrkdwn: flag != Value::Bool(false)
                })
            );
        }
    }
}

#[test]
fn malformed_file_fallback_is_unavailable_without_coercion_or_generic_traversal() {
    for fallback in [
        json!({ "text":"decoy" }),
        json!(["decoy"]),
        json!(1),
        json!(true),
    ] {
        let mut value = raw();
        value["files"] = json!([{"id":"F123","title":"embedded decoy"}]);
        value["text"] = Value::Null;
        value["markdown_text"] = fallback;
        let m = message(value.clone());
        let before = serde_json::to_value(&m).unwrap();
        assert_eq!(root_body(&m), Some(RootBody::Unavailable));
        assert_eq!(serde_json::to_value(&m).unwrap(), before);
        value["blocks"] = json!([{"type":"unsupported"}]);
        assert!(matches!(
            root_body(&message(value)),
            Some(RootBody::Blocks(_))
        ));
    }
}

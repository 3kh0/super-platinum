use super::*;
use crate::slack::{events::RtEvent, realtime::parse_event};
use serde_json::{Value, json};

fn original(value: Value, subtype: &str) -> (String, SlackMessage, MessageUpdate) {
    match parse_event(
        &json!({"type":"message","subtype":subtype,"channel":"C1","message":value}).to_string(),
    )
    .unwrap()
    {
        RtEvent::MessageChanged {
            channel,
            message,
            original_update: Some(original),
        } => (channel, message, original),
        _ => panic!("actual parser evidence required"),
    }
}
fn stored() -> SlackMessage {
    serde_json::from_value(json!({"ts":"1719800400.000000","text":"Old fallback","blocks":[{"type":"section","text":{"type":"plain_text","text":"Old body"}}],"attachments":[{"text":"Old attachment"}],"reply_count":3,"reactions":[{"name":"eyes","count":1,"users":["U1"]}]})).unwrap()
}

#[test]
fn original_explicit_empty_arrays_remove_old_body_and_attachments_while_omitted_reactions_and_counts_survive()
 {
    let mut bag = ChannelMessages::default();
    bag.upsert(stored());
    let (channel, message, original) = original(
        json!({"ts":"1719800400.000000","text":"New plain body","blocks":[],"attachments":[]}),
        "message_changed",
    );
    assert!(!bag.merge_original_update(&channel, message, Some(&original)));
    let result = &bag.messages[0];
    assert_eq!(result.text.as_deref(), Some("New plain body"));
    assert!(result.blocks.is_empty() && result.attachments.is_empty());
    assert_eq!(result.reply_count, Some(3));
    assert_eq!(result.reactions[0].name, "eyes");
    assert_eq!(result.reactions[0].count, 1);
    assert_eq!(bag.messages.len(), 1);
}

#[test]
fn omission_legacy_replied_conflicting_context_and_delivered_identity_never_certify_clearing() {
    for case in [
        "omitted",
        "legacy",
        "replied",
        "nested-conflict",
        "nested-null",
        "typed-ts",
        "typed-channel",
        "stored-channel",
    ] {
        let mut value = json!({"ts":"1719800400.000000","text":"Updated fallback","blocks":[],"attachments":[]});
        if case == "omitted" {
            value.as_object_mut().unwrap().remove("blocks");
            value.as_object_mut().unwrap().remove("attachments");
        }
        if case == "nested-conflict" {
            value["channel"] = json!("C_OTHER");
        }
        if case == "nested-null" {
            value["channel"] = Value::Null;
        }
        let (channel, mut message, original) = original(
            value,
            if case == "replied" {
                "message_replied"
            } else {
                "message_changed"
            },
        );
        let mut existing = stored();
        if case == "typed-ts" {
            message.ts = Some("1719800401.000000".into());
        }
        if case == "typed-channel" {
            message.channel = Some("C_OTHER".into());
        }
        if case == "stored-channel" {
            existing.channel = Some("C_OTHER".into());
        }
        merge(
            &mut existing,
            &channel,
            message,
            if case == "legacy" {
                None
            } else {
                Some(&original)
            },
        );
        assert!(
            !existing.blocks.is_empty() && !existing.attachments.is_empty(),
            "{case}"
        );
        assert_eq!(existing.reactions[0].name, "eyes", "{case}");
    }
}

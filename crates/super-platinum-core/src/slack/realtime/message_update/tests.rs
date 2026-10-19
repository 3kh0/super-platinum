use super::*;
use crate::slack::{events::RtEvent, realtime::parse_event};
use serde_json::json;

fn parsed(frame: Value) -> (String, Message, MessageUpdate) {
    match parse_event(&frame.to_string()).expect("existing typed frame must parse") {
        RtEvent::MessageChanged {
            channel,
            message,
            original_update: Some(original),
        } => (channel, message, original),
        other => panic!("expected original update evidence, got {other:?}"),
    }
}

fn frame(subtype: &str, nested: Value) -> Value {
    json!({"type":"message","subtype":subtype,"channel":"C1","message":nested})
}

#[test]
fn original_absent_null_and_empty_members_survive_typed_defaults() {
    let absent = json!({"ts":"1.2"});
    let (_, absent_message, absent_update) = parsed(frame("message_changed", absent.clone()));
    let explicit = json!({
        "ts":"1.2","text":null,"edited":null,"blocks":[],"files":[],
        "attachments":[],"reactions":[],"reply_users":[],
        "opaque":{"kept":null,"array":[],"object":{}}
    });
    let (_, explicit_message, explicit_update) = parsed(frame("message_changed", explicit.clone()));
    assert_eq!(absent_message.text, explicit_message.text);
    assert!(absent_message.blocks.is_empty() && explicit_message.blocks.is_empty());
    assert_eq!(absent_update.raw(), &absent);
    assert_eq!(explicit_update.raw(), &explicit);
    assert!(absent_update.raw().get("text").is_none());
    assert_eq!(explicit_update.raw().get("text"), Some(&Value::Null));
    for key in ["blocks", "files", "attachments", "reactions", "reply_users"] {
        assert!(absent_update.raw().get(key).is_none());
        assert_eq!(explicit_update.raw()[key], json!([]));
    }
    assert!(absent_update.raw().get("channel").is_none());
    assert_eq!(absent_update.typed_channel(), Some("C1"));
}

#[test]
fn changed_and_replied_keep_original_kind_with_same_legacy_delivery() {
    for (subtype, kind) in [
        ("message_changed", MessageUpdateKind::Changed),
        ("message_replied", MessageUpdateKind::Replied),
    ] {
        let nested = json!({"ts":"1.2","thread_ts":"1.0","text":"same","user":"U1"});
        let (channel, message, original) = parsed(frame(subtype, nested.clone()));
        assert_eq!(original.kind(), kind);
        assert_eq!(original.raw(), &nested);
        assert_eq!(original.outer_channel(), "C1");
        assert_eq!(original.typed_ts(), Some("1.2"));
        assert_eq!(original.typed_channel(), Some("C1"));
        assert!(original.matches_identity(&channel, &message));
        assert_eq!(message.text.as_deref(), Some("same"));
        assert_eq!(message.subtype, None);
    }
}

#[test]
fn null_arrays_and_malformed_typed_fields_still_refuse_whole_frame() {
    for key in ["blocks", "files", "attachments", "reactions", "reply_users"] {
        for malformed in [Value::Null, json!({}), json!("not an array")] {
            let mut nested = json!({"ts":"1.2"});
            nested[key] = malformed;
            for subtype in ["message_changed", "message_replied"] {
                assert!(parse_event(&frame(subtype, nested.clone()).to_string()).is_none());
            }
        }
    }
    assert!(
        parse_event(&frame("message_changed", json!({"ts":"1.2","text":42})).to_string()).is_none()
    );
}

#[test]
fn original_nested_channel_conflict_and_null_are_not_erased_by_fallback() {
    for nested_channel in [Value::Null, json!(""), json!("C_OTHER"), json!("C1")] {
        let nested = json!({"ts":"1.2","channel":nested_channel});
        let (outer, message, original) = parsed(frame("message_changed", nested.clone()));
        assert_eq!(outer, "C1");
        assert_eq!(original.raw(), &nested);
        assert_eq!(original.outer_channel(), "C1");
        let expected = nested_channel.as_str().unwrap_or("C1");
        assert_eq!(message.channel.as_deref(), Some(expected));
        assert_eq!(original.typed_channel(), Some(expected));
        // Identity matching is intentionally NOT agreement between nested and
        // outer channels, nor permission for a particular loaded surface.
        assert!(original.matches_identity(&outer, &message));
    }
}

#[test]
fn later_typed_mutations_cannot_reconstruct_or_change_original_payload() {
    let nested = json!({"ts":"1.2","text":"original","blocks":[],"edited":null});
    let (channel, mut message, original) = parsed(frame("message_changed", nested.clone()));
    let retained = original.clone();
    message.text = Some("changed locally".into());
    message.blocks.push(json!({"type":"divider"}));
    message.extra.insert("invented".into(), json!(true));
    assert_eq!(original.raw(), &nested);
    assert_eq!(retained.raw(), &nested);
    assert!(original.matches_identity(&channel, &message));
    message.ts = Some("9.9".into());
    assert!(!original.matches_identity(&channel, &message));
    message.ts = Some("1.2".into());
    message.channel = Some("C_OTHER".into());
    assert!(!original.matches_identity(&channel, &message));
    message.channel = Some("C1".into());
    assert!(!original.matches_identity("C_OTHER", &message));
    assert_eq!(original.typed_ts(), Some("1.2"));
    assert_eq!(original.typed_channel(), Some("C1"));
    assert_eq!(retained.raw(), &nested);
}

#[test]
fn witness_does_not_add_identity_validation_to_legacy_parser() {
    for nested in [json!({}), json!({"ts":null}), json!({"ts":""})] {
        let (channel, message, original) = parsed(frame("message_changed", nested.clone()));
        assert_eq!(original.raw(), &nested);
        assert_eq!(original.typed_ts(), message.ts.as_deref());
        assert!(original.matches_identity(&channel, &message));
    }
    let synthetic = RtEvent::MessageChanged {
        channel: "C1".into(),
        message: Message::default(),
        original_update: None,
    };
    assert!(matches!(
        synthetic,
        RtEvent::MessageChanged {
            original_update: None,
            ..
        }
    ));
}

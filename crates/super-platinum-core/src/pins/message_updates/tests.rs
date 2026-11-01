use super::*;
use crate::{
    pins::{PinnedItem, PinsPage},
    slack::{events::RtEvent, realtime::parse_event},
};
use serde_json::json;

fn rich(text: &str) -> Message {
    serde_json::from_value(json!({"channel":"C1","ts":"1.0","text":text,"blocks":[{"type":"section","text":{"type":"plain_text","text":text}}],"attachments":[{"text":text}],"reply_count":3,"reactions":[{"name":"eyes","count":1,"users":["U1"]}]})).unwrap()
}
fn page(message: Message) -> PinsPage {
    PinsPage {
        items: vec![PinnedItem {
            kind: "message".into(),
            channel: Some("C1".into()),
            message: Some(message),
            ..Default::default()
        }],
    }
}
fn cleared() -> (Message, MessageUpdate) {
    match parse_event(&json!({"type":"message","subtype":"message_changed","channel":"C1","message":{"ts":"1.0","text":"Replacement plain body","blocks":[],"attachments":[]}}).to_string()).unwrap() {
        RtEvent::MessageChanged { message, original_update:Some(original),.. } => (message,original),
        _ => panic!("original parser event"),
    }
}
fn current(pins: &PinsState) -> &Message {
    pins.items["message:1.0"].message.as_ref().unwrap()
}

#[test]
fn pin_preview_original_clear_survives_old_page_but_not_a_new_request_replacement_delete_or_foreign_context()
 {
    let mut pins = PinsState::default();
    pins.ensure_scope(1, "T1");
    let initial = pins.begin("C1");
    assert!(pins.apply(initial, page(rich("Old rich body"))));
    let pending = pins.begin("C1");
    let (clear, original) = cleared();
    pins.update_original_message("C1", &clear, Some(&original));
    let count = serde_json::from_value(json!({"channel":"C1","ts":"1.0","reply_count":4})).unwrap();
    pins.update_message("C1", &count);
    assert!(current(&pins).blocks.is_empty() && current(&pins).attachments.is_empty());
    assert!(
        pins.message_updates["1.0"]
            .as_ref()
            .unwrap()
            .blocks
            .is_empty()
    );
    assert!(pins.apply(pending, page(rich("Late old body"))));
    assert!(current(&pins).blocks.is_empty() && current(&pins).attachments.is_empty());
    assert_eq!(
        current(&pins).text.as_deref(),
        Some("Replacement plain body")
    );
    assert_eq!(current(&pins).reply_count, Some(4));
    assert_eq!(current(&pins).reactions[0].name, "eyes");

    let fresh = pins.begin("C1");
    pins.update_message("C1", &count);
    assert!(pins.apply(fresh, page(rich("Fresh rich response"))));
    assert!(!current(&pins).blocks.is_empty() && !current(&pins).attachments.is_empty());
    assert_eq!(current(&pins).text.as_deref(), Some("Fresh rich response"));

    let older = pins.begin("C1");
    pins.update_original_message("C1", &clear, Some(&original));
    pins.update_message("C1", &rich("Newer replacement"));
    assert!(pins.apply(older, page(rich("Old in-flight body"))));
    assert_eq!(current(&pins).text.as_deref(), Some("Newer replacement"));
    assert!(!current(&pins).blocks.is_empty() && !current(&pins).attachments.is_empty());

    let deleted = pins.begin("C1");
    pins.delete_message("C1", "1.0");
    pins.update_original_message("C1", &clear, Some(&original));
    assert!(pins.apply(deleted, page(rich("Must not resurrect"))));
    assert!(pins.items.is_empty());

    pins.ensure_scope(2, "T1");
    let other = pins.begin("C1");
    assert!(pins.apply(other, page(rich("Different account snapshot"))));
    assert!(!current(&pins).blocks.is_empty());
    let conflicting = pins.begin("C1");
    pins.update_original_message("C1", &clear, Some(&original));
    let mut foreign = page(rich("Conflicting pin context"));
    foreign.items[0].channel = Some("C_OTHER".into());
    foreign.items[0].message.as_mut().unwrap().channel = None;
    assert!(pins.apply(conflicting, foreign));
    assert!(!current(&pins).blocks.is_empty() && !current(&pins).attachments.is_empty());
    pins.update_original_message("C1", &clear, Some(&original));
    assert!(!current(&pins).blocks.is_empty() && !current(&pins).attachments.is_empty());
}

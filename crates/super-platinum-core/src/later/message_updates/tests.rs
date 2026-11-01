use super::*;
use crate::slack::{events::RtEvent, realtime::parse_event};
use serde_json::json;

fn rich(text: &str) -> Message {
    serde_json::from_value(json!({"channel":"C1","ts":"1.0","text":text,"blocks":[{"type":"section","text":{"type":"plain_text","text":text}}],"attachments":[{"text":text}],"reply_count":3,"reactions":[{"name":"eyes","count":1,"users":["U1"]}]})).unwrap()
}
fn cleared() -> (Message, MessageUpdate) {
    match parse_event(&json!({"type":"message","subtype":"message_changed","channel":"C1","message":{"ts":"1.0","text":"Replacement plain body","blocks":[],"attachments":[]}}).to_string()).unwrap() {
        RtEvent::MessageChanged { message, original_update:Some(original),.. } => (message,original),
        _ => panic!("original parser event"),
    }
}
fn count(count: u64) -> Message {
    serde_json::from_value(json!({"channel":"C1","ts":"1.0","reply_count":count})).unwrap()
}

#[test]
fn saved_preview_original_clear_survives_old_hydration_without_renewal_over_fresh_body_and_keeps_replacement_delete_scope_order()
 {
    let mut saved = LaterState::default();
    saved.ensure_scope(1, "T1");
    let key = SavedKey::message("C1", "1.0");
    saved.items.insert(
        key.clone(),
        serde_json::from_value(json!({"item_type":"message","item_id":"C1","ts":"1.0"})).unwrap(),
    );
    saved.hydrate_message(key.clone(), rich("Old rich body"));
    let requested = saved.begin_hydration();
    let (clear, original) = cleared();
    saved.update_original_message("C1", &clear, Some(&original));
    saved.update_message("C1", &count(4));
    for message in [
        &saved.hydrated[&key],
        saved.preview_updates[&key].message.as_ref().unwrap(),
    ] {
        assert!(message.blocks.is_empty() && message.attachments.is_empty());
    }
    saved.hydrate_message_since(key.clone(), rich("Late old body"), requested);
    let current = &saved.hydrated[&key];
    assert!(current.blocks.is_empty() && current.attachments.is_empty());
    assert_eq!(current.text.as_deref(), Some("Replacement plain body"));
    assert_eq!(current.reply_count, Some(4));
    assert_eq!(current.reactions[0].name, "eyes");

    // The count event must not renew the earlier explicit body removals over a
    // response requested after them. Each existing per-field revision survives.
    let fresh = saved.begin_hydration();
    saved.update_message("C1", &count(5));
    saved.hydrate_message_since(key.clone(), rich("Fresh rich response"), fresh);
    assert!(!saved.hydrated[&key].blocks.is_empty());
    assert!(!saved.hydrated[&key].attachments.is_empty());
    assert_eq!(
        saved.hydrated[&key].text.as_deref(),
        Some("Fresh rich response")
    );
    assert_eq!(saved.hydrated[&key].reply_count, Some(5));

    let older = saved.begin_hydration();
    saved.update_original_message("C1", &clear, Some(&original));
    saved.update_message("C1", &rich("Newer replacement"));
    saved.hydrate_message_since(key.clone(), rich("Old in-flight body"), older);
    assert_eq!(
        saved.hydrated[&key].text.as_deref(),
        Some("Newer replacement")
    );
    assert!(
        !saved.hydrated[&key].blocks.is_empty() && !saved.hydrated[&key].attachments.is_empty()
    );

    saved.delete_message("C1", "1.0");
    saved.update_original_message("C1", &clear, Some(&original));
    saved.hydrate_message_since(key.clone(), rich("Must not resurrect"), older);
    assert_eq!(
        saved.hydrated[&key].text.as_deref(),
        Some("This message was deleted.")
    );
    saved.ensure_scope(2, "T1");
    saved.hydrate_message(key.clone(), rich("Different account snapshot"));
    assert!(
        !saved.hydrated[&key].blocks.is_empty() && !saved.hydrated[&key].attachments.is_empty()
    );
}

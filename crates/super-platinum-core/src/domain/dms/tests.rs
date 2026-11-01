use super::*;
use crate::slack::{events::RtEvent, realtime::parse_event};
use serde_json::json;

fn entry(channel: &str, ts: &str, rich: &str) -> DmEntry {
    serde_json::from_value(json!({"id":channel,"latest":ts,"channel":{"id":channel,"is_im":true},"message":{"ts":ts,"text":"old fallback","blocks":[{"type":"section","text":{"type":"plain_text","text":rich}}],"attachments":[{"text":"old attachment"}],"reactions":[{"name":"eyes","count":1,"users":["U1"]}]}})).unwrap()
}
fn edit(channel: &str, body: serde_json::Value) -> (Message, MessageUpdate) {
    let event = parse_event(
        &json!({"type":"message","subtype":"message_changed","channel":channel,"message":body})
            .to_string(),
    )
    .unwrap();
    let RtEvent::MessageChanged {
        message,
        original_update: Some(original),
        ..
    } = event
    else {
        panic!("original edit")
    };
    (message, original)
}
fn published() -> DmsState {
    let mut s = DmsState::default();
    s.begin_preview_read(3, "T", 1);
    s.publish_preview_page(
        3,
        "T",
        1,
        false,
        vec![
            entry("D1", "2.1", "Old rich"),
            entry("D2", "3.1", "Other rich"),
        ],
    );
    s
}

#[test]
fn selected_preview_edits_replay_only_during_the_original_read_and_keep_new_snapshots_and_order() {
    let mut s = published();
    let before = s
        .entries
        .iter()
        .map(|e| (e.id.clone(), e.latest.clone()))
        .collect::<Vec<_>>();
    s.begin_preview_read(3, "T", 2);
    let (message, original) = edit(
        "D1",
        json!({"ts":"2.1","text":"Edited plain","blocks":[],"attachments":[]}),
    );
    assert!(s.update_original_preview(3, "T", "D1", &message, &original));
    let current = s
        .entries
        .iter()
        .find(|e| e.id == "D1")
        .unwrap()
        .message
        .as_ref()
        .unwrap();
    assert_eq!(current.text.as_deref(), Some("Edited plain"));
    assert!(current.blocks.is_empty() && current.attachments.is_empty());
    assert_eq!(current.reactions[0].name, "eyes");
    assert_eq!(
        s.entries
            .iter()
            .map(|e| (e.id.clone(), e.latest.clone()))
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(s.preview_updates.read.as_ref().unwrap().updates.len(), 1);
    s.publish_preview_page(
        3,
        "T",
        2,
        false,
        vec![
            entry("D1", "2.1", "Late rich"),
            entry("D2", "3.1", "Other rich"),
        ],
    );
    let current = s
        .entries
        .iter()
        .find(|e| e.id == "D1")
        .unwrap()
        .message
        .as_ref()
        .unwrap();
    assert!(current.blocks.is_empty() && current.attachments.is_empty());
    assert_eq!(current.text.as_deref(), Some("Edited plain"));
    assert_eq!(current.reactions[0].name, "eyes");

    // The old clear is before this new read; a count-only edit cannot renew it.
    s.begin_preview_read(3, "T", 3);
    let (count, original) = edit("D1", json!({"ts":"2.1","reply_count":8}));
    assert!(s.update_original_preview(3, "T", "D1", &count, &original));
    s.finish_preview_read(2); // retired lease must not clear read3
    assert_eq!(s.preview_updates.read.as_ref().unwrap().sequence, 3);
    s.publish_preview_page(3, "T", 3, false, vec![entry("D1", "2.1", "Fresh rich")]);
    let current = s.entries[0].message.as_ref().unwrap();
    assert!(!current.blocks.is_empty() && !current.attachments.is_empty());
    assert_eq!(current.reply_count, Some(8));

    // Within one read, a newer populated value supersedes its field's removal.
    s.begin_preview_read(3, "T", 4);
    assert!(
        s.update_original_preview(
            3,
            "T",
            "D1",
            &message,
            &edit(
                "D1",
                json!({"ts":"2.1","text":"Edited plain","blocks":[],"attachments":[]})
            )
            .1
        )
    );
    let (newer, original) = edit(
        "D1",
        json!({"ts":"2.1","blocks":[{"type":"section","text":{"type":"plain_text","text":"Newest rich"}}],"attachments":[{"text":"Newest attachment"}]}),
    );
    assert!(s.update_original_preview(3, "T", "D1", &newer, &original));
    s.publish_preview_page(3, "T", 4, false, vec![entry("D1", "2.1", "Old again")]);
    assert_eq!(s.entries[0].message.as_ref().unwrap().blocks, newer.blocks);
    assert_eq!(
        s.entries[0].message.as_ref().unwrap().attachments[0]
            .text
            .as_deref(),
        Some("Newest attachment")
    );
    assert!(s.preview_updates.read.is_none());
}

#[test]
fn original_preview_identity_never_inserts_retimes_or_claims_conflicting_or_ambiguous_rows() {
    for case in 0..9 {
        let mut s = published();
        let mut epoch = 3;
        let mut team = "T";
        match case {
            0 => epoch = 4,
            1 => team = "OTHER",
            2 => s.entries.iter_mut().find(|e| e.id == "D1").unwrap().latest = Some("9.1".into()),
            3 => {
                s.entries
                    .iter_mut()
                    .find(|e| e.id == "D1")
                    .unwrap()
                    .channel
                    .as_mut()
                    .unwrap()
                    .id = "OTHER".into()
            }
            4 => {
                s.entries
                    .iter_mut()
                    .find(|e| e.id == "D1")
                    .unwrap()
                    .message
                    .as_mut()
                    .unwrap()
                    .channel = Some("OTHER".into())
            }
            5 => s
                .entries
                .push(s.entries.iter().find(|e| e.id == "D1").unwrap().clone()),
            6 => s.preview_updates.published = None,
            7 => s.entries.retain(|e| e.id != "D1"),
            _ => {}
        }
        let body = if case == 8 {
            json!({"ts":"2.1","channel":"OTHER","text":"Wrong","blocks":[]})
        } else {
            json!({"ts":"2.1","text":"Wrong","blocks":[]})
        };
        let (message, original) = edit("D1", body);
        let before = serde_json::to_value(&s.entries).unwrap();
        s.begin_preview_read(epoch, team, 2);
        assert!(
            !s.update_original_preview(epoch, team, "D1", &message, &original),
            "case{case}"
        );
        assert_eq!(
            serde_json::to_value(&s.entries).unwrap(),
            before,
            "case{case}"
        );
        assert!(s.preview_updates.read.as_ref().unwrap().updates.is_empty());
    }
    let mut s = published();
    s.begin_preview_read(3, "T", 2);
    let (message, original) = edit("D1", json!({"ts":"2.1","text":"Edited","blocks":[]}));
    assert!(s.update_original_preview(3, "T", "D1", &message, &original));
    // A returned row choosing a different latest cannot take a same-channel patch.
    s.publish_preview_page(
        3,
        "T",
        2,
        false,
        vec![entry("D1", "4.1", "Different selected body")],
    );
    assert_eq!(s.entries[0].latest.as_deref(), Some("4.1"));
    assert!(!s.entries[0].message.as_ref().unwrap().blocks.is_empty());
}

mod deleted;

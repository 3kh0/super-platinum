use super::*;
use crate::slack::{events::RtEvent, realtime::parse_event};
use serde_json::json;

const A: &str = "1719801000.000100";
const B: &str = "1719802000.000100";
const C: &str = "1719803000.000100";
fn entry(id: &str, ts: &str, text: &str) -> DmEntry {
    serde_json::from_value(json!({"id":id,"latest":ts,"channel":{"id":id,"is_im":true},"message":{"ts":ts,"text":"fallback","blocks":[{"type":"section","text":{"type":"plain_text","text":text}}],"reactions":[{"name":"eyes","count":1,"users":["U1"]}]}})).unwrap()
}
fn message(ts: &str) -> Message {
    serde_json::from_value(json!({"type":"message","channel":"D1","user":"U2","ts":ts,"text":"New ordinary body","blocks":[{"type":"section","text":{"type":"plain_text","text":"New rich body"}}]})).unwrap()
}
fn state() -> DmsState {
    let mut s = DmsState::default();
    s.begin_preview_read(3, "T", 1);
    s.publish_preview_page(
        3,
        "T",
        1,
        false,
        vec![entry("D1", A, "Old A"), entry("D2", B, "Old B")],
    );
    s
}
fn edit(body: Value) -> (Message, crate::slack::events::MessageUpdate) {
    let e = parse_event(
        &json!({"type":"message","subtype":"message_changed","channel":"D1","message":body})
            .to_string(),
    )
    .unwrap();
    let RtEvent::MessageChanged {
        message,
        original_update: Some(original),
        ..
    } = e
    else {
        panic!("original edit")
    };
    (message, original)
}

#[test]
fn ordinary_arrival_reorders_existing_row_and_composes_with_edit_replay_without_overriding_newer_page()
 {
    let mut s = state();
    assert_eq!(s.entries[0].id, "D2");
    s.begin_preview_read(3, "T", 2);
    assert!(s.update_ordinary_preview(3, "T", "U1", "D1", &message(C)));
    assert_eq!(
        s.entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["D1", "D2"]
    );
    assert!(
        s.entries[0].message.as_ref().unwrap().reactions.is_empty(),
        "new identity never inherits old reactions"
    );
    let (change, original) =
        edit(json!({"ts":C,"text":"Edited after arrival","blocks":[],"attachments":[]}));
    assert!(s.update_original_preview(3, "T", "D1", &change, &original));
    s.publish_preview_page(
        3,
        "T",
        2,
        false,
        vec![entry("D1", A, "Late A"), entry("D2", B, "Late B")],
    );
    assert_eq!(s.entries[0].latest.as_deref(), Some(C));
    let current = s.entries[0].message.as_ref().unwrap();
    assert_eq!(current.text.as_deref(), Some("Edited after arrival"));
    assert!(current.blocks.is_empty());
    assert!(current.reactions.is_empty());
    assert_eq!(s.entries.len(), 2);

    s.begin_preview_read(3, "T", 3);
    let (count, original) = edit(json!({"ts":C,"reply_count":7}));
    assert!(s.update_original_preview(3, "T", "D1", &count, &original));
    s.publish_preview_page(3, "T", 3, false, vec![entry("D1", C, "Fresh rich body")]);
    assert!(!s.entries[0].message.as_ref().unwrap().blocks.is_empty());
    assert_eq!(s.entries[0].message.as_ref().unwrap().reply_count, Some(7));

    s.begin_preview_read(3, "T", 4);
    assert!(s.update_ordinary_preview(3, "T", "U1", "D1", &message("1719804000.000100")));
    s.publish_preview_page(
        3,
        "T",
        4,
        false,
        vec![entry("D1", "1719805000.000100", "Even newer HTTP")],
    );
    assert_eq!(s.entries[0].latest.as_deref(), Some("1719805000.000100"));
    assert!(!s.entries[0].message.as_ref().unwrap().blocks.is_empty());
    // Existing page membership still wins; a pending body is not an insertion grant.
    s.begin_preview_read(3, "T", 5);
    assert!(s.update_ordinary_preview(3, "T", "U1", "D1", &message("1719806000.000100")));
    s.publish_preview_page(3, "T", 5, false, vec![]);
    assert!(s.entries.is_empty());
}

#[test]
fn unhandled_ordinary_inputs_and_conflicting_owners_never_insert_retime_or_reorder_rows() {
    for case in 0..27 {
        let mut s = state();
        let mut m = message(C);
        let mut epoch = 3;
        let mut team = "T";
        let mut user = "U1";
        match case {
            0 => epoch = 4,
            1 => team = "OTHER",
            2 => user = "",
            3 => m.user = None,
            4 => m.channel = Some("OTHER".into()),
            5 => m.kind = None,
            6 => m.subtype = Some("thread_broadcast".into()),
            7 => m.thread_ts = Some(A.into()),
            8 => m.ts = Some("1719803000.x00100".into()),
            9 => m.ts = Some("1719803000.aaaaaa".into()),
            10 => m.ts = Some(A.into()),
            11 => m.ts = Some("1719800000.000100".into()),
            12 => {
                m.extra.insert("isNonExistent".into(), json!(true));
            }
            13 => {
                m.extra.insert("no_display".into(), json!("hidden"));
            }
            14 => {
                m.extra.insert("_hidden_reply".into(), json!([]));
            }
            15 => {
                m.extra.insert("_hidden_stub".into(), json!({}));
            }
            16 => {
                m.extra.insert("dnd_suppressed".into(), json!(1));
            }
            17 => {
                m.extra.insert("is_ephemeral".into(), json!(true));
            }
            18 => {
                m.extra.insert("comment".into(), Value::Null);
            }
            19 => {
                m.extra.insert(
                    "metadata".into(),
                    json!({"event_type":"slack_system.data_retention.notice"}),
                );
            }
            20 => {
                m.extra.insert("workflow_id".into(), json!("W1"));
            }
            21 => m.bot_id = Some("B1".into()),
            22 => {
                s.entries.retain(|e| e.id != "D1");
            }
            23 => {
                let a = s.entries.iter().find(|e| e.id == "D1").unwrap().clone();
                s.entries.push(a);
            }
            24 => {
                s.entries
                    .iter_mut()
                    .find(|e| e.id == "D1")
                    .unwrap()
                    .channel
                    .as_mut()
                    .unwrap()
                    .id = "OTHER".into()
            }
            25 => {
                s.entries
                    .iter_mut()
                    .find(|e| e.id == "D1")
                    .unwrap()
                    .message
                    .as_mut()
                    .unwrap()
                    .ts = Some(B.into())
            }
            _ => {
                m.text = None;
                m.blocks.clear();
            }
        }
        s.begin_preview_read(epoch, team, 2);
        let before = serde_json::to_value(&s.entries).unwrap();
        assert!(
            !s.update_ordinary_preview(epoch, team, user, "D1", &m),
            "case{case}"
        );
        assert_eq!(
            serde_json::to_value(&s.entries).unwrap(),
            before,
            "case{case}"
        );
        assert!(s.preview_updates.read.as_ref().unwrap().updates.is_empty());
    }
    // Both ordinary author branches work; falsey JSON flags follow Source truthiness.
    for author in ["U1", "U2"] {
        let mut s = state();
        let mut m = message(C);
        m.user = Some(author.into());
        for (field, value) in [
            ("isNonExistent", json!(false)),
            ("no_display", json!(0)),
            ("_hidden_reply", json!("")),
            ("_hidden_stub", Value::Null),
        ] {
            m.extra.insert(field.into(), value);
        }
        assert!(s.update_ordinary_preview(3, "T", "U1", "D1", &m));
        assert_eq!(s.entries.len(), 2);
    }
    assert!(!ordered_ts("0000000000.000100"));
    assert!(!ordered_ts("ééééé.000100"));
}

mod bots;

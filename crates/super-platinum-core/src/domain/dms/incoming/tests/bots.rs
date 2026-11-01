use super::*;

fn bot(ts: &str) -> Message {
    let mut m = message(ts);
    m.subtype = Some("bot_message".into());
    m.bot_id = Some("B1".into());
    m.bot_profile = Some(
        serde_json::from_value(json!({"id":"B1","user_id":"U2","name":"Build helper"})).unwrap(),
    );
    m
}

#[test]
fn explicit_authored_bot_advances_only_owned_existing_dm_and_replays_original_edits() {
    let mut s = state();
    s.begin_preview_read(3, "T", 2);
    assert!(
        !s.update_ordinary_preview(3, "T", "U1", "D1", &bot(C)),
        "human path stays unchanged"
    );
    assert!(s.update_bot_preview(3, "T", "U1", "D1", &bot(C)));
    assert_eq!(
        s.entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["D1", "D2"]
    );
    assert!(s.entries[0].message.as_ref().unwrap().reactions.is_empty());
    let (change, original) =
        edit(json!({"ts":C,"text":"Edited bot result","blocks":[],"attachments":[]}));
    assert!(s.update_original_preview(3, "T", "D1", &change, &original));
    s.publish_preview_page(
        3,
        "T",
        2,
        false,
        vec![
            entry("D1", A, "Late old rich"),
            entry("D2", B, "Independent peer"),
        ],
    );
    let m = s.entries[0].message.as_ref().unwrap();
    assert_eq!(m.text.as_deref(), Some("Edited bot result"));
    assert_eq!(m.bot_id.as_deref(), Some("B1"));
    assert_eq!(m.bot_profile.as_ref().unwrap().id.as_deref(), Some("B1"));
    assert!(m.blocks.is_empty() && m.attachments.is_empty() && m.reactions.is_empty());
    assert_eq!(s.entries.len(), 2);
    s.begin_preview_read(3, "T", 3);
    assert!(s.update_bot_preview(3, "T", "U1", "D1", &bot("1719804000.000100")));
    s.publish_preview_page(
        3,
        "T",
        3,
        false,
        vec![entry("D1", "1719805000.000100", "Newer server body")],
    );
    assert_eq!(s.entries[0].latest.as_deref(), Some("1719805000.000100"));
    s.begin_preview_read(3, "T", 4);
    assert!(s.update_bot_preview(3, "T", "U1", "D1", &bot("1719806000.000100")));
    s.publish_preview_page(3, "T", 4, false, vec![]);
    assert!(
        s.entries.is_empty(),
        "arrival replay never grants membership"
    );

    for case in 0..21 {
        let mut s = state();
        let mut m = bot(C);
        let (mut epoch, mut team, mut user) = (3, "T", "U1");
        match case {
            0 => m.user = None,
            1 => m.user = Some("U1".into()),
            2 => m.bot_id = None,
            3 => m.bot_id = Some(String::new()),
            4 => m.subtype = None,
            5 => m.subtype = Some("thread_broadcast".into()),
            6 => m.kind = None,
            7 => m.channel = Some("OTHER".into()),
            8 => m.thread_ts = Some(A.into()),
            9 => {
                m.extra.insert("no_display".into(), json!(true));
            }
            10 => {
                m.extra.insert("dnd_suppressed".into(), json!(true));
            }
            11 => {
                m.extra.insert("metadata".into(), Value::Null);
            }
            12 => {
                m.extra.insert("comment".into(), Value::Null);
            }
            13 => {
                m.extra.insert("workflow_id".into(), Value::Null);
            }
            14 => m.bot_profile.as_mut().unwrap().id = Some("OTHER".into()),
            15 => m.bot_profile.as_mut().unwrap().user_id = Some("OTHER".into()),
            16 => m.ts = Some(A.into()),
            17 => epoch += 1,
            18 => team = "OTHER",
            19 => user = "",
            _ => {
                let duplicate = s.entries.iter().find(|e| e.id == "D1").unwrap().clone();
                s.entries.push(duplicate);
            }
        }
        s.begin_preview_read(epoch, team, 2);
        let before = serde_json::to_value(&s.entries).unwrap();
        assert!(
            !s.update_bot_preview(epoch, team, user, "D1", &m),
            "case{case}"
        );
        assert_eq!(
            serde_json::to_value(&s.entries).unwrap(),
            before,
            "case{case}"
        );
        assert!(s.preview_updates.read.as_ref().unwrap().updates.is_empty());
    }
}

use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{events::RtEvent, realtime::parse_event},
    state::Workspace,
};
use serde_json::json;
fn core() -> CoreAppState {
    let mut c = CoreAppState::new(Default::default());
    c.active_team = Some("T1".into());
    c.workspaces.insert(
        "T1".into(),
        Workspace::from_session(&WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            name: "Test".into(),
            url: "https://example.test".into(),
            token: "synthetic".into(),
            enterprise_id: None,
        }),
    );
    c.activity.ensure_scope(c.account_epoch, "T1");
    c.activity.items=serde_json::from_value(json!([{"key":"mention","feed_ts":"2.0","is_unread":true,"item":{"type":"at_user","message":{"channel":"C1","ts":"1.0","text":"Stale index text"}}}])).unwrap();
    c
}
fn old() -> Message {
    serde_json::from_value(json!({"ts":"1.0","text":"Old fallback","blocks":[{"type":"section","text":{"type":"plain_text","text":"Old rich"}}],"attachments":[{"text":"Old attachment"}],"reactions":[{"name":"eyes","count":1,"users":["U1"]}]})).unwrap()
}
fn page(message: Message) -> MessagesListPage {
    serde_json::from_value(json!({"messages_data":{"C1":{"messages":[message]}}})).unwrap()
}
fn read(c: &mut CoreAppState) -> (ActivityMessageRead, FileDisplayRead) {
    (
        c.activity
            .begin_message_hydration(&[("C1".into(), vec!["1.0".into()])])
            .unwrap(),
        c.begin_file_display_read("T1"),
    )
}
fn edit(c: &mut CoreAppState, body: serde_json::Value) {
    let e = parse_event(
        &json!({"type":"message","subtype":"message_changed","channel":"C1","message":body})
            .to_string(),
    )
    .unwrap();
    let RtEvent::MessageChanged {
        channel,
        message,
        original_update,
        ..
    } = e
    else {
        panic!("edit")
    };
    c.update_activity_original_message("T1", &channel, &message, original_update.as_ref());
}

#[test]
fn original_activity_edits_survive_cold_hydration_and_count_only_does_not_renew_old_removal() {
    let mut c = core();
    let key = ("C1".into(), "1.0".into());
    let (request, files) = read(&mut c);
    edit(
        &mut c,
        json!({"ts":"1.0","text":"Edited Activity","blocks":[],"attachments":[]}),
    );
    assert!(
        c.activity.hydrated.is_empty(),
        "a partial edit does not declare a cold body hydrated"
    );
    assert_eq!(
        c.apply_activity_message_page("T1", request, page(old()), files),
        Ok(true)
    );
    let current = &c.activity.hydrated[&key];
    assert_eq!(current.text.as_deref(), Some("Edited Activity"));
    assert!(current.blocks.is_empty() && current.attachments.is_empty());
    assert_eq!(current.reactions[0].name, "eyes");
    let before = (
        c.activity.items[0].is_unread,
        c.activity.selected.clone(),
        c.activity.next_cursor.clone(),
    );
    let (older, older_files) = read(&mut c);
    let (fresh, fresh_files) = read(&mut c);
    edit(&mut c, json!({"ts":"1.0","reply_count":5}));
    let mut newer = old();
    newer.text = Some("Fresh fallback".into());
    newer.blocks = vec![json!({"type":"section","text":{"type":"plain_text","text":"Fresh rich"}})];
    assert_eq!(
        c.apply_activity_message_page("T1", fresh, page(newer.clone()), fresh_files),
        Ok(true)
    );
    assert_eq!(c.activity.hydrated[&key].blocks, newer.blocks);
    assert_eq!(c.activity.hydrated[&key].reply_count, Some(5));
    assert_eq!(
        c.apply_activity_message_page("T1", older, page(old()), older_files),
        Ok(true)
    );
    assert_eq!(
        c.activity.hydrated[&key].blocks, newer.blocks,
        "older concurrent response cannot replace fresh adoption"
    );
    assert_eq!(
        (
            c.activity.items[0].is_unread,
            c.activity.selected.clone(),
            c.activity.next_cursor.clone()
        ),
        before
    );
    assert!(c.activity.has_original_body_edit("C1", "1.0"));
}

#[test]
fn activity_hydration_rejects_foreign_coordinates_and_retired_scope_and_keeps_file_tombstones() {
    for case in 0..4 {
        let mut c = core();
        let (request, files) = read(&mut c);
        let mut body = old();
        match case {
            0 => body.channel = Some("OTHER".into()),
            1 => body.ts = Some("OTHER".into()),
            2 => body.ts = None,
            _ => {}
        }
        let mut result = page(body);
        if case == 3 {
            result
                .messages_data
                .get_mut("C1")
                .unwrap()
                .messages
                .push(old());
        }
        assert!(
            c.apply_activity_message_page("T1", request, result, files)
                .is_err()
        );
        assert!(c.activity.hydrated.is_empty());
    }
    let mut c = core();
    assert!(
        c.activity
            .begin_message_hydration(&[("OTHER".into(), vec!["1.0".into()])])
            .is_none()
    );
    let (request, files) = read(&mut c);
    c.account_epoch += 1;
    c.activity.ensure_scope(c.account_epoch, "T1");
    assert_eq!(
        c.apply_activity_message_page("T1", request, page(old()), files),
        Ok(false)
    );
    assert!(c.activity.hydrated.is_empty());
    let mut c = core();
    let (request, files) = read(&mut c);
    let same_items = c.activity.items.clone();
    c.activity = Default::default();
    c.activity.ensure_scope(c.account_epoch, "T1");
    c.activity.items = same_items;
    assert_eq!(c.activity.load_seq, request.sequence);
    assert_eq!(
        c.apply_activity_message_page("T1", request, page(old()), files),
        Ok(false)
    );
    assert!(c.activity.hydrated.is_empty());
    let mut c = core();
    let (request, files) = read(&mut c);
    edit(
        &mut c,
        json!({"ts":"1.0","text":"With file","files":[{"id":"F1","name":"Before"}]}),
    );
    c.remove_file("T1", "F1");
    assert_eq!(
        c.apply_activity_message_page("T1", request, page(old()), files),
        Ok(true)
    );
    assert!(crate::file_actions::deleted(
        &c.activity.hydrated[&("C1".into(), "1.0".into())].files[0]
    ));
    let request = c.activity.begin(false).unwrap();
    assert!(c.activity.apply_page(
        &request,
        serde_json::from_value(json!({"items":[]})).unwrap()
    ));
    assert!(
        c.activity.sync.messages.updates.is_empty()
            && c.activity.sync.messages.edited_body.is_empty()
    );
}

mod deleted;

use super::*;
use serde_json::json;
use super_platinum_core::slack::models::{Channel, HuddleInvite};
use super_platinum_core::slack::realtime::parse_event;

fn core() -> super_platinum_core::CoreAppState {
    let mut core = super_platinum_core::CoreAppState::new(Default::default());
    let mut ws = super_platinum_core::state::Workspace::from_session(
        &super_platinum_core::config::WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U_SELF".into(),
            name: "Test".into(),
            url: "https://test".into(),
            token: String::new(),
        },
    );
    ws.rt_generation = 1;
    ws.channels.insert(
        "C1".into(),
        Channel {
            id: "C1".into(),
            is_channel: true,
            ..Default::default()
        },
    );
    ws.notifications.apply_change("all_notifications_prefs", json!({"global":{"global_desktop":"mentions_dms","threads_everything":false},"channels":{}}));
    core.workspaces.insert("T1".into(), ws);
    core
}

fn original() -> RtEvent {
    parse_event(
        r#"{"type":"message","channel":"C1","user":"U1","text":"hello <@U_SELF>","ts":"200.0"}"#,
    )
    .unwrap()
}

#[test]
fn thread_parent_updates_do_not_repeat_mention_notifications() {
    let core = core();
    let mut tracker = NotificationTracker::default();
    assert!(tracker.for_event(&core, "T1", 1, &original()).is_some());
    let parent = r#"{"type":"message","subtype":"message_replied","channel":"C1","message":{"user":"U1","text":"hello <@U_SELF>","ts":"200.0","reply_count":2}}"#;
    for _ in 0..3 {
        assert!(
            tracker
                .for_event(&core, "T1", 1, &parse_event(parent).unwrap())
                .is_none()
        );
    }
}

#[test]
fn stale_frames_read_messages_and_replays_do_not_deliver() {
    let mut core = core();
    let mut tracker = NotificationTracker::default();
    assert!(tracker.for_event(&core, "T1", 0, &original()).is_none());
    assert!(tracker.for_event(&core, "T1", 1, &original()).is_some());
    core.workspaces.get_mut("T1").unwrap().rt_generation = 2;
    assert!(tracker.for_event(&core, "T1", 2, &original()).is_none());
    let ws = core.workspaces.get_mut("T1").unwrap();
    ws.channels.get_mut("C1").unwrap().last_read = Some("201.0".into());
    assert!(for_event(&core, "T1", 2, &original()).is_none());
    let ws = core.workspaces.get_mut("T1").unwrap();
    ws.channels.get_mut("C1").unwrap().last_read = None;
    if let RtEvent::Message(m) = original() {
        ws.messages.entry("C1".into()).or_default().upsert(m);
    }
    assert!(for_event(&core, "T1", 2, &original()).is_none());
}

#[test]
fn thread_server_notification_and_message_share_one_delivery_identity() {
    let mut core = core();
    core.workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .global
        .insert("threads_everything".into(), json!(true));
    let mut tracker = NotificationTracker::default();
    let message = parse_event(r#"{"type":"message","channel":"C1","thread_ts":"100.0","user":"U1","text":"<@U_SELF> hi","ts":"200.0"}"#).unwrap();
    let server = parse_event(r#"{"type":"desktop_notification","channel":"C1","thread_ts":"100.0","sender_id":"U1","content":"a reply","ts":"200.0"}"#).unwrap();
    assert!(tracker.for_event(&core, "T1", 1, &message).is_some());
    assert!(tracker.for_event(&core, "T1", 1, &server).is_none());
    let ordinary = parse_event(r#"{"type":"desktop_notification","channel":"C1","sender_id":"U1","content":"hello","ts":"201.0"}"#).unwrap();
    assert!(tracker.for_event(&core, "T1", 1, &ordinary).is_none());
}

#[test]
fn privacy_hides_message_content_and_mute_sound_keeps_the_banner() {
    let mut core = core();
    let ws = core.workspaces.get_mut("T1").unwrap();
    ws.notifications
        .global
        .insert("no_text_in_notifications".into(), json!(true));
    ws.notifications.apply_change("mute_sounds", json!(true));
    let notice = for_event(&core, "T1", 1, &original()).unwrap();
    assert_eq!(notice.body, "New message");
    assert_eq!(notice.sound, None);
}

#[test]
fn viewed_conversations_only_suppress_when_focused_and_threads_match() {
    let mut core = core();
    core.active_team = Some("T1".into());
    core.active_channel = Some("C1".into());
    let focused = NotificationView {
        focused: true,
        thread_root: None,
    };
    let background = NotificationView {
        focused: false,
        thread_root: None,
    };
    assert!(for_event_in_view(&core, "T1", 1, &original(), focused).is_none());
    assert!(for_event_in_view(&core, "T1", 1, &original(), background).is_some());
    let reply = parse_event(r#"{"type":"message","channel":"C1","thread_ts":"100.0","user":"U1","text":"<@U_SELF> hi","ts":"200.0"}"#).unwrap();
    assert!(for_event_in_view(&core, "T1", 1, &reply, focused).is_some());
    assert!(
        for_event_in_view(
            &core,
            "T1",
            1,
            &reply,
            NotificationView {
                focused: true,
                thread_root: Some("100.0")
            }
        )
        .is_none()
    );
}

#[test]
fn previews_resolve_group_mentions_and_preserve_spaces_between_rich_runs() {
    let mut core = core();
    let ws = core.workspaces.get_mut("T1").unwrap();
    ws.usergroups.insert(
        "S1".into(),
        super_platinum_core::slack::models::UserGroup {
            id: "S1".into(),
            handle: "team".into(),
            users: vec!["U_SELF".into()],
            ..Default::default()
        },
    );
    let event = parse_event(r#"{"type":"message","channel":"C1","user":"U1","ts":"200.0","text":"<!subteam^S1> hi there"}"#).unwrap();
    assert_eq!(
        for_event(&core, "T1", 1, &event).unwrap().body,
        "@team hi there"
    );
}

#[test]
fn huddle_invites_respect_desktop_permissions_dnd_and_deduplication() {
    let mut core = core();
    let mut tracker = NotificationTracker::default();
    let event = RtEvent::HuddleInvite(HuddleInvite {
        channel_id: "C1".into(),
        call_id: "R1".into(),
        sender_user_id: Some("U1".into()),
    });
    assert!(tracker.for_event(&core, "T1", 1, &event).is_some());
    assert!(tracker.for_event(&core, "T1", 1, &event).is_none());
    let ws = core.workspaces.get_mut("T1").unwrap();
    ws.self_dnd.snooze_enabled = true;
    assert!(for_event(&core, "T1", 1, &event).is_none());
    let ws = core.workspaces.get_mut("T1").unwrap();
    ws.self_dnd.snooze_enabled = false;
    ws.notifications
        .global
        .insert("global_desktop_push_enabled".into(), json!(false));
    assert!(for_event(&core, "T1", 1, &event).is_none());
}

use super::super::tests::{core, inbox};
use super::*;
use serde_json::json;
use super_platinum_core::slack::{
    events::RtEvent,
    models::{ActivityFeedPage, ActivityItem, Message},
};

fn item(key: &str, ts: &str, root: Option<&str>) -> ActivityItem {
    serde_json::from_value(json!({"key":key,"feed_ts":ts,"is_unread":true,
        "item":{"type":"at_user","message":{"channel":"C1","ts":ts,"thread_ts":root}}}))
    .unwrap()
}

fn setup() -> CoreAppState {
    let mut c = core();
    inbox(&mut c);
    c.activity.items.clear();
    for entry in [
        item("old", "2.0", None),
        item("new", "4.0", None),
        item("reply", "1.8", Some("1.0")),
        item("other-root", "1.9", Some("0.5")),
    ] {
        c.activity.upsert(entry);
    }
    c.workspaces
        .get_mut("T1")
        .unwrap()
        .apply_activity_counts(serde_json::from_value(json!({"at_user":4})).unwrap());
    c
}

fn unread(c: &CoreAppState, key: &str) -> bool {
    c.activity
        .items
        .iter()
        .find(|item| item.key == key)
        .unwrap()
        .is_pending()
}

#[test]
fn accepted_partial_ack_reconciles_only_the_acknowledged_channel_cursor() {
    let mut c = setup();
    let messages = c
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry("C1".into())
        .or_default();
    messages.last_read = Some("1.0".into());
    messages.latest = Some("4.0".into());
    messages.unread_count = 3;
    messages.upsert(Message {
        ts: Some("4.0".into()),
        ..Default::default()
    });
    assert!(confirm_channel_read(
        &mut c,
        "T1",
        "C1",
        "2.0",
        Some("1.0"),
        0
    ));
    assert_eq!(c.workspaces["T1"].messages["C1"].unread_count, 3);
    assert!(!unread(&c, "old"));
    assert!(unread(&c, "new") && unread(&c, "reply") && unread(&c, "other-root"));
    assert_eq!(c.workspaces["T1"].activity_badge_count(), Some(3));
    assert!(confirm_channel_read(
        &mut c,
        "T1",
        "C1",
        "2.0",
        Some("1.0"),
        0
    ));
    assert_eq!(c.workspaces["T1"].activity_badge_count(), Some(3));
    assert!(!confirm_channel_read(
        &mut c,
        "T1",
        "C1",
        "4.0",
        Some("1.0"),
        0,
    ));
    assert!(unread(&c, "new"));
}

#[test]
fn accepted_thread_ack_uses_exact_root_and_rejects_intervening_unread() {
    let mut c = setup();
    let key = ("T1".into(), "C1".into(), "1.0".into());
    c.apply_thread_mark(&key, "1.5");
    assert!(!confirm_thread_read(&mut c, &key, "4.0", 0));
    assert!(unread(&c, "reply"));
    assert!(confirm_thread_read(&mut c, &key, "2.0", 1));
    assert!(!unread(&c, "reply"));
    assert!(unread(&c, "old") && unread(&c, "other-root"));
    assert_eq!(c.workspaces["T1"].activity_badge_count(), Some(3));
}

#[test]
fn socket_marks_are_scoped_and_bounded_during_activity_pagination() {
    let mut c = setup();
    c.workspaces.get_mut("T1").unwrap().rt_generation = 4;
    let mut background = c.workspaces["T1"].clone();
    background.team_id = "T2".into();
    c.workspaces.insert("T2".into(), background);
    let page = c.activity.begin(false).unwrap();
    let channel = || RtEvent::ChannelMarked {
        channel: "C1".into(),
        ts: "2.0".into(),
        unread_count: Some(1),
        mention_count: Some(1),
    };
    for (team, generation) in [("missing", 4), ("T2", 4), ("T1", 3)] {
        crate::realtime::apply_in_view(&mut c, team, generation, channel(), false);
    }
    assert!(unread(&c, "old"));
    crate::realtime::apply_in_view(&mut c, "T1", 4, channel(), false);
    let thread = || RtEvent::ThreadMarked {
        channel: "C1".into(),
        root_ts: "1.0".into(),
        last_read: "2.0".into(),
    };
    crate::realtime::apply_in_view(&mut c, "T1", 3, thread(), false);
    assert!(unread(&c, "reply"));
    crate::realtime::apply_in_view(&mut c, "T1", 4, thread(), false);
    crate::realtime::apply_in_view(&mut c, "T1", 4, thread(), false);
    assert!(c.activity.apply_page(
        &page,
        ActivityFeedPage {
            items: vec![
                item("old", "2.0", None),
                item("new", "4.0", None),
                item("reply", "1.8", Some("1.0")),
                item("unseen-old", "1.5", None),
                item("unseen-new", "5.0", None)
            ],
            ..Default::default()
        }
    ));
    assert!(!unread(&c, "old") && !unread(&c, "reply") && !unread(&c, "unseen-old"));
    assert!(unread(&c, "new") && unread(&c, "unseen-new"));
    assert_eq!(c.workspaces["T1"].activity_badge_count(), Some(2));
}

#[test]
fn cursor_owner_rejects_credential_account_transport_and_socket_replacement() {
    let changes: [fn(&mut CoreAppState); 8] = [
        |c| c.account_epoch += 1,
        |c| c.active_account = Some("replacement".into()),
        |c| c.session.as_mut().unwrap().d_cookie = "replacement".into(),
        |c| {
            c.session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token = "replacement".into()
        },
        |c| {
            c.session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .url = "https://other.invalid".into()
        },
        |c| c.workspaces.get_mut("T1").unwrap().self_user_id = "replacement".into(),
        |c| c.transport = Some(Arc::new(Transport::new("replacement").unwrap())),
        |c| c.workspaces.get_mut("T1").unwrap().rt_generation += 1,
    ];
    for change in changes {
        let mut c = setup();
        let owner = CursorOwner::capture(
            &c,
            c.transport.as_ref().unwrap(),
            &c.session.as_ref().unwrap().workspaces["T1"],
        )
        .unwrap();
        assert!(owner.current(&c));
        change(&mut c);
        assert!(!owner.current(&c));
        assert_eq!(c.activity.unread_count(), 4);
    }
}

#[test]
fn navigation_retains_cursor_ownership_but_cannot_consume_another_feed() {
    let mut c = setup();
    let owner = CursorOwner::capture(
        &c,
        c.transport.as_ref().unwrap(),
        &c.session.as_ref().unwrap().workspaces["T1"],
    )
    .unwrap();
    c.active_team = Some("T2".into());
    assert!(owner.current(&c));
    mark_channel_read(&mut c, "T1", "C1", "99.0");
    mark_thread_read(&mut c, "T1", "C1", "1.0", "99.0");
    assert_eq!(c.activity.unread_count(), 4);
    c.active_team = Some("T1".into());
    c.account_epoch += 1;
    mark_channel_read(&mut c, "T1", "C1", "99.0");
    assert_eq!(c.activity.unread_count(), 4);
}

#[test]
fn raw_false_bundle_updates_legacy_total_without_manufacturing_inbox_delta() {
    for canonical in [false, true] {
        let mut c = core();
        if canonical {
            inbox(&mut c);
        }
        c.activity.items.clear();
        c.activity.upsert(serde_json::from_value(json!({"key":"bundle","feed_ts":"2.0","is_unread":false,
            "item":{"type":"thread_v2","bundle_info":{"payload":{"thread_entry":{"channel_id":"C1","thread_ts":"1.0","latest_ts":"3.0","min_unread_ts":"2.0","unread_msg_count":5}}}}})).unwrap());
        c.workspaces
            .get_mut("T1")
            .unwrap()
            .apply_activity_counts(serde_json::from_value(json!({"thread_v2":7})).unwrap());
        assert_eq!(c.activity.unread_count(), 1);
        mark_thread_read(&mut c, "T1", "C1", "1.0", "2.0");
        assert_eq!(c.activity.unread_count(), 0);
        assert_eq!(
            c.workspaces["T1"].activity_unread_count,
            Some(if canonical { 7 } else { 6 })
        );
    }
}

#[test]
fn delayed_channel_ack_rejects_cursor_returning_to_the_previous_value() {
    let mut c = setup();
    let mark = |cursor: &str| RtEvent::ChannelMarked {
        channel: "C1".into(),
        ts: cursor.into(),
        unread_count: Some(2),
        mention_count: Some(1),
    };
    crate::realtime::apply_in_view(&mut c, "T1", 0, mark("1.0"), false);
    let revision = c.workspaces["T1"].read_revision("C1");
    for cursor in ["3.0", "1.0"] {
        crate::realtime::apply_in_view(&mut c, "T1", 0, mark(cursor), false);
    }
    // A fresh unread item after the backwards mark must survive the old ACK.
    c.activity.upsert(item("old", "2.0", None));
    let badge = c.workspaces["T1"].activity_badge_count();
    assert!(!confirm_channel_read(
        &mut c,
        "T1",
        "C1",
        "2.0",
        Some("1.0"),
        revision
    ));
    assert_eq!(c.workspaces["T1"].read_cursor("C1"), Some("1.0"));
    assert_eq!(c.workspaces["T1"].messages["C1"].unread_count, 2);
    assert_eq!(c.workspaces["T1"].activity_badge_count(), badge);
    assert!(unread(&c, "old"));
}

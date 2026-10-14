use super::{ActivityState, tests::*};
use crate::slack::models::ActivityItem;

fn thread(root: &str, ts: &str, min: Option<&str>, raw_unread: bool) -> ActivityItem {
    serde_json::from_value(serde_json::json!({
        "key":format!("thread-{root}-{ts}"), "feed_ts":ts, "is_unread":raw_unread,
        "item":{"type":"thread_v2","bundle_info":{"payload":{"thread_entry":{
            "channel_id":"C1", "thread_ts":root, "latest_ts":ts,
            "min_unread_ts":min,"unread_msg_count":9
        }}}}
    }))
    .unwrap()
}

fn mention(root: &str, ts: &str) -> ActivityItem {
    let mut notification = item(ts);
    notification.item.message.as_mut().unwrap().thread_ts = Some(root.into());
    notification
}

fn unread_at(state: &ActivityState, ts: &str) -> bool {
    state
        .items
        .iter()
        .find(|item| item.feed_ts == ts)
        .unwrap()
        .is_pending()
}

#[test]
fn channel_cursor_excludes_threaded_mentions_and_newer_messages() {
    let mut state = feed();
    for notification in [item("1.0"), item("2.0"), item("3.0"), mention("1.0", "1.5")] {
        state.upsert(notification);
    }
    let mut other = item("1.1");
    other.item.message.as_mut().unwrap().channel = Some("C2".into());
    state.upsert(other);
    assert_eq!(
        state.mark_channel_read_categories("C1", "2.0"),
        [("at_user".into(), 2)].into()
    );
    assert!(!unread_at(&state, "1.0"));
    assert!(!unread_at(&state, "2.0"));
    for ts in ["3.0", "1.5", "1.1"] {
        assert!(unread_at(&state, ts));
    }
    assert!(state.mark_channel_read_categories("C1", "2.0").is_empty());
    assert!(state.mark_channel_read_categories("C1", "1.0").is_empty());
}

#[test]
fn thread_cursor_requires_exact_channel_root_and_message_bound() {
    let mut state = feed();
    for notification in [
        item("2.0"),
        mention("1.0", "2.1"),
        mention("1.0", "2.2"),
        mention("9.0", "2.3"),
        thread("1.0", "4.0", Some("2.1"), true),
    ] {
        state.upsert(notification);
    }
    let mut other = mention("1.0", "2.05");
    other.item.message.as_mut().unwrap().channel = Some("C2".into());
    state.upsert(other);
    assert_eq!(
        state.mark_thread_read_categories("C1", "1.0", "2.1"),
        [("at_user".into(), 1), ("thread_v2".into(), 1)].into()
    );
    assert!(!unread_at(&state, "2.1"));
    assert!(!unread_at(&state, "4.0"));
    for ts in ["2.0", "2.2", "2.3", "2.05"] {
        assert!(unread_at(&state, ts));
    }
    assert!(
        state
            .mark_thread_read_categories("C1", "1.0", "2.1")
            .is_empty()
    );
    assert!(
        state
            .mark_thread_read_categories("C1", "1.0", "2.0")
            .is_empty()
    );
}

#[test]
fn thread_bundle_maps_first_unread_then_latest_never_root_or_feed_timestamp() {
    for min in [None, Some("")] {
        let mut state = feed();
        let mut notification = thread("1.0", "3.0", min, true);
        notification.feed_ts = "9.0".into();
        assert_eq!(
            notification.read_cursor_target(),
            Some(("C1", Some("1.0"), "3.0"))
        );
        state.upsert(notification);
        assert!(
            state
                .mark_thread_read_categories("C1", "1.0", "2.0")
                .is_empty()
        );
        assert_eq!(
            state.mark_thread_read_categories("C1", "1.0", "3.0"),
            [("thread_v2".into(), 1)].into()
        );
    }
    let mut state = feed();
    state.upsert(thread("1.0", "9.0", Some("2.0"), false));
    assert!(
        state
            .mark_thread_read_categories("C1", "1.0", "2.0")
            .is_empty()
    );
    assert_eq!(
        state.unread_count(),
        0,
        "raw-false bundles clear without category units"
    );
}

#[test]
fn pending_page_cursor_covers_unseen_rows_without_reading_newer_rows() {
    let mut state = feed();
    let request = state.begin(false).unwrap();
    assert!(state.mark_channel_read_categories("C1", "2.0").is_empty());
    assert!(
        state
            .mark_thread_read_categories("C1", "1.0", "2.2")
            .is_empty()
    );
    assert!(state.apply_page(
        &request,
        page(
            vec![
                item("1.0"),
                item("3.0"),
                mention("1.0", "2.1"),
                mention("1.0", "2.3"),
                mention("9.0", "2.0"),
            ],
            Some("next")
        )
    ));
    for ts in ["1.0", "2.1"] {
        assert!(!unread_at(&state, ts));
    }
    for ts in ["3.0", "2.3", "2.0"] {
        assert!(unread_at(&state, ts));
    }
    let next = state.begin(true).unwrap();
    state.mark_channel_read_categories("C1", "4.0");
    state.mark_channel_read_categories("C1", "2.0");
    assert!(state.apply_page(&next, page(vec![item("3.5"), item("5.0")], None)));
    assert!(!unread_at(&state, "3.5"));
    assert!(unread_at(&state, "5.0"));
}

#[test]
fn pending_page_read_order_respects_socket_changes_on_each_side_of_mark() {
    let mut state = feed();
    let request = state.begin(false).unwrap();
    state.upsert(item("1.0"));
    state.mark_channel_read_categories("C1", "2.0");
    state.upsert(item("2.0"));
    state.upsert(item("3.0"));
    assert!(state.apply_page(&request, page(vec![item("1.0"), item("2.0")], None)));
    assert!(!unread_at(&state, "1.0"));
    assert!(
        unread_at(&state, "2.0"),
        "later socket change wins over HTTP baseline read"
    );
    assert!(unread_at(&state, "3.0"));
}

#[test]
fn bundle_read_survives_changed_page_key_but_newer_cursor_target_stays_unread() {
    let mut state = feed();
    let old = thread("1.0", "2.0", Some("2.0"), true);
    state.upsert(old.clone());
    let request = state.begin(false).unwrap();
    assert_eq!(
        state.mark_thread_read_categories("C1", "1.0", "2.0"),
        [("thread_v2".into(), 1)].into()
    );
    let mut newer = thread("1.0", "3.0", Some("2.0"), true);
    newer.key = "new-http-key".into();
    assert!(state.apply_page(&request, page(vec![newer], None)));
    assert_eq!(state.items.len(), 1);
    assert_eq!(state.unread_count(), 0);
    let request = state.begin(false).unwrap();
    state.mark_thread_read_categories("C1", "1.0", "2.0");
    assert!(state.apply_page(
        &request,
        page(vec![thread("1.0", "4.0", Some("3.0"), true)], None)
    ));
    assert_eq!(state.unread_count(), 1);
}

#[test]
fn failed_or_replaced_request_retires_cursor_overlays() {
    let mut state = feed();
    let request = state.begin(false).unwrap();
    state.mark_channel_read_categories("C1", "5.0");
    assert!(state.fail(&request));
    let next = state.begin(false).unwrap();
    assert!(state.apply_page(&next, page(vec![item("1.0")], None)));
    assert!(unread_at(&state, "1.0"));
    let old = state.begin(false).unwrap();
    state.mark_channel_read_categories("C1", "5.0");
    state.ensure_scope(2, "T1");
    let next = state.begin(false).unwrap();
    assert!(!state.apply_page(&old, page(vec![item("1.0")], None)));
    assert!(state.apply_page(&next, page(vec![item("1.0")], None)));
    assert!(unread_at(&state, "1.0"));
}

#[test]
fn raw_false_bundle_read_survives_newer_page_key_without_hiding_later_activity() {
    let mut state = feed();
    state.upsert(thread("1.0", "2.0", Some("2.0"), false));
    let request = state.begin(false).unwrap();
    assert!(
        state
            .mark_thread_read_categories("C1", "1.0", "2.0")
            .is_empty()
    );
    let mut page_bundle = thread("1.0", "3.0", Some("2.0"), false);
    page_bundle.key = "raw-false-new-page-key".into();
    assert!(state.apply_page(&request, page(vec![page_bundle], None)));
    assert_eq!(state.items.len(), 1);
    assert_eq!(state.items[0].key, "raw-false-new-page-key");
    assert_eq!(state.items[0].feed_ts, "3.0");
    assert_eq!(state.unread_count(), 0);
    assert!(
        state
            .mark_thread_read_categories("C1", "1.0", "2.0")
            .is_empty()
    );

    let request = state.begin(false).unwrap();
    state.mark_thread_read_categories("C1", "1.0", "2.0");
    // A later accepted event is authoritative even while the older page is
    // outstanding. Its unread count is separate from the raw category flag.
    let mut later = thread("1.0", "4.0", Some("3.0"), false);
    later.key = "raw-false-later-event".into();
    state.upsert(later);
    assert!(state.apply_page(
        &request,
        page(vec![thread("1.0", "3.5", Some("2.0"), false)], None)
    ));
    assert_eq!(state.items.len(), 1);
    assert_eq!(state.items[0].key, "raw-false-later-event");
    assert_eq!(state.items[0].feed_ts, "4.0");
    assert_eq!(state.unread_count(), 1);
    assert!(
        state
            .mark_thread_read_categories("C1", "1.0", "2.0")
            .is_empty()
    );
    assert_eq!(state.unread_count(), 1);
}

#[test]
fn cursor_timestamps_are_exact_and_invalid_coordinates_do_not_read() {
    let mut state = feed();
    for ts in [
        "1791795731.106028",
        "1791795731.106029",
        "1791795731.106030",
    ] {
        state.upsert(item(ts));
    }
    assert_eq!(
        state.mark_channel_read_categories("C1", "1791795731.106029"),
        [("at_user".into(), 2)].into()
    );
    assert!(unread_at(&state, "1791795731.106030"));
    let mut state = feed();
    state.upsert(item("2.000002"));
    state.upsert(item("2.1"));
    assert_eq!(
        state.mark_channel_read_categories("C1", "2.000002"),
        [("at_user".into(), 1)].into()
    );
    assert!(unread_at(&state, "2.1"));
    for ts in ["", "2", "bad", "2.", "2.bad", "2.1234567", "-2.0"] {
        assert!(state.mark_channel_read_categories("C1", ts).is_empty());
        let mut invalid = item(ts);
        invalid.key = format!("invalid-{ts}");
        state.upsert(invalid);
    }
    let before = state.unread_count();
    assert_eq!(
        state.mark_channel_read_categories("C1", "3.0"),
        [("at_user".into(), 1)].into()
    );
    assert_eq!(state.unread_count(), before - 1);
}

#[test]
fn missing_thread_coordinates_and_timestamps_remain_unread() {
    let mut state = feed();
    for raw in [
        serde_json::json!({"type":"at_user","message":{"channel":"C1"}}),
        serde_json::json!({"type":"thread_reply","message":{"channel":"C1","ts":"2.0"}}),
        serde_json::json!({"type":"thread_v2","bundle_info":{"payload":{"thread_entry":{"channel_id":"C1","thread_ts":"1.0"}}}}),
    ] {
        let notification: ActivityItem =
            serde_json::from_value(serde_json::json!({"item":raw,"is_unread":true})).unwrap();
        assert!(notification.read_cursor_target().is_none());
        state.upsert(notification);
    }
    assert!(state.mark_channel_read_categories("C1", "9.0").is_empty());
    assert!(
        state
            .mark_thread_read_categories("C1", "1.0", "9.0")
            .is_empty()
    );
    assert!(state.items.iter().all(|item| item.is_unread));
}

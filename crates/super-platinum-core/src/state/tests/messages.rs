use super::super::*;
use super::common::msg;
use crate::slack::models::File;

#[test]
fn upsert_keeps_ascending_order_and_dedupes() {
    let mut cm = ChannelMessages::default();
    assert!(cm.upsert(msg("1783372360.741769", "b")));
    assert!(cm.upsert(msg("1783372350.000000", "a")));
    assert!(cm.upsert(msg("1783372370.000000", "c")));
    assert!(!cm.upsert(msg("1783372360.741769", "b-edited")));

    let texts: Vec<_> = cm
        .messages
        .iter()
        .map(|m| m.text.clone().unwrap())
        .collect();
    assert_eq!(texts, vec!["a", "b-edited", "c"]);
}

#[test]
fn remove_by_ts() {
    let mut cm = ChannelMessages::default();
    cm.upsert(msg("100.0", "x"));
    cm.upsert(msg("200.0", "y"));
    assert!(cm.remove("100.0"));
    assert!(!cm.remove("100.0"));
    assert_eq!(cm.messages.len(), 1);
    assert_eq!(cm.messages[0].text.as_deref(), Some("y"));
}

#[test]
fn confirm_reconciles_pending_by_client_msg_id() {
    let mut cm = ChannelMessages::default();
    let mut pending = msg("9999999999.000000", "hi");
    pending.client_msg_id = Some("cid-1".to_owned());
    cm.upsert(pending);
    cm.pending.push("9999999999.000000".to_owned());
    assert!(cm.is_pending("9999999999.000000"));

    let mut confirmed = msg("1783372400.111111", "hi");
    confirmed.client_msg_id = Some("cid-1".to_owned());
    assert!(cm.confirm("cid-1", confirmed));

    assert_eq!(cm.messages.len(), 1);
    assert_eq!(cm.messages[0].ts.as_deref(), Some("1783372400.111111"));
    assert!(!cm.is_pending("1783372400.111111"));
}

#[test]
fn latest_confirmed_ts_ignores_optimistic_pending_messages() {
    let mut cm = ChannelMessages::default();
    cm.upsert(msg("1783372400.111111", "confirmed"));
    cm.upsert(msg("9999999999.000000", "pending"));
    cm.pending.push("9999999999.000000".to_owned());

    assert_eq!(
        cm.latest_confirmed_ts().as_deref(),
        Some("1783372400.111111")
    );
}

#[test]
fn matching_pending_confirm_skips_ambiguous_duplicates() {
    let mut cm = ChannelMessages::default();
    cm.upsert(msg("9999999999.000001", "hi"));
    cm.upsert(msg("9999999999.000002", "hi"));
    cm.pending.push("9999999999.000001".to_owned());
    cm.pending.push("9999999999.000002".to_owned());

    assert!(!cm.confirm_matching_pending(None, Some("hi"), msg("1783372400.111111", "hi")));
    assert_eq!(cm.messages.len(), 2);
    assert!(cm.is_pending("9999999999.000001"));
    assert!(cm.is_pending("9999999999.000002"));
}
#[test]
fn message_text_fallbacks() {
    assert_eq!(message_text(&msg("1.0", "hello")), "hello");

    let file_only = SlackMessage {
        ts: Some("1.0".into()),
        files: vec![File {
            name: Some("mock.png".into()),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(message_text(&file_only), "");

    let empty = SlackMessage {
        ts: Some("1.0".into()),
        text: Some("   ".into()),
        subtype: Some("channel_join".into()),
        ..Default::default()
    };
    assert_eq!(message_text(&empty), "[channel_join]");

    let bare = SlackMessage {
        ts: Some("1.0".into()),
        ..Default::default()
    };
    assert_eq!(message_text(&bare), "[no text]");
}

#[test]
fn visible_message_unwraps_message_replied_envelope() {
    let envelope = SlackMessage {
        subtype: Some("message_replied".into()),
        channel: Some("C1".into()),
        reply_count: Some(2),
        message: Some(Box::new(SlackMessage {
            user: Some("U1".into()),
            ts: Some("1.0".into()),
            text: Some("actual".into()),
            ..Default::default()
        })),
        ..Default::default()
    };

    let visible = visible_message(envelope);
    assert_eq!(visible.user.as_deref(), Some("U1"));
    assert_eq!(visible.text.as_deref(), Some("actual"));
    assert_eq!(visible.channel.as_deref(), Some("C1"));
    assert_eq!(visible.reply_count, Some(2));
    assert_ne!(visible.subtype.as_deref(), Some("message_replied"));
}

#[test]
fn channel_timeline_hides_thread_replies_but_keeps_roots_and_broadcasts() {
    let root = SlackMessage {
        ts: Some("100.0".into()),
        thread_ts: Some("100.0".into()),
        reply_count: Some(3),
        ..Default::default()
    };
    assert!(is_channel_timeline_visible(&root));

    let plain = SlackMessage {
        ts: Some("101.0".into()),
        ..Default::default()
    };
    assert!(is_channel_timeline_visible(&plain));

    let reply = SlackMessage {
        ts: Some("102.0".into()),
        thread_ts: Some("100.0".into()),
        ..Default::default()
    };
    assert!(!is_channel_timeline_visible(&reply));

    let broadcast = SlackMessage {
        ts: Some("103.0".into()),
        thread_ts: Some("100.0".into()),
        subtype: Some("thread_broadcast".into()),
        ..Default::default()
    };
    assert!(is_channel_timeline_visible(&broadcast));

    let replied_envelope = SlackMessage {
        ts: Some("104.0".into()),
        subtype: Some("message_replied".into()),
        ..Default::default()
    };
    assert!(!is_channel_timeline_visible(&replied_envelope));
}
#[test]
fn applies_reaction_add_remove_without_double_counting() {
    let mut cm = ChannelMessages::default();
    cm.upsert(msg("1.0", "hello"));

    assert!(cm.apply_reaction("1.0", "U1", "thumbsup", true));
    assert_eq!(cm.messages[0].reactions[0].count, 1);
    assert_eq!(cm.messages[0].reactions[0].users, vec!["U1"]);

    assert!(!cm.apply_reaction("1.0", "U1", "thumbsup", true));
    assert_eq!(cm.messages[0].reactions[0].count, 1);

    assert!(cm.apply_reaction("1.0", "U2", "thumbsup", true));
    assert_eq!(cm.messages[0].reactions[0].count, 2);

    assert!(cm.apply_reaction("1.0", "U1", "thumbsup", false));
    assert_eq!(cm.messages[0].reactions[0].count, 1);
    assert_eq!(cm.messages[0].reactions[0].users, vec!["U2"]);

    assert!(cm.apply_reaction("1.0", "U2", "thumbsup", false));
    assert!(cm.messages[0].reactions.is_empty());
}

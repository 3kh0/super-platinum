use super::*;
use crate::slack::realtime::parse_event;
use serde_json::json;

fn page() -> ThreadsViewPage {
    serde_json::from_value(json!({"threads":[{
        "root_msg":{"channel":"C1","ts":"1719800400.000000","text":"old root","reply_count":1,"blocks":[{"type":"section"}],"attachments":[{"text":"old root attachment"}],"reactions":[{"name":"eyes","count":1,"users":["U1"]}]},
        "latest_replies":[{"channel":"C1","ts":"1719800401.000000","thread_ts":"1719800400.000000","text":"old reply","blocks":[{"type":"section"}],"attachments":[{"text":"old reply attachment"}]}],
        "unread_replies":[{"channel":"C1","ts":"1719800401.000000","thread_ts":"1719800400.000000","text":"old reply","blocks":[{"type":"section"}],"attachments":[{"text":"old reply attachment"}]}]
    }],"has_more":false})).unwrap()
}
#[test]
fn original_root_and_reply_clears_replay_over_late_index_page_without_resurrecting_rich_body_or_dropping_other_fields()
 {
    let mut core = CoreAppState::new(Default::default());
    core.active_team = Some("T1".into());
    core.threads_view.ensure_scope(core.account_epoch, "T1");
    let first = core.threads_view.begin(false).unwrap();
    assert!(core.apply_thread_feed_page("T1", &first, page(), &Default::default()));
    let pending = core.threads_view.begin(false).unwrap();
    for (ts, text) in [
        ("1719800400.000000", "new root"),
        ("1719800401.000000", "new reply"),
    ] {
        let event=parse_event(&json!({"type":"message","subtype":"message_changed","channel":"C1","message":{"ts":ts,"text":text,"blocks":[],"attachments":[]}}).to_string()).unwrap();
        core.threads_view.observe(&event);
    }
    assert!(core.apply_thread_feed_page("T1", &pending, page(), &Default::default()));
    let item = &core.threads_view.items[0];
    assert_eq!(item.root_msg.text.as_deref(), Some("new root"));
    assert!(item.root_msg.blocks.is_empty() && item.root_msg.attachments.is_empty());
    assert_eq!(item.root_msg.reply_count, Some(1));
    assert_eq!(item.root_msg.reactions[0].name, "eyes");
    for reply in [&item.latest_replies[0], &item.unread_replies[0]] {
        assert_eq!(reply.text.as_deref(), Some("new reply"));
        assert!(reply.blocks.is_empty() && reply.attachments.is_empty());
    }
}

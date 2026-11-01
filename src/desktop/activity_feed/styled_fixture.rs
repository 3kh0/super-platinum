//! Credential-free presentation only; does not create a read/navigation owner.
use super::*;
use serde_json::json;

pub(super) fn install(shell: &mut ShellState) {
    shell.main_view = MainView::Activity;
    shell.overlay = None;
    shell.core.active_account = None;
    shell.core.session = None;
    shell.core.transport = None;
    let epoch = shell.core.account_epoch;
    let feed = &mut shell.core.activity;
    *feed = Default::default();
    feed.ensure_scope(epoch, "T1");
    feed.loaded = true;
    feed.items = serde_json::from_value(json!([{
        "key":"styled-mention", "feed_ts":"1719801000.000100", "is_unread":true,
        "item":{"type":"at_user","message":{"channel":"C1","ts":"1719801000.000100","author_user_id":"U2"}}
    }])).unwrap();
    let message = serde_json::from_value(json!({
        "ts":"1719801000.000100", "user":"U2", "text":"Please review this important update.",
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"Please review "},
            {"type":"text","text":"this important update","style":{"bold":true}},
            {"type":"text","text":"."}
        ]}]}]
    }))
    .unwrap();
    feed.hydrated
        .insert(("C1".into(), "1719801000.000100".into()), message);
}

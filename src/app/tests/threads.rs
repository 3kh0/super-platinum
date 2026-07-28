use super::*;

#[test]
fn threads_page_populates_feed_and_existing_thread_cache() {
    let mut app = test_app();
    app.main_view = MainView::Threads;
    app.threads_view.loading = true;
    app.threads_view.load_seq = 4;
    let mut root = msg("U_ALICE", "1783372300.000100", "Thread root");
    root.channel = Some("C_GENERAL".into());
    root.thread_ts = root.ts.clone();
    let mut reply = msg("U_BOB", "1783372400.000100", "Unread reply");
    reply.channel = Some("C_GENERAL".into());
    reply.thread_ts = root.ts.clone();

    let _ = update(
        &mut app,
        Message::Runtime(RuntimeMessage::ThreadsLoaded {
            team: "T_TEST".into(),
            max_ts: None,
            seq: 4,
            result: Ok(crate::slack::models::ThreadsViewPage {
                threads: vec![crate::slack::models::ThreadViewItem {
                    root_msg: root,
                    unread_replies: vec![reply],
                    ..Default::default()
                }],
                has_more: true,
                max_ts: Some("1783372200.000100".into()),
                ..Default::default()
            }),
        }),
    );

    assert!(app.threads_view.loaded);
    assert!(app.threads_view.has_more);
    assert_eq!(app.threads_view.items.len(), 1);
    let cached = app
        .threads
        .get(&(
            "T_TEST".into(),
            "C_GENERAL".into(),
            "1783372300.000100".into(),
        ))
        .expect("thread cache");
    assert!(cached.loaded);
    assert_eq!(cached.messages.len(), 2);
}

#[test]
fn thread_feed_item_can_target_a_dm() {
    let app = threads_app();
    assert!(app.threads_view.items.iter().any(|item| {
        item.channel() == Some(&"D_ALICE".to_owned())
            && item.root_msg.text.as_deref() == Some("Can you review this before tomorrow?")
    }));
}

#[test]
fn successful_thread_mark_clears_feed_unread_replies() {
    let mut app = threads_app();
    let root_ts = "1783372300.000100".to_owned();
    let _ = update(
        &mut app,
        Message::Conversation(ConversationMessage::ThreadMarked {
            team: "T_TEST".into(),
            channel: "C_GENERAL".into(),
            root_ts: root_ts.clone(),
            ts: "1783372400.000100".into(),
            result: Ok(()),
        }),
    );

    let item = app
        .threads_view
        .items
        .iter()
        .find(|item| item.root_ts() == Some(&root_ts))
        .expect("thread feed item");
    assert!(item.unread_replies.is_empty());
}

#[test]
fn selecting_vip_replaces_the_current_feed() {
    let mut app = threads_app();
    assert!(!app.threads_view.vip_only);
    assert!(!app.threads_view.items.is_empty());

    let _ = update(
        &mut app,
        Message::Runtime(RuntimeMessage::ThreadsVipSelected(true)),
    );

    assert!(app.threads_view.vip_only);
    assert!(app.threads_view.items.is_empty());
    assert!(!app.threads_view.loaded);
}

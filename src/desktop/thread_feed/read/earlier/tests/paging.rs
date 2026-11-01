//! Genuine cursor pages are one owned read, with no visible partial publication.
use super::*;
fn long_item() -> ThreadViewItem {
    let mut i = item(ROOT);
    i.root_msg.reply_count = Some(190);
    i.root_msg.latest_reply = Some(ts(ROOT, 190));
    i.latest_replies = (181..=190).map(|n| message(ROOT, n)).collect();
    i
}
async fn publish_long(state: Signal<ShellState>) {
    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, Mode::Refresh, |_| async {
                Ok(ThreadsViewPage {
                    threads: vec![long_item()],
                    has_more: true,
                    ..Default::default()
                })
            })
            .await
            .is_some()
    );
}
fn first_page() -> Value {
    let mut rows = vec![serde_json::to_value(long_item().root_msg).unwrap()];
    rows.extend((1..=99).map(|n| serde_json::to_value(message(ROOT, n)).unwrap()));
    json!({"ok":true,"messages":rows,"has_more":true,"response_metadata":{"next_cursor":"older:2"}})
}
fn last_page() -> Value {
    json!({"ok":true,"messages":(100..=181).map(|n|message(ROOT,n)).collect::<Vec<_>>(),"has_more":false,"response_metadata":{"next_cursor":""}})
}
#[tokio::test(flavor = "current_thread")]
async fn thread_earlier_network_cursor_mounted_pages_publish_once_retry_original_boundary_and_share_ingress_retirement()
 {
    let (mut dom, mut state, captured) = mount("thread-feed", render);
    state.write().overlay = None;
    let (connection, initial) = realtime::protected_connection(8);
    {
        let mut s = state.write();
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.rt_generation = 8;
        w.rt = super_platinum_core::state::RealtimeStatus::Connected(connection.clone());
    }
    apply_updates(state, initial);
    publish_long(state).await;
    let io = Rc::new(test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    dom.render_immediate_to_vec();
    let id = action(state, ROOT).unwrap().control_id();
    let before = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    let first = io.expect(ROOT, &ts(ROOT, 181));
    let last = io.expect_page(ROOT, &ts(ROOT, 181), Some("older:2"));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &id).unwrap(),
    );
    settle(&mut dom, || io.reads.borrow().len() == 1).await;
    first.send(Ok(first_page())).unwrap();
    settle(&mut dom, || io.reads.borrow().len() == 2).await;
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        before,
        "the genuine first page does not replace the loaded tail"
    );
    assert_eq!(shown(state).len(), 10);
    assert!(
        button(captured.borrow().as_ref().unwrap(), &id)
            .unwrap()
            .disabled
    );
    last.send(Ok(last_page())).unwrap();
    settle(&mut dom, || shown(state).len() == 190).await;
    assert_eq!(shown(state).first(), Some(&ts(ROOT, 1)));
    assert_eq!(shown(state).last(), Some(&ts(ROOT, 190)));
    assert!(text(captured.borrow().as_ref().unwrap()).contains(&format!("[NETWORK {ROOT} 001]")));
    assert!(action(state, ROOT).is_none());

    // A later-page outage keeps all original rows; retry starts from the
    // original boundary, never a cached cursor or a partially accepted prefix.
    publish_long(state).await;
    dom.render_immediate_to_vec();
    let original = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    let first = io.expect(ROOT, &ts(ROOT, 181));
    let last = io.expect_page(ROOT, &ts(ROOT, 181), Some("older:2"));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &id).unwrap(),
    );
    settle(&mut dom, || io.reads.borrow().len() == 3).await;
    first.send(Ok(first_page())).unwrap();
    settle(&mut dom, || io.reads.borrow().len() == 4).await;
    last.send(Err(Error::Offline("second page outage".into())))
        .unwrap();
    settle(&mut dom, || {
        io.completed.get() == 4 && action(state, ROOT).is_some_and(|a| !a.pending())
    })
    .await;
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        original
    );
    // A malformed later wire page also preserves the original tail and
    // cannot be made valid merely by sorting its rows before publication.
    let first = io.expect(ROOT, &ts(ROOT, 181));
    let last = io.expect_page(ROOT, &ts(ROOT, 181), Some("older:2"));
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &id).unwrap(),
    );
    settle(&mut dom, || io.reads.borrow().len() == 5).await;
    first.send(Ok(first_page())).unwrap();
    settle(&mut dom, || io.reads.borrow().len() == 6).await;
    let mut malformed = last_page();
    malformed["messages"].as_array_mut().unwrap().swap(0, 1);
    last.send(Ok(malformed)).unwrap();
    settle(&mut dom, || {
        io.completed.get() == 6 && action(state, ROOT).is_some_and(|a| !a.pending())
    })
    .await;
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        original
    );
    let first = io.expect(ROOT, &ts(ROOT, 181));
    let last = io.expect_page(ROOT, &ts(ROOT, 181), Some("older:2"));
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &id).unwrap(),
    );
    settle(&mut dom, || io.reads.borrow().len() == 7).await;
    first.send(Ok(first_page())).unwrap();
    settle(&mut dom, || io.reads.borrow().len() == 8).await;
    last.send(Ok(last_page())).unwrap();
    settle(&mut dom, || shown(state).len() == 190).await;
    assert_eq!(
        *io.cursors.borrow(),
        vec![
            None,
            Some("older:2".into()),
            None,
            Some("older:2".into()),
            None,
            Some("older:2".into()),
            None,
            Some("older:2".into())
        ]
    );

    // The continuation never rearms the watch. A raw dropped frame during
    // page2 wins even before the native queue has consumed its barrier.
    publish_long(state).await;
    dom.render_immediate_to_vec();
    let first = io.expect(ROOT, &ts(ROOT, 181));
    let last = io.expect_page(ROOT, &ts(ROOT, 181), Some("older:2"));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &id).unwrap(),
    );
    settle(&mut dom, || io.reads.borrow().len() == 9).await;
    first.send(Ok(first_page())).unwrap();
    settle(&mut dom, || io.reads.borrow().len() == 10).await;
    let queued = realtime::protected_updates(
        &connection,
        8,
        r#"{"type":"message","subtype":"message_changed","channel":"C1"}"#,
    );
    last.send(Ok(last_page())).unwrap();
    settle(&mut dom, || io.completed.get() == 10).await;
    assert_eq!(shown(state).len(), 10);
    apply_updates(state, queued);
    assert!(!state.peek().core.thread_open && state.peek().core.profile_pane.is_none());
}

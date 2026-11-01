//! Actual earlier read -> strict human acquisition -> mounted read-only profile.
use super::*;

fn render_profiles(state: Signal<ShellState>) -> Element {
    rsx! { div {
        {crate::view::thread_list_for_test(state, &state.read())}
        {crate::profile::profile_pane(state, &state.read())}
    } }
}
fn raw_member(id: &str) -> Value {
    json!({"id":id,"name":format!("member_{id}"),"team_id":"T1","enterprise_user":null,
        "is_bot":false,"is_app_user":false,"deleted":false,"can_interact":true,
        "profile":{"display_name":format!("Fresh {id}"),"title":"Earlier reply author"}})
}
fn author_response() -> Value {
    let mut raw = response(ROOT);
    for (n, row) in raw["messages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        if (1..=80).contains(&n) {
            row["user"] = json!("UNET");
        }
        if n == 2 {
            // Actual Source sender is the document editor, not this user field.
            row["user"] = json!("UFALLBACK");
            row["document_comment"] = json!({"most_recent_editor":"UEDITOR"});
        }
    }
    raw
}
fn profile(state: Signal<ShellState>, n: usize) -> Option<crate::thread_feed::ProfileOpen> {
    let s = state.peek();
    let item = s
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_ts().is_some_and(|t| t == ROOT))?;
    let message = item
        .latest_replies
        .iter()
        .find(|m| m.ts.as_ref() == Some(&ts(ROOT, n)))?;
    Source::capture(&s)?.profile(&s, item, message)
}
fn change_author(state: Signal<ShellState>, connection: &realtime::Connection, user: &str) {
    let raw = json!({"type":"message","subtype":"message_changed","channel":"C1",
        "message":{"type":"message","channel":"C1","team":"T1","thread_ts":ROOT,
            "ts":ts(ROOT,5),"user":user,"text":"Observed actor change"}});
    apply_updates(
        state,
        realtime::protected_updates(connection, 9, &raw.to_string()),
    );
}

#[tokio::test(flavor = "current_thread")]
async fn thread_network_profiles_mounted_earlier_author_requires_own_raw_intake_and_human_read_without_cache_or_retired_actor_grants()
 {
    let (mut dom, mut state, captured) = mount("thread-feed", render_profiles);
    state.write().overlay = None;
    let (connection, initial) = realtime::protected_connection(9);
    {
        let mut s = state.write();
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.rt_generation = 9;
        w.rt = super_platinum_core::state::RealtimeStatus::Connected(connection.clone());
        w.users.insert(
            "UNET".into(),
            serde_json::from_value(json!({"id":"UNET","name":"Cached only"})).unwrap(),
        );
    }
    apply_updates(state, initial);
    let original_users = serde_json::to_value(&state.peek().core.workspaces["T1"].users).unwrap();
    publish(state).await;
    let replies = Rc::new(test_io::Io::default());
    let members = Rc::new(profiles::test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || {
        provide_context(replies.clone());
        provide_context(members.clone());
    });
    dom.render_immediate_to_vec();
    let id = action(state, ROOT).unwrap().control_id();

    // An outage publishes neither bodies nor profile candidates/member reads.
    let failed = replies.expect(ROOT, &ts(ROOT, 81));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &id).unwrap(),
    );
    settle(&mut dom, || replies.reads.borrow().len() == 1).await;
    failed
        .send(Err(Error::Offline("earlier reply outage".into())))
        .unwrap();
    settle(&mut dom, || {
        action(state, ROOT).is_some_and(|a| !a.pending())
    })
    .await;
    assert_eq!(shown(state).len(), 10);
    assert!(members.reads.borrow().is_empty());

    let raw = replies.expect(ROOT, &ts(ROOT, 81));
    let known = members.expect(vec!["U1".into(), "UNET".into()]);
    dom.render_immediate_to_vec();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &id).unwrap(),
    );
    settle(&mut dom, || replies.reads.borrow().len() == 2).await;
    raw.send(Ok(author_response())).unwrap();
    settle(&mut dom, || members.reads.borrow().len() == 1).await;
    assert_eq!(shown(state).len(), 90);
    assert!(text(captured.borrow().as_ref().unwrap()).contains(&format!("[NETWORK {ROOT} 001]")));
    assert!(
        profile(state, 1).is_none(),
        "cached user and successful replies are not a human-member receipt"
    );
    assert!(
        profile(state, 2).is_none(),
        "document-comment sender override never uses raw user as a profile grant"
    );
    assert_eq!(
        *members.reads.borrow(),
        vec![vec!["U1".to_owned(), "UNET".to_owned()]]
    );
    // A held member response cannot retain ANY completed reply-read watch.
    let watches = (0..16)
        .map(|_| {
            connection
                .watch_message_read("C1")
                .expect("member IO released reply watch")
        })
        .collect::<Vec<_>>();
    assert!(connection.watch_message_read("C1").is_none());
    drop(watches);

    // Native observed identity loss while the real member read is pending is
    // sticky for that occurrence, even when the same public author returns.
    change_author(state, &connection, "UOTHER");
    change_author(state, &connection, "UNET");
    known
        .send(Ok(json!({"results":[raw_member("U1"),raw_member("UNET")]})))
        .unwrap();
    settle(&mut dom, || {
        members.completed.get() == 1 && profile(state, 1).is_some()
    })
    .await;
    assert!(profile(state, 5).is_none());
    assert!(profile(state, 2).is_none());
    // A genuine later reply retires the earlier-read window, not this separate
    // profile occurrence. Its own selected author/member lineage remains live.
    let arrival = json!({"type":"message","channel":"C1","team":"T1",
        "thread_ts":ROOT,"ts":ts(ROOT,91),"user":"U1","text":"Actual later reply"});
    apply_updates(
        state,
        realtime::protected_updates(&connection, 9, &arrival.to_string()),
    );
    dom.render_immediate_to_vec();
    assert_eq!(shown(state).len(), 91);
    assert_eq!(shown(state).first(), Some(&ts(ROOT, 1)));
    assert!(profile(state, 1).is_some());
    let owned = profile(state, 1).unwrap();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Fresh UNET"));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &owned.control_id()).unwrap(),
    );
    dom.render_immediate_to_vec();
    assert_eq!(state.peek().profile_user.as_deref(), Some("UNET"));
    let body = text(captured.borrow().as_ref().unwrap());
    assert!(body.contains("Read-only profile details") && body.contains("Earlier reply author"));
    assert!(!body.contains("Huddle"));
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces["T1"].users).unwrap(),
        original_users
    );
    state.write().close_profile();
    dom.render_immediate_to_vec();
    assert!(
        profile(state, 1).is_some(),
        "Close preserves this independently accepted read-only member version"
    );

    // New actual getView+earlier intake, then an observed account retirement:
    // a delayed human response cannot publish after public scalar restoration.
    // Use a distinct native owner. The first owner's actual native edits keep
    // their canonical copies; the earlier reader must never overwrite those.
    let prior_state = state;
    let prior_messages = format!("{:?}", &prior_state.peek().core.workspaces["T1"].messages);
    assert!(
        prior_state.peek().core.workspaces["T1"].messages["C1"]
            .messages
            .iter()
            .any(|m| m.ts.as_ref() == Some(&ts(ROOT, 5)))
    );
    let (mut dom, mut state, captured) = mount("thread-feed", render_profiles);
    state.write().overlay = None;
    let (next_connection, initial) = realtime::protected_connection(10);
    {
        let mut s = state.write();
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.rt_generation = 10;
        w.rt = super_platinum_core::state::RealtimeStatus::Connected(next_connection);
    }
    apply_updates(state, initial);
    dom.in_scope(ScopeId::ROOT, || {
        provide_context(replies.clone());
        provide_context(members.clone());
    });
    let original_users = serde_json::to_value(&state.peek().core.workspaces["T1"].users).unwrap();
    assert_eq!(
        format!("{:?}", &prior_state.peek().core.workspaces["T1"].messages),
        prior_messages
    );
    publish(state).await;
    dom.render_immediate_to_vec();
    let raw = replies.expect(ROOT, &ts(ROOT, 81));
    let late = members.expect(vec!["U1".into(), "UNET".into()]);
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &id).unwrap(),
    );
    settle(&mut dom, || replies.reads.borrow().len() == 3).await;
    raw.send(Ok(author_response())).unwrap();
    settle(&mut dom, || members.reads.borrow().len() == 2).await;
    let epoch = state.peek().core.account_epoch;
    let observed = Source::capture(&state.peek()).unwrap();
    state.write().core.account_epoch = epoch + 1;
    {
        // Invoke the already-captured original reader, just as a queued render
        // callback would. A fresh capture is not needed to observe retirement.
        let s = state.peek();
        let item = s
            .core
            .threads_view
            .items
            .iter()
            .find(|i| i.root_ts().is_some_and(|t| t == ROOT))
            .unwrap();
        let message = item
            .latest_replies
            .iter()
            .find(|m| m.ts.as_ref() == Some(&ts(ROOT, 1)))
            .unwrap();
        assert!(observed.profile(&s, item, message).is_none());
    }
    assert!(profile(state, 1).is_none());
    state.write().core.account_epoch = epoch;
    late.send(Ok(json!({"results":[raw_member("U1"),raw_member("UNET")]})))
        .unwrap();
    settle(&mut dom, || members.completed.get() == 2).await;
    assert!(profile(state, 1).is_none() && state.peek().core.profile_pane.is_none());
    owned.start(state);
    assert!(state.peek().core.profile_pane.is_none());
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces["T1"].users).unwrap(),
        original_users
    );
}

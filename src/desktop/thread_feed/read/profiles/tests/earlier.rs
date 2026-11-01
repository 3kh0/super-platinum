//! Actual local reveal→original missing-member read→earliest owned profile.
use super::*;
use crate::resource_reads::render_test::settle;
const ROOT: &str = "1719800000.000000";
const PEER: &str = "1719700000.000000";
fn long_item(root: &str, prefix: &str) -> Value {
    let seconds = root.split('.').next().unwrap().parse::<u64>().unwrap() + 1;
    json!({"root_msg":{"type":"message","channel":"C1","team":"T1","ts":root,"thread_ts":root,"subscribed":true,"user":format!("{prefix}ROOT"),"reply_count":160,"text":"Original long discussion"},
        "latest_replies":(0..160).map(|n| json!({"type":"message","channel":"C1","team":"T1","thread_ts":root,"ts":format!("{seconds}.{:06}",n+1),"user":format!("{prefix}{n:03}"),"text":format!("[{prefix} EARLIER {n:03}]")})).collect::<Vec<_>>()})
}
fn first(state: Signal<ShellState>, root: &str) -> Option<Open> {
    at(state, root, 0)
}
fn at(state: Signal<ShellState>, root: &str, index: usize) -> Option<Open> {
    let shell = state.peek();
    let item = shell
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_ts().is_some_and(|r| r == root))?;
    let message = item.latest_replies.get(index)?;
    Source::capture(&shell)?.profile(&shell, item, message)
}
fn earlier(state: Signal<ShellState>, root: &str) -> super::super::super::windows::Action {
    let shell = state.peek();
    let item = shell
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_ts().is_some_and(|r| r == root))
        .unwrap();
    Source::capture(&shell)
        .unwrap()
        .earlier_replies(&shell, item)
        .unwrap()
}
fn ids(request: super_platinum_core::slack::PreparedRequest) -> Vec<String> {
    let super_platinum_core::slack::client::RequestBody::Json(body) = request.body else {
        panic!()
    };
    let mut ids = body["updated_ids"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    ids.sort();
    ids
}
fn response(ids: &[String]) -> Value {
    json!({"results":ids.iter().map(|id|member(id,&format!("Fresh {id}"))).collect::<Vec<_>>()})
}
fn observed_actor(mut state: Signal<ShellState>, user: &str) {
    let mut shell = state.write();
    let message = serde_json::from_value(json!({"type":"message","channel":"C1","team":"T1","thread_ts":ROOT,"ts":"1719800001.000002","user":user})).unwrap();
    shell.core.threads_view.observe(
        &super_platinum_core::slack::events::RtEvent::MessageChanged {
            channel: "C1".into(),
            message,
            original_update: None,
        },
    );
    let generation = shell.core.workspaces["T1"].rt_generation;
    crate::thread_feed::after_native_mutation(&shell, "T1", generation);
}

#[tokio::test(flavor = "current_thread")]
async fn thread_profile_earlier_original_candidates_follow_mounted_reveal_without_cache_or_retired_actor_grants()
 {
    let h = harness();
    let mut initial = h.state;
    setup(initial);
    for id in ["U000", "U001", "U002", "UV000", "UV002"] {
        initial
            .write()
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .users
            .insert(
                id.into(),
                serde_json::from_value(json!({"id":id,"name":"Cached only"})).unwrap(),
            );
    }
    let before = users(initial);
    let reader = source(initial);
    let packet: ThreadsViewPage = serde_json::from_value(
        json!({"threads":[long_item(ROOT,"U"),long_item(PEER,"UV")],"has_more":false}),
    )
    .unwrap();
    assert!(
        reader
            .run_with(initial, Mode::Refresh, |_| async { Ok(packet) })
            .await
            .is_some()
    );
    // Probing an unshown actor is itself an observed refusal under the ORIGINAL
    // guard. Keep that refusal on U002; reserve U000 for the positive journey.
    assert!(at(initial, ROOT, 2).is_none() && at(initial, PEER, 2).is_none());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let seen = calls.clone();
    members::load_all_with(reader, initial, move |_, request| {
        let ids = ids(request);
        seen.borrow_mut().push(ids.clone());
        async move { Ok(response(&ids)) }
    })
    .await;
    assert_eq!(
        calls.borrow().iter().map(Vec::len).collect::<Vec<_>>(),
        vec![100, 100, 2]
    );
    assert!(calls.borrow().iter().flatten().all(|id| {
        id.ends_with("ROOT")
            || id
                .trim_start_matches("UV")
                .trim_start_matches('U')
                .parse::<usize>()
                .unwrap()
                >= 60
    }));
    assert!(
        at(initial, ROOT, 2).is_none() && at(initial, PEER, 2).is_none(),
        "unshown data is neither an opener nor a cache grant"
    );
    assert_eq!(users(initial), before);
    // Observed loss while deferred must latch before the same actor is restored.
    observed_actor(initial, "U999");
    observed_actor(initial, "U001");
    let shell = std::mem::replace(
        &mut *initial.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    let (mut dom, mut state, captured) = mount(shell);
    let io = Rc::new(test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    let expected = (0..60)
        .filter(|n| ![1, 2].contains(n))
        .map(|n| format!("U{n:03}"))
        .collect::<Vec<_>>();
    let reply = io.expect(expected.clone());
    let saved = earlier(state, ROOT);
    let queued_peer = earlier(state, PEER);
    let data = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &saved.control_id()).unwrap(),
    );
    settle(&mut dom, || !io.reads.borrow().is_empty()).await;
    assert_eq!(*io.reads.borrow(), vec![expected.clone()]);
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        data
    );
    assert!(text(captured.borrow().as_ref().unwrap()).contains("[U EARLIER 000]"));
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("[UV EARLIER 000]"));
    assert!(
        first(state, ROOT).is_none(),
        "a newly visible cached user still needs the original raw member read"
    );
    saved.reveal(state);
    queued_peer.reveal(state);
    dom.render_immediate_to_vec();
    assert_eq!(
        io.reads.borrow().len(),
        1,
        "queued old/peer windows cannot restart acquisition"
    );
    reply.send(Ok(response(&expected))).unwrap();
    settle(&mut dom, || {
        io.completed.get() == 1 && first(state, ROOT).is_some()
    })
    .await;
    assert!(first(state, PEER).is_none());
    let denied = {
        let shell = state.peek();
        let item = shell
            .core
            .threads_view
            .items
            .iter()
            .find(|i| i.root_ts().is_some_and(|r| r == ROOT))
            .unwrap();
        Source::capture(&shell)
            .unwrap()
            .profile(&shell, item, &item.latest_replies[1])
    };
    assert!(
        denied.is_none(),
        "observed retired deferred actor cannot revive through the restored row/cache"
    );
    assert!(
        at(state, ROOT, 2).is_none(),
        "the original unshown-opener refusal stays retired after reveal"
    );
    let header = first(state, ROOT).unwrap();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Fresh U000"));
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &header.control_id()).unwrap(),
    );
    dom.render_immediate_to_vec();
    assert_eq!(state.peek().profile_user.as_deref(), Some("U000"));
    let body = text(captured.borrow().as_ref().unwrap());
    assert!(body.contains("Read-only profile details") && body.contains("Fresh U000"));
    assert!(!body.contains("Huddle"));
    {
        let shell = state.peek();
        let pane = shell.core.profile_pane.as_ref().unwrap();
        assert_eq!(pane.user, "U000");
        assert!(!pane.loading && pane.error.is_none());
    }
    assert_eq!(users(state), before);
    assert_eq!(io.reads.borrow().len(), 1);
    state.write().close_profile();
    dom.render_immediate_to_vec();
    assert!(
        first(state, ROOT).is_some(),
        "Close/reopen keeps the accepted original member version"
    );
}

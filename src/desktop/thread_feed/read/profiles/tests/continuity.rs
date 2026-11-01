//! A real100+20 member chain interleaved with its owned profile UI transition.
use super::*;
use crate::resource_reads::render_test::settle;

fn roster() -> ThreadsViewPage {
    let threads=(0..30).map(|i| {
        let seconds=1719800000u64+i*10;
        let root=format!("{seconds}.000000");
        json!({"root_msg":{"type":"message","channel":"C1","team":"T1","ts":root,"thread_ts":root,"subscribed":true,"user":format!("U{:03}",i*4),"reply_count":3,"text":"Continuity discussion"},
            "latest_replies":(1..=3).map(|n|json!({"type":"message","channel":"C1","team":"T1","thread_ts":root,"ts":format!("{}.{n:06}",seconds+1),"user":format!("U{:03}",i*4+n),"text":"Continuity reply"})).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    serde_json::from_value(json!({"threads":threads,"has_more":false})).unwrap()
}
fn author(state: Signal<ShellState>, id: &str) -> Option<Open> {
    let s = state.peek();
    let source = Source::capture(&s)?;
    for item in &s.core.threads_view.items {
        for message in std::iter::once(&item.root_msg)
            .chain(&item.latest_replies)
            .chain(&item.unread_replies)
        {
            if message.user.as_deref() == Some(id) {
                return source.profile(&s, item, message);
            }
        }
    }
    None
}
fn ids(from: usize, to: usize) -> Vec<String> {
    (from..to).map(|i| format!("U{i:03}")).collect()
}
fn response(from: usize, to: usize) -> Value {
    json!({"results":ids(from,to).into_iter().map(|id|member(&id,&format!("Continuity {id}"))).collect::<Vec<_>>()})
}
async fn admit(state: Signal<ShellState>) -> Source {
    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, Mode::Refresh, |_| async { Ok(roster()) })
            .await
            .is_some()
    );
    source
}
fn start(dom: &VirtualDom, source: Source, state: Signal<ShellState>) {
    dom.in_scope(ScopeId::ROOT, || {
        dioxus::core::spawn_forever(super::super::load(source, state));
    });
}

#[tokio::test(flavor = "current_thread")]
async fn thread_profile_continuity_owned_open_keeps_held_remainder_without_reauthorizing_stale_or_unowned_changes()
 {
    let h = harness();
    let mut initial = h.state;
    setup(initial);
    initial
        .write()
        .begin_thread("1719800000.000000".into(), None);
    let visit = crate::bootstrap::reconnect::resource_test_visit(&mut initial.write());
    let reader = Source::capture_with(&initial.peek(), Some(visit)).unwrap();
    assert!(
        reader
            .run_with(initial, Mode::Refresh, |_| async { Ok(roster()) })
            .await
            .is_some()
    );
    let original_users = users(initial);
    let shell = std::mem::replace(
        &mut *initial.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    let (mut dom, mut state, captured) = mount(shell);
    let io = Rc::new(test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    // A transient pre-admission refusal must not consume the original roster.
    let denied_reads = std::cell::Cell::new(0);
    state.write().core.threads_view.loading = true;
    members::load_with(reader.clone(), state, |_, _| {
        denied_reads.set(denied_reads.get() + 1);
        async { Err(Error::Offline("Unreachable refused admission".into())) }
    })
    .await;
    assert_eq!(denied_reads.get(), 0);
    assert!(
        !state
            .peek()
            .thread_feed_reads
            .profiles
            .as_ref()
            .unwrap()
            .started
            .load(Ordering::Acquire)
    );
    state.write().core.threads_view.loading = false;
    let first = io.expect(ids(0, 100));
    let remainder = io.expect(ids(100, 120));
    start(&dom, reader.clone(), state);
    settle(&mut dom, || io.reads.borrow().len() == 1).await;
    // A later reconnect serial does not retire this already-admitted REST chain.
    state.write().reconnect_generation += 1;
    assert!(reader.admitted_current(&state.peek()));
    first.send(Ok(response(0, 100))).unwrap();
    settle(&mut dom, || {
        io.reads.borrow().len() == 2 && author(state, "U000").is_some()
    })
    .await;
    assert!(author(state, "U119").is_none());
    let owned = author(state, "U000").unwrap();
    let stale = author(state, "U001").unwrap();
    let before = state.peek().thread_generation;
    assert!(state.peek().thread_root.is_some());
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &owned.control_id()).unwrap(),
    );
    dom.render_immediate_to_vec();
    assert_eq!(
        state.peek().thread_generation,
        before.checked_add(1).unwrap()
    );
    assert!(state.peek().thread_root.is_none());
    assert_eq!(state.peek().profile_user.as_deref(), Some("U000"));
    assert!(
        !reader.admitted_current(&state.peek()),
        "Source and reconnect Visit generations remain unchanged and strict"
    );
    stale.start(state);
    assert_eq!(state.peek().profile_user.as_deref(), Some("U000"));
    assert_eq!(
        state.peek().thread_generation,
        before + 1,
        "queued old callbacks cannot consume the private permitted generation"
    );
    remainder.send(Ok(response(100, 120))).unwrap();
    settle(&mut dom, || {
        io.completed.get() == 2 && author(state, "U119").is_some()
    })
    .await;
    assert_eq!(
        state.peek().profile_user.as_deref(),
        Some("U000"),
        "later member publication never retargets the already-open pane"
    );
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Continuity U119"));
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Read-only profile details"));
    assert_eq!(*io.reads.borrow(), vec![ids(0, 100), ids(100, 120)]);
    assert_eq!(users(state), original_users);
    state.write().close_profile();
    dom.render_immediate_to_vec();
    let last = author(state, "U119").unwrap();
    click(
        &dom,
        button(captured.borrow().as_ref().unwrap(), &last.control_id()).unwrap(),
    );
    dom.render_immediate_to_vec();
    assert_eq!(state.peek().profile_user.as_deref(), Some("U119"));
    state.write().close_profile();
    dom.render_immediate_to_vec();

    // A new ORIGINAL page starts a separate acquisition. The same generation
    // change without an owned successful profile commit must still retire it.
    let again = admit(state).await;
    let first = io.expect(ids(0, 100));
    let remainder = io.expect(ids(100, 120));
    start(&dom, again.clone(), state);
    settle(&mut dom, || io.reads.borrow().len() == 3).await;
    first.send(Ok(response(0, 100))).unwrap();
    settle(&mut dom, || {
        io.reads.borrow().len() == 4 && author(state, "U000").is_some()
    })
    .await;
    let before = state.peek().thread_generation;
    state.write().close_thread();
    remainder.send(Ok(response(100, 120))).unwrap();
    settle(&mut dom, || io.completed.get() == 4).await;
    assert!(author(state, "U119").is_none());
    assert!(
        author(state, "U000").is_some(),
        "already-loaded genuine members remain readable"
    );
    state.write().thread_generation = before;
    start(&dom, again, state);
    dom.render_immediate_to_vec();
    for _ in 0..4 {
        dom.render_immediate_to_vec();
        tokio::task::yield_now().await;
    }
    assert_eq!(
        io.reads.borrow().len(),
        4,
        "public scalar restoration cannot restart the retired original chain"
    );
    assert!(author(state, "U119").is_none());
    assert_eq!(users(state), original_users);
}

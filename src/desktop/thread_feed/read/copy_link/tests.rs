//! Original intake -> real account producer -> fresh data -> mounted callback.
//! Clipboard IO is an explicit model; it cannot mint a native promise receipt.
use super::*;
use crate::resource_reads::render_test::{self, button, click, settle};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::realtime::{self, RtUpdate},
    state::posting_metadata::authority::BootRead,
};
const T: &str = "T12345678";
const TS: &str = "1700000001.000001";
const URL: &str = "https://copyspace.enterprise.slack.com/archives/C1/p1700000001000001";
fn page() -> ThreadsViewPage {
    serde_json::from_value(json!({"threads":[{"root_msg":{"type":"message","channel":"C1","team":T,"ts":TS,"thread_ts":TS,"subscribed":true,"user":"U0","reply_count":1,"text":"Original root"},"latest_replies":[{"type":"message","channel":"C1","team":T,"ts":"1700000002.000001","thread_ts":TS,"user":"U0","text":"Reply"}]}],"has_more":false})).unwrap()
}
fn shell(
    epoch: u64,
    generation: u64,
    connection: realtime::Connection,
    transport: Arc<Transport>,
) -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    let old = s.core.workspaces.remove("T1").unwrap();
    let session = WorkspaceSession {
        team_id: T.into(),
        user_id: "U0".into(),
        name: "Copy fixture".into(),
        url: "https://session-must-not-qualify.invalid".into(),
        token: "synthetic-thread-copy".into(),
        enterprise_id: None,
    };
    let mut w = super_platinum_core::state::Workspace::from_session(&session);
    w.channels = old.channels;
    w.users = old.users;
    w.rt_generation = generation;
    w.rt = super_platinum_core::state::RealtimeStatus::Connected(connection);
    s.core.workspaces.clear();
    s.core.workspaces.insert(T.into(), w);
    s.core.active_team = Some(T.into());
    s.core.active_channel = Some("C1".into());
    s.core.active_account = Some("thread-copy-account".into());
    s.core.account_epoch = epoch;
    s.core.transport = Some(transport);
    s.core.session = Some(Session {
        d_cookie: "synthetic-thread-copy".into(),
        workspaces: [(T.into(), session)].into(),
        unified_user_client_enabled: [(T.into(), false)].into(),
    });
    s.main_view = MainView::Threads;
    s.overlay = None;
    s.core.threads_view = Default::default();
    s.core.threads_view.ensure_scope(epoch, T);
    // Explicit model of the original observed route, not an OS network probe.
    s.connection.routable = true;
    s.connection.routed_at = Some(std::time::Instant::now());
    s
}
fn boot(mut state: Signal<ShellState>) {
    let context = crate::scheduled::Context::capture(&state.peek()).unwrap();
    let mut s = state.write();
    let request = BootRead::begin(&mut s.core, &context.transport, &context.session).unwrap();
    assert!(request.apply(&mut s.core,json!({"ok":true,"team":{"id":T,"plan":"plus"},"self":{"id":"U0","team_id":T},"workspaces":[{"id":T}],"read_only_channels":[],"thread_only_channels":[]}),false).unwrap());
}
fn apply(mut state: Signal<ShellState>, updates: Vec<RtUpdate>) {
    for u in updates {
        match u {
            RtUpdate::ReadBarrier {
                generation,
                barrier,
            } => {
                crate::thread_feed::accept_read_barrier(&mut state.write(), T, generation, barrier)
            }
            RtUpdate::Connected { .. } => (),
            RtUpdate::Event { generation, event } => crate::realtime::apply_in_view_with_context(
                &mut state.write(),
                T,
                generation,
                *event,
                true,
                true,
            ),
            _ => panic!("only original data/barrier fixture"),
        }
    }
}
fn mount(
    s: ShellState,
) -> (
    VirtualDom,
    Signal<ShellState>,
    render_test::Captured,
    test_io::Rendered,
) {
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let captured = render_test::Captured::default();
    let ready = test_io::Rendered::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot, captured, ready): (
            Input,
            Slot,
            render_test::Captured,
            test_io::Rendered,
        )| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            use_hook(|| provide_context(ready));
            let rendered = crate::view::thread_list_for_test(state, &state.read());
            *captured.borrow_mut() = Some(rendered.as_ref().unwrap().clone());
            rendered
        },
        (
            Rc::new(RefCell::new(Some(s))),
            slot.clone(),
            captured.clone(),
            ready.clone(),
        ),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), captured, ready)
}
async fn admit(state: Signal<ShellState>) {
    let source = Source::capture(&state.peek()).unwrap();
    assert!(
        source
            .run_with(state, Mode::Refresh, |_| async { Ok(page()) })
            .await
            .is_some()
    );
}
fn action(state: Signal<ShellState>) -> Action {
    let s = state.peek();
    let source = Source::capture(&s).unwrap();
    source.copy_link(&s, &s.core.threads_view.items[0]).unwrap()
}
fn send(reply: tokio::sync::oneshot::Sender<Result<Vec<u8>, Error>>, v: Value) {
    reply.send(Ok(serde_json::to_vec(&v).unwrap())).unwrap();
}
fn calls(io: &test_io::Io) -> usize {
    io.calls.lock().unwrap().len()
}
async fn prepare(
    dom: &mut VirtualDom,
    state: Signal<ShellState>,
    captured: &render_test::Captured,
    io: &test_io::Io,
    cold: bool,
    context: &str,
) {
    let n = calls(io);
    let g = cold.then(|| io.expect("client.gantryBoot"));
    let prefs = cold.then(|| io.expect("users.prefs.get"));
    let experiments = cold.then(|| io.expect("experiments.getByUser"));
    let history = io.expect("conversations.history");
    let info = io.expect("conversations.info");
    dom.render_immediate_to_vec();
    let a = action(state);
    let control = button(captured.borrow().as_ref().unwrap(), &a.button_id())
        .expect("actual current Copy preparation control");
    assert!(!control.disabled);
    click(dom, control);
    let mut next = n;
    if let Some(g) = g {
        next += 1;
        settle(dom, || calls(io) == next).await;
        send(
            g,
            json!({"ok":true,"self":{"id":"U0"},"team":{"id":T},"workspaces":[{"id":T,"url":"https://copyspace.enterprise.slack.com/","prefs":{"display_real_names":true}}]}),
        );
        next += 1;
        settle(dom, || calls(io) == next).await;
        send(
            prefs.unwrap(),
            json!({"ok":true,"prefs":{"display_real_names_override":1,"hidden_users":"","frecency":"{}"}}),
        );
        next += 1;
        settle(dom, || calls(io) == next).await;
        send(experiments.unwrap(), json!({"ok":true,"assignments":{}}));
    }
    next += 1;
    settle(dom, || calls(io) == next).await;
    send(
        history,
        json!({"ok":true,"messages":[{"type":"message","channel":"C1","ts":TS,"thread_ts":TS,"user":"U0","text":"Fresh exact root"}]}),
    );
    next += 1;
    settle(dom, || calls(io) == next).await;
    send(
        info,
        json!({"ok":true,"channel":{"id":"C1","name":"general","is_channel":true,"is_member":true,"is_private":false,"is_org_shared":false,"context_team_id":context}}),
    );
    settle(dom, || {
        state
            .peek()
            .thread_feed_reads
            .copy_link
            .intent
            .as_ref()
            .is_some_and(|p| p.phase.load(Ordering::Acquire) != 0)
    })
    .await;
}
fn event(peer: &test_io::Peer, c: &Config, kind: &str, seq: u64, outcome: Option<&str>) {
    let mut v = json!({"kind":kind,"id":c.id,"mount":c.mount,"nonce":c.nonce,"seq":seq});
    if let Some(o) = outcome {
        v["outcome"] = json!(o)
    }
    peer.to_rust.send(v).unwrap();
}
async fn receive(dom: &mut VirtualDom, peer: &mut test_io::Peer) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(3),async{loop{
        dom.render_immediate_to_vec();tokio::select!{v=peer.from_rust.recv()=>return v.expect("owned model reply"),_=dom.wait_for_work()=>()}
    }}).await.expect("model promise protocol settles")
}
async fn mounted(
    dom: &mut VirtualDom,
    state: Signal<ShellState>,
    ready: &test_io::Rendered,
    io: &test_io::Io,
) -> (test_io::Peer, Config) {
    let peer = io.bridge();
    let n = io.configs.lock().unwrap().len();
    dom.render_immediate_to_vec();
    let value = ready.0.borrow();
    let v = value.as_ref().expect("actual ReadyControl component body");
    let listener = v
        .dynamic_attrs
        .iter()
        .flat_map(|a| a.iter())
        .find(|a| a.name == "onmounted")
        .expect("actual mounted callback")
        .value
        .clone();
    drop(value);
    let dioxus::core::AttributeValue::Listener(callback) = listener else {
        panic!("mounted listener")
    };
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    dom.in_scope(ScopeId::ROOT, || {
        callback.call(dioxus::core::Event::new(
            dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default()).into_any(),
            false,
        ))
    });
    // Pinned Dioxus converts this headless event to MountedData::from(()), an
    // unsupported backing. Only inert IO follows; no native mount is asserted.
    settle(dom, || io.configs.lock().unwrap().len() == n + 1).await;
    let c = io.configs.lock().unwrap().last().unwrap().clone();
    event(&peer, &c, "mounted", 0, None);
    settle(dom, || {
        state
            .peek()
            .thread_feed_reads
            .copy_link
            .intent
            .as_ref()
            .unwrap()
            .native_ready
            .load(Ordering::Acquire)
    })
    .await;
    (peer, c)
}
#[tokio::test(flavor = "current_thread")]
async fn thread_copy_owned_root_cold_policy_exact_link_model_promise_and_retired_shell_journey() {
    let transport = crate::bootstrap::test_support::transport("synthetic-thread-copy");
    let (conn, initial) = realtime::protected_connection(12);
    let io = Arc::new(test_io::Io::new("synthetic-thread-copy"));
    let mut initial_shell = shell(0, 12, conn, transport.clone());
    initial_shell.thread_feed_reads.copy_link.io = Some(io.clone());
    let (mut dom, mut state, captured, ready) = mount(initial_shell);
    apply(state, initial);
    boot(state);
    // Public Core publication and a loaded vector cannot produce Copy intake.
    {
        let mut s = state.write();
        let r = s.core.threads_view.begin(false).unwrap();
        assert!(
            s.core
                .apply_thread_feed_page(T, &r, page(), &Default::default())
        );
    }
    assert!(state.peek().thread_feed_reads.copy_link.scope.is_none());
    assert_eq!(calls(&io), 0);
    admit(state).await;
    assert!(action(state).current(&state.peek()));
    let canonical = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    prepare(&mut dom, state, &captured, &io, true, "T99999999").await;
    assert_eq!(
        state
            .peek()
            .thread_feed_reads
            .copy_link
            .intent
            .as_ref()
            .unwrap()
            .phase
            .load(Ordering::Acquire),
        3
    );
    assert!(
        io.configs.lock().unwrap().is_empty(),
        "wrong context never opens a clipboard control"
    );
    prepare(&mut dom, state, &captured, &io, false, T).await;
    let (mut peer, c) = mounted(&mut dom, state, &ready, &io).await;
    event(&peer, &c, "pressed", 1, None);
    let permit = receive(&mut dom, &mut peer).await;
    assert_eq!(permit["kind"], "permit");
    assert_eq!(permit["url"], URL);
    event(&peer, &c, "settled", 1, Some("fulfilled"));
    assert_eq!(receive(&mut dom, &mut peer).await["kind"], "ack");
    let receipt = state
        .peek()
        .thread_feed_reads
        .copy_link
        .registry
        .receipt()
        .unwrap();
    assert!(receipt.outcome == registry::Outcome::Fulfilled);
    assert!(
        !receipt.native,
        "model IO cannot manufacture native completion"
    );
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        canonical,
        "Copy reads never replace canonical/feed rows"
    );
    event(&peer, &c, "pressed", 2, None);
    assert_eq!(receive(&mut dom, &mut peer).await["url"], URL);
    event(&peer, &c, "settled", 2, Some("rejected"));
    assert_eq!(receive(&mut dom, &mut peer).await["kind"], "ack");
    assert!(!state.peek().thread_feed_reads.copy_link.registry.blocked());
    event(&peer, &c, "pressed", 3, None);
    assert_eq!(receive(&mut dom, &mut peer).await["kind"], "permit");
    let (new_conn, updates) = realtime::protected_connection(13);
    let mut loaded = shell(1, 13, new_conn, transport);
    loaded.thread_feed_reads.copy_link.io = Some(io.clone());
    dom.in_scope(ScopeId::ROOT, || {
        crate::bootstrap::install_loaded(state, loaded)
    });
    apply(state, updates);
    boot(state);
    admit(state).await;
    let n = calls(&io);
    dom.render_immediate_to_vec();
    let a = action(state);
    let b = button(captured.borrow().as_ref().unwrap(), &a.button_id()).unwrap();
    click(&dom, b);
    assert_eq!(calls(&io), n);
    assert!(
        state.peek().thread_feed_reads.copy_link.registry.blocked(),
        "issued original promise owns physical slot across actual Shell replacement"
    );
    event(&peer, &c, "settled", 3, Some("fulfilled"));
    assert_eq!(receive(&mut dom, &mut peer).await["kind"], "ack");
    assert!(!state.peek().thread_feed_reads.copy_link.registry.blocked());
    event(&peer, &c, "retired", 3, None);
    prepare(&mut dom, state, &captured, &io, true, T).await;
    let (mut peer, c) = mounted(&mut dom, state, &ready, &io).await;
    event(&peer, &c, "pressed", 1, None);
    assert_eq!(receive(&mut dom, &mut peer).await["kind"], "permit");
    peer.from_rust.close();
    event(&peer, &c, "settled", 1, Some("fulfilled"));
    settle(&mut dom, || {
        state
            .peek()
            .thread_feed_reads
            .copy_link
            .intent
            .as_ref()
            .unwrap()
            .phase
            .load(Ordering::Acquire)
            == 3
    })
    .await;
    assert!(
        !state.peek().thread_feed_reads.copy_link.registry.blocked(),
        "lost ACK cannot overwrite the known physical result or strand the slot"
    );
    assert!(
        state
            .peek()
            .thread_feed_reads
            .copy_link
            .registry
            .receipt()
            .unwrap()
            .outcome
            == registry::Outcome::Fulfilled
    );
    drop(peer);
    prepare(&mut dom, state, &captured, &io, false, T).await;
    let (mut peer, c) = mounted(&mut dom, state, &ready, &io).await;
    event(&peer, &c, "pressed", 1, None);
    assert_eq!(receive(&mut dom, &mut peer).await["kind"], "permit");
    drop(peer);
    settle(&mut dom, || {
        state.peek().thread_feed_reads.copy_link.registry.unknown()
    })
    .await;
    let n = calls(&io);
    dom.render_immediate_to_vec();
    let a = action(state);
    let b = button(captured.borrow().as_ref().unwrap(), &a.button_id()).unwrap();
    click(&dom, b);
    assert_eq!(
        calls(&io),
        n,
        "unknown clipboard result never replays or launches fresh preparation"
    );
    assert!(state.peek().thread_feed_reads.copy_link.registry.blocked());
    assert!(
        !state
            .peek()
            .thread_feed_reads
            .copy_link
            .registry
            .receipt()
            .unwrap()
            .native
    );
}

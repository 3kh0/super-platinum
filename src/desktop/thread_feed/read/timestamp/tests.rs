//! Actual getView/CFM producers and mounted callback; no native/live acceptance.
use super::*;
use crate::{
    app_home::channel_fetches::{calendar_member_test_support as renderer, canonical},
    resource_reads::render_test::{self, Control, click},
};
use dioxus::core::{AttributeValue, DynamicNode, ScopeId, VNode};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use super_platinum_core::slack::{
    api::thread_timestamp::Kind,
    realtime::{self, RtUpdate},
};
const T: &str = "TINIT0001";
const U: &str = "UINIT0001";
const FIRST: &str = "CINIT0001";
const C: &str = "CINIT0002";
const TS: &str = "1700000001.000001";
fn channel(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"is_channel":true,"is_member":true,"is_private":false,"is_archived":false,"is_im":false,"is_group":false,"is_mpim":false,"is_org_shared":false,"context_team_id":T,"internal_team_ids":[]})
}
fn page() -> ThreadsViewPage {
    serde_json::from_value(json!({"threads":[{"root_msg":{"type":"message","channel":C,"team":T,"ts":TS,"thread_ts":TS,"user":U,"subscribed":true,"text":"Original timestamp root","reply_count":1},"latest_replies":[]}],"has_more":false})).unwrap()
}
fn history() -> Value {
    json!({"ok":true,"messages":[{"type":"message","channel":C,"ts":TS,"thread_ts":TS,"user":U,"text":"Separate fresh private data"}],"has_more":false})
}
fn info() -> Value {
    json!({"ok":true,"channel":channel(C,"second-harbor")})
}
fn mount(shell: ShellState) -> (VirtualDom, Signal<ShellState>) {
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): (Input, Slot)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            crate::view::thread_list_for_test(state, &state.read())
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap())
}
fn control(dom: &VirtualDom, id: &str) -> Option<Control> {
    fn find(dom: &VirtualDom, v: &VNode, id: &str) -> Option<Control> {
        if let Some(c) = render_test::button(v, id) {
            return Some(c);
        }
        for (index, node) in v.dynamic_nodes.iter().enumerate() {
            let found = match node {
                DynamicNode::Component(c) => c
                    .mounted_scope(index, v, dom)
                    .and_then(|s| find(dom, s.root_node(), id)),
                DynamicNode::Fragment(nodes) => nodes.iter().find_map(|v| find(dom, v, id)),
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }
    find(dom, dom.base_scope().root_node(), id)
}
fn current_control(dom: &VirtualDom, state: Signal<ShellState>) -> Control {
    let s = state.peek();
    let source = Source::capture(&s).unwrap();
    let a = source.timestamp(&s, &s.core.threads_view.items[0]).unwrap();
    control(dom, &a.button_id()).expect("actual keyed timestamp component/button")
}
fn event(dom: &VirtualDom, c: Control, field: &str, value: Value) {
    let mut raw = serde_json::to_value(dioxus::html::SerializedMouseData::default()).unwrap();
    raw[field] = value;
    let data = dioxus::html::EventData::Mouse(serde_json::from_value(raw).unwrap());
    let AttributeValue::Listener(callback) = c.click else {
        panic!("real timestamp listener")
    };
    dom.in_scope(ScopeId::ROOT, || {
        callback.call(dioxus::core::Event::new(data.into_any(), true))
    });
}
#[track_caller]
fn pump<'a>(
    dom: &'a mut VirtualDom,
    done: impl Fn() -> bool + 'a,
) -> impl std::future::Future<Output = ()> + 'a {
    let caller = std::panic::Location::caller();
    async move {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                dom.process_events();
                dom.render_immediate_to_vec();
                if done() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap_or_else(|e| panic!("timestamp workflow from {caller}: {e}"));
    }
}
fn apply(mut state: Signal<ShellState>, updates: Vec<RtUpdate>) {
    for update in updates {
        match update {
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
            _ => panic!("only original protected ingress/barrier model"),
        }
    }
}
async fn initialize(dom: &mut VirtualDom, mut state: Signal<ShellState>, io: Arc<test_io::Io>) {
    let initial = {
        let mut s = state.write();
        crate::app_home::channel_fetches::fixture::install_people(&mut s);
        renderer::connect(&mut s).unwrap();
        renderer::assignments(&mut s,&json!({"ok":true,"assignments":{"enterprise_search":{"group":"off"},"search_normalize_visible_external_users":{"group":"off"},"preserve_omitted_user_change_member_fields":{"group":"on"}}})).unwrap()
    };
    renderer::provide(dom, Rc::new(renderer::Renderer::default()));
    renderer::run_pumped(dom, initial.drain(state))
        .await
        .unwrap();
    let init = json!({"ok":true,"self":{"id":U,"name":"fixture-self","team_id":T,"profile":{}},"team":{"id":T,"name":"Timestamp navigation"},"workspaces":[{"id":T}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123,"prefs":{}});
    let channels = json!({"ok":true,"channels":{"channels":[channel(FIRST,"warm-harbor"),channel(C,"second-harbor")],"ims":[],"mpims":[],"is_open":[],"has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}});
    renderer::run_pumped(dom, initial.complete_channels(state, init, channels))
        .await
        .unwrap_or_else(|e| panic!("{e:?}: {}", initial.diagnostic(&state.peek())));
    let generation = state.peek().core.workspaces[T].rt_generation;
    let (connection, updates) = realtime::protected_connection(generation);
    {
        let mut s = state.write();
        s.core.workspaces.get_mut(T).unwrap().rt =
            super_platinum_core::state::RealtimeStatus::Connected(connection);
        s.refresh_from_core();
        s.main_view = MainView::Threads;
        s.overlay = None;
        s.thread_feed_reads.timestamp.io = Some(io);
        s.connection.routable = true;
        s.connection.routed_at = Some(std::time::Instant::now());
    }
    apply(state, updates);
    assert!(
        canonical::observed(&state.peek(), T)
            .unwrap()
            .current(&state.peek())
    );
    assert!(
        crate::app_home::cfm_lifecycle::post_boot(&state.peek(), T)
            .unwrap()
            .current(&state.peek())
    );
    dom.render_immediate_to_vec();
}
async fn admit(dom: &mut VirtualDom, state: Signal<ShellState>) {
    let source = Source::capture(&state.peek()).unwrap();
    renderer::run_pumped(
        dom,
        source.run_with(state, Mode::Refresh, |_| async { Ok(page()) }),
    )
    .await
    .unwrap();
    dom.render_immediate_to_vec();
}
fn calls(io: &test_io::Io) -> usize {
    io.calls.lock().unwrap().len()
}
fn failed(state: Signal<ShellState>) -> bool {
    state
        .peek()
        .thread_feed_reads
        .timestamp
        .intent
        .as_ref()
        .is_some_and(|p| p.phase.load(Ordering::Acquire) == 2)
}
async fn fresh(dom: &mut VirtualDom, state: Signal<ShellState>, io: &test_io::Io, reply: Value) {
    let n = calls(io);
    let h = io.expect(Kind::History);
    let c = io.expect(Kind::Channel);
    click(dom, current_control(dom, state));
    pump(dom, || calls(io) == n + 1).await;
    h.send(Ok(history())).unwrap();
    pump(dom, || calls(io) == n + 2).await;
    c.send(Ok(reply)).unwrap();
}
#[tokio::test(flavor = "current_thread")]
async fn thread_timestamp_actual_original_root_keyed_control_policy_global_anchor_retirement_and_cooldown_journey()
 {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    for alias in ["thread-feed-timestamp", "thread-feed-timestamp-narrow"] {
        let shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        assert!(layout(&shell));
        assert!(shell.thread_feed_reads.timestamp.scope.is_none());
        assert!(Source::capture(&shell).is_none());
        assert!(shell.core.session.is_none());
        let (_dom, _state) = mount(shell);
    }
    let (mut dom, mut state) = mount(ShellState::fixture(crate::media::MediaRegistry::default()));
    let io = Arc::new(test_io::Io::default());
    initialize(&mut dom, state, io.clone()).await;
    // Public Core/data cannot construct the new original timestamp roster.
    {
        let mut s = state.write();
        let epoch = s.core.account_epoch;
        s.core.threads_view.ensure_scope(epoch, T);
        let r = s.core.threads_view.begin(false).unwrap();
        assert!(
            s.core
                .apply_thread_feed_page(T, &r, page(), &Default::default())
        );
    }
    assert!(state.peek().thread_feed_reads.timestamp.scope.is_none());
    admit(&mut dom, state).await;
    assert!(target::Prepared::capture(&state.peek(), T, C, TS).is_some());
    let canonical_before = serde_json::to_value(&state.peek().core.workspaces[T].channels).unwrap();
    let feed_before = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    state.write().core.composer.text = "Keep first-channel draft".into();
    // Unmodified primary/keyboard click only; no browser/auxiliary fallback.
    event(&dom, current_control(&dom, state), "ctrl_key", json!(true));
    event(&dom, current_control(&dom, state), "button", json!(1));
    assert_eq!(calls(&io), 0);
    assert!(state.peek().thread_feed_reads.timestamp.intent.is_none());
    // A removed/recreated component permanently retires its old callback.
    let retired = current_control(&dom, state);
    state.write().overlay = Some(crate::state::Overlay::Search);
    dom.render_immediate_to_vec();
    state.write().overlay = None;
    dom.render_immediate_to_vec();
    click(&dom, retired);
    assert_eq!(calls(&io), 0);
    // First callback reserves synchronously; queued peer cannot borrow it.
    let h = io.expect(Kind::History);
    let queued = current_control(&dom, state);
    click(&dom, current_control(&dom, state));
    click(&dom, queued);
    pump(&mut dom, || calls(&io) == 1).await;
    h.send(Err(Error::Offline("synthetic timestamp outage".into())))
        .unwrap();
    pump(&mut dom, || failed(state)).await;
    assert_eq!(calls(&io), 1);
    assert_eq!(state.peek().main_view, MainView::Threads);
    assert_eq!(state.peek().core.active_channel.as_deref(), Some(FIRST));
    assert_eq!(state.peek().core.composer.text, "Keep first-channel draft");
    // Fresh wrong namespace refuses, without canonical/feed mutation.
    let mut wrong = info();
    wrong["channel"]["context_team_id"] = json!("T99999999");
    fresh(&mut dom, state, &io, wrong).await;
    pump(&mut dom, || failed(state)).await;
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces[T].channels).unwrap(),
        canonical_before
    );
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        feed_before
    );
    // Actual mounted callback -> current own data -> Home/exact message anchor.
    let history_done = io.expect_history();
    fresh(&mut dom, state, &io, info()).await;
    pump(&mut dom, || io.history_calls.lock().unwrap().len() == 1).await;
    {
        let s = state.peek();
        assert_eq!(s.main_view, MainView::Home);
        assert_eq!(s.core.active_channel.as_deref(), Some(C));
        assert!(s.thread_root.is_none());
        assert!(!s.stick_to_bottom);
        assert!(
            matches!(s.core.pending_scroll_to.as_ref(),Some((c,super_platinum_core::domain::PendingScrollTarget::Message(ts)))if c==C&&ts==TS)
        );
        assert!(
            s.thread_feed_reads.timestamp.intent.is_none(),
            "outgoing watch/attempt released before history hold"
        );
    }
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        feed_before
    );
    let index = state
        .peek()
        .channels
        .iter()
        .position(|c| c.id == FIRST)
        .unwrap();
    state
        .write()
        .select_channel(index, crate::state::ChannelOpen::Global);
    assert_eq!(state.peek().core.composer.text, "Keep first-channel draft");
    history_done.send(()).unwrap();
    pump(&mut dom, || io.work_completed.load(Ordering::Acquire) == 1).await;
    // A newer route before successor first poll cannot launch history.
    state.write().main_view = MainView::Threads;
    admit(&mut dom, state).await;
    let gate = io.hold_before_history();
    let handoffs = io.handoffs.load(Ordering::Acquire);
    fresh(&mut dom, state, &io, info()).await;
    pump(&mut dom, || {
        io.handoffs.load(Ordering::Acquire) == handoffs + 1
    })
    .await;
    let index = state
        .peek()
        .channels
        .iter()
        .position(|c| c.id == FIRST)
        .unwrap();
    state
        .write()
        .select_channel(index, crate::state::ChannelOpen::Global);
    gate.send(()).unwrap();
    pump(&mut dom, || io.work_completed.load(Ordering::Acquire) == 2).await;
    assert_eq!(
        io.history_calls.lock().unwrap().len(),
        1,
        "retired successor constructs no history future"
    );
    // Raw ingress defeats a held read before native event/barrier delivery.
    state.write().main_view = MainView::Threads;
    admit(&mut dom, state).await;
    let n = calls(&io);
    let done = io.completed.load(Ordering::Acquire);
    let held = io.expect(Kind::History);
    click(&dom, current_control(&dom, state));
    pump(&mut dom, || calls(&io) == n + 1).await;
    let (connection, generation) = {
        let s = state.peek();
        let w = &s.core.workspaces[T];
        let super_platinum_core::state::RealtimeStatus::Connected(c) = &w.rt else {
            panic!("original protected socket")
        };
        (c.clone(), w.rt_generation)
    };
    let queued = realtime::protected_updates(
        &connection,
        generation,
        &json!({"type":"future_message_shape","message":{"channel":C}}).to_string(),
    );
    held.send(Ok(history())).unwrap();
    pump(&mut dom, || {
        io.completed.load(Ordering::Acquire) == done + 1
    })
    .await;
    assert_eq!(calls(&io), n + 1);
    assert_eq!(state.peek().main_view, MainView::Threads);
    assert_eq!(io.history_calls.lock().unwrap().len(), 1);
    apply(state, queued);
    dom.render_immediate_to_vec();
    // Issued data after account retirement cannot trigger the next read/open.
    state.write().main_view = MainView::Threads;
    admit(&mut dom, state).await;
    let n = calls(&io);
    let done = io.completed.load(Ordering::Acquire);
    let held = io.expect(Kind::History);
    click(&dom, current_control(&dom, state));
    pump(&mut dom, || calls(&io) == n + 1).await;
    state.write().core.account_epoch += 1;
    held.send(Ok(history())).unwrap();
    pump(&mut dom, || {
        io.completed.load(Ordering::Acquire) == done + 1
    })
    .await;
    assert_eq!(calls(&io), n + 1);
    assert_eq!(state.peek().core.active_channel.as_deref(), Some(FIRST));
    assert_eq!(io.history_calls.lock().unwrap().len(), 1);
    // New genuine boot/page, then actual Shell replacement while429 is held.
    initialize(&mut dom, state, io.clone()).await;
    admit(&mut dom, state).await;
    let n = calls(&io);
    let done = io.completed.load(Ordering::Acquire);
    let held = io.expect(Kind::History);
    click(&dom, current_control(&dom, state));
    pump(&mut dom, || calls(&io) == n + 1).await;
    let loaded = ShellState::fixture(crate::media::MediaRegistry::default());
    dom.in_scope(ScopeId::ROOT, || {
        crate::bootstrap::install_loaded(state, loaded)
    });
    initialize(&mut dom, state, io.clone()).await;
    held.send(Err(Error::RateLimited {
        retry_after_secs: Some(u64::MAX),
    }))
    .unwrap();
    pump(&mut dom, || {
        io.completed.load(Ordering::Acquire) == done + 1
    })
    .await;
    admit(&mut dom, state).await;
    assert!(cooling(&state.peek()));
    let b = current_control(&dom, state);
    assert!(b.disabled);
    click(&dom, b);
    assert_eq!(calls(&io), n + 1);
    // Cooldown custody is actor data, never a replacement account's permission.
    let account = state.peek().core.active_account.clone();
    state.write().core.active_account = Some("other-synthetic-account".into());
    assert!(!cooling(&state.peek()));
    state.write().core.active_account = account;
    assert!(cooling(&state.peek()));
    assert_eq!(io.history_calls.lock().unwrap().len(), 1);
}

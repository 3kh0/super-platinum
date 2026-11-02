//! Actual original raw publication, mounted pill, cold producer, RT and receipts.
use super::*;
use crate::resource_reads::render_test::{self, Captured, Control, click, text};
use dioxus::core::DynamicNode;
use serde_json::json;
use std::{cell::Cell, rc::Rc};
use super_platinum_core::slack::realtime::{self, Connection, RtUpdate};
const ROOT: &str = "1710000001.000001";
const OTHER: &str = "U999";
fn render(state: Signal<ShellState>) -> Element {
    crate::view::thread_list_for_test(state, &state.read())
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
fn root(own: bool, user: &str, same: bool) -> Value {
    let mut users = vec![OTHER];
    if own {
        users.push(user);
    }
    let mut raw = json!({"type":"message","channel":"C1","ts":ROOT,"user":"U1","text":"Original Root reading","reply_count":1,"is_locked":false,"is_beyond_free_limit":false,"reactions":[{"name":"eyes","count":users.len(),"users":users}]});
    if same {
        raw["thread_ts"] = json!(ROOT);
    }
    raw
}
fn page(raw: Value) -> Value {
    json!({"ok":true,"threads":[{"root_msg":raw,"latest_replies":[{"type":"message","channel":"C1","ts":"1710000002.000001","thread_ts":ROOT,"user":"U1","text":"Reply reading"}]}],"has_more":false,"max_ts":ROOT})
}
fn history(raw: &Value) -> Value {
    json!({"ok":true,"messages":[raw],"has_more":false})
}
fn info() -> Value {
    json!({"ok":true,"channel":{"id":"C1","name":"ship","is_channel":true,"is_member":true,"is_private":false,"is_frozen":false}})
}
fn prefs() -> Value {
    json!({"ok":true,"prefs":{"hidden_users":"","preferred_skin_tone":3}})
}
fn members(raw: &Value) -> Value {
    json!({"ok":true,"type":"message","channel":"C1","message":raw})
}
fn updates(mut state: Signal<ShellState>, updates: Vec<RtUpdate>) {
    for update in updates {
        match update {
            RtUpdate::Event { generation, event } => {
                let mut s = state.write();
                super::event(&mut s, "T1", generation, &event);
                crate::realtime::apply_in_view_with_context(
                    &mut s, "T1", generation, *event, true, true,
                );
            }
            RtUpdate::ReadBarrier {
                generation,
                barrier,
            } => {
                let mut s = state.write();
                accept_barrier(&mut s, "T1", generation, barrier.clone());
                crate::bootstrap::search_message_pages::reaction_actions::accept_barrier(
                    &mut s, "T1", generation, barrier,
                );
            }
            RtUpdate::Connected { .. } => {}
            _ => panic!("original fixture updates only"),
        }
    }
}
fn mount() -> (
    VirtualDom,
    Signal<ShellState>,
    Captured,
    Rc<test_io::Io>,
    Connection,
) {
    let (dom, mut state, captured) = render_test::mount("thread-feed", render);
    let io = Rc::new(test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    let (connection, initial) = realtime::protected_connection(7);
    {
        let mut s = state.write();
        s.overlay = None;
        s.main_view = MainView::Threads;
        s.core.threads_view = Default::default();
        let epoch = s.core.account_epoch;
        s.core.threads_view.ensure_scope(epoch, "T1");
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.rt_generation = 7;
        w.rt = super_platinum_core::state::RealtimeStatus::Connected(connection.clone());
        connection_changed(&mut s, "T1");
    }
    updates(state, initial);
    (dom, state, captured, io, connection)
}
async fn pump(dom: &mut VirtualDom, ready: impl Fn() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(6), async {
        loop {
            dom.render_immediate_to_vec();
            if ready() {
                break;
            }
            dom.wait_for_work().await;
        }
    })
    .await
    .expect("original Root workflow settles");
}
async fn publish(dom: &mut VirtualDom, state: Signal<ShellState>, io: &test_io::Io, raw: Value) {
    let response = io.expect("/api/subscriptions.thread.getView");
    let source = Source::capture(&state.peek()).unwrap();
    let finished = Rc::new(Cell::new(None));
    let done = finished.clone();
    let n = io.requests.borrow().len();
    dom.in_scope(ScopeId::ROOT, || {
        dioxus::core::spawn_forever(async move {
            let result = source
                .run_with(state, Mode::Refresh, super::super::fetch)
                .await;
            done.set(Some(result.is_some()));
        })
    });
    pump(dom, || io.requests.borrow().len() == n + 1).await;
    response.send(Ok(page(raw))).unwrap();
    pump(dom, || finished.get().is_some()).await;
    assert_eq!(finished.get(), Some(true));
}
fn action(state: Signal<ShellState>) -> Option<Action> {
    let s = state.peek();
    let item = s.core.threads_view.items.first()?;
    Source::capture(&s)?.root_reaction(&s, item, "eyes")
}
fn writes(io: &test_io::Io) -> usize {
    io.paths()
        .iter()
        .filter(|p| matches!(p.as_str(), "/api/reactions.add" | "/api/reactions.remove"))
        .count()
}
fn metadata(io: &test_io::Io, raw: &Value) {
    for (path, body) in [
        ("/api/conversations.history", history(raw)),
        ("/api/conversations.info", info()),
        ("/api/users.prefs.get", prefs()),
        ("/api/reactions.get", members(raw)),
    ] {
        io.expect(path).send(Ok(body)).unwrap();
    }
}
fn field(request: &super_platinum_core::slack::PreparedRequest, name: &str) -> String {
    let super_platinum_core::slack::client::RequestBody::Form(fields) = &request.body else {
        panic!("actual form")
    };
    fields.iter().find(|(k, _)| k == name).unwrap().1.clone()
}
#[tokio::test(flavor = "current_thread")]
async fn root_reaction_original_cold_mounted_add_own_rt_before_ack_refresh_remove_and_check() {
    let (mut dom, state, captured, io, connection) = mount();
    let user = state.peek().core.workspaces["T1"].self_user_id.clone();
    assert_ne!(user, OTHER);
    let raw = root(false, &user, true);
    assert!(action(state).is_none());
    for denied in ["hidden", "is_hidden", "deleted", "is_tombstone"] {
        let mut copy = raw.clone();
        copy[denied] = json!(true);
        {
            let mut s = state;
            s.write()
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .messages
                .entry("C1".into())
                .or_default()
                .messages
                .push(serde_json::from_value(copy).unwrap());
        }
        publish(&mut dom, state, &io, raw.clone()).await;
        assert!(
            action(state).is_none(),
            "fresh raw omission must not erase an explicit canonical denial: {denied}"
        );
        {
            let mut s = state;
            s.write()
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .messages
                .get_mut("C1")
                .unwrap()
                .messages
                .pop()
                .unwrap();
        }
    }
    assert!(
        crate::emoji_catalog::current(&state.peek()).is_none(),
        "genuine cold catalogue, no fixture-ready constructor"
    );
    publish(&mut dom, state, &io, raw.clone()).await;
    let add = action(state).expect("original raw getView publishes existing-pill intent");
    let key = add.physical();
    let emoji = io.expect("/api/emoji.list");
    metadata(&io, &raw);
    let ack = io.expect("/api/reactions.add");
    let transcript = format!("{:?}", &state.peek().core.workspaces["T1"].messages);
    dom.render_immediate_to_vec();
    click(&dom, control(&dom, &add.id()).unwrap());
    emoji.send(Ok(json!({"ok":true,"emoji":{}}))).unwrap();
    pump(&mut dom, || writes(&io) == 1).await;
    {
        let requests = io.requests.borrow();
        let request = requests.last().unwrap();
        assert_eq!(field(request, "channel"), "C1");
        assert_eq!(field(request, "timestamp"), ROOT);
        assert_eq!(field(request, "name"), "eyes");
    }
    assert_eq!(
        format!("{:?}", &state.peek().core.workspaces["T1"].messages),
        transcript,
        "Root has no optimistic canonical mutation"
    );
    updates(state, realtime::protected_updates(&connection, 7, &json!({"type":"reaction_added","user":user,"reaction":"eyes","item":{"type":"message","channel":"C1","ts":ROOT}}).to_string()));
    assert!(
        crate::reaction_writes::registry(&state.peek()).pending(&key),
        "own RT retires intent without dropping pending receipt"
    );
    ack.send(Ok(json!({"ok":true}))).unwrap();
    pump(&mut dom, || {
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .is_some()
    })
    .await;
    assert_eq!(writes(&io), 1);
    assert!(action(state).is_none());
    let own = root(true, &user, true);
    publish(&mut dom, state, &io, own.clone()).await;
    let remove = action(state).unwrap();
    metadata(&io, &own);
    let ack = io.expect("/api/reactions.remove");
    dom.render_immediate_to_vec();
    click(&dom, control(&dom, &remove.id()).unwrap());
    pump(&mut dom, || writes(&io) == 2).await;
    assert_eq!(io.paths().last().unwrap(), "/api/reactions.remove");
    ack.send(Ok(json!({"ok":true}))).unwrap();
    pump(&mut dom, || {
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .is_some_and(|r| !r.adding)
    })
    .await;
    let check = {
        let s = state.peek();
        status(&s, &s.core.threads_view.items[0])
            .unwrap()
            .check
            .unwrap()
    };
    metadata(&io, &raw);
    dom.render_immediate_to_vec();
    click(&dom, control(&dom, &check.id()).unwrap());
    pump(&mut dom, || {
        let s = state.peek();
        displayed(&s, &s.core.threads_view.items[0]).is_some()
    })
    .await;
    assert_eq!(
        writes(&io),
        2,
        "Check only reads; it cannot replay the mutation"
    );
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Original Root reading"));
    assert!(state.peek().viewer.is_none());
    assert!(state.peek().core.active_thread.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn root_reaction_queued_ingress_unsent_reentry_legacy_search_overlap_late_advice_and_owner_isolation()
 {
    use crate::bootstrap::search_message_pages::tests::root_overlap::original_search_control;
    use crate::reaction_writes::{Actor, Key, Outcome};
    let (mut dom, mut state, captured, io, connection) = mount();
    let user = state.peek().core.workspaces["T1"].self_user_id.clone();
    let raw = root(false, &user, false);
    let draft = state.peek().core.composer.text.clone();
    original_search_control(&mut dom, state, raw.clone(), false).await;
    publish(&mut dom, state, &io, raw.clone()).await;
    let original = action(state).unwrap();
    let key = original.physical();
    let before = io.requests.borrow().len();
    let peer = realtime::protected_updates(
        &connection,
        7,
        r#"{"type":"reaction_added","user":"U2","reaction":"wave","item":{"type":"message","channel":"C2","ts":"1710000001.000009"}}"#,
    );
    assert!(original.disabled(&state.peek()));
    dom.in_scope(ScopeId::ROOT, || start(state, original.clone()));
    assert_eq!(
        io.requests.borrow().len(),
        before,
        "queued peer must be applied before admitting a new purpose"
    );
    updates(state, peer);
    assert!(
        !original.disabled(&state.peek()),
        "accepted unrelated peer does not retire this original root"
    );
    let queued = realtime::protected_updates(
        &connection,
        7,
        r#"{"type":"message","subtype":"message_changed","channel":"C1"}"#,
    );
    assert!(original.disabled(&state.peek()));
    dom.in_scope(ScopeId::ROOT, || start(state, original.clone()));
    assert_eq!(io.requests.borrow().len(), before);
    updates(state, queued);
    dom.in_scope(ScopeId::ROOT, || start(state, original.clone()));
    assert_eq!(
        io.requests.borrow().len(),
        before,
        "old captured control cannot borrow a later barrier"
    );
    publish(&mut dom, state, &io, raw.clone()).await;
    let unsent = action(state).unwrap();
    let emoji = io.expect("/api/emoji.list");
    let held = io.expect("/api/conversations.history");
    dom.in_scope(ScopeId::ROOT, || start(state, unsent.clone()));
    emoji.send(Ok(json!({"ok":true,"emoji":{}}))).unwrap();
    pump(&mut dom, || {
        io.paths()
            .last()
            .is_some_and(|p| p == "/api/conversations.history")
    })
    .await;
    let n = io.requests.borrow().len();
    dom.in_scope(ScopeId::ROOT, || start(state, unsent.clone()));
    assert_eq!(
        io.requests.borrow().len(),
        n,
        "reentry cannot admit another pending purpose"
    );
    let queued = realtime::protected_updates(
        &connection,
        7,
        r#"{"type":"reaction_added","user":"U2","reaction":"wave","item":{"type":"message","channel":"C1","ts":"1710000001.000001"}}"#,
    );
    held.send(Ok(history(&raw))).unwrap();
    pump(&mut dom, || {
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .is_some()
    })
    .await;
    assert!(matches!(
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .outcome,
        Outcome::NotSent
    ));
    assert_eq!(
        io.requests.borrow().len(),
        n,
        "queued own ingress defeats follow-on info/membership/write before native dequeue"
    );
    assert_eq!(writes(&io), 0);
    updates(state, queued);

    // The old awaited API now reserves an ambiguous Root before its optimistic
    // projection. Local account materialization cannot split the physical key.
    state.write().core.active_account = None;
    publish(&mut dom, state, &io, raw.clone()).await;
    let legacy_reply = io.expect("/api/reactions.add");
    let completed = Rc::new(Cell::new(false));
    let completion = completed.clone();
    dom.in_scope(ScopeId::ROOT, || {
        dioxus::core::spawn_forever(async move {
            crate::messaging::toggle_reaction(
                state,
                "C1".into(),
                ROOT.into(),
                "eyes".into(),
                false,
            )
            .await;
            completion.set(true);
        })
    });
    pump(&mut dom, || writes(&io) == 1).await;
    assert!(
        !completed.get(),
        "original API still awaits the detached driver's completion"
    );
    state.write().core.active_account = Some("materialized-root-owner".into());
    let optimistic = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    let mut replacement = root(true, &user, false);
    replacement["text"] = json!("Original Root reading — independently refreshed");
    publish(&mut dom, state, &io, replacement).await;
    let fresh = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    let blocked = action(state).unwrap();
    assert!(blocked.disabled(&state.peek()));
    let n = io.requests.borrow().len();
    dom.in_scope(ScopeId::ROOT, || start(state, blocked.clone()));
    original_search_control(&mut dom, state, raw.clone(), true).await;
    dom.in_scope(ScopeId::ROOT, || {
        dioxus::core::spawn_forever(async move {
            crate::messaging::toggle_reaction(state, "C1".into(), ROOT.into(), "eyes".into(), true)
                .await;
        })
    });
    dom.render_immediate_to_vec();
    assert_eq!(io.requests.borrow().len(), n);
    assert_eq!(
        writes(&io),
        1,
        "Root, actual Search Owner and legacy cannot overlap the held original write"
    );
    legacy_reply
        .send(Err(Error::RateLimited {
            retry_after_secs: Some(11),
        }))
        .unwrap();
    pump(&mut dom, || completed.get()).await;
    let registry = crate::reaction_writes::registry(&state.peek());
    assert!(matches!(
        registry.receipt(&key).unwrap().outcome,
        Outcome::Unknown
    ));
    assert!(
        registry.cooling(&key.actor),
        "late original 429 advice survives UI owner retirement"
    );
    let other = Key {
        actor: Actor {
            account: "materialized-root-owner".into(),
            team: "T1".into(),
            user: "UOTHER".into(),
        },
        channel: "C1".into(),
        ts: ROOT.into(),
    };
    assert!(
        !registry.blocked(&other),
        "an old actor's advice cannot contaminate another authenticated self"
    );
    assert_eq!(writes(&io), 1);
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        fresh,
        "a late stale-owner Remove inverse must not erase fresh own membership"
    );
    assert_ne!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        optimistic,
        "fresh original getView, not stale-owner rollback, owns retained rows"
    );
    assert_eq!(state.peek().core.composer.text, draft);

    // A permanent advice record dominates a later finite one. Expiry wake-up
    // uses the existing poll and never clears the ambiguous mutation receipt.
    registry.permanent(&key.actor);
    registry.cooldown(&key.actor, Some(1));
    assert!(registry.cooling(&key.actor));
    original_search_control(&mut dom, state, raw.clone(), true).await;
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Original Root reading"));
    assert_eq!(writes(&io), 1);
}

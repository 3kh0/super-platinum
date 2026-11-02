//! Whole original-root/picker flows; no physical-input, service or PNG claim.
mod aliases;
mod name_search;
mod preferred;
use super::*;
use crate::resource_reads::render_test::{self, Captured, Control, click, text};
use dioxus::core::{AttributeValue, DynamicNode};
use serde_json::json;
use std::{cell::Cell, rc::Rc};
use super_platinum_core::slack::realtime::{self, Connection, RtUpdate};
const ROOT: &str = "1712000001.000001";
type Io = super::super::test_io::Io;
fn render(state: Signal<ShellState>) -> Element {
    let s = state.read();
    rsx! {
        {crate::view::thread_list_for_test(state, &s)}
        if let Some(overlay) = s.overlay.clone() {
            {crate::overlays::overlay_view(state, overlay, &s)}
        }
    }
}
fn raw(own: bool, user: &str) -> Value {
    let mut root = json!({"type":"message","channel":"C1","ts":ROOT,"thread_ts":ROOT,"user":"U1","text":"First Add root content","reply_count":1,"is_locked":false,"is_beyond_free_limit":false});
    if own {
        root["reactions"] = json!([{"name":"eyes","count":1,"users":[user]}]);
    }
    root
}
fn rows(own: bool, user: &str) -> Value {
    if own {
        json!([{"name":"eyes","count":1,"users":[user]}])
    } else {
        json!([])
    }
}
fn page(raw: Value) -> Value {
    json!({"ok":true,"threads":[{"root_msg":raw,"latest_replies":[{"type":"message","channel":"C1","ts":"1712000002.000001","thread_ts":ROOT,"user":"U1","text":"Reply remains readable"}]}],"has_more":false,"max_ts":ROOT})
}
fn history(root: &Value) -> Value {
    json!({"ok":true,"messages":[root],"has_more":false})
}
fn info() -> Value {
    json!({"ok":true,"channel":{"id":"C1","name":"ship","is_channel":true,"is_member":true,"is_private":false,"is_frozen":false}})
}
fn prefs() -> Value {
    json!({"ok":true,"prefs":{"hidden_users":"","preferred_skin_tone":3}})
}
fn full(root: &Value, rows: Value) -> Value {
    let mut message = root.clone();
    message["reactions"] = rows;
    json!({"ok":true,"type":"message","channel":"C1","message":message})
}
fn metadata(io: &Io, root: &Value, rows: Value) {
    for (path, body) in [
        ("/api/conversations.history", history(root)),
        ("/api/conversations.info", info()),
        ("/api/users.prefs.get", prefs()),
        ("/api/reactions.get", full(root, rows)),
    ] {
        io.expect(path).send(Ok(body)).unwrap();
    }
}
fn updates(mut state: Signal<ShellState>, updates: Vec<RtUpdate>) {
    for update in updates {
        match update {
            RtUpdate::Event { generation, event } => {
                let mut s = state.write();
                super::super::event(&mut s, "T1", generation, &event);
                crate::realtime::apply_in_view_with_context(
                    &mut s, "T1", generation, *event, true, true,
                );
            }
            RtUpdate::ReadBarrier {
                generation,
                barrier,
            } => {
                let mut s = state.write();
                super::super::accept_barrier(&mut s, "T1", generation, barrier.clone());
                crate::bootstrap::search_message_pages::reaction_actions::accept_barrier(
                    &mut s, "T1", generation, barrier,
                );
            }
            RtUpdate::Connected { .. } => {}
            _ => panic!("original fixture updates only"),
        }
    }
}
fn mount() -> (VirtualDom, Signal<ShellState>, Captured, Rc<Io>, Connection) {
    let (dom, mut state, captured) = render_test::mount("thread-feed", render);
    let io = Rc::new(Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    let (connection, initial) = realtime::protected_connection(31);
    {
        let mut s = state.write();
        s.overlay = None;
        s.main_view = MainView::Threads;
        s.core.threads_view = Default::default();
        let epoch = s.core.account_epoch;
        s.core.threads_view.ensure_scope(epoch, "T1");
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.rt_generation = 31;
        w.rt = super_platinum_core::state::RealtimeStatus::Connected(connection.clone());
        super::super::connection_changed(&mut s, "T1");
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
            // Scoped IO can change while its response is held without dirtying
            // a UI scope. Drive Dioxus and recheck the external predicate within
            // the same bounded six-second deadline.
            tokio::select! {
                _ = dom.wait_for_work() => {},
                _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {},
            }
        }
    })
    .await
    .expect("original first-Add workflow settles");
}
async fn publish(dom: &mut VirtualDom, state: Signal<ShellState>, io: &Io, root: Value) {
    let reply = io.expect("/api/subscriptions.thread.getView");
    let source = Source::capture(&state.peek()).unwrap();
    let done = Rc::new(Cell::new(None));
    let completed = done.clone();
    let before = io.requests.borrow().len();
    dom.in_scope(ScopeId::ROOT, || {
        dioxus::core::spawn_forever(async move {
            let result = source
                .run_with(state, Mode::Refresh, super::super::super::fetch)
                .await;
            completed.set(Some(result.is_some()));
        })
    });
    pump(dom, || io.requests.borrow().len() == before + 1).await;
    reply.send(Ok(page(root))).unwrap();
    pump(dom, || done.get().is_some()).await;
    assert_eq!(done.get(), Some(true));
}
fn opener(state: Signal<ShellState>) -> Option<Open> {
    let s = state.peek();
    let item = s.core.threads_view.items.first()?;
    Source::capture(&s)?.root_first_add(&s, item)
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
fn listener(dom: &VirtualDom, id: &str, event: &str) -> Option<AttributeValue> {
    fn find(dom: &VirtualDom, v: &VNode, id: &str, event: &str) -> Option<AttributeValue> {
        if let Some(c) = render_test::listener(v, id, event) {
            return Some(c);
        }
        for (index, node) in v.dynamic_nodes.iter().enumerate() {
            let found = match node {
                DynamicNode::Component(c) => c
                    .mounted_scope(index, v, dom)
                    .and_then(|s| find(dom, s.root_node(), id, event)),
                DynamicNode::Fragment(nodes) => nodes.iter().find_map(|v| find(dom, v, id, event)),
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }
    find(dom, dom.base_scope().root_node(), id, event)
}
fn input(dom: &VirtualDom, id: &str, value: &str) {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let AttributeValue::Listener(callback) = listener(dom, id, "oninput").unwrap() else {
        panic!("actual picker input listener")
    };
    let data = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        value.into(),
        Vec::new(),
    ));
    dom.in_scope(ScopeId::ROOT, || {
        callback.call(dioxus::core::Event::new(data.into_any(), true))
    });
}
fn physical(state: Signal<ShellState>) -> crate::reaction_writes::Key {
    let s = state.peek();
    crate::reaction_writes::Key {
        actor: crate::reaction_writes::Actor {
            account: s.core.active_account.clone().unwrap_or_default(),
            team: "T1".into(),
            user: s.core.workspaces["T1"].self_user_id.clone(),
        },
        channel: "C1".into(),
        ts: ROOT.into(),
    }
}
fn writes(io: &Io) -> usize {
    io.paths()
        .iter()
        .filter(|p| matches!(p.as_str(), "/api/reactions.add" | "/api/reactions.remove"))
        .count()
}
fn choice(state: Signal<ShellState>, name: &str) -> Choice {
    presentation(&state.peek())
        .unwrap()
        .choices
        .into_iter()
        .find(|c| c.name == name)
        .unwrap()
        .action
        .unwrap()
}
fn field(request: &super_platinum_core::slack::PreparedRequest, name: &str) -> String {
    let super_platinum_core::slack::client::RequestBody::Form(fields) = &request.body else {
        panic!("original form")
    };
    fields.iter().find(|(k, _)| k == name).unwrap().1.clone()
}
async fn open_ready(
    dom: &mut VirtualDom,
    state: Signal<ShellState>,
    io: &Io,
    root: &Value,
    rows: Value,
    cold: bool,
) -> u64 {
    let emoji = cold.then(|| io.expect("/api/emoji.list"));
    metadata(io, root, rows);
    let open = opener(state).unwrap();
    dom.render_immediate_to_vec();
    click(dom, control(dom, &open.id()).unwrap());
    if let Some(reply) = emoji {
        reply.send(Ok(json!({"ok":true,"emoji":{}}))).unwrap();
    }
    pump(dom, || presentation(&state.peek()).is_some_and(|p| p.ready)).await;
    presentation(&state.peek()).unwrap().id
}

#[tokio::test(flavor = "current_thread")]
async fn root_first_add_raw_zero_bar_cold_owned_picker_filter_add_refresh_exact_remove_and_cancel()
{
    let (mut dom, state, captured, io, connection) = mount();
    let user = state.peek().core.workspaces["T1"].self_user_id.clone();
    let root = raw(false, &user);
    assert!(
        root.get("reactions").is_none(),
        "no [] invented from original getView absence"
    );
    assert!(
        opener(state).is_none(),
        "warm typed state cannot mint the original firstAdd scope"
    );
    publish(&mut dom, state, &io, root.clone()).await;
    {
        let s = state.peek();
        let source = Source::capture(&s).unwrap();
        assert!(
            source
                .root_reaction(&s, &s.core.threads_view.items[0], "eyes")
                .is_none(),
            "new toolbar purpose is independent of old existing-name grant"
        );
    }
    assert!(crate::emoji_catalog::current(&state.peek()).is_none());
    let key = physical(state);
    let draft = state.peek().core.composer.text.clone();
    let canonical = format!("{:?}", &state.peek().core.workspaces["T1"].messages);
    let id = open_ready(&mut dom, state, &io, &root, rows(false, &user), true).await;
    {
        let p = presentation(&state.peek()).unwrap();
        assert_eq!(p.total, 1892, "1582 tone-free plus310 preferred bases");
        assert!(p.choices.len() <= 120);
        assert!(
            !crate::reaction_writes::registry(&state.peek()).pending(&key),
            "browsing holds no physical mutation slot"
        );
    }
    input(&dom, &format!("thread-root-first-add-query-{id}"), "eyes");
    dom.render_immediate_to_vec();
    let p = presentation(&state.peek()).unwrap();
    assert_eq!(p.query, "eyes");
    assert_eq!(
        p.total, 16,
        "current name-model oracle, previously substring10"
    );
    assert_eq!(p.choices.first().unwrap().name, "eyes");
    assert!(p.ready);
    let add = choice(state, "eyes");
    metadata(&io, &root, rows(false, &user));
    let ack = io.expect("/api/reactions.add");
    click(&dom, control(&dom, &add.id()).unwrap());
    assert!(presentation(&state.peek()).is_none());
    assert_eq!(
        state.peek().overlay,
        None,
        "choice owns the intentional close/handoff"
    );
    assert!(crate::reaction_writes::registry(&state.peek()).pending(&key));
    dom.in_scope(ScopeId::ROOT, || choose(state, add.clone()));
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
        canonical,
        "no optimistic canonical message seed/update"
    );
    updates(state,realtime::protected_updates(&connection,31,&json!({"type":"reaction_added","user":user,"reaction":"eyes","item":{"type":"message","channel":"C1","ts":ROOT}}).to_string()));
    assert!(
        crate::reaction_writes::registry(&state.peek()).pending(&key),
        "own RT retires UI custody but must keep the original pending write"
    );
    ack.send(Ok(json!({"ok":true}))).unwrap();
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
        crate::reaction_writes::Outcome::Acknowledged
    ));
    assert_eq!(writes(&io), 1);
    assert!(
        opener(state).is_none(),
        "issued intent spends the original write roster"
    );

    let owned = raw(true, &user);
    publish(&mut dom, state, &io, owned.clone()).await;
    let id = open_ready(&mut dom, state, &io, &owned, rows(true, &user), false).await;
    input(&dom, &format!("thread-root-first-add-query-{id}"), "eyes");
    dom.render_immediate_to_vec();
    let remove = choice(state, "eyes");
    metadata(&io, &owned, rows(true, &user));
    let ack = io.expect("/api/reactions.remove");
    click(&dom, control(&dom, &remove.id()).unwrap());
    dom.in_scope(ScopeId::ROOT, || choose(state, add));
    pump(&mut dom, || writes(&io) == 2).await;
    assert_eq!(io.paths().last().unwrap(), "/api/reactions.remove");
    ack.send(Ok(json!({"ok":true}))).unwrap();
    pump(&mut dom, || {
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .is_some_and(|r| !r.adding)
    })
    .await;
    let completed = crate::reaction_writes::registry(&state.peek())
        .receipt(&key)
        .unwrap();
    assert!(matches!(
        completed.outcome,
        crate::reaction_writes::Outcome::Acknowledged
    ));

    publish(&mut dom, state, &io, root.clone()).await;
    let id = open_ready(&mut dom, state, &io, &root, rows(false, &user), false).await;
    assert!(!crate::reaction_writes::registry(&state.peek()).pending(&key));
    dom.render_immediate_to_vec();
    click(
        &dom,
        control(&dom, &format!("thread-root-first-add-close-{id}")).unwrap(),
    );
    dom.render_immediate_to_vec();
    assert!(presentation(&state.peek()).is_none());
    assert_eq!(writes(&io), 2);
    assert!(!crate::reaction_writes::registry(&state.peek()).pending(&key));
    assert_eq!(
        crate::reaction_writes::registry(&state.peek())
            .receipt(&key)
            .unwrap()
            .id,
        completed.id,
        "cancel does not create a mutation receipt"
    );
    assert_eq!(state.peek().core.composer.text, draft);
    assert!(text(captured.borrow().as_ref().unwrap()).contains("First Add root content"));
    assert!(state.peek().viewer.is_none());
    assert!(state.peek().core.active_thread.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn root_first_add_outage_retry_query_peer_retirement_idle_legacy_conflict_and_late_actor_advice()
 {
    let (mut dom, mut state, captured, io, connection) = mount();
    let user = state.peek().core.workspaces["T1"].self_user_id.clone();
    let root = raw(false, &user);
    publish(&mut dom, state, &io, root.clone()).await;
    let key = physical(state);
    let draft = state.peek().core.composer.text.clone();
    let original = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    let emoji = io.expect("/api/emoji.list");
    let history_reply = io.expect("/api/conversations.history");
    let open = opener(state).unwrap();
    dom.render_immediate_to_vec();
    click(&dom, control(&dom, &open.id()).unwrap());
    emoji.send(Ok(json!({"ok":true,"emoji":{}}))).unwrap();
    pump(&mut dom, || {
        io.paths()
            .last()
            .is_some_and(|p| p == "/api/conversations.history")
    })
    .await;
    history_reply
        .send(Err(Error::Offline("synthetic picker outage".into())))
        .unwrap();
    pump(&mut dom, || {
        presentation(&state.peek()).is_some_and(|p| p.can_retry)
    })
    .await;
    assert_eq!(writes(&io), 0);
    assert!(!crate::reaction_writes::registry(&state.peek()).pending(&key));
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        original,
        "outage retains root reading"
    );
    let id = presentation(&state.peek()).unwrap().id;
    metadata(&io, &root, rows(false, &user));
    dom.render_immediate_to_vec();
    click(
        &dom,
        control(&dom, &format!("thread-root-first-add-retry-{id}")).unwrap(),
    );
    pump(&mut dom, || {
        presentation(&state.peek()).is_some_and(|p| p.ready)
    })
    .await;
    input(&dom, &format!("thread-root-first-add-query-{id}"), "eyes");
    dom.render_immediate_to_vec();
    let stale = choice(state, "eyes");
    let old_frame = presentation(&state.peek()).unwrap().frame;
    input(&dom, &format!("thread-root-first-add-query-{id}"), "heart");
    dom.render_immediate_to_vec();
    let n = io.requests.borrow().len();
    dom.in_scope(ScopeId::ROOT, || {
        choose(state, stale);
        choose_active(state, id, old_frame);
    });
    assert_eq!(io.requests.borrow().len(), n);
    assert_eq!(writes(&io), 0);
    assert_eq!(presentation(&state.peek()).unwrap().query, "heart");
    let held_choice = choice(state, "heart");
    let queued = realtime::protected_updates(
        &connection,
        31,
        r#"{"type":"reaction_added","user":"U2","reaction":"wave","item":{"type":"message","channel":"C2","ts":"1712000001.000009"}}"#,
    );
    let waiting = presentation(&state.peek()).unwrap();
    assert!(!waiting.ready);
    assert!(
        !waiting.choices.is_empty(),
        "queued peer fences new choices without blanking loaded glyphs"
    );
    dom.in_scope(ScopeId::ROOT, || choose(state, held_choice.clone()));
    assert_eq!(io.requests.borrow().len(), n);
    assert!(presentation(&state.peek()).is_some());
    updates(state, queued);
    dom.render_immediate_to_vec();
    assert!(presentation(&state.peek()).unwrap().ready);
    assert!(
        !crate::reaction_writes::registry(&state.peek()).pending(&key),
        "idle picker never reserves the physical target"
    );

    // A genuine old caller may claim while this picker is idle. The subsequent
    // choice must observe the common bank before doing any new metadata/write.
    let legacy_reply = io.expect("/api/reactions.add");
    let done = Rc::new(Cell::new(false));
    let completed = done.clone();
    dom.in_scope(ScopeId::ROOT, || {
        dioxus::core::spawn_forever(async move {
            crate::messaging::toggle_reaction(
                state,
                "C1".into(),
                ROOT.into(),
                "heart".into(),
                false,
            )
            .await;
            completed.set(true);
        })
    });
    pump(&mut dom, || writes(&io) == 1).await;
    assert!(!done.get());
    let n = io.requests.borrow().len();
    dom.in_scope(ScopeId::ROOT, || choose(state, held_choice));
    assert_eq!(io.requests.borrow().len(), n);
    assert_eq!(
        writes(&io),
        1,
        "picker cannot overlap the held legacy write"
    );
    dom.render_immediate_to_vec();
    click(
        &dom,
        control(&dom, &format!("thread-root-first-add-close-{id}")).unwrap(),
    );
    dom.render_immediate_to_vec();
    assert!(
        state.peek().overlay.is_none(),
        "pending-slot isolation must not pass merely because another overlay is open"
    );
    state.write().core.active_account = Some("rematerialized-root-first-add".into());
    let mut replacement = root.clone();
    replacement["text"] = json!("First Add root content independently refreshed");
    replacement["reactions"] = json!([{"name":"heart","count":1,"users":[user]}]);
    publish(&mut dom, state, &io, replacement).await;
    let fresh = serde_json::to_value(&state.peek().core.threads_view.items).unwrap();
    assert!(
        opener(state).unwrap().disabled(&state.peek()),
        "local account label cannot split a held team+self physical slot"
    );
    legacy_reply
        .send(Err(Error::RateLimited {
            retry_after_secs: Some(13),
        }))
        .unwrap();
    pump(&mut dom, || done.get()).await;
    let registry = crate::reaction_writes::registry(&state.peek());
    assert!(matches!(
        registry.receipt(&key).unwrap().outcome,
        crate::reaction_writes::Outcome::Unknown
    ));
    assert!(registry.cooling(&key.actor));
    assert_eq!(
        serde_json::to_value(&state.peek().core.threads_view.items).unwrap(),
        fresh,
        "late stale-owner Remove inverse must not erase the fresh own heart"
    );
    let other = crate::reaction_writes::Key {
        actor: crate::reaction_writes::Actor {
            account: "rematerialized-root-first-add".into(),
            team: "T1".into(),
            user: "UOTHER".into(),
        },
        channel: "C1".into(),
        ts: ROOT.into(),
    };
    assert!(
        !registry.blocked(&other),
        "old response advice remains scoped to its authenticated actor"
    );
    assert_eq!(writes(&io), 1);
    assert_eq!(state.peek().core.composer.text, draft);
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("First Add root content"));
}

mod custom;

//! Original Book publication -> actual row callback -> inert raw IO -> result.
//! Catalogue choice callback is headless; no native gesture/paint/HTTP claim.
use super::*;
use crate::bootstrap::search_message_pages::reaction_actions as actions;
use crate::resource_reads::render_test::{self, button, click, settle};
use super_platinum_core::slack::realtime::{self, Connection, RtUpdate};
const TS: &str = "1700000001.000001";
fn render(state: Signal<ShellState>) -> Element {
    crate::overlays::overlay_view(state, crate::state::Overlay::Search, &state.read())
}
fn updates(mut state: Signal<ShellState>, items: Vec<RtUpdate>) {
    for u in items {
        match u {
            RtUpdate::ReadBarrier {
                generation,
                barrier,
            } => actions::accept_barrier(&mut state.write(), "T1", generation, barrier),
            RtUpdate::Event { generation, event } => crate::realtime::apply_in_view_with_context(
                &mut state.write(),
                "T1",
                generation,
                *event,
                true,
                true,
            ),
            RtUpdate::Connected { .. } => (),
            _ => panic!("original fixture data only"),
        }
    }
}
fn rig() -> (
    Harness,
    render_test::Captured,
    Arc<actions::test_io::Io>,
    Connection,
) {
    let (dom, mut state, captured) = render_test::mount("search-messages-pages", render);
    let (transport, session) = {
        let s = state.peek();
        (
            s.core.transport.clone().unwrap(),
            s.core.session.as_ref().unwrap().workspaces["T1"].clone(),
        )
    };
    let io = Arc::new(actions::test_io::Io::new(&session.token));
    let (connection, initial) = realtime::protected_connection(23);
    {
        let mut s = state.write();
        s.overlay = Some(crate::state::Overlay::Search);
        s.clear_search_results();
        s.set_search_kind(SearchKind::Messages);
        s.set_search_query("release".into());
        s.search_reaction_actions.io = Some(io.clone());
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.rt_generation = 23;
        w.rt = super_platinum_core::state::RealtimeStatus::Connected(connection.clone());
        w.emoji_preferences.replace(&BTreeMap::new());
    }
    updates(state, initial);
    (
        Harness {
            _dom: dom,
            state,
            transport,
            session,
        },
        captured,
        io,
        connection,
    )
}
async fn publish(h: &Harness) {
    let (request, read) = first(h);
    let mut raw = response(h.state, 1, 2, 2, false);
    raw["items"][0]["messages"][0]["ts"] = json!(TS);
    raw["items"][0]["messages"][0]["type"] = json!("message");
    load(h, request, read, Ok(raw), &Cell::new(0), &Cell::new(0)).await;
}
fn c(h: &Harness) -> String {
    rows(&h.state.peek())[0].channel_id.clone()
}
fn history() -> Value {
    json!({"ok":true,"messages":[{"type":"message","ts":TS,"user":"U1","text":"Original server root"}],"has_more":false})
}
fn info(h: &Harness) -> Value {
    json!({"ok":true,"channel":{"id":c(h),"name":"ship","is_channel":true,"is_member":true,"is_frozen":false}})
}
fn members(h: &Harness, name: Option<&str>) -> Value {
    json!({"ok":true,"type":"message","channel":c(h),"message":{"type":"message","ts":TS,"reactions":name.map(|n|json!([{"name":n,"count":1,"users":[h.session.user_id]}])).unwrap_or_else(||json!([]))}})
}
fn reply(send: tokio::sync::oneshot::Sender<Result<Vec<u8>, Error>>, raw: Value) {
    send.send(Ok(serde_json::to_vec(&raw).unwrap())).unwrap();
}
fn count(io: &actions::test_io::Io) -> usize {
    io.calls.lock().unwrap().len()
}
async fn ready(h: &mut Harness, cap: &render_test::Captured, io: &actions::test_io::Io) -> u64 {
    let n = count(io);
    let a = io.expect("conversations.history");
    let b = io.expect("conversations.info");
    h._dom.render_immediate_to_vec();
    let control = button(cap.borrow().as_ref().unwrap(), "message-search-react-1-0")
        .expect("actual owned row Add reaction");
    assert!(!control.disabled);
    click(&h._dom, control);
    settle(&mut h._dom, || count(io) == n + 1).await;
    reply(a, history());
    settle(&mut h._dom, || count(io) == n + 2).await;
    reply(b, info(h));
    settle(&mut h._dom, || {
        actions::presentation(&h.state.peek()).is_some_and(|p| p.ready)
            && crate::emoji_catalog::current(&h.state.peek()).is_some()
    })
    .await;
    let calls = io.calls.lock().unwrap();
    assert_eq!(calls[n].1["latest"], TS);
    assert_eq!(calls[n].1["limit"], "1");
    assert_eq!(calls[n].1["inclusive"], "true");
    assert!(!calls[n].1.contains_key("cursor"));
    actions::presentation(&h.state.peek()).unwrap().id
}
fn choose(h: &Harness, id: u64, name: &str) {
    let loaded = crate::emoji_catalog::current(&h.state.peek()).unwrap();
    h._dom.in_scope(ScopeId::ROOT, || {
        actions::choose(
            h.state,
            id,
            crate::emoji_catalog::Selection {
                name: name.into(),
                query: String::new(),
                loaded,
            },
        )
    });
}
async fn checked(h: &mut Harness, io: &actions::test_io::Io, id: u64, name: Option<&str>) {
    let n = count(io);
    let get = io.expect("reactions.get");
    h._dom
        .in_scope(ScopeId::ROOT, || actions::check(h.state, id));
    settle(&mut h._dom, || count(io) == n + 1).await;
    reply(get, members(h, name));
    settle(&mut h._dom, || {
        actions::presentation(&h.state.peek()).is_some_and(|p| !p.checking)
    })
    .await;
    assert_eq!(io.calls.lock().unwrap()[n].1["full"], "true");
}
#[tokio::test(flavor = "current_thread")]
async fn search_reaction_owned_picker_add_remove_ack_reject_unknown_and_retired_driver_journey() {
    let (mut h, cap, io, connection) = rig();
    publish(&h).await;
    let indexed = rows(&h.state.peek()).to_vec();
    let shared = format!("{:?}", &h.state.peek().core.workspaces["T1"].messages);
    let id = ready(&mut h, &cap, &io).await;
    let name = crate::emoji_catalog::current(&h.state.peek())
        .unwrap()
        .picker[0]
        .name
        .clone();
    let n = count(&io);
    let membership = io.expect("reactions.get");
    let ack = io.expect("reactions.add");
    choose(&h, id, &name);
    settle(&mut h._dom, || count(&io) == n + 1).await;
    reply(membership, members(&h, None));
    settle(&mut h._dom, || count(&io) == n + 2).await;
    assert_eq!(io.calls.lock().unwrap()[n + 1].1["name"], name);
    // Our own ingress retires prewrite policy but must not erase its issued ACK.
    let packet=json!({"type":"reaction_added","user":h.session.user_id,"reaction":name,"item":{"type":"message","channel":c(&h),"ts":TS}}).to_string();
    updates(
        h.state,
        realtime::protected_updates(&connection, 23, &packet),
    );
    reply(ack, json!({"ok":true}));
    settle(&mut h._dom, || {
        actions::presentation(&h.state.peek()).is_some_and(|p| p.terminal)
    })
    .await;
    assert!(
        actions::presentation(&h.state.peek())
            .unwrap()
            .notice
            .unwrap()
            .contains("Reaction added")
    );
    checked(&mut h, &io, id, Some(&name)).await;
    assert_eq!(
        actions::fresh_reactions(&h.state.peek(), 0).unwrap()[0].count,
        1
    );
    assert_eq!(rows(&h.state.peek()), indexed);
    assert_eq!(
        format!("{:?}", &h.state.peek().core.workspaces["T1"].messages),
        shared,
        "no optimistic/canonical seeding"
    );
    actions::close(h.state, id);
    let id = ready(&mut h, &cap, &io).await;
    let n = count(&io);
    let membership = io.expect("reactions.get");
    let ack = io.expect("reactions.remove");
    choose(&h, id, &name);
    settle(&mut h._dom, || count(&io) == n + 1).await;
    reply(membership, members(&h, Some(&name)));
    settle(&mut h._dom, || count(&io) == n + 2).await;
    reply(ack, json!({"ok":true}));
    settle(&mut h._dom, || {
        actions::presentation(&h.state.peek()).is_some_and(|p| p.terminal)
    })
    .await;
    checked(&mut h, &io, id, None).await;
    assert!(
        actions::fresh_reactions(&h.state.peek(), 0)
            .unwrap()
            .is_empty()
    );
    actions::close(h.state, id);
    let id = ready(&mut h, &cap, &io).await;
    let n = count(&io);
    let membership = io.expect("reactions.get");
    let reject = io.expect("reactions.add");
    choose(&h, id, &name);
    settle(&mut h._dom, || count(&io) == n + 1).await;
    reply(membership, members(&h, None));
    settle(&mut h._dom, || count(&io) == n + 2).await;
    reply(reject, json!({"ok":false,"error":"not_reactable"}));
    settle(&mut h._dom, || {
        actions::presentation(&h.state.peek()).is_some_and(|p| p.terminal)
    })
    .await;
    assert!(
        actions::presentation(&h.state.peek())
            .unwrap()
            .notice
            .unwrap()
            .contains("rejected")
    );
    actions::close(h.state, id);
    // Ingress during a held policy read must settle the existing dialog, without
    // publishing policy or rearming the retired watch from a later barrier.
    let n = count(&io);
    let held = io.expect("conversations.history");
    h._dom.render_immediate_to_vec();
    let control = button(cap.borrow().as_ref().unwrap(), "message-search-react-1-0").unwrap();
    click(&h._dom, control);
    settle(&mut h._dom, || count(&io) == n + 1).await;
    let packet = json!({"type":"reaction_added","user":"U2","reaction":"wave","item":{"type":"message","channel":c(&h),"ts":TS}}).to_string();
    updates(
        h.state,
        realtime::protected_updates(&connection, 23, &packet),
    );
    reply(held, history());
    settle(&mut h._dom, || {
        actions::presentation(&h.state.peek()).is_some_and(|p| p.terminal)
    })
    .await;
    let view = actions::presentation(&h.state.peek()).unwrap();
    assert!(!view.ready);
    assert!(
        view.notice
            .as_ref()
            .unwrap()
            .contains("This message changed")
    );
    assert_eq!(
        count(&io),
        n + 1,
        "no follow-on info, membership or write after retirement"
    );
    actions::close(h.state, view.id);
    // A held physical write survives actual full Shell replacement. A new Book
    // of the same actor cannot overlap it or turn its late outcome into a send.
    let id = ready(&mut h, &cap, &io).await;
    let n = count(&io);
    let membership = io.expect("reactions.get");
    let lost = io.expect("reactions.add");
    choose(&h, id, &name);
    settle(&mut h._dom, || count(&io) == n + 1).await;
    reply(membership, members(&h, None));
    settle(&mut h._dom, || count(&io) == n + 2).await;
    let mut fresh = ShellState::fixture(crate::media::MediaRegistry::default());
    {
        let s = h.state.peek();
        fresh.core.active_account = s.core.active_account.clone();
        fresh.core.account_epoch = s.core.account_epoch + 1;
        fresh.core.transport = Some(h.transport.clone());
        fresh.core.session = s.core.session.clone();
    }
    fresh.overlay = Some(crate::state::Overlay::Search);
    fresh.set_search_kind(SearchKind::Messages);
    fresh.set_search_query("release".into());
    fresh.search_reaction_actions.io = Some(io.clone());
    let (conn, initial) = realtime::protected_connection(24);
    fresh.core.workspaces.get_mut("T1").unwrap().rt_generation = 24;
    fresh.core.workspaces.get_mut("T1").unwrap().rt =
        super_platinum_core::state::RealtimeStatus::Connected(conn);
    h._dom.in_scope(ScopeId::ROOT, || {
        crate::bootstrap::install_loaded(h.state, fresh)
    });
    updates(h.state, initial);
    publish(&h).await;
    let before = count(&io);
    h._dom.in_scope(ScopeId::ROOT, || {
        let row = RowSource::capture(&h.state.peek(), 0).unwrap();
        actions::open(h.state, row)
    });
    assert!(actions::presentation(&h.state.peek()).unwrap().terminal);
    assert_eq!(count(&io), before);
    lost.send(Err(Error::Transport("synthetic lost response".into())))
        .unwrap();
    let state = h.state;
    settle(&mut h._dom, || {
        let mut current = state;
        actions::poll(&mut current.write());
        actions::presentation(&current.peek())
            .is_some_and(|p| p.notice.as_ref().is_some_and(|n| n.contains("unconfirmed")))
    })
    .await;
    let new_id = actions::presentation(&h.state.peek()).unwrap().id;
    actions::close(h.state, new_id);
    h._dom.in_scope(ScopeId::ROOT, || {
        let row = RowSource::capture(&h.state.peek(), 0).unwrap();
        actions::open(h.state, row)
    });
    assert!(actions::presentation(&h.state.peek()).unwrap().terminal);
    assert_eq!(count(&io), before, "Unknown must never replay");
}

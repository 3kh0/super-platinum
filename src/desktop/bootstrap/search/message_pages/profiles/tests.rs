use super::*;
use crate::bootstrap::search_message_pages as pages;
use crate::resource_reads::render_test::{self, Captured, button, click, settle, text};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};
use super_platinum_core::slack::realtime::{self, Connection, RtUpdate};
use super_platinum_core::slack::{Error, PreparedRequest, client::RequestBody};

mod pagination;

const ROOT: &str = "1700000001.000001";
const SECOND: &str = "1700000002.000001";
fn render(state: Signal<ShellState>) -> Element {
    let shell = state.read();
    rsx! {div {
        if shell.overlay==Some(crate::state::Overlay::Search) {
            {crate::overlays::overlay_view(state,crate::state::Overlay::Search,&shell)}
        }
        {crate::profile::profile_pane(state,&shell)}
    }}
}
fn raw_page(channel: &str) -> Value {
    json!({"ok":true,"items":[
        {"team":"T1","channel":{"id":channel,"name":"ship"},"messages":[
            {"type":"message","channel":channel,"team":"T1","user":"U1","ts":ROOT,
             "text":"Original release root","blocks":[{"type":"rich_text","elements":[
                {"type":"rich_text_section","elements":[{"type":"text","text":"Original release root","style":{"bold":true}}]}]}]}
        ]},
        {"team":"T1","channel":{"id":channel,"name":"ship"},"messages":[
            {"type":"message","channel":channel,"team":"T1","user":"U2","ts":SECOND,"text":"Original peer release"}
        ]}
    ],"pagination":{"page":1,"page_count":2,"total_count":40}})
}
fn member(id: &str, name: &str) -> Value {
    json!({"id":id,"name":format!("member_{id}"),"team_id":"T1","enterprise_user":null,
        "is_bot":false,"is_app_user":false,"deleted":false,"can_interact":true,
        "profile":{"display_name":name,"title":"Original read-only details","email":format!("{id}@fixture.invalid")}})
}
fn reply(send: tokio::sync::oneshot::Sender<Result<Value, Error>>, raw: Value) {
    send.send(Ok(raw)).unwrap();
}
fn count(io: &test_io::Io) -> usize {
    io.calls.borrow().len()
}
fn users(state: Signal<ShellState>) -> Value {
    serde_json::to_value(&state.peek().core.workspaces["T1"].users).unwrap()
}
async fn publish(dom: &mut VirtualDom, mut state: Signal<ShellState>, raw: Value) {
    let (transport, client, session) = {
        let s = state.peek();
        (
            s.core.transport.clone().unwrap(),
            s.core.client.clone(),
            s.core.session.as_ref().unwrap().workspaces["T1"].clone(),
        )
    };
    let query = state.peek().search_query.clone();
    let request = crate::bootstrap::search::begin(&mut state.write(), "T1", &query, false).unwrap();
    let read = pages::initial(&state.peek(), &request, &transport, &client, &session).unwrap();
    let done = Rc::new(Cell::new(false));
    let finished = done.clone();
    dom.in_scope(ScopeId::ROOT, || {
        spawn(async move {
            crate::bootstrap::search::messages::load_owned_with_profiles(
                state,
                request,
                &transport,
                &client,
                &session,
                None,
                Some(read),
                move |request: PreparedRequest| async move {
                    assert!(
                        request
                            .url
                            .split('?')
                            .next()
                            .unwrap()
                            .ends_with("/api/search.modules.messages")
                    );
                    let RequestBody::Form(fields) = request.body else {
                        panic!("original Search form")
                    };
                    let fields = fields
                        .into_iter()
                        .collect::<std::collections::BTreeMap<_, _>>();
                    assert_eq!(fields["query"], "release");
                    assert_eq!(fields["page"], "1");
                    assert_eq!(fields["count"], "20");
                    Ok(raw)
                },
                |_| std::future::ready(Ok(Vec::new())),
            )
            .await;
            finished.set(true);
        })
    });
    settle(dom, || done.get()).await;
}
fn updates(mut state: Signal<ShellState>, updates: Vec<RtUpdate>) {
    for update in updates {
        match update {
            RtUpdate::ReadBarrier {
                generation,
                barrier,
            } => {
                accept_barrier(&mut state.write(), "T1", generation, barrier);
                after_barrier(state, "T1", generation);
            }
            RtUpdate::Event { generation, event } => {
                crate::realtime::apply_in_view_with_context(
                    &mut state.write(),
                    "T1",
                    generation,
                    *event,
                    true,
                    true,
                );
            }
            RtUpdate::Preference {
                generation,
                carrier,
            } => {
                crate::realtime::apply_in_view_with_context(
                    &mut state.write(),
                    "T1",
                    generation,
                    carrier.event().clone(),
                    true,
                    true,
                );
            }
            RtUpdate::Connected {
                generation,
                connection,
            } => {
                let mut shell = state.write();
                connected(&mut shell, "T1");
                let workspace = shell.core.workspaces.get_mut("T1").unwrap();
                workspace.rt_generation = generation;
                workspace.rt = super_platinum_core::state::RealtimeStatus::Connected(connection);
            }
            _ => panic!("original fixture data only"),
        }
    }
}
fn rig_pending_barrier() -> (
    VirtualDom,
    Signal<ShellState>,
    Captured,
    Rc<test_io::Io>,
    Connection,
    Vec<RtUpdate>,
) {
    let (dom, mut state, captured) = render_test::mount("search-messages-pages", render);
    let (connection, initial) = realtime::protected_connection(37);
    {
        let mut shell = state.write();
        shell.clear_search_results();
        shell.set_search_kind(super_platinum_core::search::SearchKind::Messages);
        shell.set_search_query("release".into());
        shell.search_message_pages_fixture = None;
        shell.overlay = Some(crate::state::Overlay::Search);
        let workspace = shell.core.workspaces.get_mut("T1").unwrap();
        workspace.rt_generation = 37;
        workspace.rt = super_platinum_core::state::RealtimeStatus::Disconnected;
    }
    let io = Rc::new(test_io::Io::default());
    dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
    (dom, state, captured, io, connection, initial)
}
fn rig() -> (
    VirtualDom,
    Signal<ShellState>,
    Captured,
    Rc<test_io::Io>,
    Connection,
) {
    let (dom, state, captured, io, connection, initial) = rig_pending_barrier();
    dom.in_scope(ScopeId::ROOT, || updates(state, initial));
    (dom, state, captured, io, connection)
}

#[tokio::test(flavor = "current_thread")]
async fn search_author_original_page_prepares_without_blocking_results_opens_read_only_and_returns_with_fresh_controls()
 {
    let (mut dom, state, captured, io, connection, initial) = rig_pending_barrier();
    let original_users = users(state);
    let channel = state.peek().core.active_channel.clone().unwrap();
    let prefs = io.expect("users.prefs.get");
    let humans = io.expect("users.info");
    let mut original = raw_page(&channel);
    original["items"][1]["channel"] = json!({"id":"D2","is_im":true,"user":"U2"});
    original["items"][1]["messages"][0]["channel"] = json!("D2");
    publish(&mut dom, state, original).await;
    assert_eq!(
        count(&io),
        0,
        "Search publication cannot invent an applied initial barrier"
    );
    let indexed = pages::rows(&state.peek()).to_vec();
    assert_eq!(indexed.len(), 2);
    assert!(!state.peek().search_pager.loading);
    assert!(
        button(
            captured.borrow().as_ref().unwrap(),
            "message-search-hit-1-0"
        )
        .is_some()
    );
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Original release root"));
    assert!(control(&state.peek(), 0).is_none());
    dom.in_scope(ScopeId::ROOT, || updates(state, initial));
    // The production post-barrier continuation is driven by updates below.
    settle(&mut dom, || count(&io) == 1).await;
    assert_eq!(
        count(&io),
        1,
        "human IO must wait for actual policy exclusion"
    );
    reply(prefs, json!({"ok":true,"prefs":{"hidden_users":""}}));
    settle(&mut dom, || count(&io) == 2).await;
    assert_eq!(
        io.calls.borrow()[1].1["updated_ids"],
        json!({"U1":0,"U2":0})
    );
    assert!(
        control(&state.peek(), 0).is_none(),
        "cached users cannot enable authors"
    );
    reply(
        humans,
        json!({"ok":true,"results":[member("U1","Fresh Maya"),member("U2","Fresh Jules")]}),
    );
    settle(&mut dom, || {
        control(&state.peek(), 0).is_some() && control(&state.peek(), 1).is_some()
    })
    .await;
    assert_eq!(users(state), original_users);

    // Raw ingress fences clicks before native apply, without permanently
    // retiring good metadata after an unrelated ordinary message is applied.
    let packet = json!({"type":"message","channel":channel,"user":"U3","ts":"1700000010.000001","text":"Unrelated event"}).to_string();
    let pending = realtime::protected_updates(&connection, 37, &packet);
    assert!(control(&state.peek(), 0).is_none());
    updates(state, pending);
    assert!(control(&state.peek(), 0).is_some());
    assert_eq!(
        count(&io),
        2,
        "unrelated events cannot cause metadata IO loops"
    );
    dom.render_immediate_to_vec();
    let root = button(
        captured.borrow().as_ref().unwrap(),
        "message-search-author-1-0",
    )
    .unwrap();
    let peer = button(
        captured.borrow().as_ref().unwrap(),
        "message-search-author-1-1",
    )
    .unwrap();
    let old_hit = button(
        captured.borrow().as_ref().unwrap(),
        "message-search-hit-1-0",
    )
    .unwrap();
    click(&dom, root);
    dom.render_immediate_to_vec();
    assert_eq!(state.peek().profile_user.as_deref(), Some("U1"));
    assert_eq!(state.peek().overlay, None);
    assert!(state.peek().search_opening.is_none());
    let rendered = text(captured.borrow().as_ref().unwrap());
    assert!(rendered.contains("Fresh Maya") && rendered.contains("Read-only profile details"));
    assert!(!rendered.contains("Huddle") && !rendered.contains("Send message"));
    let back = button(
        captured.borrow().as_ref().unwrap(),
        "message-search-profile-back",
    )
    .unwrap();
    let old_back = render_test::Control {
        click: back.click.clone(),
        disabled: back.disabled,
    };
    click(&dom, peer);
    assert_eq!(
        state.peek().profile_user.as_deref(),
        Some("U1"),
        "queued peer cannot retarget the pane"
    );
    click(&dom, back);
    dom.render_immediate_to_vec();
    assert_eq!(state.peek().overlay, Some(crate::state::Overlay::Search));
    assert_eq!(state.peek().search_query, "release");
    assert_eq!(pages::rows(&state.peek()), indexed);
    assert!(state.peek().profile_user.is_none());
    let dialog = state.peek().search_open_generation;
    click(&dom, old_hit);
    assert_eq!(state.peek().search_open_generation, dialog);
    assert!(
        state.peek().search_opening.is_none(),
        "the previous dialog's whole-hit callback is retired"
    );
    pagination::round_trip(&mut dom, state, &captured, &io).await;
    let prepared_reads = count(&io);
    assert_eq!(prepared_reads, 6);
    let fresh_peer = button(
        captured.borrow().as_ref().unwrap(),
        "message-search-author-1-1",
    )
    .unwrap();
    click(&dom, fresh_peer);
    dom.render_immediate_to_vec();
    assert_eq!(state.peek().profile_user.as_deref(), Some("U2"));
    click(&dom, old_back);
    assert_eq!(
        state.peek().profile_user.as_deref(),
        Some("U2"),
        "Back belongs to one exact pane visit"
    );
    assert_eq!(
        state.peek().core.active_channel.as_deref(),
        Some(channel.as_str()),
        "a DM author Profile does not open or join that conversation"
    );
    assert_eq!(users(state), original_users);
    assert_eq!(count(&io), prepared_reads);

    let queued = realtime::protected_updates(&connection,37,&json!({"type":"message","channel":channel,"user":"U3","ts":"1700000011.000001","text":"Another unrelated event"}).to_string());
    assert!(
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .is_some_and(|p| p.user.is_some()),
        "queued unrelated ingress cannot blank an already owned read-only pane"
    );
    updates(state, queued);
    assert!(
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .is_some_and(|p| p.user.is_some())
    );
    assert_eq!(count(&io), prepared_reads);

    // Refresh is a Search-owned full-policy + human read. An outage retains
    // the already loaded pane; an actual unqualified human response clears it.
    let outage = io.expect("users.prefs.get");
    let retry = crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
        .unwrap()
        .retry
        .unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        crate::bootstrap::profile_search::retry(state, retry)
    });
    settle(&mut dom, || count(&io) == prepared_reads + 1).await;
    outage
        .send(Err(Error::Offline("Synthetic outage".into())))
        .unwrap();
    settle(&mut dom, || {
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .is_some_and(|p| !p.loading)
    })
    .await;
    let loaded = crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
        .unwrap()
        .user
        .cloned();
    assert_eq!(
        loaded
            .as_ref()
            .unwrap()
            .profile
            .as_ref()
            .unwrap()
            .display_name
            .as_deref(),
        Some("Fresh Jules")
    );
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Fresh Jules"));

    let schema = io.expect("users.prefs.get");
    let retry = crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
        .unwrap()
        .retry
        .unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        crate::bootstrap::profile_search::retry(state, retry)
    });
    settle(&mut dom, || count(&io) == prepared_reads + 2).await;
    reply(schema, json!({"ok":true,"prefs":{"hidden_users":null}}));
    settle(&mut dom, || {
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .is_some_and(|p| !p.loading)
    })
    .await;
    assert!(
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .unwrap()
            .user
            .is_none(),
        "an actual malformed policy reply is not a transport outage"
    );

    let prefs = io.expect("users.prefs.get");
    let human = io.expect("users.info");
    let retry = crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
        .unwrap()
        .retry
        .unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        crate::bootstrap::profile_search::retry(state, retry)
    });
    settle(&mut dom, || count(&io) == prepared_reads + 3).await;
    reply(prefs, json!({"ok":true,"prefs":{"hidden_users":""}}));
    settle(&mut dom, || count(&io) == prepared_reads + 4).await;
    reply(
        human,
        json!({"ok":true,"results":[member("U2","Recovered Jules")]}),
    );
    settle(&mut dom, || {
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .is_some_and(|p| !p.loading && p.user.is_some())
    })
    .await;
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Recovered Jules"));

    let prefs = io.expect("users.prefs.get");
    let human = io.expect("users.info");
    let retry = crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
        .unwrap()
        .retry
        .unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        crate::bootstrap::profile_search::retry(state, retry)
    });
    settle(&mut dom, || count(&io) == prepared_reads + 5).await;
    reply(prefs, json!({"ok":true,"prefs":{"hidden_users":""}}));
    settle(&mut dom, || count(&io) == prepared_reads + 6).await;
    assert_eq!(
        io.calls.borrow()[prepared_reads + 5].1["updated_ids"],
        json!({"U2":0})
    );
    let mut denied = member("U2", "Denied replacement");
    denied["can_interact"] = json!(false);
    reply(human, json!({"ok":true,"results":[denied]}));
    settle(&mut dom, || {
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .is_some_and(|p| !p.loading)
    })
    .await;
    assert!(
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .unwrap()
            .user
            .is_none()
    );
    let rendered = text(captured.borrow().as_ref().unwrap());
    assert!(rendered.contains("Read-only profile details"));
    assert!(
        !rendered.contains("Fresh Jules")
            && !rendered.contains("Recovered Jules")
            && !rendered.contains("Denied replacement")
    );
    assert!(!rendered.contains("Send message") && !rendered.contains("Huddle"));
    assert_eq!(users(state), original_users);
    assert_eq!(count(&io), prepared_reads + 6);
}

#[tokio::test(flavor = "current_thread")]
async fn search_author_actual_policy_missing_literal_csv_lossy_ingress_and_late_human_refuse_cache_or_equal_query_reentry()
 {
    let (mut dom, mut state, captured, io, connection) = rig();
    let original_users = users(state);
    let channel = state.peek().core.active_channel.clone().unwrap();
    let missing = io.expect("users.prefs.get");
    publish(&mut dom, state, raw_page(&channel)).await;
    settle(&mut dom, || count(&io) == 1).await;
    reply(missing, json!({"ok":true,"prefs":{}}));
    settle(&mut dom, || {
        button(
            captured.borrow().as_ref().unwrap(),
            "message-search-author-retry",
        )
        .is_some()
    })
    .await;
    assert_eq!(
        count(&io),
        1,
        "absent policy never becomes empty/visible or starts human IO"
    );
    assert!(control(&state.peek(), 0).is_none() && control(&state.peek(), 1).is_none());
    assert_eq!(pages::rows(&state.peek()).len(), 2);

    let prefs = io.expect("users.prefs.get");
    let humans = io.expect("users.info");
    let retry = button(
        captured.borrow().as_ref().unwrap(),
        "message-search-author-retry",
    )
    .unwrap();
    assert!(!retry.disabled);
    click(&dom, retry);
    settle(&mut dom, || count(&io) == 2).await;
    // Source splits literally: the entry " U1" is distinct from actor U1.
    reply(prefs, json!({"ok":true,"prefs":{"hidden_users":" U1,U2"}}));
    settle(&mut dom, || count(&io) == 3).await;
    assert_eq!(io.calls.borrow()[2].1["updated_ids"], json!({"U1":0}));
    reply(
        humans,
        json!({"ok":true,"results":[member("U1","Qualified original")]}),
    );
    settle(&mut dom, || control(&state.peek(), 0).is_some()).await;
    assert!(
        control(&state.peek(), 1).is_none(),
        "hidden peer cannot borrow cached human data"
    );
    let old_author = button(
        captured.borrow().as_ref().unwrap(),
        "message-search-author-1-0",
    )
    .unwrap();

    let lossy = realtime::protected_updates(&connection, 37, "not json");
    assert!(
        !lossy.is_empty()
            && lossy
                .iter()
                .all(|u| matches!(u, RtUpdate::ReadBarrier { .. }))
    );
    assert!(
        control(&state.peek(), 0).is_none(),
        "raw ingress fences queued controls"
    );
    updates(state, lossy);
    assert!(
        control(&state.peek(), 0).is_none(),
        "a bare barrier must not revive old policy"
    );
    click(&dom, old_author);
    assert!(state.peek().profile_user.is_none());
    assert_eq!(
        count(&io),
        3,
        "retirement never starts an automatic read loop"
    );
    dom.render_immediate_to_vec();

    let prefs = io.expect("users.prefs.get");
    let humans = io.expect("users.info");
    let retry = button(
        captured.borrow().as_ref().unwrap(),
        "message-search-author-retry",
    )
    .unwrap();
    click(&dom, retry);
    settle(&mut dom, || count(&io) == 4).await;
    reply(prefs, json!({"ok":true,"prefs":{"hidden_users":"U1"}}));
    settle(&mut dom, || count(&io) == 5).await;
    assert_eq!(io.calls.borrow()[4].1["updated_ids"], json!({"U2":0}));
    let mut denied = member("U2", "Cannot interact");
    denied["can_interact"] = json!(false);
    reply(humans, json!({"ok":true,"results":[denied]}));
    settle(&mut dom, || io.completed.get() == 5).await;
    assert!(control(&state.peek(), 0).is_none() && control(&state.peek(), 1).is_none());
    assert_eq!(users(state), original_users);

    // A fresh original page may prepare again, but its held response cannot
    // revive the old intent after an edit/reversion of the query text.
    let prefs = io.expect("users.prefs.get");
    let humans = io.expect("users.info");
    publish(&mut dom, state, raw_page(&channel)).await;
    settle(&mut dom, || count(&io) == 6).await;
    reply(prefs, json!({"ok":true,"prefs":{"hidden_users":""}}));
    settle(&mut dom, || count(&io) == 7).await;
    state.write().set_search_query("another query".into());
    state.write().set_search_query("release".into());
    humans
        .send(Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }))
        .unwrap();
    settle(&mut dom, || io.completed.get() == 7).await;
    assert!(control(&state.peek(), 0).is_none() && control(&state.peek(), 1).is_none());
    assert!(state.peek().profile_user.is_none());
    assert_eq!(users(state), original_users);
    assert_eq!(count(&io), 7);
    let original_rate_owner = {
        let s = state.peek();
        let (owner, deadline, blocked) = s
            .search_author_profiles
            .cooldowns
            .get("T1")
            .expect("the original attempted late429 survives retired query/UI work");
        assert!(owner.current(&s.core));
        assert!(*blocked || deadline.is_some_and(|at| at > std::time::Instant::now()));
        owner.clone()
    };
    publish(&mut dom, state, raw_page(&channel)).await;
    assert_eq!(pages::rows(&state.peek()).len(), 2);
    assert!(control(&state.peek(), 0).is_none() && control(&state.peek(), 1).is_none());
    assert_eq!(
        count(&io),
        7,
        "new query/page preparation cannot reset the original account cooldown"
    );
    state.write().core.account_epoch += 1;
    assert!(
        !original_rate_owner.current(&state.peek().core),
        "a rate observation cannot poison a successor account owner"
    );
}

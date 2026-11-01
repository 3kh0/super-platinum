use super::*;
use crate::bootstrap::test_support::{harness, pending, users};
use crate::resource_reads::render_test::{Captured, Control, button, click, text};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn page(root: &str, reply: &str) -> ThreadsViewPage {
    serde_json::from_value(json!({"threads":[{"root_msg":{"type":"message","channel":"C1","team":"T1","ts":root,"user":"U1","text":"Original root","reply_count":1},"latest_replies":[{"type":"message","channel":"C1","team":"T1","thread_ts":root,"ts":reply,"user":"U2","text":"Original reply"}]}],"has_more":true,"max_ts":"9999.0"})).unwrap()
}
fn member(id: &str, name: &str) -> Value {
    json!({"id":id,"name":format!("member_{id}"),"team_id":"T1","enterprise_user":null,"is_bot":false,"is_app_user":false,"deleted":false,"can_interact":true,"profile":{"display_name":name,"title":"Reader","email":format!("{id}@fixture.invalid")}})
}
fn setup(mut state: Signal<ShellState>) {
    let mut shell = state.write();
    shell.main_view = MainView::Threads;
    shell.overlay = None;
    shell.toast = None;
    shell.core.threads_view = Default::default();
    let epoch = shell.core.account_epoch;
    shell.core.threads_view.ensure_scope(epoch, "T1");
}
fn source(state: Signal<ShellState>) -> Source {
    Source::capture(&state.peek()).unwrap()
}
fn open(state: Signal<ShellState>, ts: &str) -> Option<Open> {
    let s = state.peek();
    let item = s
        .core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.ts.as_deref() == Some("10.0"))?;
    let msg = if ts == "10.0" {
        &item.root_msg
    } else {
        item.latest_replies
            .iter()
            .find(|m| m.ts.as_deref() == Some(ts))?
    };
    Source::capture(&s)?.profile(&s, item, msg)
}
async fn admitted(state: Signal<ShellState>) -> Source {
    let reader = source(state);
    assert!(
        reader
            .run_with(state, Mode::Refresh, |_| async { Ok(page("10.0", "11.0")) })
            .await
            .is_some()
    );
    reader
}
async fn members_ready(reader: Source, state: Signal<ShellState>) {
    members::load_with(reader, state, |transport, request| async move {
        assert_eq!(transport.d_cookie(), "synthetic");
        assert!(
            request
                .url
                .starts_with("https://edgeapi.slack.com/cache/T1/users/info?")
        );
        let super_platinum_core::slack::client::RequestBody::Json(body) = request.body else {
            panic!()
        };
        assert_eq!(body["updated_ids"], json!({"U1":0,"U2":0}));
        assert_eq!(body["check_interaction"], json!(true));
        assert_eq!(body["include_profile_only_users"], json!(true));
        Ok(json!({"results":[member("U1","Fresh Maya"),member("U2","Fresh Jules")]}))
    })
    .await;
}
fn mount(shell: ShellState) -> (VirtualDom, Signal<ShellState>, Captured) {
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let captured = Captured::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot, captured): (Input, Slot, Captured)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            let rendered = rsx! {div{{crate::view::thread_list_for_test(state,&state.read())}{crate::profile::profile_pane(state,&state.read())}}};
            *captured.borrow_mut() = Some(rendered.as_ref().unwrap().clone());
            rendered
        },
        (
            Rc::new(RefCell::new(Some(shell))),
            slot.clone(),
            captured.clone(),
        ),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), captured)
}

#[tokio::test(flavor = "current_thread")]
async fn thread_profile_original_root_reply_headers_open_owned_read_only_details_and_refresh_without_canonical_actions()
 {
    let h = harness();
    let mut initial = h.state;
    setup(initial);
    let before = users(initial);
    let reader = admitted(initial).await;
    assert!(
        open(initial, "10.0").is_none(),
        "a cached known user is not a member receipt"
    );
    members_ready(reader, initial).await;
    assert_eq!(users(initial), before);
    let shell = std::mem::replace(
        &mut *initial.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    let (mut dom, mut state, captured) = mount(shell);
    let root_id = "thread-profile-C1-10.0-10.0";
    let reply_id = "thread-profile-C1-10.0-11.0";
    let root = button(captured.borrow().as_ref().unwrap(), root_id).unwrap();
    let reply = button(captured.borrow().as_ref().unwrap(), reply_id).unwrap();
    let stale = Control {
        click: reply.click.clone(),
        disabled: reply.disabled,
    };
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Fresh Maya"));
    click(&dom, root);
    dom.render_immediate_to_vec();
    assert_eq!(state.peek().profile_user.as_deref(), Some("U1"));
    let rendered = text(captured.borrow().as_ref().unwrap());
    assert!(rendered.contains("Read-only profile details"));
    assert!(!rendered.contains("Message") && !rendered.contains("Huddle"));
    assert_eq!(users(state), before);
    click(&dom, stale);
    assert_eq!(
        state.peek().profile_user.as_deref(),
        Some("U1"),
        "queued peer from before the pane commit cannot retarget it"
    );
    dom.render_immediate_to_vec();
    let reply = button(captured.borrow().as_ref().unwrap(), reply_id).unwrap();
    click(&dom, reply);
    dom.render_immediate_to_vec();
    assert_eq!(state.peek().profile_user.as_deref(), Some("U2"));
    let token = crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
        .unwrap()
        .retry
        .unwrap();
    crate::bootstrap::profile_search::refresh_member_with(state, token, |_, request| async move {
        let super_platinum_core::slack::client::RequestBody::Json(body) = request.body else {
            panic!()
        };
        assert_eq!(body["updated_ids"], json!({"U2":0}));
        Err(Error::Offline("synthetic outage".into()))
    })
    .await;
    assert_eq!(
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .unwrap()
            .user
            .unwrap()
            .profile
            .as_ref()
            .unwrap()
            .display_name
            .as_deref(),
        Some("Fresh Jules")
    );
    assert!(!state.peek().core.profile_pane.as_ref().unwrap().loading);
    let old_opener = open(state, "11.0").unwrap();
    let token = crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
        .unwrap()
        .retry
        .unwrap();
    crate::bootstrap::profile_search::refresh_member_with(state, token, |_, _| async {
        let mut denied = member("U2", "Denied");
        denied["deleted"] = json!(true);
        Ok(json!({"results":[denied]}))
    })
    .await;
    assert!(
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .unwrap()
            .user
            .is_none()
    );
    assert!(open(state, "11.0").is_none());
    old_opener.start(state);
    assert!(
        crate::bootstrap::profile_search::presentation(&state.peek(), "U2")
            .unwrap()
            .user
            .is_none(),
        "denied member version cannot resurrect through an old header"
    );
    assert_eq!(users(state), before);
}

#[tokio::test(flavor = "current_thread")]
async fn thread_profile_public_vectors_actor_replacement_and_observed_credential_aba_cannot_mint_or_revive_openers()
 {
    let h = harness();
    let mut state = h.state;
    setup(state);
    state.write().core.threads_view.items = page("10.0", "11.0").threads;
    state.write().core.threads_view.loaded = true;
    assert!(open(state, "10.0").is_none());
    let reader = admitted(state).await;
    members_ready(reader, state).await;
    let root = open(state, "10.0").unwrap();
    state.write().core.threads_view.items[0].root_msg.user = Some("U2".into());
    root.start(state);
    assert!(state.peek().profile_user.is_none());
    assert!(open(state, "10.0").is_none());
    state.write().core.threads_view.items[0].root_msg.user = Some("U1".into());
    assert!(
        open(state, "10.0").is_none(),
        "an observed actor replacement cannot revive by restoring public fields"
    );
    let reader = admitted(state).await;
    members_ready(reader, state).await;
    let root = open(state, "10.0").unwrap();
    let old = state.peek().core.session.clone();
    state.write().core.session.as_mut().unwrap().d_cookie = "changed".into();
    root.start(state);
    assert!(state.peek().profile_user.is_none());
    state.write().core.session = old;
    assert!(
        open(state, "10.0").is_none(),
        "observed original owner retirement is permanent"
    );
    root.start(state);
    assert!(state.peek().profile_user.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn thread_profile_original_append_keeps_prior_actor_member_receipts_without_redundant_lookup()
{
    let h = harness();
    let state = h.state;
    setup(state);
    let reader = admitted(state).await;
    members_ready(reader, state).await;
    let previous = open(state, "10.0").unwrap();
    let next = source(state);
    assert!(
        next.run_with(state, Mode::Older, |r| async move {
            assert!(r.cursor.is_some());
            Ok(page("20.0", "21.0"))
        })
        .await
        .is_some()
    );
    members::load_with(next, state, |_, _| async {
        panic!("existing original member ids need no second lookup");
        #[allow(unreachable_code)]
        Ok(Value::Null)
    })
    .await;
    assert!(open(state, "10.0").is_some());
    previous.start(state);
    assert!(
        state.peek().profile_user.is_none(),
        "old publication cannot borrow successor actor scope"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn thread_profile_late_raw_member_reply_cannot_publish_across_client_or_actor_replacement() {
    for change_client in [false, true] {
        let h = harness();
        let mut state = h.state;
        setup(state);
        let reader = admitted(state).await;
        let before = users(state);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut work = Box::pin(members::load_with(reader, state, |_, _| async {
            rx.await.unwrap()
        }));
        pending(work.as_mut()).await;
        if change_client {
            let mut config = super_platinum_core::slack::client::SlackClientConfig::default();
            config.identity.user_agent.push_str(" changed");
            state.write().core.client = SlackClient::new(config);
        } else {
            state.write().core.threads_view.items[0].root_msg.user = Some("OTHER".into());
            state.write().core.threads_view.items[0]
                .latest_replies
                .clear();
        }
        tx.send(Ok(
            json!({"results":[member("U1","Wrong scope"),member("U2","Wrong scope")]}),
        ))
        .unwrap();
        work.await;
        assert!(open(state, "10.0").is_none());
        assert!(open(state, "11.0").is_none());
        assert_eq!(users(state), before);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn thread_profile_owned_pane_late_refresh_and_canonical_cache_cannot_cross_an_account_or_peer_pane()
 {
    let h = harness();
    let mut state = h.state;
    setup(state);
    let reader = admitted(state).await;
    members_ready(reader, state).await;
    open(state, "10.0").unwrap().start(state);
    let token = crate::bootstrap::profile_search::presentation(&state.peek(), "U1")
        .unwrap()
        .retry
        .unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(crate::bootstrap::profile_search::refresh_member_with(
        state,
        token,
        |_, _| async { rx.await.unwrap() },
    ));
    pending(work.as_mut()).await;
    state.write().core.active_account = Some("peer".into());
    state.write().core.account_epoch += 1;
    let generation = state.peek().profile_generation;
    tx.send(Ok(json!({"results":[member("U1","Late wrong account")]})))
        .unwrap();
    work.await;
    let s = state.peek();
    let pane = crate::bootstrap::profile_search::presentation(&s, "U1").unwrap();
    assert!(pane.user.is_none() && pane.retry.is_none());
    assert_eq!(s.profile_generation, generation);
    assert!(
        s.core.workspaces["T1"].users.contains_key("U1"),
        "existing canonical row must not trigger actionable fallback"
    );
}

mod batches;
mod continuity;
#[cfg(feature = "protected-native-fixture")]
mod document;
mod earlier;
mod refresh;

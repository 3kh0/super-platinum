use super::*;
use crate::{media::MediaRegistry, state::Overlay};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::Poll,
};
use super_platinum_core::{
    config::Session,
    search::{SearchKind, SearchSort},
    slack::{client::RequestBody, models::User},
};
mod attachments;
mod context_filters;
mod conversations;

fn user(id: &str, name: &str) -> User {
    serde_json::from_value(json!({"id":id,"profile":{"display_name":name}})).unwrap()
}
fn setup(
    author: &str,
    thin: bool,
) -> (
    VirtualDom,
    Signal<ShellState>,
    Arc<Transport>,
    WorkspaceSession,
) {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    #[cfg(feature = "protected-native-fixture")]
    let transport = Arc::new(Transport::disconnected_fixture("synthetic"));
    #[cfg(not(feature = "protected-native-fixture"))]
    let transport = Arc::new(Transport::new("synthetic").unwrap());
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: shell.core.workspaces["T1"].self_user_id.clone(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    };
    shell.core.active_account = Some("message-author-fixture".into());
    shell.core.transport = Some(transport.clone());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [("T1".into(), session.clone())].into(),
    });
    if thin {
        shell.core.workspaces.get_mut("T1").unwrap().users.insert(author.into(),serde_json::from_value(json!({"id":author,"is_admin":true,"deleted":true,"profile":{"status_text":"Keep status"}})).unwrap());
    }
    shell.overlay = Some(Overlay::Search);
    shell.set_search_query("release".into());
    shell.set_search_kind(SearchKind::Messages);
    shell.search_pager.change_sort(SearchSort::Oldest);
    shell.core.composer.text = "Keep draft".into();
    type Props = (
        Rc<RefCell<Option<ShellState>>>,
        Rc<Cell<Option<Signal<ShellState>>>>,
    );
    let slot = Rc::new(Cell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): Props| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            VNode::empty()
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), transport, session)
}
fn begin(mut state: Signal<ShellState>) -> SearchRequest {
    let query = state.peek().search_query.clone();
    super::super::begin(&mut state.write(), "T1", &query, false).unwrap()
}
fn page(state: Signal<ShellState>, author: &str) -> Value {
    json!({"ok":true,"items":[{"channel":{"id":state.peek().core.active_channel,"name":"ship"},
        "messages":[{"user":author,"ts":"120.000001","thread_ts":"120.000000","text":"Found release"}]}],
        "pagination":{"page":1,"page_count":2,"total_count":2}})
}
fn request_fields(request: PreparedRequest) {
    assert!(request.url.contains("search.modules.messages"));
    let RequestBody::Form(fields) = request.body else {
        panic!("form required")
    };
    let fields = fields
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(fields["query"], "release");
    assert_eq!(fields["page"], "1");
    assert_eq!(fields["sort"], "timestamp");
    assert_eq!(fields["sort_dir"], "asc");
}
async fn pending<F: Future>(mut work: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(work.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
fn toast(state: Signal<ShellState>) -> Option<(String, std::time::Instant)> {
    state
        .peek()
        .toast
        .as_ref()
        .map(|t| (t.text.clone(), t.shown_at))
}

#[tokio::test]
async fn cached_id_only_author_gets_a_published_name_without_losing_roles_or_status() {
    let (_dom, state, transport, session) = setup("U_THIN_AUTHOR", true);
    let client = SlackClient::default();
    let request = begin(state);
    let payload = page(state, "U_THIN_AUTHOR");
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    load_with_profiles(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        |request| async move {
            request_fields(request);
            Ok(payload)
        },
        move |ids| {
            assert_eq!(ids, ["U_THIN_AUTHOR"]);
            observed.set(observed.get() + 1);
            std::future::ready(Ok(vec![user("U_THIN_AUTHOR", "Readable author")]))
        },
    )
    .await;
    assert_eq!(calls.get(), 1);
    let shell = state.peek();
    assert_eq!(shell.search_results.len(), 1);
    assert_eq!(shell.search_results[0].author, "Readable author");
    assert_eq!(
        shell.search_results[0].thread_ts.as_deref(),
        Some("120.000000")
    );
    assert_eq!((shell.search_pager.page, shell.search_pager.total), (1, 2));
    assert!(!shell.search_pager.loading && !shell.search_pager.failed);
    let cached = &shell.core.workspaces["T1"].users["U_THIN_AUTHOR"];
    assert!(cached.is_admin && cached.deleted);
    assert_eq!(
        cached.profile.as_ref().unwrap().status_text.as_deref(),
        Some("Keep status")
    );
    assert_eq!(shell.core.composer.text, "Keep draft");
}

#[tokio::test]
async fn held_author_lookup_projects_the_intervening_realtime_name_and_metadata() {
    let (_dom, mut state, transport, session) = setup("U_REALTIME_AUTHOR", false);
    let client = SlackClient::default();
    let request = begin(state);
    let payload = page(state, "U_REALTIME_AUTHOR");
    let (release, held) = tokio::sync::oneshot::channel();
    let mut hold = Some(held);
    let mut work = Box::pin(load_with_profiles(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        |_| async move { Ok(payload) },
        move |ids| {
            assert_eq!(ids, ["U_REALTIME_AUTHOR"]);
            let held = hold.take();
            async move {
                held.unwrap().await.unwrap();
                Ok(vec![user("U_REALTIME_AUTHOR", "Older lookup name")])
            }
        },
    ));
    pending(work.as_mut()).await;
    state.write().core.workspaces.get_mut("T1").unwrap().apply_user_update(serde_json::from_value(json!({"id":"U_REALTIME_AUTHOR","is_admin":true,"profile":{"display_name":"Realtime author","status_text":"Realtime status"}})).unwrap());
    release.send(()).unwrap();
    work.await;
    let shell = state.peek();
    assert_eq!(shell.search_results[0].author, "Realtime author");
    let cached = &shell.core.workspaces["T1"].users["U_REALTIME_AUTHOR"];
    assert!(cached.is_admin);
    assert_eq!(
        cached.profile.as_ref().unwrap().status_text.as_deref(),
        Some("Realtime status")
    );
    assert_eq!(shell.search_pager.page, 1);
    assert!(!shell.search_pager.loading);
}

#[tokio::test]
async fn query_or_session_replacement_during_author_lookup_keeps_new_state_and_cache_untouched() {
    for change in ["query", "session"] {
        let (_dom, mut state, transport, session) = setup("U_RETIRED_AUTHOR", false);
        let client = SlackClient::default();
        let request = begin(state);
        let payload = page(state, "U_RETIRED_AUTHOR");
        let (release, held) = tokio::sync::oneshot::channel();
        let mut hold = Some(held);
        let mut work = Box::pin(load_with_profiles(
            state,
            request,
            &transport,
            &client,
            &session,
            None,
            |_| async move { Ok(payload) },
            move |_| {
                let held = hold.take();
                async move {
                    held.unwrap().await.unwrap();
                    Ok(vec![user("U_RETIRED_AUTHOR", "Retired lookup")])
                }
            },
        ));
        pending(work.as_mut()).await;
        let before = toast(state);
        let next = if change == "query" {
            state.write().set_search_query("replacement".into());
            Some(begin(state))
        } else {
            state
                .write()
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token = "replacement".into();
            None
        };
        release.send(()).unwrap();
        work.await;
        let shell = state.peek();
        if let Some(next) = next {
            assert!(shell.search_pager.accepts(&next));
        } else {
            assert!(shell.search_pager.failed && !shell.search_pager.loading);
        }
        assert!(shell.search_results.is_empty());
        assert!(
            !shell.core.workspaces["T1"]
                .users
                .contains_key("U_RETIRED_AUTHOR")
        );
        drop(shell);
        assert_eq!(toast(state), before);
    }
}

#[tokio::test]
async fn optional_author_lookup_failure_still_publishes_fallback_and_finishes_pagination() {
    for close_dialog in [false, true] {
        let (_dom, mut state, transport, session) = setup("U_FALLBACK_AUTHOR", false);
        let client = SlackClient::default();
        let request = begin(state);
        let payload = page(state, "U_FALLBACK_AUTHOR");
        let before = toast(state);
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        load_with_profiles(
            state,
            request,
            &transport,
            &client,
            &session,
            None,
            |_| async move { Ok(payload) },
            move |ids| {
                assert_eq!(ids, ["U_FALLBACK_AUTHOR"]);
                observed.set(observed.get() + 1);
                if close_dialog {
                    state.write().dismiss_overlay();
                }
                std::future::ready(Err(Error::Offline("synthetic".into())))
            },
        )
        .await;
        assert_eq!(calls.get(), 1);
        let shell = state.peek();
        assert_eq!(shell.search_results[0].author, "U_FALLBACK_AUTHOR");
        assert_eq!(shell.search_results[0].text, "Found release");
        assert_eq!((shell.search_pager.page, shell.search_pager.total), (1, 2));
        assert!(shell.search_pager.has_more());
        assert!(!shell.search_pager.failed && !shell.search_pager.loading);
        if close_dialog {
            assert_eq!(shell.overlay, None);
        }
        drop(shell);
        assert_eq!(toast(state), before);
    }
}

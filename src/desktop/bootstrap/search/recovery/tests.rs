use super::*;
mod people;
use crate::bootstrap::reconnect::Visit;
use crate::media::MediaRegistry;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::poll_fn,
    pin::Pin,
    rc::Rc,
    task::Poll,
};
use super_platinum_core::{
    config::Session, search_query::SearchFilters, slack::client::RequestBody,
};

fn file(id: &str) -> Value {
    json!({"id":id,"title":id,"name":format!("{id}.txt"),"mimetype":"text/plain","mode":"hosted"})
}
fn setup(kind: SearchKind, loaded: bool) -> (VirtualDom, Signal<ShellState>) {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    #[cfg(feature = "protected-native-fixture")]
    let transport = Transport::disconnected_fixture("synthetic");
    #[cfg(not(feature = "protected-native-fixture"))]
    let transport = Transport::new("synthetic").unwrap();
    shell.core.transport = Some(Arc::new(transport));
    shell.core.active_account = Some("offline-search-recovery".into());
    let user = shell.core.workspaces["T1"].self_user_id.clone();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: user,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    };
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [("T1".into(), session)].into(),
    });
    shell.overlay = Some(Overlay::Search);
    shell.set_search_query("release in:ship".into());
    shell.set_search_kind(kind);
    shell.search_pager.change_sort(SearchSort::Oldest);
    let query = shell.search_query.clone();
    let first = super::super::begin(&mut shell, "T1", &query, false).unwrap();
    let failed = if loaded {
        shell.search_pager.complete(
            &first,
            Some(
                &serde_json::from_value(json!({"page":1,"page_count":3,"total_count":42})).unwrap(),
            ),
        );
        shell.search_file_results = vec![serde_json::from_value(file("F1")).unwrap()];
        super::super::begin(&mut shell, "T1", &query, true).unwrap()
    } else {
        first
    };
    shell.search_pager.fail(&failed);
    shell.core.composer.text = "Conversation draft stays".into();
    shell.search_filters = Some(SearchFilters {
        on: "2026-02-30".into(),
        ..Default::default()
    });
    shell.search_history_open = true;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .recent_searches
        .record(
            "distinct MRU head",
            SearchKind::Messages,
            SearchSort::Newest,
        );
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
    (dom, slot.get().unwrap())
}
fn candidate(mut state: Signal<ShellState>) -> (Visit, Candidate) {
    let visit = Visit::begin(&mut state.write());
    let candidate = Candidate::capture(&state.peek()).unwrap();
    (visit, candidate)
}
fn custody(state: Signal<ShellState>) -> Value {
    let shell = state.peek();
    json!({"query":shell.search_query,"kind":format!("{:?}",shell.search_pager.kind),
        "sort":format!("{:?}",shell.search_pager.sort),"historyOpen":shell.search_history_open,
        "history":crate::view::search::history::entries(&shell),
        "filters":shell.search_filters.as_ref().map(|f|f.on.clone()),
        "draft":shell.core.composer.text,"channel":shell.core.active_channel})
}
fn toast(state: Signal<ShellState>) -> Option<(String, std::time::Instant)> {
    state
        .peek()
        .toast
        .as_ref()
        .map(|toast| (toast.text.clone(), toast.shown_at))
}
fn ids(state: Signal<ShellState>) -> Vec<String> {
    state
        .peek()
        .search_file_results
        .iter()
        .filter_map(|f| f.id.clone())
        .collect()
}
fn response(state: Signal<ShellState>, kind: SearchKind, page: u32) -> Value {
    if kind == SearchKind::Files {
        json!({"ok":true,"files":{"matches":[file("F1"),file("F2"),file("F2")],"paging":{"page":page,"pages":3,"total":42}}})
    } else {
        let shell = state.peek();
        let workspace = &shell.core.workspaces["T1"];
        let user = workspace.users.keys().next().unwrap();
        json!({"ok":true,"items":[{"channel":{"id":shell.core.active_channel},"messages":[{"user":user,"ts":"10.1","thread_ts":"10.0","text":"Found release"}]}],
            "pagination":{"page":page,"page_count":3,"total_count":42}})
    }
}
fn request_fields(request: PreparedRequest, kind: SearchKind, page: u32) {
    let endpoint = if kind == SearchKind::Files {
        "search.files"
    } else {
        "search.modules.messages"
    };
    assert!(request.url.contains(endpoint));
    let RequestBody::Form(fields) = request.body else {
        panic!("form required")
    };
    let fields = fields
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(fields["query"], "release in:ship");
    assert_eq!(fields["sort"], "timestamp");
    assert_eq!(fields["sort_dir"], "asc");
    assert_eq!(fields["page"], page.to_string());
}
async fn pending<F: Future>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn failed_message_query_retries_on_restore_without_changing_filter_history_or_draft() {
    let (_dom, state) = setup(SearchKind::Messages, false);
    let before = custody(state);
    let (visit, old) = candidate(state);
    recover_with(state, visit, old, |request| async move {
        request_fields(request, SearchKind::Messages, 1);
        Err(Error::Offline("synthetic".into()))
    })
    .await;
    assert!(state.peek().search_pager.failed && !state.peek().search_pager.loading);
    assert_eq!(custody(state), before);
    let payload = response(state, SearchKind::Messages, 1);
    let (visit, next) = candidate(state);
    recover_with(state, visit, next, |request| async move {
        request_fields(request, SearchKind::Messages, 1);
        Ok(payload)
    })
    .await;
    let shell = state.peek();
    assert_eq!(shell.search_pager.page, 1);
    assert!(!shell.search_pager.failed && !shell.search_pager.loading);
    assert_eq!(shell.search_results.len(), 1);
    assert_eq!(shell.search_results[0].thread_ts.as_deref(), Some("10.0"));
    assert_eq!(shell.search_results[0].text, "Found release");
    drop(shell);
    assert_eq!(custody(state), before);
}

#[tokio::test]
async fn failed_file_page_retains_loaded_order_through_failure_and_next_restore() {
    let (_dom, state) = setup(SearchKind::Files, true);
    let before = custody(state);
    let (visit, old) = candidate(state);
    recover_with(state, visit, old, |request| async move {
        request_fields(request, SearchKind::Files, 2);
        Err(Error::Offline("synthetic".into()))
    })
    .await;
    assert_eq!(ids(state), ["F1"]);
    assert_eq!(state.peek().search_pager.page, 1);
    assert_eq!(state.peek().search_pager.total, 42);
    assert_eq!(custody(state), before);
    let payload = response(state, SearchKind::Files, 2);
    let (visit, next) = candidate(state);
    recover_with(state, visit, next, |request| async move {
        request_fields(request, SearchKind::Files, 2);
        Ok(payload)
    })
    .await;
    assert_eq!(ids(state), ["F1", "F2"]);
    assert_eq!(state.peek().search_pager.page, 2);
    assert!(!state.peek().search_pager.failed && !state.peek().search_pager.loading);
    assert_eq!(custody(state), before);
}

#[cfg(feature = "protected-native-fixture")]
#[tokio::test]
async fn real_reconnect_entry_retries_search_before_the_underlying_resource_early_return() {
    let (mut dom, mut state) = setup(SearchKind::Files, true);
    state.write().channel_tab = crate::state::ChannelTab::Canvas;
    let before = custody(state);
    let revision = state.peek().search_pager.revision();
    dom.in_runtime(|| {
        dom.in_scope(dioxus::core::ScopeId::ROOT, || {
            spawn(crate::bootstrap::reload_after_outage(state))
        })
    });
    for _ in 0..16 {
        dom.render_immediate_to_vec();
        if state.peek().search_pager.revision() > revision && !state.peek().search_pager.loading {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        state.peek().search_pager.revision() > revision,
        "Search retry was skipped by the resource-tab return"
    );
    // The feature-only Transport has no HTTP client: this is a real dispatch
    // ending in Offline, not a live Slack request or network-state simulation.
    assert!(state.peek().search_pager.failed && !state.peek().search_pager.loading);
    assert_eq!(ids(state), ["F1"]);
    assert_eq!(custody(state), before);
}

#[tokio::test]
async fn an_admitted_rest_page_survives_a_new_reconnect_without_duplicate_admission() {
    let (_dom, mut state) = setup(SearchKind::Files, true);
    let payload = response(state, SearchKind::Files, 2);
    let (visit, candidate) = candidate(state);
    let (release, held) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(recover_with(state, visit, candidate, |_| async move {
        held.await.unwrap();
        Ok(payload)
    }));
    pending(work.as_mut()).await;
    let revision = state.peek().search_pager.revision();
    let next = Visit::begin(&mut state.write());
    schedule(state, next); // loading means no candidate/task, even outside a UI task.
    assert_eq!(state.peek().search_pager.revision(), revision);
    release.send(()).unwrap();
    work.await;
    assert_eq!(ids(state), ["F1", "F2"]);
    assert!(!state.peek().search_pager.loading);
}

#[tokio::test]
async fn queued_old_restore_never_admits_but_the_fresh_restore_can_recover() {
    let (_dom, mut state) = setup(SearchKind::Files, true);
    let (visit, old) = candidate(state);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let stale = recover_with(state, visit, old, move |_| async move {
        observed.set(observed.get() + 1);
        Err(Error::Api("must_not_execute".into()))
    });
    let (fresh_visit, fresh) = candidate(state);
    let revision = state.peek().search_pager.revision();
    stale.await;
    assert_eq!(calls.get(), 0);
    assert_eq!(state.peek().search_pager.revision(), revision);
    assert!(state.peek().search_pager.failed && !state.peek().search_pager.loading);
    assert_eq!(ids(state), ["F1"]);
    let payload = response(state, SearchKind::Files, 2);
    recover_with(state, fresh_visit, fresh, |_| async move { Ok(payload) }).await;
    assert_eq!(ids(state), ["F1", "F2"]);
}

#[tokio::test]
async fn retired_search_scope_never_publishes_a_held_page_or_failure_toast() {
    for kind in SearchKind::ALL {
        for change in [
            "account",
            "epoch",
            "team",
            "transport",
            "cookie",
            "cookie-error",
            "token",
            "user",
            "raw",
            "kind",
            "sort",
            "page",
            "dialog",
        ] {
            let (_dom, mut state) = setup(kind, kind == SearchKind::Files);
            let payload = response(state, kind, if kind == SearchKind::Files { 2 } else { 1 });
            let (visit, candidate) = candidate(state);
            let (release, held) = tokio::sync::oneshot::channel();
            let mut work = Box::pin(recover_with(state, visit, candidate, |_| async move {
                held.await.unwrap();
                if change == "cookie-error" {
                    Err(Error::Api("synthetic_failure".into()))
                } else {
                    Ok(payload)
                }
            }));
            pending(work.as_mut()).await;
            let before_toasts = toast(state);
            {
                let mut shell = state.write();
                match change {
                    "account" => shell.core.active_account = Some("replacement".into()),
                    "epoch" => shell.core.account_epoch += 1,
                    "team" => shell.core.active_team = Some("OTHER".into()),
                    "transport" => {
                        shell.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()))
                    }
                    "cookie" | "cookie-error" => {
                        shell.core.session.as_mut().unwrap().d_cookie = "replacement".into()
                    }
                    "token" => {
                        shell
                            .core
                            .session
                            .as_mut()
                            .unwrap()
                            .workspaces
                            .get_mut("T1")
                            .unwrap()
                            .token = "replacement".into()
                    }
                    "user" => {
                        shell
                            .core
                            .session
                            .as_mut()
                            .unwrap()
                            .workspaces
                            .get_mut("T1")
                            .unwrap()
                            .user_id = "OTHER".into()
                    }
                    "raw" => {
                        let query = format!("{} ", shell.search_query);
                        shell.set_search_query(query);
                    }
                    "kind" => {
                        shell.set_search_kind(if kind == SearchKind::Files {
                            SearchKind::Messages
                        } else {
                            SearchKind::Files
                        });
                    }
                    "sort" => {
                        shell.search_pager.change_sort(SearchSort::Relevant);
                    }
                    "page" => shell.search_pager.page += 1,
                    "dialog" => shell.cancel_search_open(),
                    _ => unreachable!(),
                }
            }
            let files = ids(state);
            let messages = state.peek().search_results.len();
            release.send(()).unwrap();
            work.await;
            assert_eq!(ids(state), files, "{kind:?}/{change}");
            assert_eq!(
                state.peek().search_results.len(),
                messages,
                "{kind:?}/{change}"
            );
            assert_eq!(toast(state), before_toasts);
            if change == "cookie-error" {
                assert!(state.peek().search_pager.failed && !state.peek().search_pager.loading);
            }
        }
    }
}

#[tokio::test]
async fn manual_replacement_owns_busy_state_while_cancel_and_disposal_retire_only_old_work() {
    for disposed in [false, true] {
        let (dom, state) = setup(SearchKind::Files, true);
        let payload = response(state, SearchKind::Files, 2);
        let (visit, candidate) = candidate(state);
        let (release, held) = tokio::sync::oneshot::channel();
        let mut work = Box::pin(recover_with(state, visit, candidate, |_| async move {
            held.await.unwrap();
            Ok(payload)
        }));
        pending(work.as_mut()).await;
        if disposed {
            drop(dom);
            release.send(()).unwrap();
            work.await;
            assert!(state.try_read().is_err());
        } else {
            drop(work);
            assert!(release.send(()).is_err());
            assert!(state.peek().search_pager.failed && !state.peek().search_pager.loading);
            assert_eq!(ids(state), ["F1"]);
        }
    }
    let (_dom, mut state) = setup(SearchKind::Messages, false);
    let (visit, candidate) = candidate(state);
    let (release, held) = tokio::sync::oneshot::channel();
    let mut work = Box::pin(recover_with(state, visit, candidate, |_| async move {
        held.await.unwrap();
        Err(Error::Offline("synthetic".into()))
    }));
    pending(work.as_mut()).await;
    state.write().set_search_query("replacement".into());
    let next = super::super::begin(&mut state.write(), "T1", "replacement", false).unwrap();
    let toasts = toast(state);
    release.send(()).unwrap();
    work.await;
    assert!(state.peek().search_pager.accepts(&next));
    assert_eq!(toast(state), toasts);
    assert!(state.peek().search_results.is_empty());
}

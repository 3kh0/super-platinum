use super::*;
use crate::{media::MediaRegistry, state::Overlay};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::poll_fn,
    pin::Pin,
    rc::Rc,
    task::Poll,
};
use super_platinum_core::{config::Session, search::SearchSort, slack::SlackClient};

fn user(id: &str, name: &str) -> User {
    serde_json::from_value(json!({"id":id,"profile":{"display_name":name}})).unwrap()
}
fn file(id: &str, author: &str) -> Value {
    json!({"id":id,"user":author,"title":id,"name":format!("{id}.txt"),"mode":"hosted","mimetype":"text/plain"})
}
fn setup() -> (
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
    shell.core.transport = Some(transport.clone());
    shell.core.active_account = Some("uploader-fixture".into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [("T1".into(), session.clone())].into(),
    });
    shell.overlay = Some(Overlay::Search);
    shell.set_search_query("release".into());
    shell.set_search_kind(SearchKind::Files);
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
fn begin(mut state: Signal<ShellState>, append: bool) -> SearchRequest {
    let query = state.peek().search_query.clone();
    super::super::begin(&mut state.write(), "T1", &query, append).unwrap()
}
fn page(number: u32, items: Value) -> Value {
    json!({"ok":true,"files":{"matches":items,"paging":{"page":number,"pages":12,"total":240}}})
}
fn name(state: Signal<ShellState>, id: &str) -> String {
    state.peek().core.workspaces["T1"].display_name(id)
}
fn toast(state: Signal<ShellState>) -> Option<(String, std::time::Instant)> {
    state
        .peek()
        .toast
        .as_ref()
        .map(|t| (t.text.clone(), t.shown_at))
}
async fn pending<F: Future>(mut work: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(work.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn published_files_stay_usable_while_chunked_names_load_and_omitted_fields_survive() {
    let (_dom, mut state, transport, session) = setup();
    let client = SlackClient::default();
    {
        let mut shell = state.write();
        let workspace = shell.core.workspaces.get_mut("T1").unwrap();
        workspace
            .users
            .insert("U_KNOWN".into(), user("U_KNOWN", "Known teammate"));
        workspace.users.insert("U_THIN".into(),serde_json::from_value(json!({"id":"U_THIN","is_admin":true,"deleted":true,"profile":{"status_text":"Keep status","status_emoji":":wave:"}})).unwrap());
    }
    // Seed five earlier published pages before the real sixth-page append.
    for number in 1..=5 {
        let request = begin(state, number > 1);
        assert!(
            state.write().search_pager.complete(
                &request,
                Some(
                    &serde_json::from_value(
                        json!({"page":number,"page_count":12,"total_count":240})
                    )
                    .unwrap()
                )
            )
        );
    }
    state.write().search_file_results = (0..100)
        .map(|n| {
            serde_json::from_value(file(
                &format!("F_ROW{n}"),
                &if n == 98 {
                    "U_THIN".into()
                } else if n == 99 {
                    "U_KNOWN".into()
                } else {
                    format!("U_LOOKUP{n:03}")
                },
            ))
            .unwrap()
        })
        .collect();
    let incoming = (100..120)
        .map(|n| {
            file(
                &format!("F_ROW{n}"),
                &if n == 100 {
                    "U_LOOKUP000".into()
                } else {
                    format!("U_LOOKUP{:03}", n - 3)
                },
            )
        })
        .collect::<Vec<_>>();
    let request = begin(state, true);
    let payload = page(6, json!(incoming));
    let calls = Rc::new(RefCell::new(Vec::<Vec<String>>::new()));
    let observed = calls.clone();
    let (release, held) = tokio::sync::oneshot::channel();
    let mut hold = Some(held);
    let mut work = Box::pin(super::super::files::load_with_profiles(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        |_| async move { Ok(payload) },
        move |ids: Vec<String>| {
            observed.borrow_mut().push(ids.clone());
            let held = hold.take();
            async move {
                if let Some(held) = held {
                    held.await.unwrap();
                }
                let mut users = ids
                    .iter()
                    .map(|id| user(id, &format!("Name {id}")))
                    .collect::<Vec<_>>();
                users.push(user("U_UNREQUESTED", "Ignore"));
                Ok(users)
            }
        },
    ));
    pending(work.as_mut()).await;
    assert_eq!(state.peek().search_file_results.len(), 120);
    assert_eq!(state.peek().search_pager.page, 6);
    assert!(!state.peek().search_pager.loading && !state.peek().search_pager.failed);
    assert!(super::super::files::Source::capture(&state.peek(), "F_ROW100").is_some());
    assert_eq!(name(state, "U_LOOKUP000"), "U_LOOKUP000");
    assert_eq!(state.peek().core.composer.text, "Keep draft");
    release.send(()).unwrap();
    work.await;
    assert_eq!(
        calls.borrow().iter().map(Vec::len).collect::<Vec<_>>(),
        [100, 18]
    );
    assert!(!calls.borrow().iter().flatten().any(|id| id == "U_KNOWN"));
    assert_eq!(name(state, "U_LOOKUP000"), "Name U_LOOKUP000");
    assert_eq!(name(state, "U_THIN"), "Name U_THIN");
    let shell = state.peek();
    let workspace = &shell.core.workspaces["T1"];
    assert!(workspace.users["U_THIN"].is_admin && workspace.users["U_THIN"].deleted);
    let thin = workspace.users["U_THIN"].profile.as_ref().unwrap();
    assert_eq!(thin.status_text.as_deref(), Some("Keep status"));
    assert_eq!(thin.status_emoji.as_deref(), Some(":wave:"));
    assert!(!workspace.users.contains_key("U_UNREQUESTED"));
}

#[tokio::test]
async fn rejected_pages_never_lookup_and_optional_failure_keeps_rows_until_query_retry() {
    let (_dom, state, transport, session) = setup();
    let client = SlackClient::default();
    let rejected = begin(state, false);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    super::super::files::load_with_profiles(
        state,
        rejected,
        &transport,
        &client,
        &session,
        None,
        |_| async { Ok(page(2, json!([file("F_BAD", "U_NEW")]))) },
        move |_| {
            observed.set(observed.get() + 1);
            std::future::ready(Ok(Vec::new()))
        },
    )
    .await;
    assert_eq!(calls.get(), 0);
    assert!(state.peek().search_pager.failed);
    assert!(state.peek().search_file_results.is_empty());
    let request = begin(state, false);
    let before = toast(state);
    super::super::files::load_with_profiles(
        state,
        request,
        &transport,
        &client,
        &session,
        None,
        |_| async { Ok(page(1, json!([file("F_GOOD", "U_NEW")]))) },
        |_| async { Err(Error::Api("synthetic_profile_failure".into())) },
    )
    .await;
    assert_eq!(state.peek().search_file_results.len(), 1);
    assert_eq!(state.peek().search_pager.page, 1);
    assert!(!state.peek().search_pager.failed && !state.peek().search_pager.loading);
    assert_eq!(toast(state), before);
    assert!(super::super::files::Source::capture(&state.peek(), "F_GOOD").is_some());
    let retry = begin(state, false);
    super::super::files::load_with_profiles(
        state,
        retry,
        &transport,
        &client,
        &session,
        None,
        |_| async { Ok(page(1, json!([file("F_GOOD", "U_NEW")]))) },
        |ids| async move {
            assert_eq!(ids, ["U_NEW"]);
            Ok(vec![user("U_NEW", "Resolved teammate")])
        },
    )
    .await;
    assert_eq!(name(state, "U_NEW"), "Resolved teammate");
    assert_eq!(state.peek().core.composer.text, "Keep draft");
}

#[tokio::test]
async fn retired_metadata_and_intervening_user_updates_cannot_overwrite_current_state() {
    for change in ["account", "page", "realtime"] {
        let (_dom, mut state, transport, session) = setup();
        let client = SlackClient::default();
        state
            .write()
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .users
            .insert(
                "U_THIN".into(),
                serde_json::from_value(json!({"id":"U_THIN","profile":{"status_text":"Before"}}))
                    .unwrap(),
            );
        let request = begin(state, false);
        let (release, held) = tokio::sync::oneshot::channel();
        let mut hold = Some(held);
        let mut work = Box::pin(super::super::files::load_with_profiles(
            state,
            request,
            &transport,
            &client,
            &session,
            None,
            |_| async {
                Ok(page(
                    1,
                    json!([file("F_NEW", "U_NEW"), file("F_THIN", "U_THIN")]),
                ))
            },
            move |_: Vec<String>| {
                let held = hold.take();
                async move {
                    if let Some(held) = held {
                        held.await.unwrap();
                    }
                    Ok(vec![
                        user("U_NEW", "Old lookup"),
                        user("U_THIN", "Old lookup"),
                    ])
                }
            },
        ));
        pending(work.as_mut()).await;
        let before = toast(state);
        let mut replacement = None;
        match change {
            "account" => state.write().core.active_account = Some("other-account".into()),
            "page" => replacement = Some(begin(state, true)),
            "realtime" => {
                let mut shell = state.write();
                let workspace = shell.core.workspaces.get_mut("T1").unwrap();
                workspace.apply_user_update(user("U_NEW", "Realtime teammate"));
                workspace.apply_user_update(serde_json::from_value(json!({"id":"U_THIN","is_admin":true,"profile":{"status_text":"Realtime status"}})).unwrap());
            }
            _ => unreachable!(),
        }
        release.send(()).unwrap();
        work.await;
        assert_eq!(toast(state), before);
        if change == "realtime" {
            assert_eq!(name(state, "U_NEW"), "Realtime teammate");
            assert_eq!(name(state, "U_THIN"), "U_THIN");
            assert_eq!(
                state.peek().core.workspaces["T1"].users["U_THIN"]
                    .profile
                    .as_ref()
                    .unwrap()
                    .status_text
                    .as_deref(),
                Some("Realtime status")
            );
        } else {
            assert_eq!(name(state, "U_NEW"), "U_NEW");
            assert_eq!(name(state, "U_THIN"), "U_THIN");
        }
        if let Some(request) = replacement {
            assert!(state.peek().search_pager.accepts(&request));
        }
        assert_eq!(state.peek().search_file_results.len(), 2);
    }
}

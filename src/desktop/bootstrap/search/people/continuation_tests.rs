use super::*;
use crate::bootstrap::test_support::{harness, pending, users};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::ready,
    rc::Rc,
};
use super_platinum_core::slack::client::{RequestBody, SlackClientConfig};

fn begin(mut state: Signal<ShellState>) -> SearchRequest {
    let query = state.peek().search_query.clone();
    super::super::begin(&mut state.write(), "T1", &query, false).unwrap()
}
fn records(start: usize, count: usize) -> Vec<Value> {
    (start..start + count)
        .map(
            |i| json!({"id":format!("U_MORE_{i}"),"profile":{"display_name":format!("Person{i}")}}),
        )
        .collect()
}

#[tokio::test]
async fn continued_people_batch_uses_markers_and_same_legacy_unique_projection() {
    for oversized in [false, true] {
        let h = harness();
        let state = h.state;
        let before = users(state);
        let calls = Cell::new(0);
        load_continued_with(
            state,
            begin(state),
            &h.transport,
            &SlackClient::default(),
            &h.session,
            None,
            |request| {
                let RequestBody::Json(body) = request.body else {
                    panic!("JSON required")
                };
                assert_eq!(body["count"], 30);
                assert_eq!(body["query"], "alex@example.test");
                let call = calls.get();
                calls.set(call + 1);
                let page = if oversized {
                    assert_eq!(call, 0);
                    assert!(body.get("marker").is_none());
                    let mut rows = vec![
                        json!({"id":"U_REMOTE"}),
                        json!({"id":"U2"}),
                        json!({"id":"U_REMOTE"}),
                        json!({"id":""}),
                    ];
                    rows.extend(records(0, 35));
                    json!({"ok":true,"results":rows,"next_marker":"unused"})
                } else if call == 0 {
                    assert!(body.get("marker").is_none());
                    json!({"ok":true,"results":records(0,20),"next_marker":"next-twenty"})
                } else {
                    assert_eq!(call, 1);
                    assert_eq!(body["marker"], "next-twenty");
                    json!({"ok":true,"results":records(20,15),"next_marker":"unused"})
                };
                ready(Ok(page))
            },
        )
        .await;
        assert_eq!(calls.get(), if oversized { 1 } else { 2 });
        assert_eq!(rows(&state.peek()).len(), 30);
        assert_eq!(
            rows(&state.peek())[0].id,
            if oversized { "U_REMOTE" } else { "U_MORE_0" }
        );
        if oversized {
            assert_eq!(rows(&state.peek())[1].id, "U2");
        }
        assert_eq!(users(state), before);
        assert_eq!(
            (
                state.peek().search_pager.page,
                state.peek().search_pager.page_count,
                state.peek().search_pager.total
            ),
            (1, 1, 0)
        );
        assert!(!state.peek().search_pager.loading && !state.peek().search_pager.has_more());
    }
}

#[tokio::test]
async fn continued_people_fences_each_await_before_successor_error_or_publication() {
    for stage in 0..3 {
        let h = harness();
        let mut state = h.state;
        let before = users(state);
        let calls = Rc::new(Cell::new(0));
        let (send, receive) = tokio::sync::oneshot::channel();
        let held = RefCell::new(Some(receive));
        let observed = calls.clone();
        let client = SlackClient::default();
        let mut work = Box::pin(load_continued_with(
            state,
            begin(state),
            &h.transport,
            &client,
            &h.session,
            None,
            |_| {
                let call = observed.get();
                observed.set(call + 1);
                let wait = if call == if stage == 2 { 1 } else { 0 } {
                    held.borrow_mut().take()
                } else {
                    None
                };
                async move {
                    if let Some(wait) = wait {
                        wait.await.unwrap()
                    } else {
                        Ok(json!({"ok":true,"results":records(0,20),"next_marker":"second"}))
                    }
                }
            },
        ));
        pending(work.as_mut()).await;
        assert_eq!(calls.get(), if stage == 2 { 2 } else { 1 });
        if stage == 0 {
            state.write().set_search_query(" alex@example.test ".into());
        } else if stage == 1 {
            let mut config = SlackClientConfig::default();
            config.xparams.frontend_build_type = "rotated".into();
            state.write().core.client = SlackClient::new(config);
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
                .token = "rotated".into();
        }
        send.send(Err(Error::Api("retired failure".into())))
            .unwrap();
        work.await;
        assert_eq!(calls.get(), if stage == 2 { 2 } else { 1 });
        assert!(rows(&state.peek()).is_empty());
        assert!(!state.peek().search_pager.loading);
        assert_eq!(users(state), before);
    }
}

#[tokio::test]
async fn continued_people_has_no_partial_success_and_cancellation_releases_only_its_slot() {
    let h = harness();
    let state = h.state;
    let calls = Cell::new(0);
    load_continued_with(
        state,
        begin(state),
        &h.transport,
        &SlackClient::default(),
        &h.session,
        None,
        |_| {
            let call = calls.get();
            calls.set(call + 1);
            ready(if call == 0 {
                Ok(json!({"ok":true,"results":records(0,20),"next_marker":"second"}))
            } else {
                Err(Error::RateLimited {
                    retry_after_secs: Some(17),
                })
            })
        },
    )
    .await;
    assert_eq!(calls.get(), 2);
    assert!(rows(&state.peek()).is_empty());
    assert!(state.peek().search_pager.failed && !state.peek().search_pager.loading);
    for successor in [false, true] {
        let h = harness();
        let mut state = h.state;
        let client = SlackClient::default();
        let mut work = Box::pin(load_continued_with(
            state,
            begin(state),
            &h.transport,
            &client,
            &h.session,
            None,
            |_| std::future::pending::<Result<Value, Error>>(),
        ));
        pending(work.as_mut()).await;
        assert!(state.peek().search_pager.loading);
        let next = if successor {
            state.write().set_search_query("successor".into());
            Some(begin(state))
        } else {
            None
        };
        drop(work);
        if let Some(next) = next {
            assert!(state.peek().search_pager.accepts(&next));
            assert!(state.peek().search_pager.loading);
            state.write().search_pager.fail(&next);
        } else {
            assert!(!state.peek().search_pager.loading && state.peek().search_pager.failed);
        }
    }
}

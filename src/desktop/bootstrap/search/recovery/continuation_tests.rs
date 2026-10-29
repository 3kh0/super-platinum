use super::*;
use crate::bootstrap::{
    reconnect::Visit,
    test_support::{harness, pending, users},
};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use super_platinum_core::slack::client::RequestBody;

#[tokio::test]
async fn restored_people_continuation_retains_scope_and_one_batch_without_membership_or_history_writes()
 {
    for retired in [false, true] {
        let h = harness();
        let mut state = h.state;
        let before = users(state);
        let query = state.peek().search_query.clone();
        let first = super::super::begin(&mut state.write(), "T1", &query, false).unwrap();
        state.write().search_pager.fail(&first);
        let visit = Visit::begin(&mut state.write());
        let candidate = Candidate::capture(&state.peek()).unwrap();
        let history =
            serde_json::to_value(&state.peek().core.workspaces["T1"].recent_searches).unwrap();
        let calls = Cell::new(0);
        let (send, receive) = tokio::sync::oneshot::channel();
        let held = RefCell::new(Some(receive));
        let mut work = Box::pin(recover_continued_with_profiles(
            state,
            visit,
            candidate,
            |request| {
                let RequestBody::Json(body) = request.body else {
                    panic!("JSON required")
                };
                assert_eq!(body["count"], 30);
                assert_eq!(body["query"], query);
                let call = calls.get();
                calls.set(call + 1);
                if call == 0 {
                    assert!(body.get("marker").is_none());
                } else {
                    assert_eq!(call, 1);
                    assert_eq!(body["marker"], "recovery-next");
                }
                let wait = if retired && call == 0 {
                    held.borrow_mut().take()
                } else {
                    None
                };
                async move {
                    if let Some(wait) = wait {
                        wait.await.unwrap()
                    } else {
                        let start = if call == 0 { 0 } else { 20 };
                        let count = if call == 0 { 20 } else { 10 };
                        Ok(
                            json!({"ok":true,"results":(start..start+count).map(|i|json!({"id":format!("U_RECOVER_{i}")})).collect::<Vec<_>>(),
                        "next_marker":if call==0{Some("recovery-next")}else{None}}),
                        )
                    }
                }
            },
            |_| std::future::ready(Ok(Vec::new())),
        ));
        if retired {
            pending(work.as_mut()).await;
            assert_eq!(calls.get(), 1);
            state.write().overlay = None;
            send.send(Ok::<_, Error>(
                json!({"ok":true,"results":[{"id":"U_STALE"}],"next_marker":"do-not-request"}),
            ))
            .unwrap();
        }
        work.await;
        assert!(!state.peek().search_pager.loading);
        assert_eq!(users(state), before);
        assert_eq!(
            serde_json::to_value(&state.peek().core.workspaces["T1"].recent_searches).unwrap(),
            history
        );
        assert_eq!(state.peek().core.composer.text, "Keep draft");
        if retired {
            assert_eq!(calls.get(), 1);
            assert!(super::super::people::rows(&state.peek()).is_empty());
        } else {
            assert_eq!(calls.get(), 2);
            assert_eq!(super::super::people::rows(&state.peek()).len(), 30);
            assert_eq!(
                (
                    state.peek().search_pager.page,
                    state.peek().search_pager.page_count,
                    state.peek().search_pager.total
                ),
                (1, 1, 0)
            );
            assert!(!state.peek().search_pager.failed && !state.peek().search_pager.has_more());
        }
    }
}

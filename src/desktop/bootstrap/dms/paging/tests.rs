use super::*;
use crate::{
    bootstrap::test_support::{harness, pending},
    state::MainView,
};
use serde_json::json;
use std::{cell::Cell, rc::Rc};
fn setup(mut state: Signal<ShellState>) -> Arc<Frame> {
    let mut s = state.write();
    s.main_view = MainView::Dms;
    s.overlay = None;
    s.set_dm_query("Older Teammate".into());
    s.core.dms.entries = serde_json::from_value(
        json!([{"id":"D_KEEP","latest":"1.1","channel":{"id":"D_KEEP","is_im":true,"user":"U2"}}]),
    )
    .unwrap();
    s.core.dms.loaded = true;
    s.core.dms.next_cursor = Some("opaque older+".into());
    assert!(!s.core.dms.entries.iter().any(|e| s.dm_row_visible(e)));
    Frame::capture(&s).unwrap()
}
fn page(id: &str, cursor: &str) -> serde_json::Value {
    json!({"ok":true,"dms":[{"id":id,"latest":"9.1","channel":{"id":id,"is_im":true,"user":"U2"}}],"response_metadata":{"next_cursor":cursor}})
}
fn source(state: Signal<ShellState>, frame: &Arc<Frame>) -> Source {
    Source::capture(&state.peek(), frame).unwrap()
}
#[tokio::test]
async fn zero_match_paging_action_uses_original_cursor_and_keeps_rows_and_draft_through_failure_and_retry()
 {
    let h = harness();
    let state = h.state;
    let frame = setup(state);
    let before = state.peek().core.active_channel.clone();
    let failed = load_if_with(
        state,
        source(state, &frame),
        async { true },
        |request| async move {
            assert!(request.url.contains("client.dms"));
            let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
                panic!("form")
            };
            let fields = fields
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            assert_eq!(fields["cursor"], "opaque older+");
            assert_eq!(fields["include_closed"], "true");
            Err(Error::RateLimited {
                retry_after_secs: Some(17),
            })
        },
    )
    .await;
    assert_eq!(failed, Outcome::Failed);
    {
        let s = state.peek();
        assert_eq!(s.core.dms.entries[0].id, "D_KEEP");
        assert_eq!(s.core.dms.next_cursor.as_deref(), Some("opaque older+"));
        assert!(!s.core.dms.loading);
        assert_eq!(s.core.composer.text, "Keep draft");
        assert_eq!(s.core.active_channel, before);
    }
    assert_eq!(
        load_if_with(
            state,
            source(state, &frame),
            async { true },
            |request| async move {
                let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body
                else {
                    panic!("form")
                };
                assert!(
                    fields
                        .iter()
                        .any(|(k, v)| k == "cursor" && v == "opaque older+")
                );
                Ok(page("D_OLDER", "next"))
            }
        )
        .await,
        Outcome::Published
    );
    let s = state.peek();
    assert_eq!(s.core.dms.entries.len(), 2);
    assert!(s.core.dms.entries.iter().any(|e| e.id == "D_KEEP"));
    assert_eq!(s.core.dms.next_cursor.as_deref(), Some("next"));
    assert_eq!(s.core.active_channel, before);
    assert_eq!(s.core.composer.text, "Keep draft");
}
#[tokio::test]
async fn held_scroll_handoff_cannot_reacquire_a_replacement_owner_or_same_cursor_refresh() {
    for replace_cookie in [false, true] {
        let h = harness();
        let mut state = h.state;
        let frame = setup(state);
        let source = source(state, &frame);
        let (release, held) = tokio::sync::oneshot::channel();
        let calls = Rc::new(Cell::new(0));
        let called = calls.clone();
        let mut work = Box::pin(load_if_with(
            state,
            source,
            async move {
                held.await.unwrap();
                true
            },
            move |_| {
                called.set(called.get() + 1);
                async { Ok(page("D_WRONG", "wrong")) }
            },
        ));
        pending(work.as_mut()).await;
        if replace_cookie {
            state.write().core.session.as_mut().unwrap().d_cookie = "replacement".into();
        } else {
            assert_eq!(
                super::super::feed::load_with(state, frame.owner(), false, None, |_| async {
                    Ok(page("D_REFRESH", "opaque older+"))
                })
                .await,
                Outcome::Published
            );
        }
        release.send(()).unwrap();
        assert_eq!(work.await, Outcome::Retired);
        assert_eq!(calls.get(), 0);
        assert!(
            !state
                .peek()
                .core
                .dms
                .entries
                .iter()
                .any(|e| e.id == "D_WRONG")
        );
        assert!(!state.peek().core.dms.loading);
    }
}

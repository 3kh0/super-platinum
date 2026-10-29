use super::*;
use crate::bootstrap::test_support::{harness, pending, transport};
use serde_json::json;
fn page(id: &str, next: &str) -> serde_json::Value {
    json!({"ok":true,"dms":[{"id":id,"latest":"9.1","channel":{"id":id,"is_im":true,"is_open":false,"user":"U2"}}],"response_metadata":{"next_cursor":next}})
}
fn setup(mut state: Signal<ShellState>) {
    let mut s = state.write();
    s.main_view = MainView::Dms;
    s.overlay = None;
    s.core.dms.entries = serde_json::from_value(json!([{"id":"D_KEEP","latest":"1.1"}])).unwrap();
    s.core.dms.next_cursor = Some("older".into());
    s.core.dms.loaded = true;
}
#[tokio::test]
async fn actual_append_keeps_existing_rows_and_cursor_and_does_not_promote_sparse_feed_channels() {
    let h = harness();
    let state = h.state;
    setup(state);
    let owner = Owner::capture(&state.peek()).unwrap();
    assert_eq!(
        load_with(state, owner, true, None, |request| async move {
            assert!(request.url.contains("client.dms"));
            let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
                panic!("form")
            };
            let f = fields
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            assert_eq!(f["cursor"], "older");
            assert_eq!(f["include_closed"], "true");
            Ok(page("D_COLD", "more"))
        })
        .await,
        Outcome::Published
    );
    let s = state.peek();
    assert_eq!(
        s.core
            .dms
            .entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["D_COLD", "D_KEEP"]
    );
    assert_eq!(s.core.dms.next_cursor.as_deref(), Some("more"));
    assert!(!s.core.dms.loading);
    assert!(!s.core.workspaces["T1"].channels.contains_key("D_COLD"));
    assert!(!s.channels.iter().any(|c| c.id == "D_COLD"));
}
#[tokio::test]
async fn failed_append_preserves_the_same_continuation_and_existing_entries_for_retry() {
    let h = harness();
    let state = h.state;
    setup(state);
    let owner = Owner::capture(&state.peek()).unwrap();
    assert_eq!(
        load_with(state, owner.clone(), true, None, |_| async {
            Err(Error::RateLimited {
                retry_after_secs: Some(17),
            })
        })
        .await,
        Outcome::Failed
    );
    assert_eq!(state.peek().core.dms.next_cursor.as_deref(), Some("older"));
    assert_eq!(state.peek().core.dms.entries[0].id, "D_KEEP");
    assert!(!state.peek().core.dms.loading);
    assert_eq!(
        load_with(state, owner, true, None, |_| async {
            Ok(page("D_RETRIED", ""))
        })
        .await,
        Outcome::Published
    );
    assert!(
        state
            .peek()
            .core
            .dms
            .entries
            .iter()
            .any(|e| e.id == "D_RETRIED")
    );
    assert!(state.peek().core.dms.next_cursor.is_none());
}
#[tokio::test]
async fn superseding_failed_first_refresh_cannot_lose_the_reserved_older_cursor() {
    let h = harness();
    let state = h.state;
    setup(state);
    let owner = Owner::capture(&state.peek()).unwrap();
    let (release, held) = tokio::sync::oneshot::channel();
    let mut older = Box::pin(load_with(
        state,
        owner.clone(),
        true,
        None,
        |_| async move {
            held.await.unwrap();
            Ok(page("D_RETIRED", "wrong"))
        },
    ));
    pending(older.as_mut()).await;
    assert_eq!(state.peek().core.dms.next_cursor.as_deref(), Some("older"));
    assert_eq!(
        load_with(state, owner, false, None, |_| async {
            Err(Error::Offline("synthetic".into()))
        })
        .await,
        Outcome::Failed
    );
    release.send(()).unwrap();
    assert_eq!(older.await, Outcome::Retired);
    assert_eq!(state.peek().core.dms.next_cursor.as_deref(), Some("older"));
    assert_eq!(state.peek().core.dms.entries[0].id, "D_KEEP");
    assert!(!state.peek().core.dms.loading);
}
#[tokio::test]
async fn held_feed_response_with_colliding_load_sequence_cannot_publish_into_replacement_account() {
    let h = harness();
    let mut state = h.state;
    setup(state);
    let owner = Owner::capture(&state.peek()).unwrap();
    let (release, held) = tokio::sync::oneshot::channel();
    let mut old = Box::pin(load_with(state, owner, true, None, |_| async move {
        held.await.unwrap();
        Ok(page("D_ACCOUNT_A", "wrong"))
    }));
    pending(old.as_mut()).await;
    let sequence = state.peek().core.dms.load_seq;
    {
        let mut s = state.write();
        s.core.active_account = Some("account-b".into());
        s.core.account_epoch += 1;
        s.core.session.as_mut().unwrap().d_cookie = "b-cookie".into();
        s.core.transport = Some(transport("b-cookie"));
        s.core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap()
            .token = "b-token".into();
        s.core.dms = Default::default();
        s.core.dms.next_cursor = Some("b-older".into());
    }
    let owner = Owner::capture(&state.peek()).unwrap();
    assert_eq!(
        load_with(state, owner, true, None, |_| async {
            Ok(page("D_ACCOUNT_B", "b-next"))
        })
        .await,
        Outcome::Published
    );
    assert_eq!(state.peek().core.dms.load_seq, sequence);
    release.send(()).unwrap();
    assert_eq!(old.await, Outcome::Retired);
    assert_eq!(state.peek().core.dms.entries[0].id, "D_ACCOUNT_B");
    assert_eq!(state.peek().core.dms.next_cursor.as_deref(), Some("b-next"));
    assert!(!state.peek().core.dms.loading);
}

#[tokio::test]
async fn feed_success_or_failure_after_surface_departure_cannot_publish_or_toast_on_return() {
    for failed in [false, true] {
        let h = harness();
        let mut state = h.state;
        setup(state);
        let owner = Owner::capture(&state.peek()).unwrap();
        let toast = state.peek().toast.as_ref().map(|t| t.text.clone());
        let (release, held) = tokio::sync::oneshot::channel();
        let mut work = Box::pin(load_with(state, owner, true, None, |_| async move {
            held.await.unwrap();
            if failed {
                Err(Error::Offline("synthetic".into()))
            } else {
                Ok(page("D_RETIRED", "wrong"))
            }
        }));
        pending(work.as_mut()).await;
        {
            let mut s = state.write();
            s.close_thread();
            s.main_view = MainView::Home;
            s.main_view = MainView::Dms;
        }
        release.send(()).unwrap();
        assert_eq!(work.await, Outcome::Retired);
        let s = state.peek();
        assert_eq!(s.core.dms.entries[0].id, "D_KEEP");
        assert_eq!(s.core.dms.next_cursor.as_deref(), Some("older"));
        assert_eq!(s.toast.as_ref().map(|t| t.text.clone()), toast);
        assert!(!s.core.dms.loading);
    }
}

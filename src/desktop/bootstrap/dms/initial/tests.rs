use super::*;
use crate::{
    bootstrap::test_support::{harness, pending, transport},
    state::MainView,
};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
fn setup(mut state: Signal<ShellState>) {
    let mut s = state.write();
    s.main_view = MainView::Dms;
    s.overlay = None;
    s.core.dms.loaded = false;
    s.core.dms.loading = false;
    s.core.dms.next_cursor = None;

    s.core.dms.entries = serde_json::from_value(
        json!([{"id":"D_KEEP","latest":"1.1","channel":{"id":"D_KEEP","is_im":true,"user":"U_RETAINED_INITIAL"}}]),
    )
    .unwrap();
}
fn source(state: Signal<ShellState>) -> Source {
    Source::capture(&state.peek(), &Frame::capture(&state.peek()).unwrap()).unwrap()
}
fn first_page() -> serde_json::Value {
    json!({"ok":true,"dms":[{"id":"D_FIRST","latest":"9.1","channel":{"id":"D_FIRST","is_im":true,"user":"U_FIRST","is_open":false}}],"response_metadata":{"next_cursor":"older"}})
}
fn user(id: &str, name: &str) -> User {
    serde_json::from_value(json!({"id":id,"profile":{"display_name":name}})).unwrap()
}
#[tokio::test]
async fn initial_dm_action_preserves_failed_data_then_explicitly_retries_first_page_and_resolves_cold_names()
 {
    let h = harness();
    let state = h.state;
    setup(state);
    let old = source(state);
    let before_rows = serde_json::to_value(&state.peek().core.dms.entries).unwrap();
    let before_channel = state.peek().core.active_channel.clone();
    assert_eq!(
        load_with(
            state,
            old.clone(),
            |request| async move {
                assert!(request.url.contains("client.dms"));
                let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body
                else {
                    panic!("form")
                };
                assert!(fields.iter().any(|(k, v)| k == "count" && v == "100"));
                assert!(!fields.iter().any(|(k, _)| k == "cursor"));
                Err(Error::RateLimited {
                    retry_after_secs: Some(17),
                })
            },
            |_| async {
                panic!("failed first-page read must not hydrate even an unresolved retained peer")
            }
        )
        .await,
        Outcome::Failed
    );
    {
        let s = state.peek();
        assert_eq!(
            serde_json::to_value(&s.core.dms.entries).unwrap(),
            before_rows
        );
        assert!(!s.core.dms.loaded && !s.core.dms.loading);
        assert!(s.core.dms.next_cursor.is_none());
        assert_eq!(s.core.active_channel, before_channel);
        assert_eq!(s.core.composer.text, "Keep draft");
    }
    assert_eq!(
        load_with(
            state,
            old.clone(),
            |_| async { panic!("old failed-attempt closure must retire") },
            |_| async { panic!("retired lookup") }
        )
        .await,
        Outcome::Retired
    );
    assert_eq!(
        load_with(
            state,
            source(state),
            |request| async move {
                let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body
                else {
                    panic!("form")
                };
                assert!(!fields.iter().any(|(k, _)| k == "cursor"));
                Ok(first_page())
            },
            |ids| async move {
                assert_eq!(ids, ["U_FIRST"]);
                assert!(state.peek().core.dms.loaded);
                assert!(!state.peek().core.dms.loading);
                assert_eq!(state.peek().core.dms.entries[0].id, "D_FIRST");
                Ok(vec![user("U_FIRST", "First Teammate")])
            }
        )
        .await,
        Outcome::Published
    );
    {
        let s = state.peek();
        assert_eq!(s.core.dms.entries.len(), 1);
        assert!(!s.core.dms.entries.iter().any(|e| e.id == "D_KEEP"));
        assert_eq!(s.core.dms.next_cursor.as_deref(), Some("older"));
        assert_eq!(s.dm_row_label(&s.core.dms.entries[0]), "First Teammate");
        assert_eq!(s.core.active_channel, before_channel);
        assert_eq!(s.core.composer.text, "Keep draft");
    }
    assert!(Source::capture(&state.peek(), &Frame::capture(&state.peek()).unwrap()).is_none());
    assert_eq!(
        load_with(
            state,
            old,
            |_| async { panic!("completed initial phase cannot replay") },
            |_| async { panic!("completed lookup") }
        )
        .await,
        Outcome::Retired
    );
    // Optional profile failure never retracts an accepted usable page.
    let other = harness();
    let other_state = other.state;
    setup(other_state);
    let other_channel = other_state.peek().core.active_channel.clone();
    assert_eq!(
        load_with(
            other_state,
            source(other_state),
            |_| async { Ok(first_page()) },
            |_| async {
                Err(Error::Transport(
                    "https://fixture.invalid/?token=HIDDEN".into(),
                ))
            }
        )
        .await,
        Outcome::Published
    );
    let shell = other_state.peek();
    assert!(shell.core.dms.loaded && !shell.core.dms.loading);
    assert_eq!(shell.core.dms.entries[0].id, "D_FIRST");
    assert_eq!(shell.core.dms.next_cursor.as_deref(), Some("older"));
    assert!(!shell.core.workspaces["T1"].users.contains_key("U_FIRST"));
    assert_eq!(shell.core.active_channel, other_channel);
    assert_eq!(shell.core.composer.text, "Keep draft");
    assert!(shell.toast.is_none());
}
#[tokio::test]
async fn optional_dm_names_cannot_publish_after_same_account_token_or_account_replacement_with_colliding_feed_sequence()
 {
    for account_replaced in [false, true] {
        let h = harness();
        let mut state = h.state;
        setup(state);
        let initial = source(state);
        let (release, held) = tokio::sync::oneshot::channel();
        let mut held = Some(held);
        let mut work = Box::pin(load_with(
            state,
            initial,
            |_| async { Ok(first_page()) },
            |ids| {
                let held = held.take().expect("one optional lookup batch");
                async move {
                    assert_eq!(ids, ["U_FIRST"]);
                    held.await.unwrap();
                    Ok(vec![user("U_FIRST", "Retired name")])
                }
            },
        ));
        pending(work.as_mut()).await;
        assert!(state.peek().core.dms.loaded);
        assert!(!state.peek().core.dms.loading);
        let sequence = state.peek().core.dms.load_seq;
        assert!(
            !state.peek().core.workspaces["T1"]
                .users
                .contains_key("U_FIRST")
        );
        {
            let mut s = state.write();
            s.core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token = "replacement-token".into();
            if account_replaced {
                s.core.active_account = Some("replacement-account".into());
                s.core.account_epoch += 1;
                s.core.session.as_mut().unwrap().d_cookie = "replacement-cookie".into();
                s.core.transport = Some(transport("replacement-cookie"));
            }
            // Keep the same feed IDs, sequence and absent user snapshot. Only
            // original session/account ownership may reject this old profile.
            assert_eq!(s.core.dms.load_seq, sequence);
        }
        release.send(()).unwrap();
        assert_eq!(work.await, Outcome::Published);
        assert!(
            !state.peek().core.workspaces["T1"]
                .users
                .contains_key("U_FIRST")
        );
    }
}
#[tokio::test]
async fn initial_dm_peer_and_preview_author_names_are_deduplicated_and_batched_at_one_hundred() {
    let h = harness();
    let state = h.state;
    setup(state);
    let mut rows = (0..55).map(|i|json!({"id":format!("D_{i}"),"latest":"9.1","channel":{"id":format!("D_{i}"),"is_im":true,"user":format!("U_PEER_{i}")},"message":{"user":format!("U_AUTHOR_{i}"),"text":"Preview","ts":"9.1"}})).collect::<Vec<_>>();
    rows.push(json!({"id":"D_DUP","latest":"9.0","channel":{"id":"D_DUP","is_im":true,"user":"U_PEER_0"},"message":{"user":"U_AUTHOR_0","text":"Repeated author","ts":"9.0"}}));
    rows.push(json!({"id":"D_MISMATCH","latest":"8.0","channel":{"id":"D_OTHER","is_im":true,"user":"U_UNTRUSTED"}}));
    let chunks = Rc::new(RefCell::new(Vec::new()));
    let recorded = chunks.clone();
    assert_eq!(load_with(state,source(state),|_|async move{Ok(json!({"ok":true,"dms":rows,"response_metadata":{"next_cursor":""}}))},move|ids|{recorded.borrow_mut().push(ids.len());async move{assert!(ids.len()<=100);Ok(ids.into_iter().map(|id|user(&id,&format!("Resolved {id}"))).collect())}}).await,Outcome::Published);
    assert_eq!(*chunks.borrow(), [100, 10]);
    let s = state.peek();
    assert_eq!(s.core.dms.entries.len(), 57);
    assert!(!s.core.workspaces["T1"].users.contains_key("U_UNTRUSTED"));
    assert!(s.core.dms.loaded && !s.core.dms.loading);
    assert!(s.core.dms.next_cursor.is_none());
    assert_eq!(s.core.composer.text, "Keep draft");
    for i in 0..55 {
        assert_eq!(
            s.core.workspaces["T1"].display_name(&format!("U_PEER_{i}")),
            format!("Resolved U_PEER_{i}")
        );
        assert_eq!(
            s.core.workspaces["T1"].display_name(&format!("U_AUTHOR_{i}")),
            format!("Resolved U_AUTHOR_{i}")
        );
    }
}

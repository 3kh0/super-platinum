use super::*;
use crate::{
    bootstrap::test_support::pending,
    slack_lists::entities::directory::test_support::{
        check_request, harness, members, replace_client, row,
    },
};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    future::ready,
};

struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|registry| {
            registry.entries.remove(&self.0);
        });
    }
}
fn shown(state: Signal<ShellState>) -> Presentation {
    presentation(&state.peek()).unwrap()
}

#[tokio::test]
async fn creation_directory_uses_later_batches_without_changing_defaults_or_canonical_members() {
    for channel in [false, true] {
        let h = harness(channel, true);
        let state = h.state;
        let _cleanup = Cleanup(Key::current(&state.peek()).unwrap());
        let stamp = Stamp::capture(&state.peek()).unwrap();
        let before = shown(state);
        let request = begin(&state.peek(), stamp, "late".into()).unwrap();
        let canonical = members(state);
        let calls = Cell::new(0);
        complete_with(state,request,&h.context,|request| {
            let call=calls.get();check_request(request,channel,call);calls.set(call+1);
            ready(Ok(if call==0 {json!({"ok":true,"results":vec![row(channel,10000001,true);15],"next_marker":"next"})}
                else {json!({"ok":true,"results":vec![row(channel,20000001,false);15],"next_marker":{"unused":true}})}))
        }).await;
        assert_eq!(calls.get(), 2);
        let after = shown(state);
        assert!(!after.directory.loading);
        assert!(after.directory.error.is_none());
        assert_eq!(
            after
                .directory
                .candidates
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            vec![if channel { "C20000001" } else { "U20000001" }]
        );
        assert_eq!(after.options, before.options);
        assert_eq!(after.selected, before.selected);
        assert_eq!(members(state), canonical);
    }
}

#[tokio::test]
async fn creation_directory_failure_and_client_retirement_preserve_defaults_cache_and_successor_spinner()
 {
    for channel in [false, true] {
        for scenario in 0..4 {
            let h = harness(channel, true);
            let state = h.state;
            let _cleanup = Cleanup(Key::current(&state.peek()).unwrap());
            let stamp = Stamp::capture(&state.peek()).unwrap();
            let request = begin(&state.peek(), stamp, "".into()).unwrap();
            let before = shown(state);
            assert!(before.directory.loading);
            assert!(!before.directory.candidates.is_empty());
            let canonical = members(state);
            let calls = Cell::new(0);
            let (send, receive) = tokio::sync::oneshot::channel();
            let held = RefCell::new(Some(receive));
            let mut work = Box::pin(complete_with(state, request, &h.context, |request| {
                let call = calls.get();
                check_request(request, channel, call);
                calls.set(call + 1);
                let wait = if scenario > 0 && call == if scenario == 3 { 1 } else { 0 } {
                    held.borrow_mut().take()
                } else {
                    None
                };
                async move {
                    if let Some(wait) = wait {
                        wait.await.unwrap()
                    } else if call == 0 {
                        Ok(
                            json!({"ok":true,"results":[row(channel,20000001,false)],"next_marker":"next"}),
                        )
                    } else {
                        Err(Error::RateLimited {
                            retry_after_secs: Some(60),
                        })
                    }
                }
            }));
            if scenario == 0 {
                work.await;
                let after = shown(state);
                assert!(!after.directory.loading);
                assert!(after.directory.error.is_some());
                assert!(after.directory.candidates == before.directory.candidates);
                assert!(begin(&state.peek(), stamp, "another".into()).is_none());
                assert!(shown(state).directory.error.unwrap().contains("wait"));
            } else {
                pending(work.as_mut()).await;
                assert_eq!(calls.get(), if scenario == 3 { 2 } else { 1 });
                let successor = if scenario == 2 {
                    begin(&state.peek(), stamp, "next owner".into()).unwrap();
                    Some(shown(state).directory)
                } else {
                    replace_client(state);
                    None
                };
                send.send(if scenario == 3 {
                    Ok(json!({"ok":true,"results":[row(channel,30000001,false)]}))
                } else {
                    Err(Error::RateLimited {
                        retry_after_secs: Some(60),
                    })
                })
                .unwrap();
                work.await;
                assert_eq!(calls.get(), if scenario == 3 { 2 } else { 1 });
                let after = shown(state);
                if let Some(successor) = successor {
                    assert!(after.directory == successor);
                    assert!(after.directory.loading);
                } else {
                    assert!(!after.directory.loading);
                    assert!(after.directory.error.is_none());
                    assert!(after.directory.candidates == before.directory.candidates);
                }
            }
            let after = shown(state);
            assert_eq!(after.options, before.options);
            assert_eq!(after.selected, before.selected);
            assert_eq!(members(state), canonical);
        }
    }
}

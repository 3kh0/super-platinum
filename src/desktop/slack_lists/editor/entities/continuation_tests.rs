use super::*;
use crate::bootstrap::test_support::pending;
use directory::test_support::{check_request, harness, members, replace_client, row};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    future::ready,
};

fn open(mut state: Signal<ShellState>, channel: bool) -> u64 {
    let mut shell = state.write();
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    super::super::begin_shell(
        &mut shell,
        "R_LIST_000",
        if channel {
            "C_EDIT_CHANNELS"
        } else {
            "C_EDIT_USERS"
        },
        visit,
    )
    .unwrap();
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .generation
}
fn shown(state: Signal<ShellState>) -> directory::Presentation {
    super::super::presentation(&state.peek()).unwrap().directory
}

#[tokio::test]
async fn cell_directory_finds_later_matches_but_stops_at_raw_count_without_selection_or_membership_writes()
 {
    for channel in [false, true] {
        let h = harness(channel, false);
        let mut state = h.state;
        let generation = open(state, channel);
        let selected = super::super::presentation(&state.peek()).unwrap().original;
        let request = begin(&mut state.write(), generation, "late".into()).unwrap();
        let before = members(state);
        let calls = Cell::new(0);
        let ingress = Cell::new(0);
        complete_with(state,request,&h.context,|request| {
            let call=calls.get();check_request(request,channel,call);calls.set(call+1);
            ready(Ok(if call==0 {json!({"ok":true,"results":vec![row(channel,10000001,true);15],"next_marker":"next"})}
                else {json!({"ok":true,"results":vec![row(channel,20000001,false);15],"next_marker":{"unused":true}})}))
        },|_,channels| {ingress.set(ingress.get()+1);assert_eq!(channels.len(),30);}).await;
        assert_eq!(calls.get(), 2);
        assert_eq!(ingress.get(), usize::from(channel));
        let after = shown(state);
        assert!(!after.loading);
        assert!(after.error.is_none());
        assert_eq!(
            after
                .candidates
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            vec![if channel { "C20000001" } else { "U20000001" }]
        );
        assert_eq!(members(state), before);
        assert!(super::super::presentation(&state.peek()).unwrap().original == selected);
    }
}

#[tokio::test]
async fn cell_directory_late_failure_preserves_cache_and_retirement_releases_only_original_loading()
{
    for channel in [false, true] {
        for scenario in 0..4 {
            let h = harness(channel, false);
            let mut state = h.state;
            let generation = open(state, channel);
            let request = begin(&mut state.write(), generation, "".into()).unwrap();
            let before = shown(state);
            assert!(before.loading);
            assert!(!before.candidates.is_empty());
            let canonical = members(state);
            let calls = Cell::new(0);
            let ingress = Cell::new(0);
            let (send, receive) = tokio::sync::oneshot::channel();
            let held = RefCell::new(Some(receive));
            let mut work = Box::pin(complete_with(
                state,
                request,
                &h.context,
                |request| {
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
                },
                |_, _| ingress.set(ingress.get() + 1),
            ));
            if scenario == 0 {
                work.await;
                let after = shown(state);
                assert!(!after.loading);
                assert!(after.error.is_some());
                assert!(after.candidates == before.candidates);
                assert!(begin(&mut state.write(), generation, "another".into()).is_none());
                assert!(shown(state).error.unwrap().contains("wait"));
            } else {
                pending(work.as_mut()).await;
                assert_eq!(calls.get(), if scenario == 3 { 2 } else { 1 });
                let successor = if scenario == 2 {
                    begin(&mut state.write(), generation, "next owner".into()).unwrap();
                    Some(shown(state))
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
                    assert!(after == successor);
                    assert!(after.loading);
                } else {
                    assert!(!after.loading);
                    assert!(after.error.is_none());
                    assert!(after.candidates == before.candidates);
                }
            }
            assert_eq!(ingress.get(), 0);
            assert_eq!(members(state), canonical);
        }
    }
}

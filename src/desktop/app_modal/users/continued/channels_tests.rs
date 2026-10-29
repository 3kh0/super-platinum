use super::*;
use crate::bootstrap::test_support::pending;
use directory::read_scope::test_support as support;
use std::cell::{Cell, RefCell};

fn setup(
    multiple: bool,
    cached_query: bool,
    conversation: bool,
) -> (
    crate::bootstrap::test_support::Harness,
    Ticket,
    Arc<Context>,
    u64,
    u64,
    String,
) {
    setup_with_mask(multiple, cached_query, conversation, None)
}
fn setup_with_mask(
    multiple: bool,
    cached_query: bool,
    conversation: bool,
    mask: Option<Value>,
) -> (
    crate::bootstrap::test_support::Harness,
    Ticket,
    Arc<Context>,
    u64,
    u64,
    String,
) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    if multiple {
        crate::app_modal::multi_channel_fixture::install(&mut s);
    } else {
        crate::app_modal::channel_fixture::install(&mut s);
    }
    if conversation {
        let mut view = if multiple {
            crate::app_modal::multi_channel_fixture::view()
        } else {
            crate::app_modal::channel_fixture::view()
        };
        let element = &mut view["blocks"][1]["element"];
        element["type"] = serde_json::json!(if multiple {
            "multi_conversations_select"
        } else {
            "conversations_select"
        });
        let value = element
            .as_object_mut()
            .unwrap()
            .remove(if multiple {
                "initial_channels"
            } else {
                "initial_channel"
            })
            .unwrap();
        element[if multiple {
            "initial_conversations"
        } else {
            "initial_conversation"
        }] = value;
        element["filter"] = mask.unwrap_or_else(|| serde_json::json!({"include":["public"]}));
        s.app_modal = Some(Session::fixture_with_origin(
            view,
            Some(crate::app_modal::conversation_fixture::ORIGIN.into()),
        ));
    }
    support::install_context(&mut s);
    let key = InputKey {
        block_id: "reviewers".into(),
        action_id: "value".into(),
    };
    let ticket = Ticket::capture(&s, &key).unwrap();
    let context = ticket.context.clone().unwrap();
    assert!(open_local(&mut s, ticket.clone()));
    let query = if cached_query {
        cached(&s, &ticket, "")[0].label.clone()
    } else {
        "later".into()
    };
    let choices = cached(&s, &ticket, &query);
    let p = s.app_modal.as_mut().unwrap().users.as_mut().unwrap();
    p.query = query.clone();
    p.query_revision = next_id();
    p.loading = true;
    p.candidates = choices;
    p.error = None;
    let (serial, revision) = (p.serial, p.query_revision);
    (support::mount(s), ticket, context, serial, revision, query)
}
fn values(
    state: Signal<ShellState>,
) -> Vec<(
    InputKey,
    super_platinum_core::block_actions::form::InputValue,
)> {
    state
        .peek()
        .app_modal
        .as_ref()
        .unwrap()
        .current_form()
        .unwrap()
        .fields
        .iter()
        .map(|(k, f)| (k.clone(), f.value.clone()))
        .collect()
}

#[tokio::test]
async fn ordinary_modal_channels_route_complete_raw24_and_ingress_only_terminal_success() {
    for multiple in [false, true] {
        let (negative, ticket, _, _, _, _) = setup(multiple, false, true);
        assert!(directory::public_channel_filter_matches(
            ticket.filter.as_ref().unwrap()
        ));
        assert!(!ordinary_channel_query(&ticket));
        drop(negative);
        for case in 0..6 {
            let (h, ticket, context, serial, revision, query) =
                setup(multiple, (2..5).contains(&case), false);
            assert!(ordinary_channel_query(&ticket));
            let mut state = h.state;
            let filter = ticket.filter.clone().unwrap();
            let before = state
                .peek()
                .app_modal
                .as_ref()
                .unwrap()
                .users
                .as_ref()
                .unwrap()
                .candidates
                .clone();
            if (2..5).contains(&case) {
                assert!(!before.is_empty());
            }
            let form = values(state);
            let canonical = support::canonical(state);
            let epoch = context.epoch;
            let team = context.session.team_id.clone();
            let calls = Cell::new(0);
            let ingress = Cell::new(0);
            let accepted_len = Cell::new(None);
            let (send, receive) = tokio::sync::oneshot::channel::<Result<Value, Error>>();
            let held = RefCell::new(Some(receive));
            let query_body = query.clone();
            let mut work = Box::pin(run_channels_with(
                state,
                ticket,
                context,
                serial,
                revision,
                query,
                filter,
                |request, scope| {
                    let call = calls.get();
                    calls.set(call + 1);
                    support::check_public_channel(&request, call, &query_body);
                    let wait = if matches!(case, 3 | 4) && call == 0 {
                        held.borrow_mut().take()
                    } else {
                        None
                    };
                    async move {
                        scope
                            .injected(request, |_| async move {
                                if let Some(wait) = wait {
                                    return wait.await.unwrap();
                                }
                                if case == 2 && call == 1 {
                                    Err(Error::RateLimited {
                                        retry_after_secs: Some(37),
                                    })
                                } else {
                                    Ok(support::public_channel_page(call, case == 1, case == 5))
                                }
                            })
                            .await
                    }
                },
                |rows| {
                    ingress.set(ingress.get() + 1);
                    accepted_len.set(Some(rows.len()));
                },
            ));
            if matches!(case, 3 | 4) {
                pending(work.as_mut()).await;
                assert_eq!(ingress.get(), 0);
                let mut s = state.write();
                let p = s.app_modal.as_mut().unwrap().users.as_mut().unwrap();
                p.query_revision = next_id();
                p.query = "successor".into();
                drop(s);
                send.send(if case == 4 {
                    Err(Error::RateLimited {
                        retry_after_secs: Some(37),
                    })
                } else {
                    Ok(support::public_channel_page(1, false, false))
                })
                .unwrap();
            }
            work.await;
            let s = state.peek();
            let p = s.app_modal.as_ref().unwrap().users.as_ref().unwrap();
            let budget = support::deadline(state, epoch, &team);
            match case {
                0 | 1 | 5 => {
                    assert_eq!(calls.get(), if case == 5 { 1 } else { 2 });
                    assert_eq!(ingress.get(), 1);
                    assert_eq!(
                        accepted_len.get(),
                        Some(if case == 5 {
                            0
                        } else if case == 1 {
                            4
                        } else {
                            1
                        })
                    );
                    assert!(!p.loading);
                    assert!(p.error.is_none());
                    assert_eq!(
                        p.candidates.len(),
                        if case == 5 {
                            0
                        } else if case == 1 {
                            4
                        } else {
                            1
                        }
                    );
                    assert!(budget.is_none());
                }
                2 => {
                    assert_eq!(calls.get(), 2);
                    assert_eq!(ingress.get(), 0);
                    assert!(!p.loading);
                    assert!(p.error.is_some());
                    assert!(p.candidates == before);
                    assert!(budget.is_some());
                }
                3 | 4 => {
                    assert_eq!(calls.get(), 1);
                    assert_eq!(ingress.get(), 0);
                    assert!(p.loading);
                    assert!(p.error.is_none());
                    assert!(p.candidates == before);
                    assert_eq!(budget.is_some(), case == 4);
                }
                _ => unreachable!(),
            }
            assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
            drop(s);
            assert_eq!(values(state), form);
            assert_eq!(support::canonical(state), canonical);
        }
    }
}

#[tokio::test]
async fn explicit_conversation_fanin_preserves_values_and_owner_specific_rate_policy() {
    use directory::fan_in::test_support::Scenario;
    for multiple in [false, true] {
        for case in 0..3u8 {
            let (h, ticket, context, serial, revision, query) = setup_with_mask(
                multiple,
                case > 0,
                true,
                Some(serde_json::json!({"include":["im","public"]})),
            );
            assert!(explicit_conversation_query(&ticket));
            assert!(!ordinary_channel_query(&ticket));
            let mut state = h.state;
            let filter = ticket.filter.clone().unwrap();
            let before = state
                .peek()
                .app_modal
                .as_ref()
                .unwrap()
                .users
                .as_ref()
                .unwrap()
                .candidates
                .clone();
            if case > 0 {
                assert!(!before.is_empty());
            }
            let form = values(state);
            let canonical = support::canonical(state);
            let epoch = context.epoch;
            let team = context.session.team_id.clone();
            let scenario = Scenario::new(case, query.clone());
            let exec = scenario.clone();
            let ingress = Cell::new(0);
            let mut work = Box::pin(run_conversations_with(
                state,
                ticket,
                context,
                serial,
                revision,
                query,
                filter,
                move |provider, request, scope, _| {
                    let exec = exec.clone();
                    async move { exec.execute(provider, request, scope).await }
                },
                |channels| {
                    assert_eq!(channels.len(), 1);
                    ingress.set(ingress.get() + 1);
                },
            ));
            if case > 0 {
                pending(work.as_mut()).await;
                assert_eq!(scenario.counts(), (1, 1));
                if case == 2 {
                    let mut s = state.write();
                    let p = s.app_modal.as_mut().unwrap().users.as_mut().unwrap();
                    p.query_revision = next_id();
                    p.query = "successor".into();
                }
                scenario.release_user();
                pending(work.as_mut()).await;
                assert_eq!(ingress.get(), 0);
                scenario.release_channel();
            }
            work.await;
            let s = state.peek();
            let p = s.app_modal.as_ref().unwrap().users.as_ref().unwrap();
            let budget = support::deadline(state, epoch, &team);
            if case == 0 {
                assert_eq!(scenario.counts(), (2, 2));
                assert_eq!(p.candidates.len(), 5);
                assert!(!p.loading);
                assert!(p.error.is_none());
                assert!(budget.is_none());
                assert_eq!(ingress.get(), 1);
            } else {
                assert_eq!(scenario.counts(), (1, 1));
                assert!(p.candidates == before);
                if case == 1 {
                    assert!(!p.loading);
                    assert!(p.error.is_some());
                    assert!(budget.is_some());
                } else {
                    assert!(p.loading);
                    assert!(p.error.is_none());
                    assert_eq!(p.query, "successor");
                    assert!(budget.is_some());
                }
                assert_eq!(ingress.get(), 0);
            }
            assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
            drop(s);
            assert_eq!(values(state), form);
            assert_eq!(support::canonical(state), canonical);
        }
    }
}

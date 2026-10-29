use super::super::read_scope::test_support as support;
use super::*;
use crate::{
    bootstrap::test_support::pending,
    message_buttons::{
        channel_fixture, conversation_fixture, inputs, multi_channel_fixture,
        multi_conversation_fixture, source::Bag, user_fixture,
    },
};
use std::cell::{Cell, RefCell};

fn setup(
    multiple: bool,
    cached_query: bool,
    conversation: bool,
) -> (
    crate::bootstrap::test_support::Harness,
    Arc<Context>,
    u64,
    u64,
    String,
    ContainerKey,
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
    Arc<Context>,
    u64,
    u64,
    String,
    ContainerKey,
) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    match (conversation, multiple) {
        (false, false) => channel_fixture::install(&mut s),
        (false, true) => multi_channel_fixture::install(&mut s),
        (true, false) => conversation_fixture::install(&mut s),
        (true, true) => multi_conversation_fixture::install(&mut s),
    }
    if conversation {
        let m = s
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .get_mut("C2")
            .unwrap()
            .messages
            .iter_mut()
            .find(|m| m.ts.as_deref() == Some(user_fixture::MAIN_TS))
            .unwrap();
        m.blocks[1]["accessory"]["filter"] =
            mask.unwrap_or_else(|| serde_json::json!({"include":["public"]}));
        user_fixture::reproject(&mut s);
    }
    support::install_context(&mut s);
    let w = &s.core.workspaces["T1"];
    let message = w.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(user_fixture::MAIN_TS))
        .unwrap();
    let row = crate::message_vm::message_vm(w, message, &s.media);
    let source = Source::capture(&s, &row, "C2", Bag::Timeline).unwrap();
    let entry = inputs::candidate(&s, &source).unwrap();
    let key = ContainerKey::new(&source);
    s.message_buttons.inputs.insert(key.clone(), entry);
    let metadata = match (conversation, multiple) {
        (false, false) => source.channel_metadata(&s, "choice", "pick").unwrap(),
        (false, true) => source.channels_metadata(&s, "choice", "pick").unwrap(),
        (true, false) => source.conversation_metadata(&s, "choice", "pick").unwrap(),
        (true, true) => source.conversations_metadata(&s, "choice", "pick").unwrap(),
    };
    let mounted = display::Mounted::new();
    // Real display projection initializes the mounted occurrence before capture.
    if multiple {
        assert_eq!(
            super::super::super::display_selected_users(&s, &source, &metadata, &mounted),
            vec!["C00000001"]
        );
    } else {
        assert_eq!(
            inputs::display_selected(&s, &source, &metadata, &mounted).as_deref(),
            Some("C00000001")
        );
    }
    let display = mounted
        .capture(&key, &field_key(&metadata).unwrap())
        .unwrap();
    let selected = if multiple {
        super::super::super::multiple::selection(&s, &source, &metadata, &mounted)
    } else {
        None
    };
    let filter = super::super::super::filter(&s, &source, &metadata).unwrap();
    let query = if cached_query {
        super::super::super::conversations::cached(&s, &source, &filter, "")[0]
            .label
            .clone()
    } else {
        "later".into()
    };
    let choices = super::super::super::conversations::cached(&s, &source, &filter, &query);
    let context = Context::capture(&s).unwrap();
    let revision = next_id();
    let query_revision = next_id();
    s.message_buttons.user_picker = Some(super::super::super::Picker {
        multiple: selected,
        context: Context::capture(&s),
        filter: Some(filter),
        candidates: choices,
        input_revision: inputs::revision(&s, &source).unwrap(),
        source,
        metadata,
        mounted,
        display,
        revision,
        query_revision,
        query: query.clone(),
        loading: true,
        agent_unavailable: false,
        error: None,
    });
    s.overlay = Some(Overlay::MessageUserSelect);
    (
        support::mount(s),
        Arc::new(context),
        revision,
        query_revision,
        query,
        key,
    )
}

#[tokio::test]
async fn ordinary_message_channels_route_complete_raw24_and_ingress_only_terminal_success() {
    for multiple in [false, true] {
        // Capture real explicit Conversation metadata with the same public-only mask.
        let (negative, _, _, _, _, _) = setup(multiple, false, true);
        assert!(!super::super::ordinary_channel_query(
            negative
                .state
                .peek()
                .message_buttons
                .user_picker
                .as_ref()
                .unwrap()
        ));
        drop(negative);
        for case in 0..6 {
            let (h, context, revision, query_revision, query, key) =
                setup(multiple, (2..5).contains(&case), false);
            let mut state = h.state;
            let p = state.peek();
            let picker = p.message_buttons.user_picker.as_ref().unwrap();
            assert!(super::super::ordinary_channel_query(picker));
            let filter = picker.filter.clone().unwrap();
            let before = picker.candidates.clone();
            if (2..5).contains(&case) {
                assert!(!before.is_empty());
            }
            let form = p.message_buttons.inputs[&key].model.clone();
            let selected = picker.multiple.as_ref().map(|m| m.selected.clone());
            assert!(p.message_buttons.operations.is_empty());
            drop(p);
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
                context,
                revision,
                query_revision,
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
                let p = s.message_buttons.user_picker.as_mut().unwrap();
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
            let p = s.message_buttons.user_picker.as_ref().unwrap();
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
            assert_eq!(s.message_buttons.inputs[&key].model, form);
            assert_eq!(p.multiple.as_ref().map(|m| m.selected.clone()), selected);
            assert!(s.message_buttons.operations.is_empty());
            drop(s);
            assert_eq!(support::canonical(state), canonical);
        }
    }
}

#[tokio::test]
async fn explicit_conversation_fanin_preserves_values_and_owner_specific_rate_policy() {
    use super::super::fan_in::test_support::Scenario;
    for multiple in [false, true] {
        for case in 0..3u8 {
            let (h, context, revision, query_revision, query, key) = setup_with_mask(
                multiple,
                case > 0,
                true,
                Some(serde_json::json!({"include":["im","public"]})),
            );
            let mut state = h.state;
            let p = state.peek();
            let picker = p.message_buttons.user_picker.as_ref().unwrap();
            assert!(super::super::explicit_conversation_query(picker));
            assert!(!super::super::ordinary_channel_query(picker));
            let filter = picker.filter.clone().unwrap();
            let before = picker.candidates.clone();
            if case > 0 {
                assert!(!before.is_empty());
            }
            let form = p.message_buttons.inputs[&key].model.clone();
            let selected = picker.multiple.as_ref().map(|m| m.selected.clone());
            drop(p);
            let canonical = support::canonical(state);
            let epoch = context.epoch;
            let team = context.session.team_id.clone();
            let scenario = Scenario::new(case, query.clone());
            let exec = scenario.clone();
            let ingress = Cell::new(0);
            let mut work = Box::pin(run_conversations_with(
                state,
                context,
                revision,
                query_revision,
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
                    let p = s.message_buttons.user_picker.as_mut().unwrap();
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
            let p = s.message_buttons.user_picker.as_ref().unwrap();
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
            assert_eq!(s.message_buttons.inputs[&key].model, form);
            assert_eq!(p.multiple.as_ref().map(|m| m.selected.clone()), selected);
            assert!(s.message_buttons.operations.is_empty());
            drop(s);
            assert_eq!(support::canonical(state), canonical);
        }
    }
}

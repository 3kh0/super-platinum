use super::super::read_scope::test_support as support;
use super::*;
use crate::{
    bootstrap::test_support::pending,
    message_buttons::{inputs, multi_user_fixture, source::Bag, user_fixture},
};
use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};

fn setup(
    multiple: bool,
    cached_query: bool,
) -> (
    crate::bootstrap::test_support::Harness,
    Arc<Context>,
    u64,
    u64,
    String,
    ContainerKey,
) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    if multiple {
        multi_user_fixture::install(&mut s);
    } else {
        user_fixture::install(&mut s);
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
    let metadata = if multiple {
        source.users_metadata(&s, "choice", "pick").unwrap()
    } else {
        source.user_metadata(&s, "choice", "pick").unwrap()
    };
    let mounted = display::Mounted::new();
    // Real display projection initializes the mounted occurrence before capture.
    if multiple {
        assert_eq!(
            super::super::super::display_selected_users(&s, &source, &metadata, &mounted),
            vec!["U00000001"]
        );
    } else {
        assert_eq!(
            inputs::display_selected(&s, &source, &metadata, &mounted).as_deref(),
            Some("U00000001")
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
    let query = if cached_query {
        cached(&s, &source, "")[0].label.clone()
    } else {
        "later".into()
    };
    let choices = cached(&s, &source, &query);
    let context = Context::capture(&s).unwrap();
    let revision = next_id();
    let query_revision = next_id();
    s.message_buttons.user_picker = Some(super::super::super::Picker {
        multiple: selected,
        context: Context::capture(&s),
        filter: None,
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
async fn message_single_and_multiple_users_continue_raw24_without_selection_or_stale_publication() {
    for multiple in [false, true] {
        for case in 0..6 {
            let (h, context, revision, query_revision, query, key) = setup(multiple, case >= 2);
            let mut state = h.state;
            let before = state
                .peek()
                .message_buttons
                .user_picker
                .as_ref()
                .unwrap()
                .candidates
                .clone();
            let form = state.peek().message_buttons.inputs[&key].model.clone();
            let selected = state
                .peek()
                .message_buttons
                .user_picker
                .as_ref()
                .unwrap()
                .multiple
                .as_ref()
                .map(|m| m.selected.clone());
            let canonical = support::canonical(state);
            let epoch = context.epoch;
            let team = context.session.team_id.clone();
            let outside = Instant::now() + Duration::from_secs(100);
            let calls = Cell::new(0);
            let (send, receive) = tokio::sync::oneshot::channel();
            let held = RefCell::new(Some(receive));
            let execution_state = state;
            let mut work = Box::pin(run_with(
                state,
                context,
                revision,
                query_revision,
                query,
                |request, scope| {
                    let call = calls.get();
                    support::check(request.clone(), call);
                    calls.set(call + 1);
                    let wait = if (case == 3 || case == 4) && call == 0 {
                        held.borrow_mut().take()
                    } else {
                        None
                    };
                    let team = team.clone();
                    async move {
                        scope
                            .injected(request, |_| async move {
                                if let Some(wait) = wait {
                                    return wait.await.unwrap();
                                }
                                if case == 5 {
                                    let mut state = execution_state;
                                    remember_cooldown(&mut state.write(), epoch, &team, outside);
                                }
                                if case == 2 && call == 1 {
                                    Err(Error::RateLimited {
                                        retry_after_secs: Some(37),
                                    })
                                } else {
                                    Ok(support::page(call, case == 1))
                                }
                            })
                            .await
                    }
                },
            ));
            if case == 3 || case == 4 {
                pending(work.as_mut()).await;
                if case == 3 {
                    let mut s = state.write();
                    let p = s.message_buttons.user_picker.as_mut().unwrap();
                    p.query_revision = next_id();
                    p.query = "successor".into();
                } else {
                    state.write().core.account_epoch += 1;
                }
                send.send(Err(Error::RateLimited {
                    retry_after_secs: Some(37),
                }))
                .unwrap();
            }
            work.await;
            let s = state.peek();
            let p = s.message_buttons.user_picker.as_ref().unwrap();
            let budget = support::deadline(state, epoch, &team);
            match case {
                0 | 1 => {
                    assert_eq!(calls.get(), 2);
                    assert!(!p.loading);
                    assert!(p.error.is_none());
                    assert_eq!(p.candidates.len(), if case == 1 { 4 } else { 1 });
                    assert!(budget.is_none());
                }
                2 => {
                    assert_eq!(calls.get(), 2);
                    assert!(!p.loading);
                    assert!(p.error.is_some());
                    assert!(p.candidates == before);
                    assert!(budget.is_some());
                }
                3 => {
                    assert_eq!(calls.get(), 1);
                    assert!(p.loading);
                    assert!(p.error.is_none());
                    assert!(p.candidates == before);
                    assert!(budget.is_some());
                }
                4 => {
                    assert_eq!(calls.get(), 1);
                    assert!(budget.is_none());
                    assert!(!support::has_budget(state));
                    assert!(p.candidates == before);
                }
                5 => {
                    assert_eq!(calls.get(), 1);
                    assert!(!p.loading);
                    assert!(p.error.as_ref().unwrap().contains("wait"));
                    assert!(p.candidates == before);
                    assert_eq!(budget, Some(outside));
                }
                _ => unreachable!(),
            }
            assert_eq!(s.message_buttons.inputs[&key].model, form);
            assert_eq!(p.multiple.as_ref().map(|m| m.selected.clone()), selected);
            drop(s);
            assert_eq!(support::canonical(state), canonical);
        }
    }
}

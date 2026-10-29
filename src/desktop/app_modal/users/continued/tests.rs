use super::*;
use crate::bootstrap::test_support::pending;
use directory::read_scope::test_support as support;
use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};

fn setup(
    multiple: bool,
    cached_query: bool,
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
        crate::app_modal::users_fixture::install(&mut s);
    } else {
        crate::app_modal::users_fixture::install_single(&mut s);
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
async fn modal_single_and_multiple_raw24_completion_preserves_local_values_and_account_rate_budget()
{
    for multiple in [false, true] {
        for case in 0..6 {
            let (h, ticket, context, serial, revision, query) = setup(multiple, case >= 2);
            let mut state = h.state;
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
            let form = values(state);
            let canonical = support::canonical(state);
            let epoch = context.epoch;
            let team = context.session.team_id.clone();
            let calls = Cell::new(0);
            let (send, receive) = tokio::sync::oneshot::channel();
            let held = RefCell::new(Some(receive));
            let outside = Instant::now() + Duration::from_secs(100);
            let execution_state = state;
            let mut work = Box::pin(run_with(
                state,
                ticket,
                context,
                serial,
                revision,
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
                                    directory::remember_cooldown(
                                        &mut state.write(),
                                        epoch,
                                        &team,
                                        outside,
                                    );
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
                    let p = s.app_modal.as_mut().unwrap().users.as_mut().unwrap();
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
            let p = s.app_modal.as_ref().unwrap().users.as_ref().unwrap();
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
            assert_eq!(s.app_modal.as_ref().unwrap().fixture_attempts, 0);
            drop(s);
            assert_eq!(values(state), form);
            assert_eq!(support::canonical(state), canonical);
        }
    }
}

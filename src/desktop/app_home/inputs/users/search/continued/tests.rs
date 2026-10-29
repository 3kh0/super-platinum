use super::*;
use crate::bootstrap::test_support::pending;
use directory::read_scope::test_support as support;
use serde_json::json;
use std::cell::{Cell, RefCell};

fn setup(
    multiple: bool,
    cached: bool,
) -> (
    crate::bootstrap::test_support::Harness,
    Ticket,
    u64,
    u64,
    String,
) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::inputs::users::fixture::install(&mut s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    let field = raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap();
    field["element"] = if multiple {
        json!({"type":"multi_users_select","action_id":"value","initial_users":[fixture::ADA],"max_selected_items":2})
    } else {
        json!({"type":"users_select","action_id":"value","initial_user":fixture::ADA})
    };
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    crate::app_home::inputs::hydrate(&mut s);
    let key = InputKey {
        block_id: "home-choice".into(),
        action_id: "value".into(),
    };
    let ticket = Ticket::capture(&s, &key).unwrap();
    assert!(open_local(&mut s, ticket.clone()));
    let query = if cached {
        super::super::super::cached(&s, &ticket, "")[0]
            .label
            .clone()
    } else {
        "later".into()
    };
    let choices = super::super::super::cached(&s, &ticket, &query);
    let p = s.app_home.users.picker.as_mut().unwrap();
    p.query = query.clone();
    p.query_revision = next();
    p.loading = true;
    p.candidates = choices;
    p.error = None;
    let (serial, revision) = (p.serial, p.query_revision);
    (support::mount(s), ticket, serial, revision, query)
}
fn values(
    state: Signal<ShellState>,
    owner: &crate::app_home::inputs::FormOwner,
) -> Vec<(
    InputKey,
    super_platinum_core::block_actions::form::InputValue,
)> {
    // Retirement denies current access, but must not rewrite the retained form.
    state
        .peek()
        .app_home
        .forms
        .get(owner)
        .expect("captured form retained for comparison")
        .fields
        .iter()
        .map(|(k, f)| (k.clone(), f.value.clone()))
        .collect()
}

#[tokio::test]
async fn home_single_and_multiple_lookup_complete_raw24_and_preserve_observed_rate_ownership() {
    for multiple in [false, true] {
        for case in 0..6 {
            let (h, ticket, serial, revision, query) = setup(multiple, case >= 2);
            let mut state = h.state;
            let before = state
                .peek()
                .app_home
                .users
                .picker
                .as_ref()
                .unwrap()
                .candidates
                .clone();
            let form_owner = ticket.form.clone();
            let form = values(state, &form_owner);
            let canonical = support::canonical(state);
            let actions = state.peek().app_home.cooldowns.clone();
            let epoch = ticket.owner.ctx.epoch;
            let team = ticket.owner.ctx.session.team_id.clone();
            let calls = Cell::new(0);
            let (send, receive) = tokio::sync::oneshot::channel();
            let held = RefCell::new(Some(receive));
            let outside = Instant::now() + Duration::from_secs(100);
            let execution_state = state;
            let mut work = Box::pin(run_with(
                state,
                ticket,
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
                    let p = s.app_home.users.picker.as_mut().unwrap();
                    p.query_revision = next();
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
            let p = s.app_home.users.picker.as_ref().unwrap();
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
                    assert!(crate::app_home::inputs::form(&s).is_none());
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
            assert!(s.app_home.cooldowns == actions);
            drop(s);
            assert_eq!(values(state, &form_owner), form);
            assert_eq!(support::canonical(state), canonical);
        }
    }
}

#[tokio::test]
async fn home_cancellation_drops_dispatched_injected_read_without_claiming_late_settlement() {
    let (h, ticket, serial, revision, query) = setup(false, true);
    let state = h.state;
    let (send, receive) = tokio::sync::oneshot::channel::<Result<Value, Error>>();
    let held = RefCell::new(Some(receive));
    let read = run_with(state, ticket, serial, revision, query, |request, scope| {
        let receive = held.borrow_mut().take().unwrap();
        async move {
            scope
                .injected(request, |_| async move { receive.await.unwrap() })
                .await
        }
    });
    let (cancel, mut cancellation) = tokio::sync::watch::channel(false);
    let mut work = Box::pin(super::super::until_cancelled(read, &mut cancellation));
    pending(work.as_mut()).await;
    cancel.send(true).unwrap();
    work.await;
    assert!(
        send.send(Err(Error::RateLimited {
            retry_after_secs: Some(37)
        }))
        .is_err()
    );
    assert!(!support::has_budget(state));
}

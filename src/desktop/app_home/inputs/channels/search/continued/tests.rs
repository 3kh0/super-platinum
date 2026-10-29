use super::*;
use crate::bootstrap::test_support::pending;
use directory::read_scope::test_support as support;
use serde_json::json;
use std::cell::{Cell, RefCell};

fn channel(n: usize, team: &str) -> Value {
    json!({"id":format!("C{n:08}"),"name":format!("later-{n}"),"is_channel":true,
        "internal_team_ids":[team],"updated":1})
}
fn page(call: usize, raw_budget: bool, team: &str) -> Value {
    if call == 0 {
        json!({"results":vec![Value::Null;if raw_budget{20}else{1}],"next_marker":"next"})
    } else if raw_budget {
        json!({"results":(20000000..20000020).map(|n|channel(n,team)).collect::<Vec<_>>(),"next_marker":{"unused":true}})
    } else {
        json!({"results":[channel(20000000,team)]})
    }
}
fn check(request: PreparedRequest, call: usize, query: &str, policy: &Value) {
    assert!(request.url.contains("/cache/EHOME0001/channels/search"));
    let super_platinum_core::slack::client::RequestBody::Json(body) = request.body else {
        panic!("JSON")
    };
    assert_eq!(body["query"], query);
    assert_eq!(body["count"], 24);
    assert_eq!(body["fuzz"], 1);
    assert_eq!(body["check_membership"], true);
    assert!(body.get("external_team").is_none());
    for key in [
        "allowed_internal_teams",
        "should_allowed_internal_teams_match_any",
    ] {
        assert_eq!(body.get(key), policy.get(key));
    }
    match call {
        0 => assert!(body.get("marker").is_none()),
        1 => assert_eq!(body["marker"], "next"),
        _ => panic!("refill"),
    }
}
fn visible(
    state: Signal<ShellState>,
    owner: &crate::app_home::inputs::FormOwner,
    key: &InputKey,
) -> Vec<String> {
    let s = state.peek();
    let index = (owner.clone(), key.clone());
    if let Some(w) = s.app_home.channels.multiples.get(&index) {
        w.visible.clone()
    } else {
        s.app_home.channels.widgets[&index]
            .visible
            .iter()
            .cloned()
            .collect()
    }
}

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
    setup_with_mask(multiple, cached, None)
}
fn setup_with_mask(
    multiple: bool,
    cached: bool,
    mask: Option<Value>,
) -> (
    crate::bootstrap::test_support::Harness,
    Ticket,
    u64,
    u64,
    String,
) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = Some("EHOME0001".into());
    let row = json!({"id":"CINITIAL1","name":"cached-project","is_channel":true,
        "is_member":true,"updated":1,"internal_team_ids":[team]});
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .insert("CINITIAL1".into(), serde_json::from_value(row).unwrap());
    let owner = Owner::capture(&s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = owner.home_visit;
    h.owner = owner;
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    let mut element = if multiple {
        json!({"type":"multi_channels_select","action_id":"pick","initial_channels":["CINITIAL1"],"max_selected_items":2})
    } else {
        json!({"type":"channels_select","action_id":"pick","initial_channel":"CINITIAL1"})
    };
    if let Some(mask) = mask {
        element["type"] = json!(if multiple {
            "multi_conversations_select"
        } else {
            "conversations_select"
        });
        let initial = element
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
        }] = initial;
        element["filter"] = mask;
    }
    raw["blocks"] = json!([
        {"type":"input","block_id":"destination","optional":true,"label":{"type":"plain_text","text":"Destination"},"element":element},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Notes"},"element":{"type":"plain_text_input","action_id":"edit","initial_value":"Retained draft"}}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    s.app_home.forms.clear();
    crate::app_home::inputs::hydrate(&mut s);
    let key = InputKey {
        block_id: "destination".into(),
        action_id: "pick".into(),
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
    let p = s.app_home.channels.picker.as_mut().unwrap();
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
async fn home_channel_short_pages_clip_raw24_and_keep_query_gated_rate_ownership() {
    for multiple in [false, true] {
        for case in 0..8 {
            let (h, ticket, serial, revision, query) = setup(multiple, case >= 2);
            let mut state = h.state;
            let before = state
                .peek()
                .app_home
                .channels
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
            let query_body = query.clone();
            let policy_body = ticket.policy.request_fields();
            let selected = visible(state, &ticket.form, &ticket.key);
            let key = ticket.key.clone();
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
                    check(request.clone(), call, &query_body, &policy_body);
                    calls.set(call + 1);
                    let wait = if matches!(case, 3 | 4 | 6) && call == 0 {
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
                                    let mut state = execution_state;
                                    directory::remember_cooldown(
                                        &mut state.write(),
                                        epoch,
                                        &team,
                                        outside,
                                    );
                                    Err(Error::RateLimited {
                                        retry_after_secs: Some(37),
                                    })
                                } else if case == 7 && call == 1 {
                                    Err(Error::Offline("synthetic channel outage".into()))
                                } else {
                                    Ok(page(call, case == 1, &team))
                                }
                            })
                            .await
                    }
                },
            ));
            if matches!(case, 3 | 4 | 6) {
                pending(work.as_mut()).await;
                if case == 3 {
                    let mut s = state.write();
                    let p = s.app_home.channels.picker.as_mut().unwrap();
                    p.query_revision = next();
                    p.query = "successor".into();
                } else if case == 4 {
                    state.write().core.account_epoch += 1;
                } else {
                    state
                        .write()
                        .core
                        .session
                        .as_mut()
                        .unwrap()
                        .unified_user_client_enabled
                        .clear();
                }
                send.send(Err(Error::RateLimited {
                    retry_after_secs: Some(37),
                }))
                .unwrap();
            }
            work.await;
            let s = state.peek();
            let p = s.app_home.channels.picker.as_ref().unwrap();
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
                    assert_eq!(budget, Some(outside));
                }
                3 | 6 => {
                    assert_eq!(calls.get(), 1);
                    assert!(p.loading);
                    assert!(p.error.is_none());
                    assert!(p.candidates == before);
                    assert!(budget.is_none());
                    assert!(!support::has_budget(state));
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
                7 => {
                    assert_eq!(calls.get(), 2);
                    assert!(!p.loading);
                    assert!(p.error.is_some());
                    assert!(p.candidates == before);
                    assert!(budget.is_some());
                }
                _ => unreachable!(),
            }
            assert!(s.app_home.cooldowns == actions);
            drop(s);
            assert_eq!(values(state, &form_owner), form);
            assert_eq!(visible(state, &form_owner, &key), selected);
            assert_eq!(support::canonical(state), canonical);
        }
    }
}

#[tokio::test]
async fn home_channel_cancellation_drops_whole_lookup_without_late_observation() {
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

#[tokio::test]
async fn explicit_conversation_fanin_preserves_values_and_owner_specific_rate_policy() {
    use directory::fan_in::test_support::Scenario;
    for multiple in [false, true] {
        for case in 0..3u8 {
            let (h, ticket, serial, revision, query) = setup_with_mask(
                multiple,
                case > 0,
                Some(serde_json::json!({"include":["im","public"]})),
            );
            assert!(
                ticket
                    .conversation
                    .as_ref()
                    .is_some_and(directory::conversation_mask_matches)
            );
            assert!(ticket.mode.is_none());
            let mut state = h.state;
            let form_owner = ticket.form.clone();
            let key = ticket.key.clone();
            let before = state
                .peek()
                .app_home
                .channels
                .picker
                .as_ref()
                .unwrap()
                .candidates
                .clone();
            if case > 0 {
                assert!(!before.is_empty());
            }
            let form = values(state, &form_owner);
            let selected = visible(state, &form_owner, &key);
            let canonical = support::canonical(state);
            let actions = state.peek().app_home.cooldowns.clone();
            let epoch = ticket.owner.ctx.epoch;
            let team = ticket.owner.ctx.session.team_id.clone();
            let scenario = Scenario::new(case, query.clone());
            let exec = scenario.clone();
            let mut work = Box::pin(run_conversations_with(
                state,
                ticket,
                serial,
                revision,
                query,
                move |provider, request, scope, _| {
                    let exec = exec.clone();
                    async move { exec.execute(provider, request, scope).await }
                },
            ));
            if case > 0 {
                pending(work.as_mut()).await;
                assert_eq!(scenario.counts(), (1, 1));
                if case == 2 {
                    let mut s = state.write();
                    let p = s.app_home.channels.picker.as_mut().unwrap();
                    p.query_revision = next();
                    p.query = "successor".into();
                }
                scenario.release_user();
                pending(work.as_mut()).await;
                scenario.release_channel();
            }
            work.await;
            let s = state.peek();
            let p = s.app_home.channels.picker.as_ref().unwrap();
            let budget = support::deadline(state, epoch, &team);
            if case == 0 {
                assert_eq!(scenario.counts(), (2, 2));
                assert_eq!(p.candidates.len(), 5);
                assert!(!p.loading);
                assert!(p.error.is_none());
                assert!(budget.is_none());
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
                    assert!(budget.is_none());
                }
            }
            assert_eq!(s.app_home.cooldowns, actions);
            drop(s);
            assert_eq!(values(state, &form_owner), form);
            assert_eq!(visible(state, &form_owner, &key), selected);
            assert_eq!(support::canonical(state), canonical);
        }
    }
}

use super::*;
use crate::bootstrap::test_support::pending;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use super_platinum_core::slack::client::RequestBody;

fn setup(cached: bool) -> (crate::bootstrap::test_support::Harness, u64, Arc<Context>) {
    let (mut s, id) = super::super::super::tests::setup();
    let ctx = context(&s, id).unwrap();
    let c = active_mut(&mut s, id).unwrap();
    c.guest_generation = 11;
    c.guest_query = " literal ".into();
    c.guest_loading = true;
    if cached {
        c.directory_endpoint = Some(endpoint(&ctx));
    }
    (
        crate::message_buttons::inputs::user::directory::read_scope::test_support::mount(s),
        id,
        ctx,
    )
}
fn endpoint(ctx: &Context) -> Arc<DirectoryEndpoint> {
    Arc::new(DirectoryEndpoint::decode(&ctx.session, &discovered(ctx, 60), Instant::now()).unwrap())
}
fn discovered(ctx: &Context, ttl: u64) -> Value {
    json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",ctx.session.team_id),"ttl_seconds":ttl})
}
fn row(n: usize) -> Value {
    json!({"id":format!("U{n:08}"),"name":format!("literal {n}"),"profile":{"display_name":format!("literal {n}")}})
}

#[tokio::test]
async fn observed_guest_requests_complete_raw24_before_decode_and_preserve_the_512_envelope_guard()
{
    for case in 0..5 {
        let (h, id, ctx) = setup(case != 0);
        let calls = Cell::new(0);
        let pages = Cell::new(0);
        let result = read_with(h.state,id,11,&ctx," literal ",|request,scope| {
            assert!(scope.admitted());
            calls.set(calls.get()+1);
            let response = if request.url.contains("api.getFlannelHttpUrl") {
                assert_eq!(case,0);
                discovered(&ctx,60)
            } else {
                assert!(request.url.starts_with(&format!("https://edgeapi.slack.com/cache/{}/users/search",ctx.session.team_id)));
                let RequestBody::Json(body)=request.body else {panic!("JSON")};
                assert_eq!(body["query"]," literal ");assert_eq!(body["count"],24);
                assert_eq!(body["fuzz"],1);assert_eq!(body["enable_workspace_ranking"],true);
                assert_eq!(body["filter"],"NOT deactivated");assert_eq!(body["include_profile_only_users"],true);
                assert_eq!(body["token"],ctx.session.token);assert!(body.get("enterprise_token").is_none());
                let page=pages.get();pages.set(page+1);
                if page==0 {assert!(body.get("marker").is_none());} else {assert_eq!(body["marker"],"guest-next");}
                match (case,page) {
                    (2,0)|(3,0)=>json!({"results":(0..if case==2 {512}else{513}).map(row).collect::<Vec<_>>()}),
                    (_,0)=>json!({"results":vec![Value::Null;if case==1 {20}else if case==4 {0}else{1}],"next_marker":"guest-next"}),
                    _=>{let mut rows=(100..120).map(row).collect::<Vec<_>>();if case==1 {rows[0]["is_profile_only_user"]=json!(true);rows[2]=row(101);rows[3]=row(102);}json!({"results":rows,"next_marker":if case==1 {json!({"unused":true})}else{Value::Null}})},
                }
            };
            async move { scope.finish_injected(async {Ok(response)}).await }
        }).await;
        if case == 3 {
            assert!(result.is_err());
            assert_eq!(pages.get(), 1);
        } else {
            let users = result.unwrap().unwrap();
            assert_eq!(
                users.len(),
                if case == 1 {
                    4
                } else if case == 2 {
                    24
                } else {
                    20
                }
            );
            if case == 1 {
                assert_eq!(users.last().unwrap().id, "U00000102");
                assert_eq!(users[0].extra["is_profile_only_user"], true);
                assert_eq!(users[1].id, users[2].id);
                assert!(!users.iter().any(|u| u.id == "U00000103"));
            }
        }
        assert_eq!(calls.get(), pages.get() + usize::from(case == 0));
    }
}

#[tokio::test]
async fn guest_owner_and_readiness_fence_discovery_pages_expiry_and_account_rate_observation() {
    for (discovery, retirement) in [
        (false, 0),
        (true, 0),
        (false, 1),
        (true, 1),
        (false, 2),
        (true, 2),
    ] {
        let (h, id, ctx) = setup(!discovery);
        let mut state = h.state;
        let (send, receive) = tokio::sync::oneshot::channel();
        let mut receive = Some(receive);
        let calls = Cell::new(0);
        let mut work = Box::pin(read_with(state, id, 11, &ctx, " literal ", |_, scope| {
            assert!(scope.admitted());
            calls.set(calls.get() + 1);
            let receive = receive.take().unwrap();
            async move {
                scope
                    .finish_injected(async { receive.await.unwrap() })
                    .await
            }
        }));
        pending(work.as_mut()).await;
        match retirement {
            0 => active_mut(&mut state.write(), id).unwrap().guest_generation = 12,
            1 => {
                state
                    .write()
                    .message_buttons
                    .shortcuts
                    .dialog
                    .as_mut()
                    .unwrap()
                    .owner
                    .ctx = Arc::new(Context {
                    transport: ctx.transport.clone(),
                    client: ctx.client.clone(),
                    session: ctx.session.clone(),
                    epoch: ctx.epoch,
                })
            }
            _ => state.write().core.account_epoch += 1,
        }
        send.send(Err(Error::RateLimited {
            retry_after_secs: Some(37),
        }))
        .unwrap();
        assert!(work.await.unwrap().is_none());
        assert_eq!(calls.get(), 1);
        let s = state.peek();
        let deadline = s
            .message_buttons
            .shortcuts
            .cooldowns
            .get(&(ctx.epoch, ctx.session.team_id.clone()))
            .copied()
            .flatten();
        if retirement == 2 {
            assert!(
                !s.message_buttons
                    .shortcuts
                    .cooldowns
                    .contains_key(&(ctx.epoch, ctx.session.team_id.clone()))
            );
            assert!(deadline.is_none());
        } else {
            let remaining = deadline.unwrap().saturating_duration_since(Instant::now());
            assert!(remaining > Duration::from_secs(30) && remaining <= Duration::from_secs(37));
        }
        let c = s
            .message_buttons
            .shortcuts
            .dialog
            .as_ref()
            .unwrap()
            .calendar
            .as_ref()
            .unwrap();
        if discovery {
            assert!(c.directory_endpoint.is_none());
        }
        assert!(c.guest_loading);
    }
    // The exact admission callbacks run again after readiness. Refusal is not an attempt.
    for expired in [false, true] {
        let (h, id, ctx) = setup(true);
        let mut state = h.state;
        let e = Arc::new(
            DirectoryEndpoint::decode(&ctx.session, &discovered(&ctx, 3), Instant::now()).unwrap(),
        );
        active_mut(&mut state.write(), id)
            .unwrap()
            .directory_endpoint = Some(e.clone());
        let (send, receive) = tokio::sync::oneshot::channel();
        let mut receive = Some(receive);
        let attempts = Cell::new(0);
        let a = &attempts;
        let mut work = Box::pin(read_with(state, id, 11, &ctx, " literal ", |_, scope| {
            let receive = receive.take().unwrap();
            async move {
                receive.await.unwrap();
                if !scope.admitted() {
                    return Ok(None);
                }
                a.set(a.get() + 1);
                scope
                    .finish_injected(async { Ok(json!({"results":[]})) })
                    .await
            }
        }));
        pending(work.as_mut()).await;
        if expired {
            tokio::time::sleep(Duration::from_millis(650)).await;
        } else {
            state.write().message_buttons.shortcuts.cooldowns.insert(
                (ctx.epoch, ctx.session.team_id.clone()),
                Some(Instant::now() + Duration::from_secs(60)),
            );
        }
        send.send(()).unwrap();
        assert!(work.await.unwrap().is_none());
        assert_eq!(attempts.get(), 0);
        assert!(active(&state.peek(), id).unwrap().guest_loading);
        // An already admitted result uses publication currentness, even when
        // readiness for a new attempt would now fail due to expiry or cooldown.
        state.write().message_buttons.shortcuts.cooldowns.clear();
        let fresh = Arc::new(
            DirectoryEndpoint::decode(
                &ctx.session,
                &discovered(&ctx, if expired { 3 } else { 60 }),
                Instant::now(),
            )
            .unwrap(),
        );
        let admitted = scope(state, id, 11, ctx.clone(), Some(fresh));
        assert!(admitted.admitted());
        if expired {
            tokio::time::sleep(Duration::from_millis(650)).await;
        } else {
            state.write().message_buttons.shortcuts.cooldowns.insert(
                (ctx.epoch, ctx.session.team_id.clone()),
                Some(Instant::now() + Duration::from_secs(60)),
            );
        }
        assert!(!admitted.admitted());
        assert!(admitted.current());
        assert!(
            admitted
                .finish_injected(async { Ok(json!({"results":[]})) })
                .await
                .unwrap()
                .is_some()
        );
    }
}

#[tokio::test]
async fn whole_guest_lookup_preserves_cached_and_selected_values_before_metadata_and_final_publication()
 {
    for outcome in 0..5 {
        let (h, id, ctx) = setup(true);
        let mut state = h.state;
        let cached: User = serde_json::from_value(row(700)).unwrap();
        state
            .write()
            .core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .unwrap()
            .users
            .insert(cached.id.clone(), cached.clone());
        {
            let mut s = state.write();
            let c = active_mut(&mut s, id).unwrap();
            c.guests = vec![Person {
                id: cached.id.clone(),
                label: "Selected remains".into(),
            }];
            c.guest_choices = vec![Person {
                id: cached.id.clone(),
                label: "Installed remains".into(),
            }];
        }
        let canonical = Rc::new(RefCell::new(
            crate::message_buttons::inputs::user::directory::read_scope::test_support::canonical(
                state,
            ),
        ));
        let (send1, receive1) = tokio::sync::oneshot::channel();
        let (send2, receive2) = tokio::sync::oneshot::channel();
        let mut receivers = VecDeque::from([receive1, receive2]);
        let calls = Cell::new(0);
        let lookup = read_with(state, id, 11, &ctx, " literal ", |_, scope| {
            calls.set(calls.get() + 1);
            let receiver = receivers.pop_front().unwrap();
            async move {
                scope
                    .finish_injected(async { receiver.await.unwrap() })
                    .await
            }
        });
        let metadata = Rc::new(Cell::new(0));
        let counted = metadata.clone();
        let mut hydrated = state;
        let expected_canonical = canonical.clone();
        let team = ctx.session.team_id.clone();
        let mut work = Box::pin(super::super::super::load::guests::run_with(
            state,
            id,
            11,
            ctx.clone(),
            " literal ".into(),
            lookup,
            move |users| async move {
                counted.set(counted.get() + 1);
                assert_eq!(users.first().unwrap().id, "U00000700");
                assert_eq!(users.len(), if outcome == 2 { 1 } else { 3 });
                assert_eq!(
                    users[0].profile.as_ref().unwrap().display_name.as_deref(),
                    Some("literal 700")
                );
                if outcome == 0 {
                    let mut newer: User = serde_json::from_value(row(701)).unwrap();
                    newer.profile.as_mut().unwrap().display_name =
                        Some("Workspace current701".into());
                    expected_canonical.borrow_mut()["users"][&newer.id] =
                        serde_json::to_value(&newer).unwrap();
                    hydrated
                        .write()
                        .core
                        .workspaces
                        .get_mut(&team)
                        .unwrap()
                        .users
                        .insert(newer.id.clone(), newer);
                }
                if outcome == 1 {
                    active_mut(&mut hydrated.write(), id)
                        .unwrap()
                        .guest_generation = 12;
                }
            },
        ));
        pending(work.as_mut()).await;
        if outcome == 4 {
            state.write().message_buttons.shortcuts.cooldowns.insert(
                (ctx.epoch, ctx.session.team_id.clone()),
                Some(Instant::now() + Duration::from_secs(60)),
            );
            send1
                .send(Ok(
                    json!({"results":vec![Value::Null;20],"next_marker":"guest-next"}),
                ))
                .unwrap();
            work.await;
            assert_eq!(calls.get(), 1);
            assert_eq!(metadata.get(), 0);
            let s = state.peek();
            let c = active(&s, id).unwrap();
            assert!(!c.guest_loading);
            assert_eq!(c.guest_choices[0].label, "Installed remains");
            assert_eq!(c.guests[0].label, "Selected remains");
            continue;
        }
        send1
            .send(Ok(
                json!({"results":vec![Value::Null;20],"next_marker":"guest-next"}),
            ))
            .unwrap();
        pending(work.as_mut()).await;
        assert_eq!(calls.get(), 2);
        assert_eq!(metadata.get(), 0);
        assert_eq!(
            active(&state.peek(), id).unwrap().guest_choices[0].label,
            "Installed remains"
        );
        if outcome == 3 {
            active_mut(&mut state.write(), id).unwrap().guest_generation = 12;
        }
        send2
            .send(if outcome == 2 {
                Err(Error::RateLimited {
                    retry_after_secs: Some(37),
                })
            } else {
                {
                    let mut duplicate = row(700);
                    duplicate["profile"]["display_name"] = json!("Remote wrong700");
                    let mut excluded = row(702);
                    excluded["is_profile_only_user"] = json!(true);
                    Ok(json!({"results":[duplicate,excluded,row(701),row(701),row(703)]}))
                }
            })
            .unwrap();
        work.await;
        let s = state.peek();
        let c = active(&s, id).unwrap();
        assert_eq!(c.guests[0].label, "Selected remains");
        assert_eq!(metadata.get(), usize::from(outcome != 3));
        if matches!(outcome, 1 | 3) {
            assert!(c.guest_loading);
            assert_eq!(c.guest_choices[0].label, "Installed remains");
        } else {
            assert!(!c.guest_loading);
            assert_eq!(c.guest_choices.len(), if outcome == 2 { 1 } else { 2 });
            assert_eq!(c.guest_choices[0].label, "literal 700");
            let choices = c
                .guest_choices
                .iter()
                .map(|p| (p.id.as_str(), p.label.as_str()))
                .collect::<Vec<_>>();
            assert_eq!(
                choices,
                if outcome == 2 {
                    vec![("U00000700", "literal 700")]
                } else {
                    vec![
                        ("U00000700", "literal 700"),
                        (
                            "U00000701",
                            if outcome == 0 {
                                "Workspace current701"
                            } else {
                                "literal 701"
                            },
                        ),
                    ]
                }
            );
        }
        drop(s);
        assert_eq!(
            crate::message_buttons::inputs::user::directory::read_scope::test_support::canonical(
                state
            ),
            *canonical.borrow()
        );
    }
}

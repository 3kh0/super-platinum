use super::*;
use crate::bootstrap::test_support::pending;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use super_platinum_core::slack::client::RequestBody;

fn setup(short: bool) -> (crate::bootstrap::test_support::Harness, u64, Arc<Context>) {
    let (mut s, id) = super::super::super::tests::setup();
    let ctx = context(&s, id).unwrap();
    let c = active_mut(&mut s, id).unwrap();
    c.share_generation = 11;
    c.guest_generation = 700;
    c.share_query = " literal ".into();
    c.share_loading = true;
    c.notice = Some("Unrelated warning".into());
    c.directory_endpoint = Some(Arc::new(
        DirectoryEndpoint::decode(
            &ctx.session,
            &json!({"ok":true,"ttl_seconds":if short {3}else{60},
        "url":format!("https://edgeapi.slack.com/cache/{}",ctx.session.team_id)}),
            Instant::now(),
        )
        .unwrap(),
    ));
    (
        crate::message_buttons::inputs::user::directory::read_scope::test_support::mount(s),
        id,
        ctx,
    )
}
fn row(n: usize) -> Value {
    json!({"id":format!("C{n:08}"),"name":format!("Channel {n}"),"is_channel":true,"is_member":true})
}
fn sibling() -> Destination {
    Destination::User(serde_json::from_value(json!({"id":"USIBLING1","name":"Sibling"})).unwrap())
}

#[tokio::test]
async fn single_channel_wire_and_terminal_ingress_preserve_siblings_selection_and_posting_authority()
 {
    let (h, id, ctx) = setup(false);
    let state = h.state;
    let legacy = queries(" literal ");
    let mut expected = legacy[1].1.clone();
    expected["check_membership"] = json!(true);
    assert!(legacy[1].1.get("check_membership").is_none());
    assert_eq!(body(" literal ", None), expected);
    assert!(legacy[0].1.get("include_profile_only_users").is_none());
    assert_eq!(legacy[2].0, "mpims/search");
    let selected = active(&state.peek(), id).unwrap().share_channel.clone();
    let (s1, r1) = tokio::sync::oneshot::channel();
    let (s2, r2) = tokio::sync::oneshot::channel();
    let mut receivers = VecDeque::from([r1, r2]);
    let calls = Cell::new(0);
    let ingresses = Rc::new(Cell::new(0));
    let counted = ingresses.clone();
    let observed = Rc::new(RefCell::new(vec![]));
    let saved = observed.clone();
    let mut work = Box::pin(run_with(
        state,
        id,
        11,
        &ctx,
        " literal ",
        |request, scope| {
            assert!(scope.admitted());
            let call = calls.get();
            calls.set(call + 1);
            let RequestBody::Json(wire) = request.body else {
                panic!("JSON")
            };
            assert_eq!(wire["check_membership"], true);
            assert_eq!(wire["count"], 100);
            assert_eq!(wire["filter"], "xws");
            assert_eq!(wire["include_record_channels"], true);
            assert_eq!(wire["query"], " literal ");
            assert!(request.url.starts_with(&format!(
                "https://edgeapi.slack.com/cache/{}/channels/search",
                ctx.session.team_id
            )));
            if call == 0 {
                assert!(wire.get("marker").is_none());
            } else {
                assert_eq!(wire["marker"], "next");
            }
            let receiver = receivers.pop_front().unwrap();
            async move {
                scope
                    .finish_injected(async { receiver.await.unwrap() })
                    .await
            }
        },
        move |rows| {
            counted.set(counted.get() + 1);
            *saved.borrow_mut() = rows.to_vec();
        },
    ));
    pending(work.as_mut()).await;
    s1.send(Ok(
        json!({"results":vec![Value::Null;99],"next_marker":"next"}),
    ))
    .unwrap();
    pending(work.as_mut()).await;
    assert_eq!(calls.get(), 2);
    assert_eq!(ingresses.get(), 0);
    s2.send(Ok(
        json!({"results":[row(800),row(801)],"next_marker":{"unused":true}}),
    ))
    .unwrap();
    let result = work.await;
    assert_eq!(ingresses.get(), 1);
    assert_eq!(observed.borrow().len(), 1);
    assert_eq!(observed.borrow()[0].id, "C00000800");
    let mut remote = vec![sibling()];
    assert!(merge_channels(&mut remote, result));
    assert_eq!(remote.len(), 2);
    assert_eq!(remote[0].id(), "USIBLING1");
    let s = state.peek();
    let c = active(&s, id).unwrap();
    assert_eq!(c.share_channel, selected);
    assert_eq!(c.notice.as_deref(), Some("Unrelated warning"));
    let Destination::Channel(channel) = &remote[1] else {
        panic!("channel")
    };
    assert!(channel_base(&s, &ctx, channel));
    assert!(!remote[1].available(&s, &ctx));
}

#[tokio::test]
async fn channel_refusal_retirement_rate_and_expiry_have_no_partial_ingress_or_sibling_loss() {
    for case in 0..7 {
        let (h, id, ctx) = setup(matches!(case, 4 | 5 | 6));
        let mut state = h.state;
        let (s1, r1) = tokio::sync::oneshot::channel();
        let (s2, r2) = tokio::sync::oneshot::channel();
        let mut receivers = VecDeque::from([r1, r2]);
        let calls = Cell::new(0);
        let attempts = Cell::new(0);
        let accepted = Cell::new(0);
        let mut work = Box::pin(run_with(
            state,
            id,
            11,
            &ctx,
            " literal ",
            |_, scope| {
                calls.set(calls.get() + 1);
                let receiver = receivers.pop_front().unwrap();
                let attempts = &attempts;
                async move {
                    if case != 6 {
                        assert!(scope.admitted());
                        attempts.set(attempts.get() + 1);
                    }
                    let result = receiver.await.unwrap();
                    if case == 6 {
                        assert!(!scope.admitted());
                        return Ok(None);
                    }
                    scope.finish_injected(async { result }).await
                }
            },
            |rows| {
                accepted.set(accepted.get() + 1);
                assert!(rows.is_empty());
            },
        ));
        pending(work.as_mut()).await;
        if case == 3 {
            state.write().message_buttons.shortcuts.cooldowns.insert(
                (ctx.epoch, ctx.session.team_id.clone()),
                Some(Instant::now() + Duration::from_secs(60)),
            );
        }
        if matches!(case, 4 | 5 | 6) {
            tokio::time::sleep(Duration::from_millis(650)).await;
        }
        if case == 5 {
            s1.send(Ok(
                json!({"results":vec![Value::Null;100],"next_marker":{"unused":true}}),
            ))
            .unwrap();
        } else {
            s1.send(Ok(
                json!({"results":vec![Value::Null;99],"next_marker":"next"}),
            ))
            .unwrap();
        }
        let result = if matches!(case, 3 | 4 | 5 | 6) {
            work.await
        } else {
            pending(work.as_mut()).await;
            if case == 1 {
                active_mut(&mut state.write(), id).unwrap().share_generation = 12;
            }
            if case == 2 {
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
                });
            }
            s2.send(Err(Error::RateLimited {
                retry_after_secs: Some(37),
            }))
            .unwrap();
            work.await
        };
        let current = owned_current(&state.peek(), id, 11, &ctx);
        let mut remote = vec![sibling()];
        if current {
            let success = merge_channels(&mut remote, result);
            assert_eq!(success, case == 5);
            assert_eq!(remote.len(), 1);
        } else {
            assert!(result.unwrap().is_none());
        }
        assert_eq!(accepted.get(), usize::from(case == 5));
        assert_eq!(calls.get(), if case <= 2 { 2 } else { 1 });
        assert_eq!(
            attempts.get(),
            if case == 6 {
                0
            } else if case <= 2 {
                2
            } else {
                1
            }
        );
        if case <= 2 {
            let s = state.peek();
            let until = s.message_buttons.shortcuts.cooldowns
                [&(ctx.epoch, ctx.session.team_id.clone())]
                .unwrap();
            let remaining = until.saturating_duration_since(Instant::now());
            assert!(remaining > Duration::from_secs(30) && remaining <= Duration::from_secs(37));
        }
        assert_eq!(
            active(&state.peek(), id).unwrap().notice.as_deref(),
            Some("Unrelated warning")
        );
    }
}

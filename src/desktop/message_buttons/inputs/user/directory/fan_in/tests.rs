use super::*;
use crate::bootstrap::test_support::pending;
use serde_json::json;
use std::cell::RefCell;

fn context() -> Context {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::fixture::install_inputs(&mut s);
    Context::capture(&s).unwrap()
}
fn filter() -> super::super::super::conversations::Filter {
    super::super::super::conversations::Filter {
        spec: super_platinum_core::block_actions::form::ConversationFilter {
            include: Some(vec!["im".into(), "public".into()]),
            ..Default::default()
        },
        ..Default::default()
    }
}
fn rate(n: u64) -> Error {
    Error::RateLimited {
        retry_after_secs: Some(n),
    }
}

#[test]
fn known_rate_survives_refusal_and_channel_errors_keep_their_legacy_priority() {
    for (users, channels, expected) in [
        (Err(rate(12)), Ok(None), 12),
        (Ok(None), Err(rate(37)), 37),
        (Err(rate(12)), Err(rate(37)), 37),
    ] {
        assert!(
            matches!(combine(users,channels),Err(Error::RateLimited{retry_after_secs:Some(n)}) if n==expected)
        );
    }
    assert!(
        matches!(combine(Err(Error::Api("member".into())),Err(Error::Api("channel".into()))),Err(Error::Api(s)) if s=="channel")
    );
    assert!(
        matches!(combine(Err(Error::Api("member".into())),Ok(Some(vec![]))),Err(Error::Api(s)) if s=="member")
    );
    assert!(
        combine(Ok(None), Err(Error::Api("channel".into())))
            .unwrap()
            .is_none()
    );
    assert!(combine(Ok(Some(vec![])), Ok(None)).unwrap().is_none());
    assert_eq!(
        combine(Ok(Some(vec![])), Ok(Some(vec![])))
            .unwrap()
            .unwrap()
            .0
            .len(),
        0
    );
}

#[tokio::test]
async fn independent_raw_budgets_gate_bundled_initial_consumption_without_refill_or_hang() {
    let ctx = context();
    let filter = filter();
    let scope = ReadScope::new(|| true, || true, |_| {});
    let users = Cell::new(0);
    let channels = Cell::new(0);
    let rows=read_with(&scope,&ctx," literal ",&filter,|provider,request,scope,initial| {
        let calls=if provider==Provider::Users {&users}else{&channels};
        let call=calls.get();calls.set(call+1);
        let super_platinum_core::slack::client::RequestBody::Json(body)=request.body else {panic!("JSON")};
        assert_eq!(body["count"],24);assert_eq!(body["query"]," literal ");
        if provider==Provider::Users {assert_eq!(body["include_profile_only_users"],true);}
        else {assert_eq!(body["check_membership"],true);assert_eq!(body["include_record_channels"],true);}
        if call==0 {assert!(body.get("marker").is_none());}
        else {assert_eq!(body["marker"],format!("{}-next",provider.key()));}
        async move {
            if let Some(gate)=initial {assert!(gate.wait().await);}
            scope.finish_injected(async move {Ok(if call==0 {
                json!({"results":vec![Value::Null;if provider==Provider::Users {20}else{0}],"next_marker":format!("{}-next",provider.key())})
            } else if provider==Provider::Users {
                json!({"results":(0..20).map(|n|json!({"id":format!("U{n:08}")})).collect::<Vec<_>>(),"next_marker":{"unused":true}})
            } else {json!({"results":[{"id":"C00000001","is_channel":true}]})})}).await
        }
    }).await.unwrap().unwrap();
    assert_eq!((users.get(), channels.get()), (2, 2));
    assert_eq!((rows.0.len(), rows.1.len()), (4, 1));

    // A participant refused before execute must release its sibling's start gate.
    let admitted = Rc::new(Cell::new(true));
    let a = admitted.clone();
    let scope = ReadScope::new(|| true, move || a.get(), |_| {});
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        read_with(
            &scope,
            &ctx,
            "literal",
            &filter,
            |provider, _, _, initial| {
                assert_eq!(provider, Provider::Users);
                admitted.set(false);
                async move {
                    assert!(!initial.unwrap().wait().await);
                    Ok(None)
                }
            },
        ),
    )
    .await
    .expect("refused participant releases gate")
    .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn own_rate_refuses_unfired_successor_but_settles_later_dispatched_rate_and_stale_budget() {
    for (later_rate, retire) in [(false, false), (true, false), (true, true)] {
        let ctx = context();
        let filter = filter();
        let current = Rc::new(Cell::new(true));
        let c = current.clone();
        let budget = Rc::new(Cell::new(0));
        let b = budget.clone();
        let observed = Rc::new(Cell::new(0));
        let o = observed.clone();
        let a = budget.clone();
        let scope = ReadScope::new(
            move || c.get(),
            move || a.get() == 0,
            move |r| {
                if let Err(Error::RateLimited {
                    retry_after_secs: Some(n),
                }) = r
                {
                    b.set(b.get().max(*n));
                    o.set(o.get() + 1);
                }
            },
        );
        let (send_users, receive_users) = tokio::sync::oneshot::channel();
        let (send_channels, receive_channels) = tokio::sync::oneshot::channel();
        let user = RefCell::new(Some(receive_users));
        let channel = RefCell::new(Some(receive_channels));
        let sent = Cell::new(0);
        let mut work = Box::pin(read_with(
            &scope,
            &ctx,
            "literal",
            &filter,
            |provider, _, scope, _| {
                assert!(scope.admitted());
                sent.set(sent.get() + 1);
                let receiver = if provider == Provider::Users {
                    user.borrow_mut().take().unwrap()
                } else {
                    channel.borrow_mut().take().unwrap()
                };
                async move {
                    scope
                        .finish_injected(async { receiver.await.unwrap() })
                        .await
                }
            },
        ));
        pending(work.as_mut()).await;
        assert_eq!(sent.get(), 2);
        if retire {
            current.set(false);
        }
        send_users.send(Err(rate(12))).unwrap();
        pending(work.as_mut()).await;
        assert_eq!(budget.get(), 12);
        send_channels
            .send(if later_rate {
                Err(rate(37))
            } else {
                Ok(json!({"results":[],"next_marker":"must-not-send"}))
            })
            .unwrap();
        let result = work.await;
        assert_eq!(sent.get(), 2);
        assert_eq!(budget.get(), if later_rate { 37 } else { 12 });
        assert_eq!(observed.get(), if later_rate { 2 } else { 1 });
        if retire {
            assert!(result.unwrap().is_none());
        } else {
            assert!(
                matches!(result,Err(Error::RateLimited{retry_after_secs:Some(n)}) if n==if later_rate {37}else{12})
            );
        }
    }
}

use super::super::{
    exchange::{self, Read},
    test_support as support,
};
use super::*;
use crate::bootstrap::test_support::pending;
use std::{cell::Cell, rc::Rc};

#[tokio::test]
async fn combined_initial_has_one_observer_original_error_cache_and_releases_lane_before_successors()
 {
    for (failure, retire, ready) in [
        (false, false, false),
        (true, false, false),
        (true, true, false),
        (true, false, true),
    ] {
        let (h, ticket, _, revision, original) = support::setup();
        let mut state = h.state;
        let observed = Rc::new(Cell::new(0));
        let count = observed.clone();
        let c = original.clone();
        let a = original.clone();
        let o = original.clone();
        let scope = ReadScope::new(
            move || c.current(),
            move || a.admitted(),
            move |result| {
                count.set(count.get() + 1);
                o.observe_attempt(result);
            },
        );
        let Read::Registered(exchange) =
            exchange::injected(state, &ticket, &scope, support::register).await
        else {
            panic!("initial exchange")
        };
        let bundle = Rc::new(Bundle::new(exchange, scope.clone()));
        let filter = conversations::Filter {
            spec: super_platinum_core::block_actions::form::ConversationFilter {
                include: Some(vec!["im".into(), "public".into()]),
                ..Default::default()
            },
            ..Default::default()
        };
        let calls_users = Cell::new(0);
        let calls_channels = Cell::new(0);
        let response = if failure {
            Err(Error::RateLimited {
                retry_after_secs: Some(37),
            })
        } else {
            Ok(
                json!({"users":{"results":[{"id":"U00000011"}]},"channels":{"results":[{"id":"C00000011","is_channel":true}],"next_marker":"channel-next"}}),
            )
        };
        if ready {
            // Without the initial barrier this429 would cool admission before
            // the other collector reaches its execute boundary.
            support::deliver(state, response.clone());
        }
        let execution_state = state;
        let mut work = Box::pin(directory::fan_in::read_with(
            &scope,
            &ticket.owner.ctx,
            "literal",
            &filter,
            |provider, request, scope, initial| {
                if provider == Provider::Users {
                    calls_users.set(calls_users.get() + 1);
                } else {
                    calls_channels.set(calls_channels.get() + 1);
                }
                let bundle = bundle.clone();
                let ticket = ticket.clone();
                async move {
                    if let Some(gate) = initial {
                        return bundle.family(provider, gate).await;
                    }
                    assert_eq!(provider, Provider::Channels);
                    let super_platinum_core::slack::client::RequestBody::Json(body) = request.body
                    else {
                        panic!("JSON")
                    };
                    assert_eq!(body["marker"], "channel-next");
                    let Read::Registered(mut exchange) =
                        exchange::injected(execution_state, &ticket, &scope, support::register)
                            .await
                    else {
                        panic!("successor exchange")
                    };
                    scope
                        .finish_injected(async {
                            let value = exchange.receive().await?;
                            value
                                .get("channels")
                                .cloned()
                                .ok_or_else(|| Error::Api("conversation_search_invalid".into()))
                        })
                        .await
                }
            },
        ));
        if !ready {
            pending(work.as_mut()).await;
            assert_eq!((calls_users.get(), calls_channels.get()), (1, 1));
            assert_eq!(observed.get(), 0);
        }
        if retire {
            state
                .write()
                .app_home
                .channels
                .picker
                .as_mut()
                .unwrap()
                .query_revision = revision + 100;
        }
        if !ready {
            support::deliver(state, response);
        }
        if !failure {
            pending(work.as_mut()).await;
            assert_eq!(observed.get(), 1);
            assert_eq!(
                state
                    .peek()
                    .app_home
                    .channels
                    .fixture
                    .as_ref()
                    .unwrap()
                    .requests
                    .len(),
                2
            );
            support::deliver(
                state,
                Ok(json!({"channels":{"results":[{"id":"C00000012","is_channel":true}]}})),
            );
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), work)
            .await
            .expect("no lane reacquisition deadlock");
        let budget = directory::read_scope::test_support::deadline(
            state,
            ticket.owner.ctx.epoch,
            &ticket.owner.ctx.session.team_id,
        );
        assert_eq!(
            (calls_users.get(), calls_channels.get()),
            (1, if failure { 1 } else { 2 })
        );
        if failure {
            assert_eq!(observed.get(), 1);
            if retire {
                assert!(result.unwrap().is_none());
                assert!(budget.is_none());
            } else {
                assert!(matches!(
                    result,
                    Err(Error::RateLimited {
                        retry_after_secs: Some(37)
                    })
                ));
                assert!(budget.is_some());
            }
            assert!(matches!(
                bundle.settlement.lock().await.result.as_ref(),
                Some(Err(Error::RateLimited {
                    retry_after_secs: Some(37)
                }))
            ));
        } else {
            let (users, channels) = result.unwrap().unwrap();
            assert_eq!((users.len(), channels.len()), (1, 2));
            assert_eq!(observed.get(), 2);
            assert!(budget.is_none());
        }
    }
}

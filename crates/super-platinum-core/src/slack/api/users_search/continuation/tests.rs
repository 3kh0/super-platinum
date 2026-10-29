use super::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::ready,
};

fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    }
}
fn records(start: usize, count: usize) -> Vec<Value> {
    (start..start + count)
        .map(|i| json!({"id":format!("U{i}")}))
        .collect()
}

#[tokio::test]
async fn continuation_keeps_count_order_empty_pages_and_final_batch_compatibility() {
    let pages = RefCell::new(VecDeque::from([
        json!({"ok":true,"results":records(0,2),"failed_ids":vec!["missing";40],"next_marker":" marker one "}),
        json!({"ok":true,"results":[],"next_marker":"marker-two"}),
        json!({"ok":true,"results":records(2,30),"next_marker":{"unused":"at raw limit"}}),
    ]));
    let calls = Cell::new(0);
    let result = fetch_users_search_continued_with(
        &SlackClient::default(),
        &session(),
        "name@example.test".into(),
        |request| {
            assert!(
                url::Url::parse(&request.url)
                    .unwrap()
                    .path()
                    .ends_with("/users/search")
            );
            let crate::slack::client::RequestBody::Json(body) = request.body else {
                panic!("JSON required")
            };
            assert_eq!(body["query"], "name@example.test");
            assert_eq!(body["count"], 30);
            for key in [
                "include_profile_only_users",
                "enable_workspace_ranking",
                "search_email",
            ] {
                assert_eq!(body[key], true);
            }
            assert_eq!(body["fuzz"], 1);
            assert!(
                body.get("page").is_none()
                    && body.get("cursor").is_none()
                    && body.get("sort").is_none()
            );
            match calls.get() {
                0 => assert!(body.get("marker").is_none()),
                1 => assert_eq!(body["marker"], " marker one "),
                2 => assert_eq!(body["marker"], "marker-two"),
                _ => panic!("no fourth request"),
            }
            calls.set(calls.get() + 1);
            ready(Ok(pages.borrow_mut().pop_front().unwrap()))
        },
        || true,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(result.len(), 32);
    assert_eq!(
        result.iter().map(|u| u.id.clone()).collect::<Vec<_>>(),
        (0..32).map(|i| format!("U{i}")).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn continuation_falsey_markers_errors_and_retirement_never_publish_partial_data() {
    for marker in [Value::Null, json!(""), json!(false), json!(0)] {
        let calls = Cell::new(0);
        let result = fetch_users_search_continued_with(
            &SlackClient::default(),
            &session(),
            "name".into(),
            |_| {
                calls.set(calls.get() + 1);
                ready(Ok(
                    json!({"ok":true,"results":records(0,1),"next_marker":marker}),
                ))
            },
            || true,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(result.len(), 1);
    }
    for error_kind in 0..3 {
        let calls = Cell::new(0);
        let result = fetch_users_search_continued_with(
            &SlackClient::default(),
            &session(),
            "name".into(),
            |_| {
                let call = calls.get();
                calls.set(call + 1);
                ready(if call == 0 {
                    Ok(json!({"ok":true,"results":records(0,2),"next_marker":"second"}))
                } else {
                    match error_kind {
                        0 => Err(Error::RateLimited {
                            retry_after_secs: Some(17),
                        }),
                        1 => Ok(json!({"ok":false,"error":"invalid_auth"})),
                        _ => Ok(json!({"ok":true,"results":[],"failed_ids":false})),
                    }
                })
            },
            || true,
        )
        .await;
        assert_eq!(calls.get(), 2);
        assert!(result.is_err());
        if error_kind == 0 {
            assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(17)
                })
            ));
        }
    }
    let active = Cell::new(true);
    let calls = Cell::new(0);
    let result = fetch_users_search_continued_with(
        &SlackClient::default(),
        &session(),
        "name".into(),
        |_| {
            calls.set(calls.get() + 1);
            active.set(false);
            ready(Err(Error::Api("retired error".into())))
        },
        || active.get(),
    )
    .await
    .unwrap();
    assert!(result.is_none());
    assert_eq!(calls.get(), 1);
}

#[tokio::test]
async fn continuation_has_explicit_marker_cycle_and_request_bounds() {
    for invalid in [
        json!(true),
        json!(1),
        json!({}),
        json!([]),
        json!("x".repeat(4097)),
    ] {
        let result = fetch_users_search_continued_with(
            &SlackClient::default(),
            &session(),
            "name".into(),
            |_| ready(Ok(json!({"ok":true,"results":[],"next_marker":invalid}))),
            || true,
        )
        .await;
        assert!(result.is_err());
    }
    for repeated in [true, false] {
        let calls = Cell::new(0);
        let result=fetch_users_search_continued_with(&SlackClient::default(),&session(),"name".into(),|_| {
            let call=calls.get();calls.set(call+1);
            ready(Ok(json!({"ok":true,"results":[],"next_marker":if repeated {"same".into()} else {format!("marker{call}")}})))
        },||true).await;
        assert!(result.is_err());
        assert_eq!(calls.get(), if repeated { 2 } else { 30 });
    }
}

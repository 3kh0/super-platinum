use super::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::ready,
};

fn prepared(marker: Option<&str>) -> Result<Option<PreparedRequest>, Error> {
    let client = SlackClient::default();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "synthetic".into(),
        enterprise_id: None,
    };
    let mut body = json!({"query":"literal query","count":24,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated"});
    if let Some(marker) = marker {
        body["marker"] = json!(marker);
    }
    client
        .edge_json(&session, "users/search", body)
        .map(Some)
        .map_err(|_| Error::Api("conversation_search_unavailable".into()))
}

#[tokio::test]
async fn raw_selector_counts_invalid_rows_then_clips_terminal_overflow_before_item_decode() {
    let pages = RefCell::new(VecDeque::from([
        json!({"results":[],"next_marker":"empty"}),
        json!({"ok":"legacy-ignored","failed_ids":[1],"results":vec![Value::Null;20],"next_marker":"next"}),
        json!({"results":(0..20).map(|i|json!({"id":format!("U{i}")})).collect::<Vec<_>>(),"next_marker":{"unused":true}}),
    ]));
    let calls = Cell::new(0);
    let rows = fetch_users_selector_rows_with(
        prepared,
        |request| {
            let super::super::super::client::RequestBody::Json(body) = request.body else {
                panic!("JSON")
            };
            assert_eq!(body["count"], 24);
            assert_eq!(body["query"], "literal query");
            assert_eq!(body["filter"], "NOT deactivated");
            assert!(body.get("include_profile_only_users").is_none());
            match calls.get() {
                0 => assert!(body.get("marker").is_none()),
                1 => assert_eq!(body["marker"], "empty"),
                2 => assert_eq!(body["marker"], "next"),
                _ => panic!("refill"),
            }
            calls.set(calls.get() + 1);
            ready(Ok(Some(pages.borrow_mut().pop_front().unwrap())))
        },
        || true,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(rows.len(), 24);
    let users = rows
        .into_iter()
        .filter_map(|v| serde_json::from_value::<User>(v).ok())
        .collect::<Vec<_>>();
    assert_eq!(
        users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(),
        vec!["U0", "U1", "U2", "U3"]
    );
}

#[tokio::test]
async fn raw_selector_builder_or_executor_nonadmission_discards_accumulated_rows() {
    for refuse_builder in [false, true] {
        let built = Cell::new(0);
        let sent = Cell::new(0);
        let rows = fetch_users_selector_rows_with(
            |marker| {
                built.set(built.get() + 1);
                if refuse_builder && marker.is_some() {
                    Ok(None)
                } else {
                    prepared(marker)
                }
            },
            |_| {
                sent.set(sent.get() + 1);
                ready(Ok(if sent.get() == 1 {
                    Some(json!({"results":[{"id":"U1"}],"next_marker":"next"}))
                } else {
                    None
                }))
            },
            || true,
        )
        .await
        .unwrap();
        assert!(rows.is_none());
        assert_eq!(built.get(), 2);
        assert_eq!(sent.get(), if refuse_builder { 1 } else { 2 });
    }
}

#[tokio::test]
async fn raw_selector_current_errors_survive_but_observed_stale_rate_has_no_partial_publication() {
    for stale in [false, true] {
        let current = Cell::new(true);
        let budget = Cell::new(0);
        let calls = Cell::new(0);
        let result = fetch_users_selector_rows_with(
            prepared,
            |_| {
                calls.set(calls.get() + 1);
                ready(if calls.get() == 1 {
                    Ok(Some(json!({"results":[{"id":"U1"}],"next_marker":"next"})))
                } else {
                    budget.set(37);
                    current.set(!stale);
                    Err(Error::RateLimited {
                        retry_after_secs: Some(37),
                    })
                })
            },
            || current.get(),
        )
        .await;
        assert_eq!(budget.get(), 37);
        assert_eq!(calls.get(), 2);
        if stale {
            assert!(result.unwrap().is_none());
        } else {
            assert!(matches!(
                result,
                Err(Error::RateLimited {
                    retry_after_secs: Some(37)
                })
            ));
        }
    }
    for page in [
        json!({"results":null}),
        json!({"results":[],"next_marker":true}),
    ] {
        assert!(
            fetch_users_selector_rows_with(prepared, |_| ready(Ok(Some(page.clone()))), || true)
                .await
                .is_err()
        );
    }
}

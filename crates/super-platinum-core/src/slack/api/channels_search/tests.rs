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
fn rows(count: usize) -> Vec<Value> {
    (0..count)
        .map(|i| json!({"id":format!("C{i}"),"name":format!("lane-{i}"),"is_channel":true}))
        .collect()
}

#[tokio::test]
async fn channels_continue_exact_flags_and_raw_order_with_whole_terminal_batch() {
    let pages = RefCell::new(VecDeque::from([
        json!({"ok":true,"results":rows(2),"failed_ids":["missing"],"next_marker":" marker one "}),
        json!({"ok":true,"results":[],"next_marker":"two"}),
        json!({"ok":true,"results":rows(30),"next_marker":{"unused":true}}),
    ]));
    let calls = Cell::new(0);
    let result = fetch_channels_search_continued_with(
        &SlackClient::default(),
        &session(),
        "lane".into(),
        |request| {
            let super::super::super::client::RequestBody::Json(body) = request.body else {
                panic!("JSON")
            };
            assert_eq!(body["count"], 30);
            assert_eq!(body["query"], "lane");
            assert_eq!(body["fuzz"], 1);
            assert_eq!(body["filter"], "xws");
            assert_eq!(body["include_record_channels"], true);
            for absent in ["check_membership", "cursor", "page", "sort"] {
                assert!(body.get(absent).is_none());
            }
            match calls.get() {
                0 => assert!(body.get("marker").is_none()),
                1 => assert_eq!(body["marker"], " marker one "),
                2 => assert_eq!(body["marker"], "two"),
                _ => panic!("unexpected refill"),
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
        result
            .iter()
            .take(4)
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["C0", "C1", "C0", "C1"]
    );
    assert!(pages.borrow().is_empty());
}

#[tokio::test]
async fn channels_reject_required_bad_pages_and_errors_without_partial_success() {
    for bad in [
        json!({"ok":false,"error":"denied"}),
        json!({"ok":true}),
        json!({"ok":true,"results":{},"next_marker":"n"}),
        json!({"ok":true,"results":[],"failed_ids":[1]}),
        json!({"ok":"yes","results":[]}),
        json!({"ok":true,"results":[],"next_marker":true}),
    ] {
        let pages = RefCell::new(VecDeque::from([
            json!({"ok":true,"results":rows(1),"next_marker":"next"}),
            bad,
        ]));
        let result = fetch_channels_search_continued_with(
            &SlackClient::default(),
            &session(),
            "lane".into(),
            |_| ready(Ok(pages.borrow_mut().pop_front().unwrap())),
            || true,
        )
        .await;
        assert!(result.is_err());
        assert!(pages.borrow().is_empty());
    }
    let calls = Cell::new(0);
    let error = fetch_channels_search_continued_with(
        &SlackClient::default(),
        &session(),
        "lane".into(),
        |_| {
            calls.set(calls.get() + 1);
            ready(if calls.get() == 1 {
                Ok(json!({"ok":true,"results":rows(1),"next_marker":"next"}))
            } else {
                Err(Error::RateLimited {
                    retry_after_secs: Some(37),
                })
            })
        },
        || true,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        Error::RateLimited {
            retry_after_secs: Some(37)
        }
    ));
    assert_eq!(calls.get(), 2);
    // Legacy absent-ok shape remains decodable in the new adapter, too.
    assert_eq!(
        fetch_channels_search_continued_with(
            &SlackClient::default(),
            &session(),
            "lane".into(),
            |_| ready(Ok(json!({"results":rows(1)}))),
            || true
        )
        .await
        .unwrap()
        .unwrap()
        .len(),
        1
    );
}

#[tokio::test]
async fn channels_retire_before_error_or_successor_and_keep_endpoint_bounds() {
    for stale_error in [false, true] {
        let current = Cell::new(true);
        let calls = Cell::new(0);
        let result = fetch_channels_search_continued_with(
            &SlackClient::default(),
            &session(),
            "lane".into(),
            |_| {
                calls.set(calls.get() + 1);
                current.set(false);
                ready(if stale_error {
                    Err(Error::Api("stale".into()))
                } else {
                    Ok(json!({"ok":true,"results":rows(1),"next_marker":"must-not-dispatch"}))
                })
            },
            || current.get(),
        )
        .await
        .unwrap();
        assert!(result.is_none());
        assert_eq!(calls.get(), 1);
    }
    let calls = Cell::new(0);
    let error = fetch_channels_search_continued_with(
        &SlackClient::default(),
        &session(),
        "lane".into(),
        |_| {
            calls.set(calls.get() + 1);
            ready(Ok(json!({"ok":true,"results":[],"next_marker":"cycle"})))
        },
        || true,
    )
    .await
    .unwrap_err();
    assert_eq!(calls.get(), 2);
    assert!(
        matches!(error,Error::Transport(s) if s=="channels/search continuation did not advance")
    );
}

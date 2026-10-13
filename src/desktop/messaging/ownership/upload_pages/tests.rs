use super::*;
use serde_json::json;
use std::{cell::Cell, collections::VecDeque};
use super_platinum_core::slack::models::{Message, ResponseMetadata};
fn expected() -> UploadReadback {
    UploadReadback {
        team: "T1".into(),
        channel: "C1".into(),
        thread_ts: Some("100.000001".into()),
        reply_broadcast: true,
        sender: "U1".into(),
        file_ids: vec!["F1".into()],
    }
}
fn matching() -> Message {
    serde_json::from_value(json!({"ts":"999.000001","channel":"C1","thread_ts":"100.000001","subtype":"thread_broadcast","user":"U1","files":[{"id":"F1"}]})).unwrap()
}
fn page(messages: Vec<Message>, cursor: Option<&str>) -> HistoryPage {
    HistoryPage {
        messages,
        has_more: cursor.is_some(),
        response_metadata: cursor.map(|cursor| ResponseMetadata {
            next_cursor: Some(cursor.into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}
#[tokio::test]
async fn exact_upload_after_first_hundred_replies_uses_cursor_without_merging_unrelated_rows() {
    let expected = expected();
    let mut unrelated = matching();
    unrelated.files[0].id = Some("other".into());
    let mut wrong_sender = matching();
    wrong_sender.user = Some("U2".into());
    let mut pages = VecDeque::from([
        page(vec![unrelated; 100], Some("second")),
        page(vec![wrong_sender], Some("third")),
        page(vec![matching()], Some("not-needed")),
    ]);
    let mut cursors = vec![];
    let found = find(
        &expected,
        |cursor| {
            cursors.push(cursor);
            std::future::ready(Ok(pages.pop_front().unwrap()))
        },
        || true,
    )
    .await
    .unwrap();
    assert_eq!(
        cursors,
        vec![None, Some("second".into()), Some("third".into())]
    );
    assert_eq!(found.messages.len(), 1);
    assert!(expected.confirms(&found.messages[0]));
}
#[tokio::test]
async fn owner_change_before_or_during_a_page_cannot_confirm_or_fetch_next_page() {
    for during in [false, true] {
        let current = Cell::new(during);
        let calls = Cell::new(0);
        let result = find(
            &expected(),
            |_| {
                calls.set(calls.get() + 1);
                current.set(false);
                std::future::ready(Ok(page(vec![matching()], Some("second"))))
            },
            || current.get(),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(calls.get(), usize::from(during));
    }
}
#[tokio::test]
async fn cursor_cycles_missing_continuations_errors_and_budget_remain_unconfirmed() {
    for mode in ["cycle", "missing", "error", "budget", "end"] {
        let calls = Cell::new(0);
        let result = find(
            &expected(),
            |_| {
                let n = calls.get() + 1;
                calls.set(n);
                std::future::ready(match mode {
                    "cycle" => Ok(page(vec![], Some("same"))),
                    "missing" => Ok(HistoryPage {
                        has_more: true,
                        ..Default::default()
                    }),
                    "error" => Err(Error::RateLimited {
                        retry_after_secs: Some(30),
                    }),
                    "end" => Ok(page(vec![], None)),
                    _ => Ok(page(vec![], Some(&format!("cursor{n}")))),
                })
            },
            || true,
        )
        .await;
        assert_eq!(
            calls.get(),
            match mode {
                "cycle" => 2,
                "budget" => MAX_PAGES,
                _ => 1,
            }
        );
        if mode == "end" {
            assert!(result.unwrap().messages.is_empty());
        } else {
            assert!(result.is_err());
        }
    }
}

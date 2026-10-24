use super::*;
use serde_json::{Value, json};
fn page(value: Value) -> HistoryPage {
    serde_json::from_value(value).unwrap()
}
fn pages() -> Pages {
    Pages::new(HistoryArgs {
        channel: "C1".into(),
        oldest: Some("10.000000".into()),
        latest: Some("90.000000".into()),
        inclusive: true,
        limit: Some(100),
        ..Default::default()
    })
}
#[test]
fn short_and_empty_cursor_pages_preserve_every_original_range_argument() {
    let a=pages().advance(&page(json!({"messages":[{"ts":"80.000000"}],"has_more":false,"response_metadata":{"next_cursor":"a"}}))).unwrap();
    let b = a
        .advance(&page(
            json!({"messages":[],"has_more":false,"response_metadata":{"next_cursor":"b"}}),
        ))
        .unwrap();
    assert!(b.has_more());
    assert_eq!(b.request().cursor.as_deref(), Some("b"));
    assert_eq!(b.request().channel, "C1");
    assert_eq!(b.request().oldest.as_deref(), Some("10.000000"));
    assert_eq!(b.request().latest.as_deref(), Some("90.000000"));
    assert!(b.request().inclusive);
    assert_eq!(b.request().limit, Some(100));
    let end = b
        .advance(&page(json!({"messages":[],"has_more":false})))
        .unwrap();
    assert!(!end.has_more());
    assert!(end.request().cursor.is_none());
}
#[test]
fn invalid_pages_preserve_pre_request_cursor_and_seen_prefix() {
    let a = pages()
        .advance(&page(json!({"response_metadata":{"next_cursor":"a"}})))
        .unwrap();
    assert_eq!(
        a.advance(&page(json!({"response_metadata":{"next_cursor":"a"}})))
            .unwrap_err(),
        Invalid::RepeatedCursor
    );
    assert_eq!(a.request().cursor.as_deref(), Some("a"));
    assert_eq!(a.seen().len(), 1);
    assert_eq!(
        a.advance(&page(json!({"messages":[],"has_more":true})))
            .unwrap_err(),
        Invalid::NoProgress
    );
    assert_eq!(
        a.advance(&page(json!({"messages":[{"ts":"bad"}]})))
            .unwrap_err(),
        Invalid::Timestamp
    );
    assert_eq!(
        a.advance(&page(json!({"messages":[{"ts":"99.000000"}]})))
            .unwrap_err(),
        Invalid::Range
    );
    assert_eq!(
        a.advance(&page(
            json!({"messages":vec![json!({"ts":"40.000000"});101]})
        ))
        .unwrap_err(),
        Invalid::PageSize
    );
}
#[test]
fn time_fallback_is_exclusive_and_must_advance() {
    let a = pages()
        .advance(&page(
            json!({"messages":[{"ts":"70.000000"},{"ts":"60.000000"}],"has_more":true}),
        ))
        .unwrap();
    assert_eq!(a.request().latest.as_deref(), Some("60.000000"));
    assert!(!a.request().inclusive);
    assert_eq!(a.request().oldest.as_deref(), Some("10.000000"));
    assert!(
        a.advance(&page(
            json!({"messages":[{"ts":"60.000000"}],"has_more":true})
        ))
        .is_err()
    );
}
#[test]
fn expired_cursor_restarts_original_range_once_without_skipping_accepted_prefix() {
    let a = pages()
        .advance(&page(
            json!({"messages":[{"ts":"30.000000"}],"response_metadata":{"next_cursor":"expired"}}),
        ))
        .unwrap();
    let reset = a.expired().unwrap();
    assert!(reset.request().cursor.is_none());
    assert_eq!(reset.request().latest.as_deref(), Some("90.000000"));
    assert_eq!(reset.request().oldest.as_deref(), Some("10.000000"));
    assert!(reset.request().inclusive);
    assert!(reset.restarted());
    let b = reset
        .advance(&page(
            json!({"messages":[{"ts":"80.000000"}],"response_metadata":{"next_cursor":"new"}}),
        ))
        .unwrap();
    assert_eq!(b.expired().unwrap_err(), Invalid::ExpiredAgain);
    assert_eq!(b.request().cursor.as_deref(), Some("new"));
}
#[test]
fn resume_keeps_seen_cursors_across_chunks_and_rejects_cycle() {
    let mut p = pages();
    for i in 0..8 {
        p = p
            .advance(&page(
                json!({"response_metadata":{"next_cursor":format!("p{i}")}}),
            ))
            .unwrap();
    }
    assert_eq!(p.seen().len(), 8);
    assert_eq!(
        p.advance(&page(json!({"response_metadata":{"next_cursor":"p0"}})))
            .unwrap_err(),
        Invalid::RepeatedCursor
    );
}

#[test]
fn inclusive_boundary_only_page_advances_once_to_exclusive() {
    let p = pages();
    let a = p
        .advance(&page(
            json!({"messages":[{"ts":"90.000000"}],"has_more":true}),
        ))
        .unwrap();
    assert_eq!(a.request().latest.as_deref(), Some("90.000000"));
    assert!(!a.request().inclusive);
    assert!(
        a.advance(&page(
            json!({"messages":[{"ts":"90.000000"}],"has_more":true})
        ))
        .is_err()
    );
}

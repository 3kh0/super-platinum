use super::*;
use serde_json::json;
fn page(number: u32, pages: u32, total: u64) -> Page {
    Page::decode(json!({"ok":true,"items":[{"id":"UA","iid":format!("{number}-a")},{"id":"UA","iid":format!("{number}-b")}],"pagination":{"page":number,"page_count":pages,"total_count":total,"last":number*2},"request_id":format!("response-{number}")}),number).unwrap()
}
fn fetch(book: &mut Book, number: u32) -> Ticket {
    let Plan::Fetch(ticket) = book.select(number) else {
        panic!("uncached explicit page")
    };
    ticket
}

#[test]
fn numbered_journey_preserves_page_arrays_and_first_count_and_retires_old_activations_on_cached_return()
 {
    let mut book = Book::default();
    let first = fetch(&mut book, 1);
    assert!(book.commit(&first, page(1, 2, 40)));
    let original = book.active().unwrap().clone();
    assert_eq!(original.page.hits().len(), 2);
    assert!(book.activation_current(&original));
    let second = fetch(&mut book, 2);
    assert!(book.commit(&second, page(2, 3, 41)));
    assert_eq!(book.active().unwrap().page.number(), 2);
    assert_eq!(
        book.page_count(),
        2,
        "source preserves original count on append"
    );
    assert_eq!(book.total(), 41);
    assert_eq!(book.last(), Some(&json!(4)));
    assert!(!book.activation_current(&original));
    assert!(matches!(book.select(1), Plan::Cached));
    let returned = book.active().unwrap();
    assert!(Arc::ptr_eq(&returned.page, &original.page));
    assert!(
        !book.activation_current(&original),
        "cached data does not revive queued old row activation"
    );
    assert!(book.activation_current(returned));
    assert_eq!(returned.page.request_id(), Some("response-1"));
    assert!(matches!(book.select(3), Plan::Refused));
    assert!(book.pending().is_none());
}

#[test]
fn page_failure_and_superseded_completions_preserve_loaded_page_and_exact_retry_target() {
    let mut book = Book::default();
    let first = fetch(&mut book, 1);
    assert!(book.commit(&first, page(1, 2, 40)));
    let loaded = book.active().unwrap().clone();
    let old = fetch(&mut book, 2);
    assert!(book.fail(&old));
    assert_eq!(book.failed_page(), Some(2));
    assert!(
        !book.activation_current(&loaded),
        "failed navigation cannot revive prior selection"
    );
    assert!(
        Arc::ptr_eq(&book.active().unwrap().page, &loaded.page),
        "loaded data remains visible"
    );
    let failure = book.failure().unwrap().clone();
    assert!(book.failure_current(&failure));
    assert_eq!(book.total(), 40);
    let Plan::Fetch(retry) = book.retry(&failure) else {
        panic!("owned exact failed page retry")
    };
    assert!(!book.failure_current(&failure));
    assert!(matches!(book.retry(&failure), Plan::Refused));
    assert!(!book.commit(&old, page(2, 2, 99)));
    assert!(book.accepts(&retry));
    assert!(matches!(book.select(1), Plan::Cached));
    assert!(!book.commit(&retry, page(2, 2, 99)));
    assert_eq!(book.total(), 40);
    assert_eq!(book.active().unwrap().page.number(), 1);
    assert!(book.failed_page().is_none());
    let mut replacement = Book::default();
    let current = fetch(&mut replacement, 1);
    assert!(!replacement.commit(&first, page(1, 2, 99)));
    assert!(replacement.accepts(&current));
}

#[test]
fn module_session_getter_order_cold_rotation_and_shared_touch_thresholds_match_retained_source() {
    let mut sessions = Sessions::default();
    let mut serial = 0u128;
    let mut next = || {
        serial += 1;
        Uuid::from_u128(serial)
    };
    let cold = sessions.enter(1_000, &mut next);
    assert_eq!(
        (cold.query, cold.search, cold.latest_api),
        (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(4))
    );
    let boundary = sessions.enter(121_000, &mut next);
    assert_eq!(boundary.query, Uuid::from_u128(3));
    assert_eq!(boundary.search, cold.search);
    let expired = sessions.enter(241_001, &mut next);
    assert_ne!(expired.query, boundary.query);
    assert_eq!(expired.search, cold.search);
    let long_idle = sessions.enter(900_000, &mut next);
    assert_ne!(long_idle.query, expired.query);
    assert_eq!(
        long_idle.search, cold.search,
        "query getter touches shared event time before search expiry check"
    );
    assert_ne!(long_idle.latest_api, long_idle.query);
}

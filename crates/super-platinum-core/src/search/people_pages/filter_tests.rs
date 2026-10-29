use super::*;
use std::cell::{Cell, RefCell};
#[test]
fn explicit_filter_query_producer_and_first_all_order_preserve_cold_search_rotation_and_normal_module_getters()
 {
    let mut s = Sessions::default();
    let events = RefCell::new(Vec::new());
    let serial = Cell::new(0u128);
    let next = || {
        events.borrow_mut().push("id");
        serial.set(serial.get() + 1);
        Uuid::from_u128(serial.get())
    };
    let time = Cell::new(1000u64);
    let clock = || {
        events.borrow_mut().push("clock");
        time.set(time.get() + 1);
        time.get()
    };
    let chosen = s.start_query(clock, next);
    assert_eq!(*events.borrow(), ["id", "clock", "clock"]);
    assert_eq!(chosen, Uuid::from_u128(1));
    assert_eq!(
        (s.last_event_ms, s.last_query_start_ms),
        (Some(1001), Some(1002))
    );
    events.borrow_mut().clear();
    let first = s.enter_first_all(1003, next);
    assert_eq!(
        *events.borrow(),
        ["id", "id", "id"],
        "latest then missing search and its query rotation"
    );
    assert_eq!(
        (first.latest_api, first.query, first.search),
        (Uuid::from_u128(2), chosen, Uuid::from_u128(3))
    );
    assert_eq!(
        s.query,
        Some(Uuid::from_u128(4)),
        "captured query must not be rebound to search getter's later stored ID"
    );
    let page2 = s.enter(1004, next);
    assert_eq!(
        (page2.query, page2.search, page2.latest_api),
        (Uuid::from_u128(4), first.search, Uuid::from_u128(5))
    );
    events.borrow_mut().clear();
    let fresh = s.start_query(clock, next);
    let warm = s.enter_first_all(1005, next);
    assert_eq!(*events.borrow(), ["id", "clock", "clock", "id"]);
    assert_eq!(
        (warm.query, warm.search, warm.latest_api),
        (fresh, first.search, Uuid::from_u128(7))
    );
    let sorted = s.enter(1006, next);
    assert_eq!(
        (sorted.query, sorted.search, sorted.latest_api),
        (fresh, first.search, Uuid::from_u128(8)),
        "sort/paging keep existing normal order and search session"
    );
}

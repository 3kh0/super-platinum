use super::*;
use serde_json::json;
fn page(n: u32, bound: u32, total: u64, items: serde_json::Value) -> Page {
    Page::decode(json!({"ok":true,"items":items,"pagination":{"page":n,"page_count":bound,"total_count":total,"last":n*20}}),n).unwrap()
}
fn ticket(book: &mut Book, n: u32) -> Ticket {
    let Plan::Fetch(t) = book.select(n) else {
        panic!("fetch {n}")
    };
    t
}
#[test]
fn files_selected_pages_preserve_order_empty_cache_initial_bound_and_failed_target_without_reviving_actions()
 {
    let mut b = Book::default();
    let first = ticket(&mut b, 1);
    assert!(b.commit(
        &first,
        page(1, 4, 61, json!([{"id":"FZ"},{"id":"FA"},{"id":"FZ"}]))
    ));
    let old = b.active().unwrap().clone();
    assert_eq!(
        old.page.hits().iter().map(|h| h.id()).collect::<Vec<_>>(),
        ["FZ", "FA", "FZ"]
    );
    let second = ticket(&mut b, 2);
    assert!(!b.activation_current(&old));
    assert_eq!(b.active().unwrap().page.number(), 1);
    assert!(b.commit(&second, page(2, 8, 0, json!([]))));
    assert_eq!(b.page_count(), 4);
    assert_eq!(b.total(), 0);
    assert!(matches!(b.select(1), Plan::Cached));
    assert!(!b.activation_current(&old));
    assert!(matches!(b.select(2), Plan::Cached));
    assert!(b.active().unwrap().page.hits().is_empty());
    let failed = ticket(&mut b, 4);
    assert!(b.fail(&failed));
    assert_eq!(b.active().unwrap().page.number(), 2);
    assert_eq!(b.failed_page(), Some(4));
    let Plan::Fetch(retry) = b.retry(&failed) else {
        panic!("retry exact4")
    };
    assert_eq!(retry.page, 4);
    assert!(!b.fail(&failed));
    assert!(!b.commit(&failed, page(4, 4, 61, json!([]))));
    assert!(b.commit(&retry, page(4, 3, 61, json!([]))));
    assert_eq!(
        b.page_count(),
        4,
        "later shrinking count cannot replace initial authority"
    );
    assert!(matches!(b.select(5), Plan::Refused));
}

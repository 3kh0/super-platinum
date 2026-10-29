use super::*;
use crate::slack::models::SearchPagination;

fn metadata(page: u32) -> SearchPagination {
    SearchPagination {
        page: Some(page),
        page_count: Some(7),
        total_count: Some(121),
        ..Default::default()
    }
}
fn loaded() -> SearchPager {
    let mut pager = SearchPager::default();
    let first = pager.begin(4, "T", "release", false).unwrap();
    assert!(pager.complete(&first, Some(&metadata(1))));
    pager
}

#[test]
fn exact_message_target_can_move_back_and_retires_old_work() {
    let mut pager = loaded();
    let fourth = pager.begin_message_page(4, "T", "release", 4).unwrap();
    assert_eq!(fourth.page, 4);
    assert!(pager.complete(&fourth, Some(&metadata(4))));
    let first = pager.begin_message_page(4, "T", "release", 1).unwrap();
    assert_eq!(first.page, 1);
    assert!(!pager.complete(&fourth, Some(&metadata(4))));
    assert!(pager.accepts(&first));
    assert!(pager.complete(&first, Some(&metadata(1))));
    assert_eq!((pager.page, pager.page_count, pager.total), (1, 7, 121));
    assert!(!pager.fail(&fourth));
}

#[test]
fn failed_arbitrary_target_retries_that_target_without_losing_loaded_page() {
    let mut pager = loaded();
    let fourth = pager.begin_message_page(4, "T", "release", 4).unwrap();
    assert!(pager.fail(&fourth));
    assert_eq!((pager.page, pager.total), (1, 121));
    let retry = pager.begin_message_page(4, "T", "release", 4).unwrap();
    assert_ne!(retry, fourth);
    assert_eq!(retry.page, 4);
    assert!(!pager.fail(&fourth));
    assert!(pager.accepts(&retry));
    assert!(!pager.complete(&retry, Some(&metadata(2))));
    assert_eq!(pager.page, 1);
    assert!(pager.failed && !pager.loading);
    let retry = pager.begin_message_page(4, "T", "release", 4).unwrap();
    assert!(pager.complete(&retry, Some(&metadata(4))));
}

#[test]
fn rejected_message_page_admission_preserves_the_original_cursor() {
    let original = loaded();
    for (epoch, team, query, page) in [
        (5, "T", "release", 2),
        (4, "OTHER", "release", 2),
        (4, "T", "other", 2),
        (4, "T", " ", 2),
        (4, "T", "release", 0),
        (4, "T", "release", 8),
    ] {
        let mut pager = original.clone();
        assert!(pager.begin_message_page(epoch, team, query, page).is_none());
        assert_eq!(format!("{pager:?}"), format!("{original:?}"));
    }
    let mut pager = original;
    let pending = pager.begin_message_page(4, "T", "release", 3).unwrap();
    assert!(pager.begin_message_page(4, "T", "release", 2).is_none());
    assert!(pager.accepts(&pending));
    for kind in [SearchKind::Files, SearchKind::People] {
        let mut pager = loaded();
        pager.kind = kind;
        assert!(pager.begin_message_page(4, "T", "release", 2).is_none());
        assert!(!pager.loading && !pager.failed);
    }
    assert!(
        SearchPager::default()
            .begin_message_page(4, "T", "release", 1)
            .is_none()
    );
}

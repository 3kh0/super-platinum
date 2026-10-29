use super::*;
fn page(urls: &[&str], number: u32) -> LinksPage {
    LinksPage {
        items: urls
            .iter()
            .map(|url| ChannelLink {
                url: (*url).into(),
                timestamp: "1.0".into(),
                ..Default::default()
            })
            .collect(),
        pagination: Some(SearchPagination {
            page: Some(number),
            page_count: Some(5),
            total_count: Some(83),
            ..Default::default()
        }),
    }
}
fn loaded() -> LinksState {
    let mut state = LinksState::default();
    state.ensure_scope(1, "T1", "C1");
    let generation = state.begin("  topic  ".into(), 1);
    assert!(state.apply(generation, page(&["https://example.test/one"], 1)));
    state
}
#[test]
fn failed_later_page_retains_rows_and_committed_pagination_until_exact_retry_succeeds() {
    let mut state = loaded();
    let pending = state.begin(" topic ".into(), 2);
    assert_eq!((state.page, state.page_count, state.total), (1, 5, 83));
    assert!(state.loaded && state.loading);
    assert_eq!(state.items[0].url, "https://example.test/one");
    assert!(state.fail(pending, "Offline"));
    assert!(!state.apply(pending, page(&["https://example.test/late"], 2)));
    assert_eq!(state.resume_page(), 2);
    assert_eq!((state.page, state.page_count, state.total), (1, 5, 83));
    let retry = state.begin(state.query.clone(), state.resume_page());
    assert!(state.apply(retry, page(&["https://example.test/two"], 2)));
    assert_eq!(state.page, 2);
    assert_eq!(state.requested_page, None);
    assert_eq!(state.items[0].url, "https://example.test/two");
    assert!(!state.apply(retry, page(&["https://example.test/duplicate-terminal"], 2)));
}
#[test]
fn same_query_refresh_and_cancellation_keep_loaded_rows_and_old_cleanup_cannot_clear_successor() {
    let mut state = loaded();
    let refresh = state.begin(state.query.clone(), 1);
    assert!(state.cancel(refresh));
    assert_eq!(state.items.len(), 1);
    assert_eq!(state.resume_page(), 1);
    assert!(!state.loading);
    let newer = state.begin(state.query.clone(), 2);
    assert!(!state.cancel(refresh));
    assert!(!state.fail(refresh, "late"));
    assert!(state.accepts(newer));
    assert!(state.fail(newer, "offline"));
    let changed = state.begin("different".into(), 1);
    assert!(state.items.is_empty() && !state.loaded);
    assert_eq!((state.page, state.page_count, state.total), (1, 0, 0));
    assert!(!state.apply(newer, page(&["https://example.test/old"], 2)));
    state.ensure_scope(1, "T1", "C2");
    let current = state.begin("different".into(), 1);
    assert!(!state.cancel(changed));
    assert!(!state.apply(changed, page(&["https://example.test/wrong-channel"], 1)));
    assert!(state.accepts(current));
}
#[test]
fn recovered_page_keeps_existing_dedup_cap_and_accepted_target_fallback_without_invented_totals() {
    let mut state = loaded();
    let interrupted = state.begin(state.query.clone(), 3);
    assert!(state.fail(interrupted, "offline"));
    let retry = state.begin(state.query.clone(), state.resume_page());
    let mut response = LinksPage {
        items: (0..25)
            .map(|i| ChannelLink {
                url: format!("https://example.test/{i}"),
                timestamp: "1.0".into(),
                ..Default::default()
            })
            .collect(),
        pagination: None,
    };
    response.items.insert(1, response.items[0].clone());
    response.items.insert(0, ChannelLink::default());
    assert!(state.apply(retry, response));
    assert_eq!(state.page, 3);
    assert_eq!(state.page_count, 1);
    assert_eq!(state.total, 27);
    assert_eq!(state.items.len(), 20);
    assert_eq!(state.items[0].url, "https://example.test/0");
    assert_eq!(state.items[19].url, "https://example.test/19");
}

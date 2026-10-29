use super::*;
fn page(ids: &[&str], number: u32) -> FilesPage {
    FilesPage {
        items: ids
            .iter()
            .map(|id| File {
                id: Some((*id).into()),
                title: Some((*id).into()),
                ..Default::default()
            })
            .collect(),
        pagination: Some(SearchPagination {
            page: Some(number),
            page_count: Some(4),
            total_count: Some(150),
            ..Default::default()
        }),
    }
}
fn loaded() -> FilesState {
    let mut state = FilesState::default();
    state.ensure_scope(1, "T1");
    let generation = state.begin(
        FilesQuery {
            text: "  report  ".into(),
            ..Default::default()
        },
        1,
    );
    assert!(state.apply(generation, page(&["F1", "F_KEEP"], 1)));
    state
}
#[test]
fn failed_next_page_retains_committed_results_and_retries_exact_target_with_normalized_query() {
    let mut state = loaded();
    let query = state.query.clone();
    assert_eq!(query.text, "report");
    assert_eq!(query.sort, FileSort::Relevant);
    let pending = state.begin(
        FilesQuery {
            text: " report ".into(),
            sort: FileSort::Viewed,
            ..query.clone()
        },
        2,
    );
    assert_eq!((state.page, state.page_count, state.total), (1, 4, 150));
    assert!(state.loaded && state.loading);
    assert_eq!(
        state
            .items
            .iter()
            .map(|f| f.id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["F1", "F_KEEP"]
    );
    assert_eq!(state.requested_page, Some(2));
    assert!(state.fail(pending, "Offline"));
    assert!(!state.apply(pending, page(&["LATE"], 2)));
    assert_eq!(state.resume_page(), 2);
    assert_eq!((state.page, state.page_count, state.total), (1, 4, 150));
    let retry = state.begin(state.query.clone(), state.resume_page());
    assert!(state.apply(retry, page(&["F2"], 2)));
    assert_eq!(state.page, 2);
    assert_eq!(state.requested_page, None);
    assert_eq!(state.items[0].id.as_deref(), Some("F2"));
    assert!(!state.apply(retry, page(&["DUPLICATE_TERMINAL"], 2)));
}
#[test]
fn cancellation_retains_target_and_stale_terminal_cannot_clear_successor_or_publish_late() {
    let mut state = loaded();
    let old = state.begin(state.query.clone(), 2);
    assert!(state.cancel(old));
    assert!(!state.loading);
    assert_eq!(state.resume_page(), 2);
    assert_eq!(state.page, 1);
    let next = state.begin(state.query.clone(), 3);
    assert!(!state.cancel(old));
    assert!(!state.fail(old, "late"));
    assert!(state.loading && state.accepts(next));
    assert!(!state.apply(old, page(&["OLD"], 2)));
    assert!(state.apply(
        next,
        FilesPage {
            items: page(&["THREE"], 3).items,
            pagination: None
        }
    ));
    assert_eq!(
        state.page, 3,
        "missing response page falls back to accepted requested target"
    );
    assert_eq!(state.page_count, 1, "do not invent missing server totals");
}
#[test]
fn deletion_during_pending_page_survives_failure_retry_and_query_change() {
    let mut state = loaded();
    let pending = state.begin(state.query.clone(), 2);
    state.remove("F1");
    assert_eq!(state.total, 149);
    assert_eq!(state.items[0].id.as_deref(), Some("F_KEEP"));
    assert!(state.fail(pending, "Offline"));
    assert_eq!(state.items.len(), 1);
    let retry = state.begin(state.query.clone(), state.resume_page());
    assert!(state.apply(retry, page(&["F1", "F2", "F2"], 2)));
    assert_eq!(state.total, 149);
    assert_eq!(state.items.len(), 1);
    assert_eq!(state.items[0].id.as_deref(), Some("F2"));
    let changed = state.begin(
        FilesQuery {
            text: "other".into(),
            ..Default::default()
        },
        1,
    );
    assert!(state.items.is_empty());
    assert!(state.deleted.contains("F1"));
    assert!(state.apply(changed, page(&["F1", "F3"], 1)));
    assert_eq!(state.items.len(), 1);
    assert_eq!(state.items[0].id.as_deref(), Some("F3"));
}
#[test]
fn changed_filter_and_scope_clear_old_identity_and_reject_all_prior_terminals() {
    let mut state = loaded();
    let pending = state.begin(state.query.clone(), 2);
    let changed = state.begin(
        FilesQuery {
            kind: FileKind::Images,
            owner: FileOwner::Yours,
            text: "  \"two  words\"  ".into(),
            ..Default::default()
        },
        1,
    );
    assert_eq!(state.query.text, "\"two  words\"");
    assert_eq!(state.requested_page, Some(1));
    assert!(state.items.is_empty() && !state.loaded);
    assert_eq!((state.page, state.page_count, state.total), (1, 0, 0));
    assert!(!state.apply(pending, page(&["WRONG_QUERY"], 2)));
    state.ensure_scope(2, "T2");
    let current = state.begin(Default::default(), 1);
    assert!(!state.fail(changed, "old account"));
    assert!(!state.apply(changed, page(&["OLD_ACCOUNT"], 1)));
    assert!(state.accepts(current));
    assert_eq!(state.requested_page, Some(1));
    assert!(state.apply(current, page(&["CURRENT"], 1)));
}

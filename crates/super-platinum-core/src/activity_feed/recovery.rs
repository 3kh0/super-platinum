use super::{
    tests::{feed, item, page},
    *,
};

#[test]
fn failed_refresh_retains_committed_cycle_history_and_success_starts_a_fresh_chain() {
    let mut state = feed();
    let first = state.begin(false).unwrap();
    assert!(state.apply_page(&first, page(vec![item("3.0")], Some("a"))));
    let older = state.begin(true).unwrap();
    assert!(state.apply_page(&older, page(vec![item("2.0")], Some("b"))));
    let refresh = state.begin(false).unwrap();
    assert!(state.fail(&refresh));
    assert_eq!(state.next_cursor.as_deref(), Some("b"));
    let older = state.begin(true).unwrap();
    assert!(state.apply_page(&older, page(vec![item("1.0")], Some("a"))));
    assert!(state.failed);
    assert!(state.next_cursor.is_none());
    assert!(
        state.failed_read().is_none(),
        "cycle stop is not a retryable fetch failure"
    );
    assert!(state.begin_retry().is_none());
    assert_eq!(state.items.len(), 3);

    let refresh = state.begin(false).unwrap();
    assert!(state.apply_page(&refresh, page(vec![item("4.0")], Some("a"))));
    assert!(!state.failed);
    assert_eq!(state.next_cursor.as_deref(), Some("a"));
    let older = state.begin(true).unwrap();
    assert!(state.apply_page(&older, page(vec![item("3.0")], Some("b"))));
    assert_eq!(state.next_cursor.as_deref(), Some("b"));
}

#[test]
fn failed_refresh_and_opaque_append_retry_exact_coordinates_with_fresh_admission() {
    let mut state = feed();
    let first = state.begin(false).unwrap();
    assert!(state.apply_page(&first, page(vec![item("2.0")], Some("opaque/+?=&older"))));
    state.unread_only = true;
    state.selected = Some("mention-2.0".into());
    state.mark_item_read("mention-2.0");
    let retained = serde_json::to_value(&state.items).unwrap();
    let refresh = state.begin(false).unwrap();
    assert!(state.fail(&refresh));
    assert_eq!(state.failed_read().unwrap().cursor, None);
    let retry = state.begin_retry().unwrap();
    assert_eq!(
        retry.cursor, None,
        "refresh retry never follows the retained older cursor"
    );
    assert_ne!(retry.sequence, refresh.sequence);
    assert!(!state.fail(&refresh));
    assert!(state.fail(&retry));
    assert_eq!(serde_json::to_value(&state.items).unwrap(), retained);
    assert_eq!(state.selected.as_deref(), Some("mention-2.0"));
    assert!(state.unread_only);

    let older = state.begin(true).unwrap();
    assert!(state.fail(&older));
    assert_eq!(
        state.failed_read().unwrap().cursor.as_deref(),
        Some("opaque/+?=&older")
    );
    let retry = state.begin_retry().unwrap();
    assert_eq!(retry.cursor, older.cursor);
    assert_ne!(retry.sequence, older.sequence);
    assert!(!state.fail(&older));
    assert!(state.apply_page(&retry, page(vec![item("1.0")], None)));
    assert_eq!(state.items.len(), 2);
    assert!(state.failed_read().is_none());
    assert!(!state.failed && !state.loading);
    assert!(state.begin_retry().is_none());
}

#[test]
fn request_owned_cancel_cannot_clear_replacement_or_manufacture_a_retry_failure() {
    let mut state = feed();
    let old = state.begin(false).unwrap();
    let mut replacement = feed();
    let current = replacement.begin(false).unwrap();
    assert_eq!(old.sequence, current.sequence);
    assert!(
        !replacement.cancel(&old),
        "equal coordinates are not a request lease"
    );
    assert!(!replacement.accepts(&old));
    assert!(!replacement.apply_page(&old, page(vec![item("9.0")], None)));
    assert!(!replacement.fail(&old));
    assert!(replacement.accepts(&current));
    assert!(state.cancel(&old));
    assert!(!state.loading && !state.failed);
    assert!(state.failed_read().is_none());
    assert!(!state.fail(&old));
    let next = state.begin(false).unwrap();
    assert!(!state.cancel(&old));
    assert!(state.accepts(&next));
    assert!(state.fail(&next));
    let mut loaded = feed();
    let first = loaded.begin(false).unwrap();
    loaded.apply_page(&first, page(vec![item("3.0")], Some("a")));
    let older = loaded.begin(true).unwrap();
    loaded.apply_page(&older, page(vec![item("2.0")], Some("b")));
    let retained = serde_json::to_value(&loaded.items).unwrap();
    let refresh = loaded.begin(false).unwrap();
    assert!(loaded.cancel(&refresh));
    assert!(loaded.loaded && !loaded.loading && !loaded.failed);
    assert_eq!(serde_json::to_value(&loaded.items).unwrap(), retained);
    assert_eq!(loaded.next_cursor.as_deref(), Some("b"));
    let older = loaded.begin(true).unwrap();
    loaded.apply_page(&older, page(vec![], Some("a")));
    assert!(loaded.failed && loaded.next_cursor.is_none());
    assert!(loaded.failed_read().is_none());
    state.ensure_scope(2, "T2");
    assert!(state.failed_read().is_none());
    assert!(state.begin_retry().is_none());
}

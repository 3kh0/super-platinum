use super::*;

fn core() -> CoreAppState {
    let mut core = CoreAppState::new(Default::default());
    core.active_team = Some("T".into());
    core.threads_view.ensure_scope(core.account_epoch, "T");
    core
}
fn page(root: &str, latest: &str, more: bool) -> ThreadsViewPage {
    serde_json::from_value(serde_json::json!({
        "threads":[{"root_msg":{"channel":"C","ts":root,"text":"Loaded root",
            "reply_count":1,"latest_reply":latest,"last_read":latest},
            "latest_replies":[{"ts":latest,"thread_ts":root,"text":"Loaded reply"}],
            "unread_replies":[]}],
        "has_more":more,"max_ts":"999999.0"
    }))
    .unwrap()
}

#[test]
fn failed_refresh_and_older_retry_keep_exact_coordinates_and_loaded_selection_filter() {
    let mut core = core();
    let first = core.threads_view.begin(false).unwrap();
    assert!(core.apply_thread_feed_page("T", &first, page("10.0", "100.0", true), &HashMap::new()));
    core.threads_view.selected = Some(("C".into(), "10.0".into()));
    core.threads_view.vip_only = true;
    let refresh = core.threads_view.begin(false).unwrap();
    core.threads_view.fail(&refresh);
    assert!(core.threads_view.failed_read().unwrap().cursor.is_none());
    assert_eq!(core.threads_view.next_cursor.as_deref(), Some("100.0"));
    assert_eq!(
        core.threads_view.items[0].root_ts().map(String::as_str),
        Some("10.0")
    );
    assert_eq!(
        core.threads_view.selected,
        Some(("C".into(), "10.0".into()))
    );
    assert!(core.threads_view.vip_only);
    let retry = core.threads_view.begin_retry().unwrap();
    assert!(retry.cursor.is_none());
    assert_ne!(retry.sequence, refresh.sequence);
    core.threads_view.fail(&refresh);
    assert!(core.threads_view.accepts(&retry));
    assert!(core.apply_thread_feed_page("T", &retry, page("20.0", "200.0", true), &HashMap::new()));
    assert_eq!(core.threads_view.items.len(), 1);
    assert_eq!(core.threads_view.next_cursor.as_deref(), Some("200.0"));
    let older = core.threads_view.begin(true).unwrap();
    core.threads_view.fail(&older);
    assert_eq!(
        core.threads_view.failed_read().unwrap().cursor.as_deref(),
        Some("200.0")
    );
    let retry = core.threads_view.begin_retry().unwrap();
    assert_eq!(retry.cursor, older.cursor);
    assert_ne!(retry.sequence, older.sequence);
    assert!(core.apply_thread_feed_page(
        "T",
        &retry,
        page("15.0", "150.0", false),
        &HashMap::new()
    ));
    assert_eq!(core.threads_view.items.len(), 2);
    assert!(core.threads_view.failed_read().is_none());
    assert!(!core.threads_view.loading && !core.threads_view.failed);
    assert!(core.threads_view.vip_only);
}

#[test]
fn public_request_shape_remains_constructible_but_only_admitted_coordinate_can_settle() {
    let mut core = core();
    let request = core.threads_view.begin(false).unwrap();
    let changed = FeedRequest {
        sequence: request.sequence,
        cursor: Some("not-admitted".into()),
    };
    assert!(!core.threads_view.accepts(&changed));
    core.threads_view.fail(&changed);
    assert!(!core.threads_view.cancel(&changed));
    assert!(!core.apply_thread_feed_page(
        "T",
        &changed,
        page("90.0", "900.0", false),
        &HashMap::new()
    ));
    assert!(core.threads_view.accepts(&request));
    assert!(core.apply_thread_feed_page(
        "T",
        &request,
        page("10.0", "100.0", true),
        &HashMap::new()
    ));
    core.threads_view.fail(&request);
    assert!(
        !core.threads_view.failed,
        "late failure must not poison accepted results"
    );
    assert!(!core.threads_view.cancel(&request));
    let successor = core.threads_view.begin(false).unwrap();
    assert!(!core.threads_view.cancel(&request));
    assert!(core.threads_view.accepts(&successor));
    assert!(core.threads_view.cancel(&successor));
    assert!(!core.threads_view.loading && !core.threads_view.failed);
    assert_eq!(core.threads_view.items.len(), 1);
    assert!(core.threads_view.failed_read().is_none());
    let after_cancel = core.threads_view.begin(false).unwrap();
    assert!(!core.threads_view.cancel(&successor));
    assert!(core.threads_view.accepts(&after_cancel));
}

#[test]
fn cursor_cycle_is_not_a_failed_http_retry_and_scope_replacement_clears_failed_intent() {
    let mut core = core();
    let first = core.threads_view.begin(false).unwrap();
    core.apply_thread_feed_page("T", &first, page("10.0", "100.0", true), &HashMap::new());
    let older = core.threads_view.begin(true).unwrap();
    core.apply_thread_feed_page("T", &older, page("5.0", "50.0", true), &HashMap::new());
    let repeating = core.threads_view.begin(true).unwrap();
    assert!(core.apply_thread_feed_page(
        "T",
        &repeating,
        page("5.0", "50.0", true),
        &HashMap::new()
    ));
    assert!(core.threads_view.failed && !core.threads_view.has_more);
    assert!(core.threads_view.failed_read().is_none());
    assert!(core.threads_view.begin_retry().is_none());
    let refresh = core.threads_view.begin(false).unwrap();
    assert!(core.apply_thread_feed_page(
        "T",
        &refresh,
        page("30.0", "100.0", true),
        &HashMap::new()
    ));
    assert!(core.threads_view.has_more && !core.threads_view.failed);
    assert_eq!(core.threads_view.next_cursor.as_deref(), Some("100.0"));
    let failed = core.threads_view.begin(true).unwrap();
    core.threads_view.fail(&failed);
    core.threads_view.ensure_scope(core.account_epoch, "OTHER");
    assert!(core.threads_view.failed_read().is_none());
    assert!(core.threads_view.begin_retry().is_none());
    assert!(!core.threads_view.cancel(&failed));
}

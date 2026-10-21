use super::*;

fn setup() -> (ShellState, u64, u64) {
    let (mut shell, visit) = super::super::tests::shell();
    let d = document_mut(&mut shell, visit).unwrap();
    d.activity.calendar.today = Day::parse("2026-10-20");
    d.activity.calendar.prepare(&d.activity.filters);
    d.activity.calendar.filter_open = true;
    {
        let id = d.id;
        (shell, visit, id)
    }
}
fn event(shell: &ShellState, visit: u64, action: &str, value: &str) -> Event {
    let d = document(shell, visit).unwrap();
    Event {
        action: action.into(),
        value: value.into(),
        seq: d.activity.calendar.ack + 1,
        epoch: d.activity.calendar.epoch.to_string(),
        revision: d.activity.filters.revision.to_string(),
        visit: visit.to_string(),
        document: d.id.to_string(),
        buffers: Some(Buffers {
            status: d.activity.filters.status.clone(),
            limit: d.activity.filters.limit.clone(),
            user: d.activity.filters.user.clone(),
            search: d.activity.user_search.clone(),
        }),
    }
}
fn act(shell: &mut ShellState, visit: u64, id: u64, action: &str, value: &str) -> bool {
    let e = event(shell, visit, action, value);
    apply(shell, visit, id, &e)
}
#[test]
fn defaults_remain_preview_until_save_then_preserve_the_exact_exception() {
    let (mut s, v, id) = setup();
    let original = document(&s, v).unwrap().activity.filters.clone();
    act(&mut s, v, id, "open", "");
    assert!(document(&s, v).unwrap().activity.filters == original);
    let c = &document(&s, v).unwrap().activity.calendar;
    assert_eq!(c.end.unwrap().0 - c.start.unwrap().0, 90);
    assert!(act(&mut s, v, id, "save", ""));
    let a = &document(&s, v).unwrap().activity;
    assert_eq!(a.filters.date_origin, "default");
    assert_eq!(a.filters.from, a.filters.default_from);
    assert_eq!(a.filters.to, a.filters.default_to);
    assert!(!a.calendar.open);
}
#[test]
fn second_selection_accepts_89_not_90_and_sorts_reverse_or_same_day() {
    let (mut s, v, id) = setup();
    act(&mut s, v, id, "open", "");
    act(&mut s, v, id, "day", "2026-06-01");
    let start = Day::parse("2026-06-01").unwrap();
    let c = &document(&s, v).unwrap().activity.calendar;
    assert!(c.allowed(Day(start.0 + 89)));
    assert!(!c.allowed(Day(start.0 + 90)));
    assert!(c.allowed(Day(start.0 - 89)));
    assert!(!c.allowed(Day(start.0 - 90)));
    act(&mut s, v, id, "day", "2026-05-31");
    assert_eq!(
        document(&s, v)
            .unwrap()
            .activity
            .calendar
            .start
            .unwrap()
            .iso(),
        "2026-05-31"
    );
    act(&mut s, v, id, "day", "2026-05-31");
    act(&mut s, v, id, "day", "2026-05-31");
    assert!(act(&mut s, v, id, "save", ""));
    let f = &document(&s, v).unwrap().activity.filters;
    assert_eq!(f.from, f.to);
    assert_eq!(f.date_origin, "selection");
}
#[test]
fn tab_retains_draft_cancel_discards_and_outer_close_reconstructs_preview() {
    let (mut s, v, id) = setup();
    act(&mut s, v, id, "open", "");
    act(&mut s, v, id, "day", "2026-07-01");
    act(&mut s, v, id, "tab", "");
    assert!(document(&s, v).unwrap().activity.calendar.dirty);
    act(&mut s, v, id, "open", "");
    assert_eq!(
        document(&s, v)
            .unwrap()
            .activity
            .calendar
            .start
            .unwrap()
            .iso(),
        "2026-07-01"
    );
    act(&mut s, v, id, "cancel", "");
    act(&mut s, v, id, "open", "");
    assert!(document(&s, v).unwrap().activity.calendar.start.is_none());
    assert!(!act(&mut s, v, id, "save", ""));
    act(&mut s, v, id, "outer-close", "");
    act(&mut s, v, id, "filter", "");
    act(&mut s, v, id, "open", "");
    assert!(document(&s, v).unwrap().activity.calendar.start.is_some());
    assert!(document(&s, v).unwrap().activity.filters.from.is_empty());
}
#[test]
fn original_event_cannot_commit_after_successor_epoch_or_native_buffer_change() {
    let (mut s, v, id) = setup();
    act(&mut s, v, id, "open", "");
    let stale = event(&s, v, "save", "");
    act(&mut s, v, id, "day", "2026-07-01");
    assert!(!apply(&mut s, v, id, &stale));
    assert!(document(&s, v).unwrap().activity.filters.from.is_empty());
    act(&mut s, v, id, "day", "2026-07-02");
    let mut changed = event(&s, v, "save", "");
    changed.buffers.as_mut().unwrap().search = "Uncommitted native search".into();
    assert!(!apply(&mut s, v, id, &changed));
    assert!(document(&s, v).unwrap().activity.calendar.open);
    assert!(document(&s, v).unwrap().activity.filters.from.is_empty());
}
#[test]
fn owned_read_refuses_open_without_changing_query_or_epoch_then_allows_it() {
    let (mut s, v, id) = setup();
    let e = event(&s, v, "open", "");
    let before = document(&s, v).unwrap().activity.filters.clone();
    let epoch = document(&s, v).unwrap().activity.calendar.epoch;
    s.app_home.about.management.surface.as_mut().unwrap().read = Some(next());
    assert!(!apply(&mut s, v, id, &e));
    assert!(!document(&s, v).unwrap().activity.calendar.open);
    assert_eq!(document(&s, v).unwrap().activity.calendar.epoch, epoch);
    assert!(document(&s, v).unwrap().activity.filters == before);
    s.app_home.about.management.surface.as_mut().unwrap().read = None;
    act(&mut s, v, id, "open", "");
    assert!(document(&s, v).unwrap().activity.calendar.open);
}
#[test]
fn unchanged_selected_pair_does_not_request_a_duplicate_page() {
    let (mut s, v, id) = setup();
    act(&mut s, v, id, "open", "");
    act(&mut s, v, id, "day", "2026-07-01");
    act(&mut s, v, id, "day", "2026-07-02");
    assert!(act(&mut s, v, id, "save", ""));
    let filters = document(&s, v).unwrap().activity.filters.clone();
    act(&mut s, v, id, "outer-close", "");
    act(&mut s, v, id, "filter", "");
    act(&mut s, v, id, "open", "");
    assert!(!act(&mut s, v, id, "save", ""));
    assert!(document(&s, v).unwrap().activity.filters == filters);
}

#[test]
fn saved_default_remount_keeps_original_default_provenance_and_current_page() {
    let (mut s, v, id) = setup();
    act(&mut s, v, id, "open", "");
    assert!(act(&mut s, v, id, "save", ""));
    let filters = document(&s, v).unwrap().activity.filters.clone();
    let q = ActivityQuery::new("WF_REVIEW", ActivityFilters::default()).unwrap();
    let raw = json!({"ok":true,"workflow_events":[]});
    document_mut(&mut s, v).unwrap().activity.page =
        Some(ActivityPage::decode(&q, raw.clone()).unwrap());
    act(&mut s, v, id, "outer-close", "");
    act(&mut s, v, id, "filter", "");
    act(&mut s, v, id, "open", "");
    assert!(!act(&mut s, v, id, "save", ""));
    let a = &document(&s, v).unwrap().activity;
    assert!(a.filters == filters);
    assert_eq!(a.page.as_ref().unwrap().raw(), &raw);
}

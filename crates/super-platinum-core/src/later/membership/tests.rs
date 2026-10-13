use super::*;
fn item(id: &str) -> SavedItem {
    SavedItem {
        item_type: "file".into(),
        item_id: id.into(),
        ..Default::default()
    }
}
fn state() -> LaterState {
    let mut state = LaterState::default();
    state.ensure_scope(1, "T1");
    state
}
fn event(kind: &str, id: &str, ts: &str) -> crate::slack::events::RawEvent {
    serde_json::from_value(serde_json::json!({"type":kind,"event_ts":ts,"saved":{"item_type":"file","item_id":id},"counts":{"uncompleted_count":1}})).unwrap()
}

#[test]
fn filtered_and_paginated_omissions_never_prove_unsaved() {
    let mut state = state();
    let key = SavedKey::file("F1");
    assert!(matches!(state.membership(&key), SavedMembership::Unknown));
    let generation = state.begin(LaterFilter::Saved, false);
    state.apply_page(
        generation,
        SavedPage {
            saved_items: vec![item("F1")],
            ..Default::default()
        },
        false,
        None,
    );
    let generation = state.begin(LaterFilter::Saved, false);
    state.apply_page(generation, SavedPage::default(), false, None);
    assert!(!state.items.contains_key(&key));
    assert!(matches!(
        state.membership(&key),
        SavedMembership::Present(_)
    ));
    let generation = state.begin(LaterFilter::Completed, false);
    state.apply_page(generation, SavedPage::default(), false, None);
    assert!(matches!(
        state.membership(&SavedKey::file("F2")),
        SavedMembership::Unknown
    ));
    assert!(matches!(
        state.membership(&key),
        SavedMembership::Present(_)
    ));
    // Explicit positive deletion is independent of filter and list visibility.
    let request = state.begin_item_request(key.clone()).unwrap();
    assert!(state.apply_item_response(&request, None));
    assert!(matches!(state.membership(&key), SavedMembership::Absent));
}

#[test]
fn realtime_and_newer_requests_win_but_unrelated_items_do_not_block_receipts() {
    let mut state = state();
    let key = SavedKey::file("F1");
    let request = state.begin_item_request(key.clone()).unwrap();
    assert!(state.apply_event(&event("saved_deleted", "F1", "10.000001")));
    assert!(!state.apply_item_response(&request, Some(item("F1"))));
    assert!(matches!(state.membership(&key), SavedMembership::Absent));
    let old = state.begin_item_request(key.clone()).unwrap();
    let new = state.begin_item_request(key.clone()).unwrap();
    state.updated(&SavedKey::file("F2"), Some(item("F2")));
    assert!(state.apply_item_response(&new, Some(item("F1"))));
    assert!(!state.apply_item_response(&old, None));
    let request = state.begin_item_request(key.clone()).unwrap();
    assert!(state.apply_event(&event("saved_updated", "F1", "11.000001")));
    assert!(!state.apply_item_response(&request, None));
    assert!(matches!(
        state.membership(&key),
        SavedMembership::Present(_)
    ));
}

#[test]
fn exact_item_scope_visit_and_one_time_response_guard_are_required() {
    let mut state = state();
    let key = SavedKey::file("F1");
    state.busy.insert(key.clone());
    let request = state.begin_item_request(key.clone()).unwrap();
    assert!(!state.apply_item_response(&request, Some(item("F2"))));
    assert!(matches!(state.membership(&key), SavedMembership::Unknown));
    assert!(state.apply_item_response(&request, Some(item("F1"))));
    assert!(!state.apply_item_response(&request, None));
    assert!(
        state.busy.contains(&key),
        "projection never releases another controller's busy ownership"
    );
    let old = state.begin_item_request(key.clone()).unwrap();
    state.ensure_scope(1, "T2");
    state.ensure_scope(1, "T1");
    state.begin_item_request(key.clone()).unwrap();
    assert!(!state.apply_item_response(&old, Some(item("F1"))));
    assert!(matches!(state.membership(&key), SavedMembership::Unknown));
    let old = state.begin_item_request(key.clone()).unwrap();
    state.ensure_scope(2, "T1");
    assert!(!state.apply_item_response(&old, Some(item("F1"))));
}

#[test]
fn positive_page_item_supersedes_old_lookup_without_poisoning_other_keys() {
    let mut state = state();
    let key = SavedKey::file("F1");
    let old = state.begin_item_request(key.clone()).unwrap();
    let other = state.begin_item_request(SavedKey::file("F2")).unwrap();
    let generation = state.begin(LaterFilter::Saved, false);
    state.apply_page(
        generation,
        SavedPage {
            saved_items: vec![item("F1")],
            ..Default::default()
        },
        false,
        None,
    );
    assert!(!state.apply_item_response(&old, None));
    assert!(state.apply_item_response(&other, Some(item("F2"))));
    state.updated(&key, Some(item("wrong")));
    assert!(
        matches!(state.membership(&key),SavedMembership::Present(value) if value.item_id=="F1")
    );
}

#[test]
fn page_started_before_explicit_item_request_cannot_defeat_its_positive_receipt() {
    let mut state = state();
    let key = SavedKey::file("F1");
    let old_page = state.begin(LaterFilter::Saved, false);
    let request = state.begin_item_request(key.clone()).unwrap();
    assert!(!state.loading);
    assert!(!state.apply_page(
        old_page,
        SavedPage {
            saved_items: vec![item("F1")],
            ..Default::default()
        },
        false,
        None,
    ));
    assert!(matches!(state.membership(&key), SavedMembership::Unknown));
    assert!(state.apply_item_response(&request, None));
    assert!(matches!(state.membership(&key), SavedMembership::Absent));
    assert!(!state.items.contains_key(&key));

    // A newly started list read is later evidence and retains the existing
    // ability to supersede an earlier item request.
    let request = state.begin_item_request(key.clone()).unwrap();
    let newer_page = state.begin(LaterFilter::Saved, false);
    assert!(state.apply_page(
        newer_page,
        SavedPage {
            saved_items: vec![item("F1")],
            ..Default::default()
        },
        false,
        None,
    ));
    assert!(!state.apply_item_response(&request, None));
    assert!(matches!(
        state.membership(&key),
        SavedMembership::Present(_)
    ));
}

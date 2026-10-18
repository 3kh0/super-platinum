use super::super::super::{local_refinement, refinement_save, sort};
use super::*;
use super_platinum_core::slack_lists::{ViewRefinementSaveAttempt, ViewSortChoice};
fn fixture() -> ShellState {
    let s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-view-sort",
    );
    discard_sort_fixture(&s);
    s
}
#[test]
fn actual_local_sort_apply_is_zero_write_then_distinct_refinement_save_has_full_wire() {
    let mut s = fixture();
    let original = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let visit = generation(&s).unwrap();
    let selected = selection(&s).unwrap();
    assert!(local_refinement::open_shell(
        &mut s,
        local_refinement::Kind::Sort,
        visit,
        selected
    ));
    let canceled = entry(&s).unwrap().serial;
    assert!(close_entry(&s, canceled));
    assert!(local_refinement::open_shell(
        &mut s,
        local_refinement::Kind::Sort,
        visit,
        selected
    ));
    let local = entry(&s).unwrap();
    assert!(sort::change(
        &s,
        local.serial,
        visit,
        local.sort_choice,
        &ViewSortChoice::None,
        ViewSortChoice::Number {
            column_id: "C_NUMBER".into(),
            ascending: true
        }
    ));
    assert!(super::super::super::prepare(&mut s, canceled).is_none());
    assert!(entry(&s).is_some());
    assert!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .refinements
            .is_empty()
    );
    assert!(super::super::super::prepare(&mut s, local.serial).is_none());
    assert!(entry(&s).is_none());
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(!v.layout.refinements.is_empty());
    assert_eq!(v.loaded.as_ref().unwrap().document, original.document);
    assert_eq!(v.loaded.as_ref().unwrap().records, original.records);
    assert_eq!(super::super::super::super::fixture::attempt_count(&s), 0);
    let action = refinement_save::capture(&s, false).unwrap();
    let shared = refinement_save::open_shell(&mut s, action, visit, selected).unwrap();
    assert_ne!(shared, local.serial);
    let (_, intent) = super::super::super::prepare(&mut s, shared).unwrap();
    let Intent::Refinement(intent) = intent else {
        panic!("Save must own a refinement intent")
    };
    let mut attempt = ViewRefinementSaveAttempt::new(intent);
    let permit = attempt
        .admit(
            &original.document,
            original.allow_lists.as_deref(),
            original.personal_task_lists.as_ref(),
        )
        .unwrap();
    let request = Permit::Refinement(permit).request(&s.core.client, &session());
    forms::metadata(&request, &json!([{"key":"estimate","ascending":true}])).unwrap();
    // No asynchronous job was dispatched by this pure admission test.
    let key = Key::current(&s).unwrap();
    with_registry(|r| r.entries.remove(&key));
}
#[test]
fn account_retirement_before_local_apply_cannot_publish_sort_or_shared_attempt() {
    let mut s = fixture();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let visit = generation(&s).unwrap();
    let selected = selection(&s).unwrap();
    assert!(local_refinement::open_shell(
        &mut s,
        local_refinement::Kind::Sort,
        visit,
        selected
    ));
    let e = entry(&s).unwrap();
    assert!(sort::change(
        &s,
        e.serial,
        visit,
        e.sort_choice,
        &ViewSortChoice::None,
        ViewSortChoice::Number {
            column_id: "C_NUMBER".into(),
            ascending: false
        }
    ));
    let key = Key::current(&s).unwrap();
    s.core.account_epoch = s.core.account_epoch.checked_add(1).unwrap();
    assert!(super::super::super::prepare(&mut s, e.serial).is_none());
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(v.layout.refinements.is_empty());
    assert_eq!(v.loaded.as_ref().unwrap().document, before.document);
    assert_eq!(v.loaded.as_ref().unwrap().records, before.records);
    assert!(refinement_save::capture(&s, false).is_none());
    with_registry(|r| r.entries.remove(&key));
}

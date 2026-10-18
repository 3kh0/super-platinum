use super::*;
use super_platinum_core::slack_lists::ViewFilterChoice;
fn shell() -> ShellState {
    ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-board-local-order",
    )
}
fn open(s: &mut ShellState, kind: Kind) -> registry::Entry {
    let visit = generation(s).unwrap();
    let selection = selection(s).unwrap();
    assert!(open_shell(s, kind, visit, selection));
    entry(s).unwrap()
}
#[test]
fn actual_visibility_editor_applies_locally_and_old_serial_cannot_confirm_replacement() {
    let mut s = shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let count = super::super::super::fixture::attempt_count(&s);
    let old = open(&mut s, Kind::Visibility);
    assert!(close_entry(&s, old.serial));
    let e = open(&mut s, Kind::Visibility);
    let Target::Visibility { column, .. } = &e.target else {
        panic!()
    };
    assert!(visibility::change(
        &s,
        e.serial,
        generation(&s).unwrap(),
        e.visibility_choice,
        column,
        None,
        Some(false)
    ));
    assert!(prepare(&mut s, old.serial).is_none());
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
    let current = entry(&s).unwrap();
    assert!(text_create::capture(&s, &current, &before).is_err());
    assert!(prepare(&mut s, e.serial).is_none());
    assert!(entry(&s).is_none());
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(!v.layout.columns.iter().any(|c| &c.id == column));
    assert_eq!(v.loaded.as_ref().unwrap().document, before.document);
    assert_eq!(v.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(super::super::super::fixture::attempt_count(&s), count);
}
#[test]
fn actual_complete_filter_capture_publishes_no_shared_intent_or_request() {
    let mut s = shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let count = super::super::super::fixture::attempt_count(&s);
    let e = open(&mut s, Kind::Filter);
    let visit = generation(&s).unwrap();
    assert!(filter::change(
        &s,
        e.serial,
        visit,
        e.filter_choice,
        &ViewFilterChoice::None,
        ViewFilterChoice::Checkbox {
            column_id: "C_CHECK".into(),
            checked: true
        }
    ));
    let e = entry(&s).unwrap();
    let captured = filter::reserve(&s, e.serial, visit, e.filter_choice).unwrap();
    let row = &e.filter_rows[0];
    let seen = filter::Snapshot {
        id: row.id.to_string(),
        epoch: row.epoch.to_string(),
        column: "C_CHECK".into(),
        operator: "is_not_empty".into(),
        value: None,
        valid: true,
    };
    assert!(filter::finish(&mut s, &captured, vec![seen]).is_none());
    assert!(entry(&s).is_none());
    assert!(refinement::changes(&s).unwrap().filters);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(v.layout.matched < 71);
    assert_eq!(v.loaded.as_ref().unwrap().document, before.document);
    assert_eq!(v.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(super::super::super::fixture::attempt_count(&s), count);
}
#[test]
fn read_only_view_can_refine_but_cancel_and_account_retirement_do_not_publish() {
    let mut s = shell();
    let l = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = l.document.raw.clone();
    raw["access"] = json!("read");
    l.document = Document::parse(raw).unwrap();
    rebuild_rows(&mut s);
    assert!(can_open(&s, Kind::Grouping));
    let e = open(&mut s, Kind::Grouping);
    assert!(close_entry(&s, e.serial));
    assert!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .refinements
            .is_empty()
    );
    let e = open(&mut s, Kind::Grouping);
    s.core.account_epoch = s.core.account_epoch.wrapping_add(1);
    assert!(!finish(&mut s, &e));
    assert!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .refinements
            .is_empty()
    );
}

#[test]
fn rendered_openers_refuse_retired_account_scope_and_layout_before_registry_insert() {
    for kind in [Kind::Filter, Kind::Sort, Kind::Visibility, Kind::Grouping] {
        for retired in 0..3 {
            let mut s = shell();
            let stamp = refinement::toolbar::stamp(&s).unwrap();
            match retired {
                0 => s.core.account_epoch = s.core.account_epoch.wrapping_add(1),
                1 => s.slack_lists.scope.as_mut().unwrap().identity = std::sync::Arc::new(()),
                _ => s.slack_lists.view.as_mut().unwrap().layout.generation += 1,
            }
            assert!(!open_stamped_shell(&mut s, kind, &stamp));
            assert!(entry(&s).is_none());
        }
    }
}

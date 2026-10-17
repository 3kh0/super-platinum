use super::*;
use crate::slack_lists::rename::sort::{self as controller, Action};
fn act(s: &ShellState, action: Action) -> Option<(u64, String)> {
    let e = entry(s).unwrap();
    let Input::Sort(value) = e.input else {
        panic!()
    };
    controller::structural(
        s,
        e.serial,
        generation(s).unwrap(),
        e.sort_choice,
        &value,
        action,
    )
}
fn rows(s: &ShellState) -> Vec<u64> {
    entry(s).unwrap().sort_rows
}
fn desired(s: &ShellState) -> ViewSortChoice {
    let Input::Sort(value) = entry(s).unwrap().input else {
        panic!()
    };
    value
}
#[test]
fn add_reorder_remove_clear_keep_stable_rows_and_do_not_change_loaded_view() {
    let (mut s, _cleanup) = sort_shell();
    s.slack_lists.view.as_mut().unwrap().layout.find.applied = "retained Find".into();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    open(&mut s);
    assert!(act(&s, Action::Add("C_N".into())).is_some());
    let a = rows(&s)[0];
    assert_eq!(desired(&s), choice(true));
    assert!(act(&s, Action::Add("C_OTHER".into())).is_some());
    let b = rows(&s)[1];
    assert_ne!(a, b);
    assert!(act(&s, Action::Add("C_N".into())).is_none());
    assert!(act(&s, Action::Move(a, true)).is_none());
    assert_eq!(
        act(&s, Action::Move(b, true)).unwrap().1,
        format!("sort-field-{b}")
    );
    assert_eq!(rows(&s), vec![b, a]);
    assert!(act(&s, Action::Direction(b, false)).is_some());
    assert_eq!(rows(&s), vec![b, a]);
    assert!(act(&s, Action::Field(a, "C_OTHER".into())).is_none());
    assert_eq!(
        act(&s, Action::Remove(b)).unwrap().1,
        format!("sort-field-{a}")
    );
    assert_eq!(rows(&s), vec![a]);
    assert_eq!(desired(&s), choice(true));
    assert!(act(&s, Action::Remove(b)).is_none());
    assert_eq!(
        act(&s, Action::Clear).unwrap().1,
        "slack-list-sort-add-field"
    );
    assert!(rows(&s).is_empty());
    assert_eq!(desired(&s), ViewSortChoice::None);
    assert!(act(&s, Action::Clear).is_none());
    let view = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(view.loaded.as_ref().unwrap().document, before.document);
    assert_eq!(view.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(view.layout.find.applied, "retained Find");
    assert!(close_entry(&s, entry(&s).unwrap().serial));
}
#[test]
fn stale_full_vector_epoch_rejects_reorder_aba_removed_row_and_selection_replacement() {
    let (mut s, _cleanup) = sort_shell();
    open(&mut s);
    act(&s, Action::Add("C_N".into())).unwrap();
    act(&s, Action::Add("C_OTHER".into())).unwrap();
    let ids = rows(&s);
    let old = entry(&s).unwrap();
    let rendered = desired(&s);
    let visit = generation(&s).unwrap();
    act(&s, Action::Move(ids[1], true)).unwrap();
    act(&s, Action::Move(ids[1], false)).unwrap();
    assert_eq!(desired(&s), rendered);
    assert!(
        controller::structural(
            &s,
            old.serial,
            visit,
            old.sort_choice,
            &rendered,
            Action::Direction(ids[0], false)
        )
        .is_none()
    );
    let current = entry(&s).unwrap();
    assert!(
        controller::structural(
            &s,
            current.serial + 1,
            visit,
            current.sort_choice,
            &rendered,
            Action::Clear
        )
        .is_none()
    );
    assert!(
        controller::structural(
            &s,
            current.serial,
            visit + 1,
            current.sort_choice,
            &rendered,
            Action::Clear
        )
        .is_none()
    );
    act(&s, Action::Remove(ids[0])).unwrap();
    act(&s, Action::Add("C_N".into())).unwrap();
    assert_ne!(rows(&s)[1], ids[0]);
    assert!(act(&s, Action::Direction(ids[0], false)).is_none());
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(act(&s, Action::Clear).is_none());
}
#[test]
fn changed_schema_refuses_structural_edit_without_rewriting_retained_draft() {
    let (mut s, _cleanup) = sort_shell();
    open(&mut s);
    act(&s, Action::Add("C_N".into())).unwrap();
    let before = entry(&s).unwrap();
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    loaded.document.raw["list_metadata"]["schema"][0]["opaque"] = json!(17);
    loaded.document = Document::parse(loaded.document.raw.clone()).unwrap();
    assert!(act(&s, Action::Add("C_OTHER".into())).is_none());
    assert_eq!(entry(&s).unwrap().input, before.input);
    assert_eq!(rows(&s), before.sort_rows);
    assert!(prepare(&mut s, before.serial).is_none());
}
#[test]
fn ordered_save_is_once_only_and_wrong_priority_readback_is_conflict_not_observation() {
    let (mut s, _cleanup) = sort_shell();
    open(&mut s);
    act(&s, Action::Add("C_N".into())).unwrap();
    act(&s, Action::Add("C_OTHER".into())).unwrap();
    let b = rows(&s)[1];
    act(&s, Action::Direction(b, false)).unwrap();
    let serial = entry(&s).unwrap().serial;
    let before = metadata(&s, None);
    let (g, i) = prepare(&mut s, serial).unwrap();
    let Intent::Sort(intent) = &i else { panic!() };
    assert_eq!(
        intent.sorts(),
        &json!([{"key":"estimate","ascending":true},{"key":"other","ascending":false}])
    );
    admitted(&s, &g, &i, &before).unwrap();
    assert!(admitted(&s, &g, &i, &before).is_none());
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert!(prepare(&mut s, serial).is_none());
    assert_eq!(metadata(&s, None).document, before.document);
    let check = checking(&s);
    let mut reversed = metadata(&s, None);
    reversed.document.raw["list_metadata"]["views"][0]["sorts"] =
        json!([{"key":"other","ascending":false},{"key":"estimate","ascending":true}]);
    reversed.document = Document::parse(reversed.document.raw).unwrap();
    assert!(checked(&mut s, &check, Ok(reversed)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert!(prepare(&mut s, serial).is_none());
    let check = checking(&s);
    let mut correct = network::Metadata {
        experiments: None,
        document: before.document.clone(),
        allow: before.allow.clone(),
        personal: before.personal.clone(),
    };
    correct.document.raw["list_metadata"]["views"][0]["sorts"] = intent.sorts().clone();
    correct.document = Document::parse(correct.document.raw).unwrap();
    assert!(checked(&mut s, &check, Ok(correct)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    let now = metadata(&s, None);
    let mut restored = now.document.raw;
    restored["list_metadata"]["views"][0]["sorts"] = json!([]);
    assert_eq!(restored, before.document.raw);
    assert_eq!(now.personal, before.personal);
    assert!(close_entry(&s, serial));
    open(&mut s);
    assert_eq!(rows(&s).len(), 2);
    act(&s, Action::Clear).unwrap();
    let serial = entry(&s).unwrap().serial;
    let (_, i) = prepare(&mut s, serial).unwrap();
    let Intent::Sort(i) = i else { panic!() };
    assert_eq!(i.sorts(), &json!([]));
}

#[test]
fn pending_action_blocks_save_until_exact_latest_vector_commits() {
    let (mut s, _cleanup) = sort_shell();
    open(&mut s);
    act(&s, Action::Add("C_N".into())).unwrap();
    let old = entry(&s).unwrap();
    let value = desired(&s);
    let visit = generation(&s).unwrap();
    let row = rows(&s)[0];
    let pending = controller::reserve(&s, old.serial, visit, old.sort_choice, &value).unwrap();
    assert!(!presentation(&s).unwrap().can_confirm);
    assert!(prepare(&mut s, old.serial).is_none());
    assert_eq!(entry(&s).unwrap().stage, Stage::Editing);
    assert!(act(&s, Action::Clear).is_none());
    assert!(controller::reserve(&s, old.serial, visit, old.sort_choice, &value).is_none());
    assert!(pending.commit(&s, Action::Direction(row, false)).is_some());
    assert_eq!(desired(&s), choice(false));
    assert!(presentation(&s).unwrap().can_confirm);
    let (_, i) = prepare(&mut s, old.serial).unwrap();
    let Intent::Sort(i) = i else { panic!() };
    assert_eq!(i.sorts(), &json!([{"key":"estimate","ascending":false}]));
    assert!(!pending.release());
}
#[test]
fn expired_or_retired_pending_cleanup_cannot_clear_a_newer_reservation() {
    let (mut s, _cleanup) = sort_shell();
    open(&mut s);
    act(&s, Action::Add("C_N".into())).unwrap();
    let old = entry(&s).unwrap();
    let value = desired(&s);
    let visit = generation(&s).unwrap();
    let mut expired = controller::reserve(&s, old.serial, visit, old.sort_choice, &value).unwrap();
    expired.expire_for_test();
    assert!(expired.commit(&s, Action::Clear).is_none());
    assert!(expired.release());
    let next = controller::reserve(&s, old.serial, visit, old.sort_choice, &value).unwrap();
    assert!(!expired.release());
    assert!(prepare(&mut s, old.serial).is_none());
    assert!(expired.commit(&s, Action::Clear).is_none());
    assert!(next.commit(&s, Action::Clear).is_some());
    let e = entry(&s).unwrap();
    let p = controller::reserve(&s, e.serial, visit, e.sort_choice, &desired(&s)).unwrap();
    assert!(close_entry(&s, e.serial));
    open(&mut s);
    assert!(p.commit(&s, Action::Add("C_OTHER".into())).is_none());
    assert!(!p.release());
    let e = entry(&s).unwrap();
    let p = controller::reserve(&s, e.serial, visit, e.sort_choice, &desired(&s)).unwrap();
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(p.commit(&s, Action::Add("C_N".into())).is_none());
    assert!(p.release());
}
#[test]
fn dropping_refused_or_cancelled_renderer_work_releases_only_its_owned_token() {
    let (mut s, _cleanup) = sort_shell();
    open(&mut s);
    act(&s, Action::Add("C_N".into())).unwrap();
    let e = entry(&s).unwrap();
    {
        let _pending = controller::reserve(
            &s,
            e.serial,
            generation(&s).unwrap(),
            e.sort_choice,
            &desired(&s),
        )
        .unwrap();
        assert!(entry(&s).unwrap().sort_pending.is_some());
    }
    assert!(entry(&s).unwrap().sort_pending.is_none());
    assert!(presentation(&s).unwrap().can_confirm);
    assert!(prepare(&mut s, e.serial).is_some());
}

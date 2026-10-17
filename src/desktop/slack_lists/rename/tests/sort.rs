use super::*;
use super_platinum_core::slack_lists::ViewSortChoice;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn sort_shell() -> (ShellState, Cleanup) {
    let mut s = shell();
    let mut m = metadata(&s, None);
    for (id, key) in [("C_N", "estimate"), ("C_OTHER", "other")] {
        m.document.raw["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"key":key,"name":id,"type":"number","options":{"precision":2}}));
    }
    m.document.raw["list_metadata"]["views"] = json!([{"id":"V_SORT","name":"Estimates","type":"table","columns":[{"id":"name","key":"name","visible":true},{"id":"C_N","key":"estimate","visible":true}],"sorts":[],"filters":[],"grouping":{},"opaque":3}]);
    m.document.raw["list_metadata"]["default_view"] = json!("V_SORT");
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .document = Document::parse(m.document.raw).unwrap();
    s.slack_lists.view.as_mut().unwrap().layout = Default::default();
    rebuild_rows(&mut s);
    assert_eq!(selected_view(&s), Some("V_SORT"));
    let cleanup = Cleanup(Key::current(&s).unwrap());
    (s, cleanup)
}
fn choice(ascending: bool) -> ViewSortChoice {
    ViewSortChoice::Number {
        column_id: "C_N".into(),
        ascending,
    }
}
fn open(s: &mut ShellState) {
    let visit = generation(s).unwrap();
    let selected = selection(s).unwrap();
    assert!(crate::slack_lists::rename::sort::open_shell(
        s, visit, selected
    ));
}
fn change(s: &ShellState, desired: ViewSortChoice) -> bool {
    let e = entry(s).unwrap();
    let Input::Sort(rendered) = e.input else {
        panic!()
    };
    crate::slack_lists::rename::sort::change(
        s,
        e.serial,
        generation(s).unwrap(),
        e.sort_choice,
        &rendered,
        desired,
    )
}
#[test]
fn staged_sort_cancel_preserves_canonical_rows_find_and_shared_exclusion() {
    let (mut s, _cleanup) = sort_shell();
    s.slack_lists.view.as_mut().unwrap().layout.find.input = "active query".into();
    s.slack_lists.view.as_mut().unwrap().layout.find.applied = "active query".into();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    open(&mut s);
    let e = entry(&s).unwrap();
    assert!(change(&s, choice(false)));
    let visit = generation(&s).unwrap();
    assert!(!open_shell(&mut s, "name", visit));
    assert!(!crate::slack_lists::rename::precision::open_shell(
        &mut s, "C_N", visit
    ));
    assert!(!open_target(&mut s, Target::View("V_SORT".into()), visit));
    assert!(!close_entry(&s, e.serial + 1));
    assert!(close_entry(&s, e.serial));
    let view = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(view.loaded.as_ref().unwrap().document, before.document);
    assert_eq!(view.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(view.layout.find.applied, "active query");
    assert!(entry(&s).is_none());
}
#[test]
fn direction_field_and_clear_rotate_callbacks_and_refuse_aba() {
    let (mut s, _cleanup) = sort_shell();
    open(&mut s);
    assert!(change(&s, choice(true)));
    let old = entry(&s).unwrap();
    assert!(change(&s, choice(false)));
    assert!(change(&s, choice(true)));
    assert!(!crate::slack_lists::rename::sort::change(
        &s,
        old.serial,
        generation(&s).unwrap(),
        old.sort_choice,
        &choice(true),
        choice(false)
    ));
    assert!(change(
        &s,
        ViewSortChoice::Number {
            column_id: "C_OTHER".into(),
            ascending: false
        }
    ));
    assert!(!change(
        &s,
        ViewSortChoice::Number {
            column_id: "name".into(),
            ascending: true
        }
    ));
    assert!(change(&s, ViewSortChoice::None));
    assert!(!change(&s, ViewSortChoice::None));
    assert!(prepare(&mut s, old.serial).is_none());
}
#[test]
fn ack_never_projects_current_readback_is_exact_and_clear_reopens_current() {
    let (mut s, _cleanup) = sort_shell();
    open(&mut s);
    assert!(change(&s, choice(true)));
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let serial = entry(&s).unwrap().serial;
    let (g, i) = prepare(&mut s, serial).unwrap();
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).unwrap().is_sort());
    completed(
        &mut s,
        &g,
        Ok(json!({"ok":true,"sorts":[{"key":"wrong","ascending":false}]})),
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(metadata(&s, None).document, before.document);
    assert!(!close_entry(&s, serial));
    assert!(prepare(&mut s, serial).is_none());
    let check = checking(&s);
    let mut m = metadata(&s, None);
    m.document.raw["list_metadata"]["views"][0]["sorts"] =
        json!([{"key":"estimate","ascending":true}]);
    m.document = Document::parse(m.document.raw).unwrap();
    assert!(checked(&mut s, &check, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        before.records
    );
    let mut restored = metadata(&s, None).document.raw;
    restored["list_metadata"]["views"][0]["sorts"] = json!([]);
    assert_eq!(restored, before.document.raw);
    assert!(close_entry(&s, serial));
    open(&mut s);
    assert_eq!(
        presentation(&s).unwrap().sort.unwrap().original,
        choice(true)
    );
    assert!(change(&s, ViewSortChoice::None));
    let serial = entry(&s).unwrap().serial;
    let (_, i) = prepare(&mut s, serial).unwrap();
    let Intent::Sort(i) = i else { panic!() };
    assert_eq!(i.sorts(), &json!([]));
}
#[test]
fn stale_selection_completion_and_changed_metadata_never_project() {
    let (mut s, _cleanup) = sort_shell();
    open(&mut s);
    assert!(change(&s, choice(false)));
    let serial = entry(&s).unwrap().serial;
    let (g, i) = prepare(&mut s, serial).unwrap();
    let mut wrong = metadata(&s, None);
    wrong.document.raw["list_metadata"]["opaque"] = json!("changed");
    wrong.document = Document::parse(wrong.document.raw).unwrap();
    assert!(admitted(&s, &g, &i, &wrong).is_none());
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    let before = metadata(&s, None).document;
    let check = checking(&s);
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    let m = metadata(&s, None);
    assert!(!checked(&mut s, &check, Ok(m)));
    assert_eq!(metadata(&s, None).document, before);
    assert!(prepare(&mut s, serial).is_none());
}
#[test]
fn unsupported_current_sort_and_board_are_not_authorable() {
    for (kind, sorts) in [
        (
            "table",
            json!([{"key":"estimate","ascending":true,"opaque":1}]),
        ),
        (
            "table",
            json!([{"key":"estimate","ascending":true},{"key":"estimate","ascending":false}]),
        ),
        ("board", json!([])),
    ] {
        let (mut s, _cleanup) = sort_shell();
        let loaded = s
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .loaded
            .as_mut()
            .unwrap();
        loaded.document.raw["list_metadata"]["views"][0]["type"] = json!(kind);
        loaded.document.raw["list_metadata"]["views"][0]["sorts"] = sorts;
        loaded.document = Document::parse(loaded.document.raw.clone()).unwrap();
        rebuild_rows(&mut s);
        assert!(!crate::slack_lists::rename::sort::can_open(&s));
    }
}

#[test]
fn cooldown_and_wrong_input_refuse_sort_admission() {
    let (mut s, _cleanup) = sort_shell();
    open(&mut s);
    assert!(change(&s, choice(true)));
    let e = entry(&s).unwrap();
    let key = Key::current(&s).unwrap();
    with_registry(|r| r.entries.get_mut(&key).unwrap().input = Input::Name("ascending".into()));
    assert!(prepare(&mut s, e.serial).is_none());
    with_registry(|r| r.entries.get_mut(&key).unwrap().input = Input::Sort(choice(true)));
    registry::record(
        &key.owner,
        &Error::RateLimited {
            retry_after_secs: Some(30),
        },
    );
    assert!(!change(&s, choice(false)));
    assert!(prepare(&mut s, e.serial).is_none());
    assert!(presentation(&s).unwrap().blocked);
}

#[test]
fn canonical_sort_readback_composes_active_find_without_reordering_raw_population() {
    let mut s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-view-sort",
    );
    let _cleanup = Cleanup(Key::current(&s).unwrap());
    // Model-level active Find; native fixture separately drives the real input/debounce.
    let view = s.slack_lists.view.as_mut().unwrap();
    let loaded = view.loaded.as_mut().unwrap();
    let mut raw = loaded.records.raw.clone();
    for i in 0..7 {
        raw["records"][i]["fields"][0]["text"] = json!(format!("Sort cohort {i}"));
    }
    loaded.records = RecordsPage::parse(raw).unwrap();
    view.layout.find.input = "Sort cohort".into();
    view.layout.find.applied = "Sort cohort".into();
    view.layout.find.revision = 17;
    rebuild_rows(&mut s);
    let before = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .clone();
    let values = [
        (
            ViewSortChoice::Number {
                column_id: "C_NUMBER".into(),
                ascending: true,
            },
            vec![2, 3, 1, 4, 0, 5, 6],
        ),
        (
            ViewSortChoice::Number {
                column_id: "C_NUMBER".into(),
                ascending: false,
            },
            vec![0, 5, 1, 4, 3, 2, 6],
        ),
        (ViewSortChoice::None, vec![0, 1, 2, 3, 4, 5, 6]),
    ];
    for (desired, order) in values {
        assert!(change(&s, desired));
        let serial = entry(&s).unwrap().serial;
        let (g, i) = prepare(&mut s, serial).unwrap();
        admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        completed(&mut s, &g, Ok(json!({"ok":true})));
        let check = checking(&s);
        let mut m = metadata(&s, None);
        let Intent::Sort(intent) = i else { panic!() };
        m.document.raw["list_metadata"]["views"][0]["sorts"] = intent.sorts().clone();
        m.document = Document::parse(m.document.raw).unwrap();
        assert!(checked(&mut s, &check, Ok(m)));
        let view = s.slack_lists.view.as_ref().unwrap();
        let ids = view
            .layout
            .entries
            .iter()
            .filter_map(|e| match e {
                crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            order
                .into_iter()
                .map(|i| format!("R_LIST_{i:03}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(view.loaded.as_ref().unwrap().records, before);
        assert_eq!(before.raw["records"].as_array().unwrap().len(), 73);
        assert_eq!(before.raw["records"][11]["parent_record_id"], "R_LIST_010");
        assert_eq!(
            before.raw["response_metadata"]["next_cursor"],
            "precision-opaque+/="
        );
        assert_eq!(
            (
                &view.layout.find.input,
                &view.layout.find.applied,
                view.layout.find.revision
            ),
            (&"Sort cohort".to_owned(), &"Sort cohort".to_owned(), 17)
        );
        assert!(close_entry(&s, serial));
        open(&mut s);
    }
}

mod supported;

mod ordered;

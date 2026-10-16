use super::*;
mod number;
use super_platinum_core::slack_lists::ViewFilterChoice;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn filter_shell() -> (ShellState, Cleanup) {
    let mut s = shell();
    let mut m = metadata(&s, None);
    for (id, key) in [("C_N", "estimate"), ("C_OTHER", "other")] {
        m.document.raw["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"key":key,"name":id,"type":"checkbox","options":{"keep":"ordinary checkbox metadata"}}));
    }
    m.document.raw["list_metadata"]["views"] = json!([{"id":"V_SORT","name":"Checks","type":"table","columns":[{"id":"name","key":"name","visible":true},{"id":"C_N","key":"estimate","visible":true}],"sorts":[],"filters":[],"info_column_filters":[],"grouping":{},"opaque":3}]);
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
fn choice(checked: bool) -> ViewFilterChoice {
    ViewFilterChoice::Checkbox {
        column_id: "C_N".into(),
        checked,
    }
}
fn open(s: &mut ShellState) {
    let visit = generation(s).unwrap();
    let selected = selection(s).unwrap();
    assert!(crate::slack_lists::rename::filter::open_shell(
        s, visit, selected
    ));
}
fn change(s: &ShellState, desired: ViewFilterChoice) -> bool {
    let e = entry(s).unwrap();
    let Input::Filter(rendered) = e.input else {
        panic!()
    };
    crate::slack_lists::rename::filter::change(
        s,
        e.serial,
        generation(s).unwrap(),
        e.filter_choice,
        &rendered,
        desired,
    )
}
#[test]
fn staged_filter_cancel_preserves_canonical_rows_find_and_shared_exclusion() {
    let (mut s, _cleanup) = filter_shell();
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
    assert!(!open_target(&mut s, Target::Sort("V_SORT".into()), visit));
    assert!(!close_entry(&s, e.serial + 1));
    assert!(close_entry(&s, e.serial));
    let view = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(view.loaded.as_ref().unwrap().document, before.document);
    assert_eq!(view.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(view.layout.find.applied, "active query");
    assert!(entry(&s).is_none());
}
#[test]
fn condition_field_and_clear_rotate_callbacks_and_refuse_aba() {
    let (mut s, _cleanup) = filter_shell();
    open(&mut s);
    assert!(change(&s, choice(true)));
    let old = entry(&s).unwrap();
    assert!(change(&s, choice(false)));
    assert!(change(&s, choice(true)));
    assert!(!crate::slack_lists::rename::filter::change(
        &s,
        old.serial,
        generation(&s).unwrap(),
        old.filter_choice,
        &choice(true),
        choice(false)
    ));
    assert!(change(
        &s,
        ViewFilterChoice::Checkbox {
            column_id: "C_OTHER".into(),
            checked: false
        }
    ));
    assert!(!change(
        &s,
        ViewFilterChoice::Checkbox {
            column_id: "name".into(),
            checked: true
        }
    ));
    assert!(change(&s, ViewFilterChoice::None));
    assert!(!change(&s, ViewFilterChoice::None));
    assert!(prepare(&mut s, old.serial).is_none());
}
#[test]
fn ack_never_projects_current_readback_is_exact_and_clear_reopens_current() {
    let (mut s, _cleanup) = filter_shell();
    open(&mut s);
    assert!(change(&s, choice(true)));
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let serial = entry(&s).unwrap().serial;
    let (g, i) = prepare(&mut s, serial).unwrap();
    assert!(
        admitted(&s, &g, &i, &metadata(&s, None))
            .unwrap()
            .is_filter()
    );
    completed(
        &mut s,
        &g,
        Ok(json!({"ok":true,"filters":[{"key":"wrong","operator":"is_empty","values":[]}]})),
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(metadata(&s, None).document, before.document);
    assert!(!close_entry(&s, serial));
    assert!(prepare(&mut s, serial).is_none());
    let check = checking(&s);
    let mut m = metadata(&s, None);
    m.document.raw["list_metadata"]["views"][0]["filters"] =
        json!([{"key":"estimate","operator":"is_not_empty","values":[]}]);
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
    restored["list_metadata"]["views"][0]["filters"] = json!([]);
    assert_eq!(restored, before.document.raw);
    assert!(close_entry(&s, serial));
    open(&mut s);
    assert_eq!(
        presentation(&s).unwrap().filter.unwrap().original,
        choice(true)
    );
    assert!(change(&s, ViewFilterChoice::None));
    let serial = entry(&s).unwrap().serial;
    let (_, i) = prepare(&mut s, serial).unwrap();
    let Intent::Filter(i) = i else { panic!() };
    assert_eq!(i.filters(), &json!([]));
    assert_eq!(i.info_column_filters(), json!([]));
}
#[test]
fn stale_selection_completion_and_changed_metadata_never_project() {
    let (mut s, _cleanup) = filter_shell();
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
fn unsupported_current_filter_shapes_and_special_completion_field_refuse() {
    for patch in [
        json!({"filters":[{"key":"estimate","operator":"is_not_empty","values":[],"opaque":1}]}),
        json!({"filters":[{"key":"estimate","operator":"is_empty","values":[]},{"key":"estimate","operator":"is_not_empty","values":[]}]}),
        json!({"filters":[{"key":"estimate","operator":"is_not_empty","values":[],"column_id":"C_N"}]}),
        json!({"filters":null}),
        json!({"info_column_filters":null}),
        json!({"info_column_filters":[{"key":"created","operator":"is_empty","values":[]}]}),
        json!({"filter_set":{"match":"all","groups":[]}}),
        json!({"type":"board"}),
    ] {
        let (mut s, _cleanup) = filter_shell();
        let loaded = s
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .loaded
            .as_mut()
            .unwrap();
        for (key, value) in patch.as_object().unwrap() {
            loaded.document.raw["list_metadata"]["views"][0][key] = value.clone();
        }
        loaded.document = Document::parse(loaded.document.raw.clone()).unwrap();
        rebuild_rows(&mut s);
        assert!(!crate::slack_lists::rename::filter::can_open(&s));
    }
    let (mut s, _cleanup) = filter_shell();
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    for column in loaded.document.raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
    {
        if column["type"] == "checkbox" {
            column["type"] = json!("completed");
        }
    }
    loaded.document = Document::parse(loaded.document.raw.clone()).unwrap();
    rebuild_rows(&mut s);
    assert!(!crate::slack_lists::rename::filter::can_open(&s));
}

#[test]
fn cooldown_and_wrong_input_refuse_filter_admission() {
    let (mut s, _cleanup) = filter_shell();
    open(&mut s);
    assert!(change(&s, choice(true)));
    let e = entry(&s).unwrap();
    let key = Key::current(&s).unwrap();
    with_registry(|r| r.entries.get_mut(&key).unwrap().input = Input::Name("checked".into()));
    assert!(prepare(&mut s, e.serial).is_none());
    with_registry(|r| r.entries.get_mut(&key).unwrap().input = Input::Filter(choice(true)));
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
fn absent_metadata_filter_readback_is_not_normalized_into_confirmation() {
    let (mut s, _cleanup) = filter_shell();
    open(&mut s);
    assert!(change(&s, choice(true)));
    let serial = entry(&s).unwrap().serial;
    let (g, i) = prepare(&mut s, serial).unwrap();
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    completed(&mut s, &g, Ok(json!({"ok":true})));
    let check = checking(&s);
    let mut m = metadata(&s, None);
    let Intent::Filter(intent) = i else { panic!() };
    m.document.raw["list_metadata"]["views"][0]["filters"] = intent.filters().clone();
    m.document.raw["list_metadata"]["views"][0]
        .as_object_mut()
        .unwrap()
        .remove("info_column_filters");
    m.document = Document::parse(m.document.raw).unwrap();
    assert!(checked(&mut s, &check, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert!(
        metadata(&s, None).document.raw["list_metadata"]["views"][0]
            .get("info_column_filters")
            .is_none()
    );
}

#[test]
fn checked_unchecked_clear_compose_saved_number_sort_and_active_find_with_raw73_unchanged() {
    let mut s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-view-filter",
    );
    let _cleanup = Cleanup(Key::current(&s).unwrap());
    // Reducer-level active Find; native DOM/debounce evidence is a separate slice.
    let view = s.slack_lists.view.as_mut().unwrap();
    let loaded = view.loaded.as_mut().unwrap();
    let mut raw = loaded.records.raw.clone();
    for i in 0..7 {
        raw["records"][i]["fields"][0]["text"] = json!(format!("Filter cohort {i}"));
    }
    loaded.records = RecordsPage::parse(raw).unwrap();
    view.layout.find.input = "Filter cohort".into();
    view.layout.find.applied = "Filter cohort".into();
    view.layout.find.revision = 17;
    rebuild_rows(&mut s);
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    assert_eq!(before.records.raw["records"].as_array().unwrap().len(), 73);
    assert_eq!(
        before.records.raw["records"][11]["parent_record_id"],
        "R_LIST_010"
    );
    assert_eq!(
        before.records.raw["response_metadata"]["next_cursor"],
        "precision-opaque+/="
    );
    let cases = [
        (
            ViewFilterChoice::Checkbox {
                column_id: "C_CHECK".into(),
                checked: true,
            },
            vec![3, 1, 5],
        ),
        (
            ViewFilterChoice::Checkbox {
                column_id: "C_CHECK".into(),
                checked: false,
            },
            vec![2, 4, 0, 6],
        ),
        (ViewFilterChoice::None, vec![2, 3, 1, 4, 0, 5, 6]),
    ];
    for (desired, order) in cases {
        assert!(change(&s, desired));
        let serial = entry(&s).unwrap().serial;
        let (g, i) = prepare(&mut s, serial).unwrap();
        let permit = admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        assert!(permit.is_filter() && !permit.is_sort());
        completed(&mut s, &g, Ok(json!({"ok":true})));
        let check = checking(&s);
        let mut m = metadata(&s, None);
        let Intent::Filter(intent) = i else { panic!() };
        m.document.raw["list_metadata"]["views"][0]["filters"] = intent.filters().clone();
        m.document.raw["list_metadata"]["views"][0]["info_column_filters"] =
            intent.info_column_filters();
        m.document = Document::parse(m.document.raw).unwrap();
        assert!(checked(&mut s, &check, Ok(m)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
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
        let loaded = view.loaded.as_ref().unwrap();
        assert_eq!(loaded.records, before.records);
        let mut restored = loaded.document.raw.clone();
        restored["list_metadata"]["views"][0]["filters"] = json!([]);
        restored["list_metadata"]["views"][0]["info_column_filters"] = json!([]);
        assert_eq!(restored, before.document.raw); // Includes existing Number sort and every other metadata byte/value.
        assert_eq!(
            (
                &view.layout.find.input,
                &view.layout.find.applied,
                view.layout.find.revision
            ),
            (&"Filter cohort".to_owned(), &"Filter cohort".to_owned(), 17)
        );
        assert!(close_entry(&s, serial));
        open(&mut s);
    }
}

mod compound;

use super::*;
use super_platinum_core::slack_lists::ViewGroupingChoice as Choice;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn group_shell(board: bool) -> (ShellState, Cleanup) {
    let mut s = shell();
    let mut m = metadata(&s, None);
    m.document.raw["list_metadata"]["schema"].as_array_mut().unwrap().extend([
        json!({"id":"C_OTHER","key":"other","name":"Other check","type":"checkbox"}),
        json!({"id":"C_STATUS","key":"status","name":"Status","type":"select","options":{"choices":[{"value":"true","label":"Ready"},{"value":"todo","label":"Todo"}]}})
    ]);
    m.document.raw["list_metadata"]["views"] = json!([
        {"id":"V_GROUP","name":"Groups","type":if board{"board"}else{"table"},"columns":[{"id":"name","key":"name","visible":true},{"id":"done","key":"done","visible":true},{"id":"C_OTHER","key":"other","visible":false},{"id":"C_STATUS","key":"status","visible":true}],"grouping":{"group_by":"done"},"sorts":[],"filters":[],"opaque":{"keep":3}},
        {"id":"V_OTHER","name":"Other","type":"table","columns":[{"id":"name","key":"name","visible":true}],"grouping":{"group_by":"done"},"sorts":[],"filters":[]}
    ]);
    m.document.raw["list_metadata"]["default_view"] = json!("V_GROUP");
    let mut rows = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .raw
        .clone();
    rows["records"][0]["fields"][1]["checkbox"] = json!(true);
    rows["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"key":"other","checkbox":true}),
            json!({"key":"status","select":["true"]}),
        ]);
    let child = json!({"id":"child","parent_record_id":"row1","fields":[{"key":"name","text":"Child"},{"key":"done","checkbox":false},{"key":"other","checkbox":false},{"key":"status","select":["todo"]}]});
    rows["records"].as_array_mut().unwrap().push(child);
    let v = s.slack_lists.view.as_mut().unwrap();
    let l = v.loaded.as_mut().unwrap();
    l.document = Document::parse(m.document.raw).unwrap();
    l.records = RecordsPage::parse(rows).unwrap();
    v.layout = Default::default();
    v.expanded.insert("row1".into());
    rebuild_rows(&mut s);
    assert_eq!(selected_view(&s), Some("V_GROUP"));
    assert!(s.slack_lists.view.as_ref().unwrap().layout.notice.is_none());
    let c = Cleanup(Key::current(&s).unwrap());
    (s, c)
}
fn col(id: &str) -> Choice {
    Choice::Column {
        column_id: id.into(),
    }
}
fn open(s: &mut ShellState) {
    let v = generation(s).unwrap();
    let g = selection(s).unwrap();
    assert!(super::super::grouping::open_shell(s, v, g));
}
fn change(s: &ShellState, value: Choice) -> bool {
    let e = entry(s).unwrap();
    let Input::Grouping(old) = e.input else {
        panic!()
    };
    super::super::grouping::change(
        s,
        e.serial,
        generation(s).unwrap(),
        e.grouping_choice,
        &old,
        value,
    )
}
fn fresh(s: &ShellState, value: Value) -> network::Metadata {
    let mut m = metadata(s, None);
    m.document.raw["list_metadata"]["views"][0]["grouping"] = value;
    m.document = Document::parse(m.document.raw).unwrap();
    m
}
fn uncertain(s: &mut ShellState) -> u64 {
    let serial = entry(s).unwrap().serial;
    let (g, i) = prepare(s, serial).unwrap();
    let p = admitted(s, &g, &i, &metadata(s, None)).unwrap();
    assert!(p.is_grouping());
    completed(
        s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    serial
}
#[test]
fn staged_choice_cancel_and_shared_exclusion_leave_records_find_and_schema_exact() {
    let (mut s, _c) = group_shell(false);
    let before = metadata(&s, None).document;
    s.slack_lists.view.as_mut().unwrap().layout.find.input = "Ship".into();
    s.slack_lists.view.as_mut().unwrap().layout.find.applied = "Ship".into();
    let records = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .clone();
    open(&mut s);
    let serial = entry(&s).unwrap().serial;
    assert!(change(&s, col("C_STATUS")));
    let visit = generation(&s).unwrap();
    assert!(!open_shell(&mut s, "name", visit));
    assert!(!open_target(&mut s, Target::View("V_GROUP".into()), visit));
    assert!(close_entry(&s, serial));
    assert_eq!(metadata(&s, None).document, before);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.loaded.as_ref().unwrap().records, records);
    assert_eq!(v.layout.find.applied, "Ship");
    assert!(entry(&s).is_none());
}
#[test]
fn field_none_aba_and_stale_save_callbacks_are_bound_to_choice_epoch() {
    let (mut s, _c) = group_shell(false);
    open(&mut s);
    let old = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    assert!(change(&s, col("C_STATUS")));
    assert!(change(&s, Choice::None));
    assert!(change(&s, col("done")));
    assert!(!super::super::grouping::change(
        &s,
        old.serial,
        visit,
        old.grouping_choice,
        &col("done"),
        col("C_OTHER")
    ));
    assert!(!super::super::grouping::confirm_current(
        &s,
        old.serial,
        visit,
        old.grouping_choice
    ));
    assert!(!change(&s, col("name")));
    assert!(!change(&s, col("missing")));
    assert!(prepare(&mut s, old.serial).is_none());
    let e = entry(&s).unwrap();
    assert!(super::super::grouping::confirm_current(
        &s,
        e.serial,
        visit,
        e.grouping_choice
    ));
    assert!(!super::super::grouping::confirm_current(
        &s,
        e.serial,
        visit + 1,
        e.grouping_choice
    ));
}
#[test]
fn ack_does_not_project_exact_check_and_clear_share_single_retained_path() {
    for board in [false, true] {
        let (mut s, _c) = group_shell(board);
        open(&mut s);
        assert!(change(&s, col("C_STATUS")));
        let before = metadata(&s, None).document;
        let serial = entry(&s).unwrap().serial;
        let (g, i) = prepare(&mut s, serial).unwrap();
        admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        completed(
            &mut s,
            &g,
            Ok(json!({"ok":true,"grouping":{"group_by":"wrong"}})),
        );
        assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
        assert_eq!(metadata(&s, None).document, before);
        assert!(!close_entry(&s, serial));
        assert!(prepare(&mut s, serial).is_none());
        let g = checking(&s);
        let m = fresh(&s, json!({"group_by":"status"}));
        assert!(checked(&mut s, &g, Ok(m)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
        assert!(s.slack_lists.view.as_ref().unwrap().layout.notice.is_none());
        assert!(close_entry(&s, serial));
        open(&mut s);
        assert_eq!(
            presentation(&s).unwrap().view_grouping.unwrap().original,
            col("C_STATUS")
        );
        assert!(change(&s, Choice::None));
        let serial = uncertain(&mut s);
        let g = checking(&s);
        let m = fresh(&s, json!({"group_by":""}));
        assert!(checked(&mut s, &g, Ok(m)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
        assert!(prepare(&mut s, serial).is_none());
        let v = s.slack_lists.view.as_ref().unwrap();
        if board {
            assert_eq!(v.layout.lanes.len(), 1);
            assert_eq!(v.layout.lanes[0].key, "");
        } else {
            assert!(
                !v.layout
                    .entries
                    .iter()
                    .any(|e| matches!(e, crate::slack_lists::TableEntry::Group(_)))
            );
        }
    }
}
#[test]
fn conflicting_current_read_retires_same_bucket_but_preserves_find_hierarchy_and_other_view() {
    let (mut s, _c) = group_shell(false);
    open(&mut s);
    assert!(change(&s, col("C_STATUS")));
    uncertain(&mut s);
    let v = s.slack_lists.view.as_mut().unwrap();
    v.layout
        .collapsed
        .insert("V_GROUP".into(), ["true".into()].into());
    v.layout
        .collapsed
        .insert("V_OTHER".into(), ["true".into()].into());
    v.layout.find.input = "Ship".into();
    v.layout.find.applied = "Ship".into();
    let old_layout = v.layout.generation;
    let old_page = v.layout.grouping_revision;
    let old_find = v.layout.find.revision;
    let expanded = v.expanded.clone();
    let records = v.loaded.as_ref().unwrap().records.clone();
    let g = checking(&s);
    let m = fresh(&s, json!({"group_by":"other"}));
    assert!(checked(&mut s, &g, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(!v.layout.collapsed.contains_key("V_GROUP"));
    assert!(v.layout.collapsed["V_OTHER"].contains("true"));
    assert_ne!(v.layout.generation, old_layout);
    assert_ne!(v.layout.grouping_revision, old_page);
    assert_eq!(v.layout.find.revision, old_find);
    assert_eq!(v.layout.find.input, "Ship");
    assert_eq!(v.layout.find.applied, "Ship");
    assert_eq!(v.expanded, expanded);
    assert_eq!(v.loaded.as_ref().unwrap().records, records);
}
#[test]
fn resolved_id_replacement_and_enriched_grouping_are_distinct_retirement_identities() {
    let (mut s, _c) = group_shell(true);
    let old = s.slack_lists.view.as_ref().unwrap().layout.generation;
    {
        let v = s.slack_lists.view.as_mut().unwrap();
        v.layout
            .collapsed
            .insert("V_GROUP".into(), ["true".into()].into());
        let l = v.loaded.as_mut().unwrap();
        l.document.raw["list_metadata"]["schema"][1]["id"] = json!("C_REPLACED");
        l.document.raw["list_metadata"]["views"][0]["columns"][1]["id"] = json!("C_REPLACED");
        l.document = Document::parse(l.document.raw.clone()).unwrap();
    }
    rebuild_rows(&mut s);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_ne!(v.layout.generation, old);
    assert!(!v.layout.collapsed.contains_key("V_GROUP"));
    let epoch = v.layout.grouping_revision;
    {
        let v = s.slack_lists.view.as_mut().unwrap();
        let l = v.loaded.as_mut().unwrap();
        l.document.raw["list_metadata"]["views"][0]["grouping"]["order"] = json!([]);
        l.document = Document::parse(l.document.raw.clone()).unwrap();
    }
    rebuild_rows(&mut s);
    assert_ne!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .grouping_revision,
        epoch
    );
    assert!(!super::super::grouping::can_open(&s));
    let epoch = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .layout
        .grouping_revision;
    rebuild_rows(&mut s);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .grouping_revision,
        epoch
    );
}
#[test]
fn stale_read_does_not_retire_layout_or_replace_canonical_document() {
    let (mut s, _c) = group_shell(false);
    open(&mut s);
    assert!(change(&s, col("C_STATUS")));
    let serial = uncertain(&mut s);
    let g = checking(&s);
    let m = fresh(&s, json!({"group_by":"status"}));
    let old = metadata(&s, None).document;
    let v = s.slack_lists.view.as_mut().unwrap();
    let layout = v.layout.generation;
    let epoch = v.layout.grouping_revision;
    v.layout.selection_generation = next_id();
    assert!(!checked(&mut s, &g, Ok(m)));
    assert_eq!(metadata(&s, None).document, old);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.layout.generation, layout);
    assert_eq!(v.layout.grouping_revision, epoch);
    assert!(prepare(&mut s, serial).is_none());
}
#[test]
fn exact_metadata_match_with_unsupported_current_population_is_not_observed_visible() {
    let (mut s, _c) = group_shell(false);
    open(&mut s);
    assert!(change(&s, col("C_STATUS")));
    uncertain(&mut s);
    // Canonical metadata alone is insufficient if actual projection cannot evaluate.
    let l = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = l.records.raw.clone();
    raw["records"][0]["fields"][3]["select"] = json!([9]);
    l.records = RecordsPage::parse(raw).unwrap();
    let g = checking(&s);
    let m = fresh(&s, json!({"group_by":"status"}));
    assert!(checked(&mut s, &g, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert!(s.slack_lists.view.as_ref().unwrap().layout.notice.is_some());
}

#[test]
fn changed_observed_state_requires_current_check_without_replaying_write() {
    let (mut s, _c) = group_shell(false);
    open(&mut s);
    assert!(change(&s, col("C_STATUS")));
    let serial = uncertain(&mut s);
    let g = checking(&s);
    let m = fresh(&s, json!({"group_by":"status"}));
    assert!(checked(&mut s, &g, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    let m = fresh(&s, json!({"group_by":"other"}));
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .document = m.document;
    rebuild_rows(&mut s);
    let p = presentation(&s).unwrap();
    assert!(p.checkable);
    assert!(!p.current_review);
    assert!(p.message.contains("changed since grouping"));
    assert!(!close_entry(&s, serial));
    assert!(prepare(&mut s, serial).is_none());
    let g = checking(&s);
    let m = metadata(&s, None);
    assert!(checked(&mut s, &g, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert!(close_entry(&s, serial));
}
#[test]
fn same_bucket_row_and_column_ids_still_retire_selected_layout_work() {
    let (mut s, _c) = group_shell(false);
    {
        let l = s
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .loaded
            .as_mut()
            .unwrap();
        l.document.raw["list_metadata"]["views"][0]["columns"] =
            json!([{"id":"name","key":"name","visible":true}]);
        l.document = Document::parse(l.document.raw.clone()).unwrap();
    }
    rebuild_rows(&mut s);
    let identities = |s: &ShellState| {
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .entries
            .iter()
            .map(|e| match e {
                crate::slack_lists::TableEntry::Group(g) => format!("group:{}", g.key),
                crate::slack_lists::TableEntry::Row(r) => r.instance_id.clone(),
            })
            .collect::<Vec<_>>()
    };
    let before = identities(&s);
    let layout = s.slack_lists.view.as_ref().unwrap().layout.generation;
    let page = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .layout
        .grouping_revision;
    open(&mut s);
    assert!(change(&s, col("C_OTHER")));
    uncertain(&mut s);
    let g = checking(&s);
    let m = fresh(&s, json!({"group_by":"other"}));
    assert!(checked(&mut s, &g, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert_eq!(identities(&s), before);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(
        v.layout
            .columns
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["name"]
    );
    assert_ne!(v.layout.generation, layout);
    assert_ne!(v.layout.grouping_revision, page);
}
#[test]
fn unrelated_view_change_retires_its_collapse_without_resetting_selected_pages() {
    let (mut s, _c) = group_shell(false);
    let v = s.slack_lists.view.as_mut().unwrap();
    let page = v.layout.grouping_revision;
    let generation = v.layout.generation;
    v.layout
        .collapsed
        .insert("V_OTHER".into(), ["true".into()].into());
    let l = v.loaded.as_mut().unwrap();
    l.document.raw["list_metadata"]["views"][1]["grouping"] = json!({"group_by":"other"});
    l.document = Document::parse(l.document.raw.clone()).unwrap();
    rebuild_rows(&mut s);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(!v.layout.collapsed.contains_key("V_OTHER"));
    assert_eq!(v.layout.grouping_revision, page);
    assert_eq!(v.layout.generation, generation);
}

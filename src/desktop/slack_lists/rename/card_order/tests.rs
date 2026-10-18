use super::*;
use super_platinum_core::slack::{client::RequestBody, transport::ListColumnRenameFailure};
fn seed() -> ShellState {
    ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-board-card-drag",
    )
}
fn loaded(s: &ShellState) -> &Loaded {
    s.slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
}
fn metadata(s: &ShellState) -> network::Metadata {
    let l = loaded(s);
    network::Metadata {
        document: l.document.clone(),
        allow: l.allow_lists.clone(),
        personal: l.personal_task_lists.clone().unwrap(),
        experiments: l.list_experiments,
    }
}
fn action(s: &ShellState) -> Action {
    let instance = json!(["OPT_PROGRESS", "R_LIST_002"]).to_string();
    let d = super::begin(s, "OPT_PROGRESS", &instance).unwrap();
    destination(s, &d, "OPT_PROGRESS", Some("R_LIST_001")).unwrap()
}
fn preparing(s: &mut ShellState) -> (Guard, Intent) {
    let a = action(s);
    assert!(open_shell(s, a));
    let e = entry(s).unwrap();
    prepare(s, e.serial).unwrap()
}
fn checking(s: &ShellState) -> Guard {
    let key = Key::current(s).unwrap();
    let e = entry(s).unwrap();
    let g = Guard::capture_read(s, key.clone(), e.serial).unwrap();
    with_registry(|r| {
        let e = r.entries.get_mut(&key).unwrap();
        e.job = Some(g.job);
        e.stage = Stage::Checking;
    });
    g
}
fn desired(s: &ShellState) -> RecordsPage {
    let mut p = loaded(s).records.raw.clone();
    p["records"][2]["view_positions"]["V_LANE"] = json!("0000000001.V");
    RecordsPage::parse(p).unwrap()
}
#[test]
fn exact_named_normal_narrow_population_and_canonical_move() {
    for name in [
        "slack-list-board-card-drag",
        "slack-list-board-card-drag-narrow",
    ] {
        let s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
        let l = loaded(&s);
        assert_eq!(l.records.records.len(), 73);
        assert_eq!(
            l.records.raw["response_metadata"]["next_cursor"],
            "precision-opaque+/="
        );
        assert_eq!(
            l.records.raw["records"][11]["parent_record_id"],
            "R_LIST_010"
        );
        let i = action(&s).capture(&s, l).unwrap();
        assert_eq!(i.desired_position(), Some("0000000001.V"));
        assert!(i.cells().is_none());
        assert!(i.observes_desired(&l.document, &desired(&s)));
    }
}
#[test]
fn stale_drag_find_selection_raw_page_and_metadata_refuse_before_registry() {
    for case in 0..5 {
        let mut s = seed();
        let a = action(&s);
        let v = s.slack_lists.view.as_mut().unwrap();
        match case {
            0 => v.layout.find.revision = next_id(),
            1 => v.layout.selection_generation = next_id(),
            2 => {
                let l = v.loaded.as_mut().unwrap();
                let mut raw = l.records.raw.clone();
                raw["records"][5]["opaque_change"] = json!(true);
                l.records = RecordsPage::parse(raw).unwrap();
            }
            3 => {
                let l = v.loaded.as_mut().unwrap();
                let mut raw = l.document.raw.clone();
                raw["opaque_change"] = json!(true);
                l.document = Document::parse(raw).unwrap();
            }
            _ => v.layout.find.input = "pending".into(),
        }
        assert!(!open_shell(&mut s, a));
        assert!(entry(&s).is_none());
    }
}
#[test]
fn actual_projected_neighbor_and_loaded_tail_are_distinct_from_dom_page() {
    let s = seed();
    let a = action(&s);
    let tail = destination(&s, &a.drag, "OPT_PROGRESS", None).unwrap();
    let i = tail.capture(&s, loaded(&s)).unwrap();
    assert_eq!(i.desired_position(), Some("0000000074"));
    assert!(destination(&s, &a.drag, "OPT_PROGRESS", Some("R_LIST_011")).is_none());
    assert!(destination(&s, &a.drag, "NOT_A_LANE", None).is_none());
}
#[test]
fn fresh_population_required_before_one_use_permit() {
    let mut s = seed();
    let (g, i) = preparing(&mut s);
    let m = metadata(&s);
    let page = loaded(&s).records.clone();
    assert!(admitted(&s, &g, &i, &m).is_none());
    let mut wrong = page.raw.clone();
    wrong["records"][4]["opaque"] = json!(1);
    let wrong = RecordsPage::parse(wrong).unwrap();
    assert!(admitted_with_records(&s, &g, &i, &m, Some(&wrong)).is_none());
    assert_eq!(entry(&s).unwrap().stage, Stage::Preparing);
    let p = admitted_with_records(&s, &g, &i, &m, Some(&page)).unwrap();
    assert!(admitted_with_records(&s, &g, &i, &m, Some(&page)).is_none());
    let request = p.request(&s.core.client, &fixture::session());
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    assert_eq!(fields.len(), 6);
    assert_eq!(fields[4], ("position".into(), "0000000001.V".into()));
    assert_eq!(fields[5], ("_x_reason".into(), "reorder-record".into()));
}
#[test]
fn uncertain_then_exact_readback_preserves_all_unrelated_raw_data() {
    let mut s = seed();
    let (g, i) = preparing(&mut s);
    let m = metadata(&s);
    let page = loaded(&s).records.clone();
    let wanted = desired(&s);
    assert!(admitted_with_records(&s, &g, &i, &m, Some(&page)).is_some());
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(loaded(&s).records, page);
    let read = checking(&s);
    assert!(checked_with_records(
        &mut s,
        &read,
        Ok(m),
        Some(wanted.clone())
    ));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert_eq!(loaded(&s).records, wanted);
    let serial = entry(&s).unwrap().serial;
    assert!(close_entry(&s, serial));
}
#[test]
fn missing_foreign_or_forged_page_cannot_confirm_or_apply() {
    for case in 0..3 {
        let mut s = seed();
        let (g, i) = preparing(&mut s);
        let m = metadata(&s);
        let page = loaded(&s).records.clone();
        assert!(admitted_with_records(&s, &g, &i, &m, Some(&page)).is_some());
        completed(&mut s, &g, Ok(json!({"ok":true})));
        let read = checking(&s);
        let records = match case {
            0 => None,
            1 => {
                let mut p = desired(&s);
                p.raw["list_id"] = json!("F_OTHER");
                Some(p)
            }
            _ => {
                let mut p = desired(&s);
                p.raw["records"][2]["opaque"] = json!(true);
                Some(p)
            }
        };
        assert!(!checked_with_records(&mut s, &read, Ok(m), records));
        assert_eq!(loaded(&s).records, page);
        assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    }
}
#[test]
fn backend_normalization_stays_conflict_and_never_resends() {
    let mut s = seed();
    let (g, i) = preparing(&mut s);
    let m = metadata(&s);
    let page = loaded(&s).records.clone();
    assert!(admitted_with_records(&s, &g, &i, &m, Some(&page)).is_some());
    completed(&mut s, &g, Ok(json!({"ok":true})));
    let read = checking(&s);
    let mut raw = desired(&s).raw;
    raw["records"][2]["date_updated"] = json!(1900000000);
    let normalized = RecordsPage::parse(raw).unwrap();
    assert!(checked_with_records(
        &mut s,
        &read,
        Ok(m),
        Some(normalized.clone())
    ));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert_eq!(loaded(&s).records, normalized);
    assert!(prepare(&mut s, g.serial).is_none());
}
#[test]
fn retired_page_completion_cannot_apply_to_new_visit() {
    let mut s = seed();
    let (g, i) = preparing(&mut s);
    let m = metadata(&s);
    let page = loaded(&s).records.clone();
    let wanted = desired(&s);
    assert!(admitted_with_records(&s, &g, &i, &m, Some(&page)).is_some());
    completed(&mut s, &g, Ok(json!({"ok":true})));
    let read = checking(&s);
    s.slack_lists.view.as_mut().unwrap().generation = next_id();
    assert!(!checked_with_records(&mut s, &read, Ok(m), Some(wanted)));
    assert_eq!(loaded(&s).records, page);
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
}
#[test]
fn cross_empty_lane_uses_captured_cell_only_contract() {
    let s = seed();
    let a = action(&s);
    let cross = destination(&s, &a.drag, "OPT_DONE", None).unwrap();
    let intent = cross.capture(&s, loaded(&s)).unwrap();
    assert!(intent.is_cell_only());
    assert_eq!(
        intent.cells(),
        Some(json!([{"row_id":"R_LIST_002","column_id":"C_SELECT","select":["OPT_DONE"]}]))
    );
}

#[test]
fn local_lane_draft_survives_card_capture_without_rewriting_canonical_grouping() {
    let mut s = seed();
    let v = s.slack_lists.view.as_mut().unwrap();
    let l = v.loaded.as_mut().unwrap();
    let mut raw = l.document.raw.clone();
    raw["list_metadata"]["views"][1]["grouping"]
        .as_object_mut()
        .unwrap()
        .remove("group_by_column_id");
    l.document = Document::parse(raw).unwrap();
    v.layout.board_order = Some(
        super_platinum_core::slack_lists::BoardLaneOrderDraft::moved(
            &l.document,
            "V_LANE",
            None,
            "OPT_DONE",
            "OPT_PROGRESS",
            super_platinum_core::slack_lists::BoardLanePlacement::Before,
            l.allow_lists.as_deref(),
            l.personal_task_lists.as_ref(),
        )
        .unwrap(),
    );
    rebuild_rows(&mut s);
    let original = loaded(&s).document.clone();
    let draft = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .layout
        .board_order
        .clone();
    let a = action(&s);
    let i = a.capture(&s, loaded(&s)).unwrap();
    assert_eq!(i.original(), &original);
    assert_eq!(
        s.slack_lists.view.as_ref().unwrap().layout.board_order,
        draft
    );
}
#[test]
fn fresh_policy_denial_does_not_advance_stage_and_review_requires_current_population() {
    let mut s = seed();
    let (g, i) = preparing(&mut s);
    let mut m = metadata(&s);
    let page = loaded(&s).records.clone();
    m.allow = Some("disabled".into());
    assert!(admitted_with_records(&s, &g, &i, &m, Some(&page)).is_none());
    assert_eq!(entry(&s).unwrap().stage, Stage::Preparing);
    m = metadata(&s);
    assert!(admitted_with_records(&s, &g, &i, &m, Some(&page)).is_some());
    completed(&mut s, &g, Ok(json!({"ok":true})));
    let read = checking(&s);
    let wanted = desired(&s);
    assert!(checked_with_records(&mut s, &read, Ok(m), Some(wanted)));
    let serial = entry(&s).unwrap().serial;
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .records = page;
    assert!(!close_entry(&s, serial));
    assert!(checkable(&s, &entry(&s).unwrap()));
}

#[test]
fn captured_scope_and_epoch_cannot_rebind_an_old_drag_to_identical_payloads() {
    for case in 0..3 {
        let mut s = seed();
        let a = action(&s);
        let before = (
            loaded(&s).document.clone(),
            loaded(&s).records.clone(),
            s.slack_lists.view.as_ref().unwrap().generation,
        );
        match case {
            0 => s.core.account_epoch += 1,
            1 => {}
            _ => s.core.active_account = Some("replacement-account".into()),
        }
        s.slack_lists.scope = Scope::capture(&s);
        assert!(owner::available(&s));
        assert_eq!(loaded(&s).document, before.0);
        assert_eq!(loaded(&s).records, before.1);
        assert_eq!(s.slack_lists.view.as_ref().unwrap().generation, before.2);
        assert!(!open_shell(&mut s, a));
        assert!(entry(&s).is_none());
    }
}
fn name_find_seed() -> ShellState {
    let mut s = seed();
    let team = s.slack_lists.scope.as_ref().unwrap().team.clone();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("U1")
        .unwrap()
        .extra
        .insert("_real_name_normalized_lc".into(), json!("Launch item 0"));
    let f = &mut s.slack_lists.view.as_mut().unwrap().layout.find;
    f.input = "Launch item 0".into();
    f.applied = f.input.clone();
    rebuild_rows(&mut s);
    s
}
fn change_name_without_layout_stamp(s: &mut ShellState) {
    let team = s.slack_lists.scope.as_ref().unwrap().team.clone();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("U1")
        .unwrap()
        .extra
        .insert("_real_name_normalized_lc".into(), json!("different name"));
}
#[test]
fn fresh_name_context_cannot_replan_stale_display_even_when_drop_bounds_survive() {
    let mut s = name_find_seed();
    let a = action(&s);
    let before = lanes(&s).unwrap();
    let stamp = s.slack_lists.view.as_ref().unwrap().layout.generation;
    change_name_without_layout_stamp(&mut s);
    assert_eq!(lanes(&s).unwrap(), before);
    assert_eq!(
        s.slack_lists.view.as_ref().unwrap().layout.generation,
        stamp
    );
    assert!(a.capture(&s, loaded(&s)).is_none());
    assert!(!open_shell(&mut s, a));
    assert!(entry(&s).is_none());
}
#[test]
fn changed_projection_context_retires_preparing_and_saving_guards() {
    for saving in [false, true] {
        let mut s = name_find_seed();
        let (g, i) = preparing(&mut s);
        let m = metadata(&s);
        let page = loaded(&s).records.clone();
        let stage = if saving {
            assert!(admitted_with_records(&s, &g, &i, &m, Some(&page)).is_some());
            Stage::Saving
        } else {
            Stage::Preparing
        };
        assert!(g.current(&s, stage));
        change_name_without_layout_stamp(&mut s);
        assert!(!g.current(&s, stage));
    }
}

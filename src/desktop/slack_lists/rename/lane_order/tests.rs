use super::*;
use super_platinum_core::slack::{client::RequestBody, transport::ListColumnRenameFailure};
fn seeded() -> ShellState {
    ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-board-lane-order",
    )
}
fn metadata(s: &ShellState) -> network::Metadata {
    let l = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    network::Metadata {
        document: l.document.clone(),
        allow: l.allow_lists.clone(),
        personal: l.personal_task_lists.clone().unwrap(),
        experiments: l.list_experiments,
    }
}
fn action(s: &ShellState) -> Action {
    adjacent(s, "OPT_DONE", Direction::Left).unwrap()
}
fn begin(s: &mut ShellState) -> (Guard, Intent) {
    let a = action(s);
    assert!(open_shell(s, a));
    let e = entry(s).unwrap();
    prepare(s, e.serial).unwrap()
}
fn desired(s: &ShellState, i: &Intent) -> network::Metadata {
    let Intent::LaneOrder(i) = i else { panic!() };
    let mut m = metadata(s);
    let mut raw = m.document.raw.clone();
    raw["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["id"] == i.view_id())
        .unwrap()["grouping"] = i.grouping().clone();
    m.document = Document::parse(raw).unwrap();
    m
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
#[test]
fn named_normal_narrow_seed_preserves_raw_population_and_hidden_empty_lane() {
    for name in [
        "slack-list-board-lane-order",
        "slack-list-board-lane-order-narrow",
    ] {
        let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
        let l = s
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap();
        assert_eq!(l.records.records.len(), 73);
        assert_eq!(
            l.records.raw["response_metadata"]["next_cursor"],
            "precision-opaque+/="
        );
        let original = l.document.clone();
        let rows = l.records.clone();
        assert_eq!(action(&s).visible, ["OPT_PROGRESS", "OPT_DONE"]);
        assert_eq!(
            BoardLaneOrder::current_order(&original, "V_LANE").unwrap(),
            ["", "OPT_PROGRESS", "OPT_DONE"]
        );
        assert!(adjacent(&s, "OPT_PROGRESS", Direction::Left).is_none());
        let a = action(&s);
        assert!(open_shell(&mut s, a));
        let e = entry(&s).unwrap();
        let p = presentation(&s, &e).unwrap();
        assert!(p.lanes);
        assert_eq!(
            p.after.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
            ["", "OPT_DONE", "OPT_PROGRESS"]
        );
        assert!(close_entry(&s, e.serial));
        assert_eq!(metadata(&s).document, original);
        assert_eq!(
            s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records,
            rows
        );
    }
}
#[test]
fn captured_drag_or_control_refuses_changed_find_layout_selection_and_owner() {
    for change in 0..9 {
        let mut s = seeded();
        let a = action(&s);
        let v = s.slack_lists.view.as_mut().unwrap();
        match change {
            0 => v.layout.generation = next_id(),
            1 => v.layout.selection_generation = next_id(),
            2 => v.layout.find.input = "Launch".into(),
            3 => v.layout.find.applied = "Launch".into(),
            4 => v.layout.find.revision = next_id(),
            5 => v.layout.grouping_revision = next_id(),
            6 => v.generation = next_id(),
            7 => v.layout.selected = Some("V_PRECISION".into()),
            _ => s.core.account_epoch += 1,
        }
        assert!(drop_action(&s, &a, "OPT_PROGRESS").is_none());
        assert!(!open_shell(&mut s, a));
        assert!(entry(&s).is_none());
    }
}
#[test]
fn single_permit_ack_uncertain_conflict_then_full_observed_metadata_preserves_rows() {
    let mut s = seeded();
    let original = metadata(&s).document;
    let rows = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .clone();
    let (g, i) = begin(&mut s);
    let serial = entry(&s).unwrap().serial;
    let expected = desired(&s, &i);
    let p = admitted(&s, &g, &i, &metadata(&s)).unwrap();
    let req = p.request(&s.core.client, &fixture::session());
    assert_eq!(
        url::Url::parse(&req.url).unwrap().path(),
        "/api/lists.views.update"
    );
    let RequestBody::Form(f) = req.body else {
        panic!()
    };
    assert_eq!(f.len(), 5);
    assert!(f.iter().any(|(k, v)| k == "id" && v == "V_LANE"));
    assert!(
        f.iter()
            .any(|(k, v)| k == "_x_reason" && v == "update-view")
    );
    assert!(admitted(&s, &g, &i, &metadata(&s)).is_none());
    completed(&mut s, &g, Ok(json!({"ok":true})));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(metadata(&s).document, original);
    assert!(prepare(&mut s, serial).is_none());
    let check = checking(&s);
    let unchanged = metadata(&s);
    assert!(checked(&mut s, &check, Ok(unchanged)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    let check = checking(&s);
    assert!(checked(&mut s, &check, Ok(expected)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert_eq!(selected_view(&s), Some("V_LANE"));
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        rows
    );
    let mut now = metadata(&s).document.raw;
    now["list_metadata"]["views"][1]["grouping"] =
        original.raw["list_metadata"]["views"][1]["grouping"].clone();
    assert_eq!(now, original.raw);
    assert!(close_entry(&s, serial));
}
#[test]
fn pending_preflight_refuses_find_cursor_schema_and_scope_drift_without_second_permit() {
    for change in 0..5 {
        let mut s = seeded();
        let (g, i) = begin(&mut s);
        let mut fresh = metadata(&s);
        match change {
            0 => s.slack_lists.view.as_mut().unwrap().layout.find.input = "late".into(),
            1 => s.slack_lists.view.as_mut().unwrap().layout.find.revision = next_id(),
            2 => {
                let l = s
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap();
                let mut r = l.records.raw.clone();
                r["response_metadata"]["next_cursor"] = json!("changed");
                l.records = RecordsPage::parse(r).unwrap();
            }
            3 => {
                let mut raw = fresh.document.raw;
                raw["list_metadata"]["opaque"] = json!("changed");
                fresh.document = Document::parse(raw).unwrap();
            }
            _ => s.core.account_epoch += 1,
        }
        assert!(admitted(&s, &g, &i, &fresh).is_none());
    }
}
#[test]
fn retired_check_cannot_replace_new_projection_and_ack_never_mutates_raw_cells() {
    let mut s = seeded();
    let (g, i) = begin(&mut s);
    assert!(admitted(&s, &g, &i, &metadata(&s)).is_some());
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    let expected = desired(&s, &i);
    let original = metadata(&s).document;
    let check = checking(&s);
    s.slack_lists.view.as_mut().unwrap().generation = next_id();
    assert!(!checked(&mut s, &check, Ok(expected)));
    assert_eq!(metadata(&s).document, original);
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
}
#[test]
fn drag_destination_uses_captured_visible_identity_and_rejects_self_or_hidden_bucket() {
    let s = seeded();
    let a = action(&s);
    assert!(drop_action(&s, &a, "OPT_DONE").is_none());
    assert!(drop_action(&s, &a, "").is_none());
    let moved = drop_action(&s, &a, "OPT_PROGRESS").unwrap();
    assert_eq!(moved.placement, BoardLanePlacement::Before);
    let from = adjacent(&s, "OPT_PROGRESS", Direction::Right).unwrap();
    assert_eq!(
        drop_action(&s, &from, "OPT_DONE").unwrap().placement,
        BoardLanePlacement::After
    );
}

#[test]
fn find_drift_retires_check_and_observed_dismissal_until_new_current_observation() {
    let mut s = seeded();
    let (g, i) = begin(&mut s);
    assert!(admitted(&s, &g, &i, &metadata(&s)).is_some());
    completed(&mut s, &g, Ok(json!({"ok":true})));
    let expected = desired(&s, &i);
    let check = checking(&s);
    s.slack_lists.view.as_mut().unwrap().layout.find.input = "pending".into();
    assert!(!checked(&mut s, &check, Ok(expected)));
    let expected = desired(&s, &i);
    let check = checking(&s);
    assert!(checked(&mut s, &check, Ok(expected)));
    let serial = entry(&s).unwrap().serial;
    s.slack_lists.view.as_mut().unwrap().layout.find.revision = next_id();
    assert!(!close_entry(&s, serial));
    assert!(super::super::presentation(&s).unwrap().checkable);
}

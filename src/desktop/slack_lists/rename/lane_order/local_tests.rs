use super::*;
fn seed(name: &str) -> ShellState {
    ShellState::fixture_variant(crate::media::MediaRegistry::default(), name)
}
fn keys(s: &ShellState) -> Vec<String> {
    s.slack_lists
        .view
        .as_ref()
        .unwrap()
        .layout
        .lanes
        .iter()
        .map(|l| l.key.clone())
        .collect()
}
fn move_left(s: &mut ShellState) -> Action {
    let a = adjacent(s, "OPT_DONE", Direction::Left).unwrap();
    assert!(!a.shared);
    assert!(apply_local(s, a.clone()));
    a
}
#[test]
fn local_normal_narrow_moves_change_projection_only_with_no_registry_or_dispatch() {
    for name in [
        "slack-list-board-local-order",
        "slack-list-board-local-order-narrow",
    ] {
        let mut s = seed(name);
        let l = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        let attempts = crate::slack_lists::fixture::attempt_count(&s);
        let selected = selected_view(&s).map(str::to_owned);
        move_left(&mut s);
        assert_eq!(keys(&s), ["OPT_DONE", "OPT_PROGRESS"]);
        assert!(has_local_order(&s));
        assert!(entry(&s).is_none());
        assert_eq!(selected_view(&s), selected.as_deref());
        let after = s
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap();
        assert_eq!(after.document, l.document);
        assert_eq!(after.records, l.records);
        assert_eq!(after.records.records.len(), 73);
        assert_eq!(crate::slack_lists::fixture::attempt_count(&s), attempts);
        assert_eq!(
            after.records.raw["response_metadata"]["next_cursor"],
            "precision-opaque+/="
        );
    }
}
#[test]
fn local_draft_survives_view_switch_and_converts_current_type_then_reopen_retires_it() {
    let mut s = seed("slack-list-board-local-order");
    let old = move_left(&mut s);
    let draft = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .layout
        .board_order
        .clone();
    for id in ["V_PRECISION", "V_PRECISION_BOARD", "V_LANE"] {
        let v = s.slack_lists.view.as_ref().unwrap();
        let visit = v.generation;
        let layout = v.layout.generation;
        assert!(crate::slack_lists::layout::select_shell(
            &mut s,
            Some(id.into()),
            visit,
            layout
        ));
        assert_eq!(
            s.slack_lists.view.as_ref().unwrap().layout.board_order,
            draft
        );
        if id == "V_PRECISION_BOARD" {
            assert_eq!(keys(&s), ["", "true"]);
        }
    }
    assert_eq!(keys(&s), ["OPT_DONE", "OPT_PROGRESS"]);
    let raw = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .document
        .raw
        .clone();
    crate::slack_lists::close_shell(&mut s);
    assert!(crate::slack_lists::open_shell(
        &mut s,
        serde_json::from_value(raw).unwrap()
    ));
    assert!(!has_local_order(&s));
    assert!(!apply_local(&mut s, old));
}
#[test]
fn second_local_move_and_pending_find_retire_earlier_control_and_drag() {
    let mut s = seed("slack-list-board-local-order");
    let stale = move_left(&mut s);
    assert!(drop_action(&s, &stale, "OPT_PROGRESS").is_none());
    assert!(!apply_local(&mut s, stale));
    let reverse = adjacent(&s, "OPT_DONE", Direction::Right).unwrap();
    s.slack_lists.view.as_mut().unwrap().layout.find.input = "pending".into();
    assert!(!apply_local(&mut s, reverse));
    assert_eq!(keys(&s), ["OPT_DONE", "OPT_PROGRESS"]);
}
#[test]
fn same_raw_document_read_keeps_local_order_and_all_unknown_records() {
    let mut s = seed("slack-list-board-local-order");
    move_left(&mut s);
    let original = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .clone();
    s.slack_lists.view.as_mut().unwrap().revision = next_id();
    rebuild_rows(&mut s);
    assert_eq!(keys(&s), ["OPT_DONE", "OPT_PROGRESS"]);
    let l = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(l.document, original.document);
    assert_eq!(l.records, original.records);
}
#[test]
fn policy_change_and_account_switch_refuse_local_apply_without_creating_receipt() {
    for change in 0..3 {
        let mut s = seed("slack-list-board-local-order");
        let a = adjacent(&s, "OPT_DONE", Direction::Left).unwrap();
        match change {
            0 => s.core.account_epoch += 1,
            1 => s.slack_lists.view.as_mut().unwrap().stale = true,
            _ => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap()
                    .allow_lists = Some("disabled".into())
            }
        };
        assert!(!apply_local(&mut s, a));
        assert!(!has_local_order(&s));
        assert!(entry(&s).is_none());
    }
}

fn choose(s: &mut ShellState, id: &str) {
    let v = s.slack_lists.view.as_ref().unwrap();
    let (visit, layout) = (v.generation, v.layout.generation);
    assert!(crate::slack_lists::layout::select_shell(
        s,
        Some(id.into()),
        visit,
        layout
    ));
}
fn population(s: &ShellState) -> Vec<(String, Vec<String>)> {
    let layout = &s.slack_lists.view.as_ref().unwrap().layout;
    assert!(layout.notice.is_none(), "{:?}", layout.notice);
    assert_eq!(
        layout.kind,
        super_platinum_core::slack_lists::saved_views::ProjectionLayout::Board
    );
    layout
        .lanes
        .iter()
        .map(|lane| {
            (
                lane.key.clone(),
                lane.rows.iter().map(|r| r.id.clone()).collect(),
            )
        })
        .collect()
}
#[test]
fn local_draft_keeps_ungrouped_and_todo_boards_readable_and_survives_return() {
    use super_platinum_core::slack_lists::RecordsPage;
    let mut s = seed("slack-list-board-local-order");
    move_left(&mut s);
    let draft = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .layout
        .board_order
        .clone();
    // Controlled offline read population, not a successful network callback.
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = loaded.document.raw.clone();
    let mut ungrouped = raw["list_metadata"]["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "V_LANE")
        .unwrap()
        .clone();
    ungrouped["id"] = serde_json::json!("V_UNGROUPED");
    ungrouped["name"] = serde_json::json!("Ungrouped board");
    ungrouped.as_object_mut().unwrap().remove("grouping");
    let mut todo = ungrouped.clone();
    todo["id"] = serde_json::json!("V_TODO");
    todo["name"] = serde_json::json!("Task board");
    todo["show_completed_items"] = serde_json::json!(true);
    todo["grouping"] = serde_json::json!({"group_by":"todo_completed","group_by_column_id":"C_TODO","order":[{"checkbox":false},{"checkbox":true}]});
    raw["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .extend([ungrouped, todo]);
    raw["list_metadata"]["schema"].as_array_mut().unwrap().push(serde_json::json!({"id":"C_TODO","key":"todo_completed","name":"Completed","type":"todo_completed"}));
    raw["list_metadata"]["todo_mode"] = serde_json::json!(true);
    loaded.document = Document::parse(raw).unwrap();
    loaded.list_experiments.as_mut().unwrap().grid_todos = true;
    let mut rows = loaded.records.raw.clone();
    for row in rows["records"].as_array_mut().unwrap() {
        let checked = row["id"] == "R_LIST_002";
        row["fields"].as_array_mut().unwrap().push(
            serde_json::json!({"key":"todo_completed","column_id":"C_TODO","checkbox":checked}),
        );
    }
    loaded.records = RecordsPage::parse(rows).unwrap();
    let canonical = loaded.clone();
    rebuild_rows(&mut s);
    for id in ["V_UNGROUPED", "V_TODO"] {
        s.slack_lists.view.as_mut().unwrap().layout.board_order = None;
        choose(&mut s, id);
        let expected = population(&s);
        assert!(expected.iter().map(|(_, rows)| rows.len()).sum::<usize>() > 0);
        s.slack_lists.view.as_mut().unwrap().layout.board_order = draft.clone();
        rebuild_rows(&mut s);
        assert_eq!(population(&s), expected);
        assert!(adjacent(&s, "true", Direction::Left).is_none());
        choose(&mut s, "V_LANE");
        assert_eq!(keys(&s), ["OPT_DONE", "OPT_PROGRESS"]);
        assert_eq!(
            s.slack_lists.view.as_ref().unwrap().layout.board_order,
            draft
        );
    }
    let loaded = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(loaded.document, canonical.document);
    assert_eq!(loaded.records, canonical.records);
    assert!(entry(&s).is_none());
}
